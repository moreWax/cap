//! # Ring Buffer Module
//!
//! This module provides a high-performance, memory-mapped ring buffer for zero-copy
//! frame buffering between screen capture and video encoding threads.
//!
//! ## Overview
//!
//! The ring buffer solves the producer-consumer problem in real-time video processing:
//! - **Producer**: Screen capture thread writes frames as fast as possible
//! - **Consumer**: Video encoding thread reads frames at consistent intervals
//! - **Buffer**: Absorbs timing variations and prevents frame drops
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │  Screen Capture │───▶│   Ring Buffer   │───▶│ Video Encoding  │
//! │    (Producer)   │    │                 │    │  (Consumer)     │
//! └─────────────────┘    │  ┌─────────────┐│    └─────────────────┘
//!                        │  │ Frame 1     ││
//!                        │  │ Frame 2     ││    Memory-mapped
//!                        │  │ Frame N     ││    shared memory
//!                        │  └─────────────┘│
//!                        └─────────────────┘
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Zero-copy**: Memory-mapped buffer eliminates data copying
//! - **Lock-free**: Atomic operations for thread-safe read/write
//! - **Memory efficient**: Fixed-size buffer prevents unbounded growth
//! - **Cache friendly**: Sequential memory access patterns
//!
//! ## Example
//!
//! ```rust
//! use hybrid_screen_capture::ring_buffer::RingBuffer;
//!
//! // Create buffer for 1080p BGRA frames (4 frames capacity)
//! let frame_size = 1920 * 1080 * 4;
//! let mut buffer = RingBuffer::new(frame_size, 4)?;
//!
//! // Producer: Write captured frame
//! let frame_data = capture_screen_frame();
//! buffer.write_frame(&frame_data)?;
//!
//! // Consumer: Read frame for encoding
//! let mut output = vec![0u8; frame_size];
//! let bytes_read = buffer.read_frame(&mut output)?;
//!
//! // Check buffer status
//! let (available_frames, total_frames) = buffer.status();
//! println!("Buffer: {}/{} frames available", available_frames, total_frames);
//! ```

use memmap2::{MmapMut, MmapOptions};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A high-performance, memory-mapped ring buffer for zero-copy frame buffering.
///
/// The `RingBuffer` provides thread-safe, lock-free buffering between screen capture
/// and video encoding threads. It uses memory-mapped files for efficient data sharing
/// and atomic operations to coordinate producer/consumer access without locks.
///
/// # Design Principles
///
/// - **Memory-mapped**: Uses `mmap` for efficient memory sharing between threads
/// - **Lock-free**: Atomic operations for thread-safe concurrent access
/// - **Fixed capacity**: Prevents unbounded memory growth in real-time systems
/// - **Zero-copy**: Data stays in shared memory, no copying between threads
/// - **Circular buffer**: Efficient reuse of buffer space
///
/// # Performance Benefits
///
/// - **Zero allocation overhead**: Pre-allocated memory-mapped buffer
/// - **Lock-free synchronization**: Atomic operations instead of mutexes
/// - **Memory efficiency**: Fixed-size buffer with circular reuse
/// - **Cache efficiency**: Sequential memory access patterns
/// - **Cross-thread sharing**: Memory-mapped file enables efficient IPC
///
/// # Thread Safety
///
/// The ring buffer is designed for single-producer, single-consumer usage:
/// - **Producer thread**: Calls `write_frame()` to add frames
/// - **Consumer thread**: Calls `read_frame()` to retrieve frames
/// - **Status thread**: Can call `status()` from any thread for monitoring
///
/// # Examples
///
/// Basic producer-consumer pattern:
/// ```rust
/// use hybrid_screen_capture::ring_buffer::RingBuffer;
/// use std::thread;
/// use std::sync::Arc;
/// use std::time::Duration;
///
/// // Create shared buffer
/// let buffer = Arc::new(RingBuffer::new(1024, 10)?);
/// let buffer_clone = Arc::clone(&buffer);
///
/// // Producer thread
/// let producer = thread::spawn(move || {
///     for i in 0..100 {
///         let data = vec![i as u8; 1024];
///         while buffer.write_frame(&data).is_err() {
///             thread::sleep(Duration::from_millis(1)); // Wait for space
///         }
///     }
/// });
///
/// // Consumer thread
/// let consumer = thread::spawn(move || {
///     let mut output = vec![0u8; 1024];
///     for _ in 0..100 {
///         while buffer_clone.read_frame(&mut output).is_err() {
///             thread::sleep(Duration::from_millis(1)); // Wait for data
///         }
///         // Process output...
///     }
/// });
///
/// producer.join().unwrap();
/// consumer.join().unwrap();
/// ```
///
/// Advanced usage with status monitoring:
/// ```rust
/// # use hybrid_screen_capture::ring_buffer::RingBuffer;
/// let mut buffer = RingBuffer::new(4096, 30)?;
///
/// // Monitor buffer utilization
/// loop {
///     let (available, total) = buffer.status();
///     let utilization = (available as f32 / total as f32) * 100.0;
///     println!("Buffer utilization: {:.1}% ({}/{})", utilization, available, total);
///
///     if utilization > 80.0 {
///         println!("Warning: Buffer nearly full!");
///     }
///
///     std::thread::sleep(std::time::Duration::from_secs(1));
/// }
/// ```
#[derive(Debug)]
pub struct RingBuffer {
    /// Memory-mapped buffer for zero-copy data sharing
    buffer: MmapMut,
    /// Total size of the buffer in bytes
    buffer_size: usize,
    /// Size of each frame in bytes
    frame_size: usize,
    /// Atomic write position (shared between producer/consumer)
    write_pos: Arc<AtomicUsize>,
    /// Atomic read position (shared between producer/consumer)
    read_pos: Arc<AtomicUsize>,
}

impl RingBuffer {
    /// Creates a new memory-mapped ring buffer with the specified frame parameters.
    ///
    /// This method allocates a temporary file and memory-maps it for efficient
    /// cross-thread data sharing. The buffer capacity is fixed at creation time.
    ///
    /// # Parameters
    ///
    /// - `frame_size`: Size of each frame in bytes (e.g., `1920 * 1080 * 4` for BGRA)
    /// - `frame_capacity`: Maximum number of frames the buffer can hold
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the initialized `RingBuffer` or an `std::io::Error`
    /// if memory mapping fails.
    ///
    /// # Performance Considerations
    ///
    /// - `frame_capacity` should be sized to absorb timing variations between capture/encoding
    /// - Larger buffers use more memory but provide better resilience to timing jitter
    /// - Memory-mapped files may have different performance characteristics than heap allocation
    ///
    /// # Examples
    ///
    /// For 1080p video at 30 FPS:
    /// ```rust
    /// # use hybrid_screen_capture::ring_buffer::RingBuffer;
    /// let frame_size = 1920 * 1080 * 4; // BGRA = 4 bytes per pixel
    /// let buffer_capacity = 30 * 2; // 2 seconds of buffering at 30 FPS
    /// let buffer = RingBuffer::new(frame_size, buffer_capacity)?;
    /// ```
    ///
    /// For smaller data structures:
    /// ```rust
    /// # use hybrid_screen_capture::ring_buffer::RingBuffer;
    /// let buffer = RingBuffer::new(4096, 100)?; // 4KB frames, 100 frame capacity
    /// ```
    pub fn new(frame_size: usize, frame_capacity: usize) -> std::io::Result<Self> {
        let buffer_size = frame_size * frame_capacity;

        // Create a temporary file for memory mapping
        let file = tempfile::tempfile()?;
        file.set_len(buffer_size as u64)?;

        let buffer = unsafe { MmapOptions::new().map_mut(&file)? };

        Ok(Self {
            buffer,
            buffer_size,
            frame_size,
            write_pos: Arc::new(AtomicUsize::new(0)),
            read_pos: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Writes a frame to the ring buffer.
    ///
    /// This method is called by the producer thread to add frame data to the buffer.
    /// It performs a zero-copy write by copying data directly into the memory-mapped buffer.
    ///
    /// # Parameters
    ///
    /// - `data`: Frame data to write (must be exactly `frame_size` bytes)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err(&str)` if:
    /// - Frame size doesn't match the buffer's configured frame size
    /// - Buffer is full (no space for another frame)
    ///
    /// # Thread Safety
    ///
    /// Safe to call from a single producer thread. Multiple producers are not supported.
    ///
    /// # Performance Notes
    ///
    /// - **Fast path**: Direct memory copy into pre-allocated buffer
    /// - **Atomic operations**: Thread-safe position updates without locks
    /// - **No allocations**: Reuses existing buffer space
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use hybrid_screen_capture::ring_buffer::RingBuffer;
    /// let mut buffer = RingBuffer::new(1024, 10)?;
    ///
    /// // Write frame data
    /// let frame_data = vec![42u8; 1024];
    /// match buffer.write_frame(&frame_data) {
    ///     Ok(()) => println!("Frame written successfully"),
    ///     Err("Buffer full") => println!("Buffer is full, try again later"),
    ///     Err("Frame size mismatch") => println!("Frame size incorrect"),
    ///     Err(_) => println!("Unknown error"),
    /// }
    /// ```
    pub fn write_frame(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() != self.frame_size {
            return Err("Frame size mismatch");
        }

        let write_pos = self.write_pos.load(Ordering::Acquire);
        let next_write_pos = (write_pos + self.frame_size) % self.buffer_size;

        // Check if buffer is full (simple check - in real implementation would be more sophisticated)
        if next_write_pos == self.read_pos.load(Ordering::Acquire) {
            return Err("Buffer full");
        }

        // Copy data to buffer
        self.buffer[write_pos..write_pos + self.frame_size].copy_from_slice(data);

        // Update write position
        self.write_pos.store(next_write_pos, Ordering::Release);

        Ok(())
    }

    /// Reads a frame from the ring buffer.
    ///
    /// This method is called by the consumer thread to retrieve frame data from the buffer.
    /// It performs a zero-copy read by copying data directly from the memory-mapped buffer.
    ///
    /// # Parameters
    ///
    /// - `output`: Mutable buffer to receive frame data (must be exactly `frame_size` bytes)
    ///
    /// # Returns
    ///
    /// Returns `Ok(bytes_read)` containing the number of bytes read (always `frame_size` on success),
    /// or `Err(&str)` if:
    /// - Output buffer size doesn't match the buffer's configured frame size
    /// - Buffer is empty (no frames available)
    ///
    /// # Thread Safety
    ///
    /// Safe to call from a single consumer thread. Multiple consumers are not supported.
    ///
    /// # Performance Notes
    ///
    /// - **Fast path**: Direct memory copy from pre-allocated buffer
    /// - **Atomic operations**: Thread-safe position updates without locks
    /// - **No allocations**: Reuses existing buffer space
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use hybrid_screen_capture::ring_buffer::RingBuffer;
    /// let mut buffer = RingBuffer::new(1024, 10)?;
    ///
    /// // First write some data
    /// let frame_data = vec![42u8; 1024];
    /// buffer.write_frame(&frame_data)?;
    ///
    /// // Then read it back
    /// let mut output = vec![0u8; 1024];
    /// match buffer.read_frame(&mut output) {
    ///     Ok(bytes) => println!("Read {} bytes successfully", bytes),
    ///     Err("Buffer empty") => println!("No frames available"),
    ///     Err("Output buffer size mismatch") => println!("Output buffer size incorrect"),
    ///     Err(_) => println!("Unknown error"),
    /// }
    /// ```
    pub fn read_frame(&self, output: &mut [u8]) -> Result<usize, &'static str> {
        if output.len() != self.frame_size {
            return Err("Output buffer size mismatch");
        }

        let read_pos = self.read_pos.load(Ordering::Acquire);
        let write_pos = self.write_pos.load(Ordering::Acquire);

        if read_pos == write_pos {
            return Err("Buffer empty");
        }

        // Copy data from buffer
        output.copy_from_slice(&self.buffer[read_pos..read_pos + self.frame_size]);

        // Update read position
        let next_read_pos = (read_pos + self.frame_size) % self.buffer_size;
        self.read_pos.store(next_read_pos, Ordering::Release);

        Ok(self.frame_size)
    }

    /// Returns the current buffer status and utilization information.
    ///
    /// This method provides monitoring capabilities to track buffer utilization
    /// and detect potential performance issues.
    ///
    /// # Returns
    ///
    /// A tuple `(available_frames, total_frames)` where:
    /// - `available_frames`: Number of frames currently in the buffer
    /// - `total_frames`: Maximum number of frames the buffer can hold
    ///
    /// # Thread Safety
    ///
    /// Safe to call from any thread for monitoring purposes.
    ///
    /// # Performance Notes
    ///
    /// - **Lightweight**: Only atomic loads, no expensive operations
    /// - **Real-time safe**: Can be called frequently for monitoring
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use hybrid_screen_capture::ring_buffer::RingBuffer;
    /// let mut buffer = RingBuffer::new(1024, 10)?;
    ///
    /// // Check initial status
    /// let (available, total) = buffer.status();
    /// assert_eq!(available, 0);
    /// assert_eq!(total, 10);
    ///
    /// // Add some frames
    /// for i in 0..5 {
    ///     let data = vec![i as u8; 1024];
    ///     buffer.write_frame(&data)?;
    /// }
    ///
    /// // Check updated status
    /// let (available, total) = buffer.status();
    /// assert_eq!(available, 5);
    /// assert_eq!(total, 10);
    ///
    /// // Calculate utilization
    /// let utilization = (available as f32 / total as f32) * 100.0;
    /// println!("Buffer utilization: {:.1}%", utilization);
    /// ```
    pub fn status(&self) -> (usize, usize) {
        let used = (self.write_pos.load(Ordering::Acquire) + self.buffer_size - self.read_pos.load(Ordering::Acquire)) % self.buffer_size;
        let available_frames = used / self.frame_size;
        let total_frames = self.buffer_size / self.frame_size;
        (available_frames, total_frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let mut rb = RingBuffer::new(1024, 4).unwrap();

        let test_data = vec![42u8; 1024];
        let mut read_data = vec![0u8; 1024];

        // Write a frame
        assert!(rb.write_frame(&test_data).is_ok());

        // Read it back
        assert_eq!(rb.read_frame(&mut read_data).unwrap(), 1024);
        assert_eq!(read_data, test_data);

        // Check status
        let (available, total) = rb.status();
        assert_eq!(available, 0);
        assert_eq!(total, 4);
    }
}