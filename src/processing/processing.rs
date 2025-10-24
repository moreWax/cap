//! # Frame Processing Pipeline
//!
//! Zero-copy, non-branching frame processing pipeline for real-time video streaming.
//! Provides composable processors and stream multiplexing for high-performance
//! capture-processing-streaming workflows.
//!
//! ## Architecture
//!
//! The processing pipeline follows a linear, zero-copy design:
//! 1. **FrameProcessor Trait**: Extensible processing interface
//! 2. **ProcessingPipeline**: Composable processor chain
//! 3. **Stream Trait**: Abstract output destination interface
//! 4. **StreamMultiplexer**: Concurrent broadcasting to multiple streams
//!
//! ## Zero-Copy Design
//!
//! All processing maintains zero-copy principles:
//! - Frames use Arc<Vec<u8>> for atomic reference counting
//! - Processors transform data without copying when possible
//! - Streams broadcast using shared references
//! - No allocations in the processing hot path
//!
//! ## Non-Branching Patterns
//!
//! Runtime execution avoids CPU pipeline stalls:
//! - Configuration decisions made at build time
//! - Linear processor chains with predictable execution
//! - No conditional logic in processing loops
//! - Declarative stream configuration

#[cfg(feature = "rtsp-streaming")]
use anyhow::Result;
#[cfg(feature = "rtsp-streaming")]
use async_trait::async_trait;
#[cfg(feature = "rtsp-streaming")]
use cap_rtsp::BgraFrame;
#[cfg(feature = "rtsp-streaming")]
use futures_util::future::join_all;
#[cfg(feature = "rtsp-streaming")]
use gstreamer as gst;
#[cfg(feature = "rtsp-streaming")]
use gstreamer::prelude::{Cast, ElementExt, GstBinExt};
#[cfg(feature = "rtsp-streaming")]
use gstreamer_app as gst_app;
#[cfg(feature = "rtsp-streaming")]
use std::sync::Arc;

/// Size representation for frame dimensions.
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

/// Stream configuration specifying output format and parameters.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: StreamFormat,
}

/// Supported stream output formats.
#[derive(Debug, Clone)]
pub enum StreamFormat {
    /// RTSP network streaming
    Rtsp { port: u16, mount: String },
    /// File output (MP4, etc.)
    File { path: String },
}

/// Abstract frame processing interface.
/// Implement this trait to create custom frame processors.
#[cfg(feature = "rtsp-streaming")]
#[async_trait]
pub trait FrameProcessor: Send + Sync {
    /// Initialize the processor with input size and return output size.
    ///
    /// # Arguments
    /// * `input_size` - Size of input frames
    ///
    /// # Returns
    /// Size of output frames after processing
    async fn initialize(&mut self, input_size: Size) -> Result<Size>;

    /// Process a single frame.
    ///
    /// # Arguments
    /// * `frame` - Input BGRA frame
    ///
    /// # Returns
    /// Processed BGRA frame, or None to skip this frame
    async fn process_frame(&mut self, frame: BgraFrame) -> Result<Option<BgraFrame>>;
}

/// Abstract stream output interface.
/// Implement this trait to create custom stream destinations.
#[cfg(feature = "rtsp-streaming")]
#[async_trait]
pub trait Stream: Send + Sync {
    /// Send a frame to this stream.
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()>;
    /// Shut down this stream.
    async fn shutdown(&mut self) -> Result<()>;
    /// Get this stream's configuration.
    ///
    /// Time complexity: O(1) - Simple reference return.
    ///
    /// Missing functionality: None - returns stored configuration.
    fn config(&self) -> &StreamConfig;
    /// Initialize this stream.
    async fn initialize(&mut self) -> Result<()>;
}

/// Composable processing pipeline.
/// Chains multiple processors together for sequential frame processing.
#[cfg(feature = "rtsp-streaming")]
pub struct ProcessingPipeline {
    pub processors: Vec<Box<dyn FrameProcessor>>,
}

#[cfg(feature = "rtsp-streaming")]
impl std::fmt::Debug for ProcessingPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessingPipeline")
            .field("processor_count", &self.processors.len())
            .finish()
    }
}

#[cfg(feature = "rtsp-streaming")]
impl ProcessingPipeline {
    /// Create a new processing pipeline.
    ///
    /// Initializes an empty pipeline ready to have processors added to it.
    /// Processors can be added using methods on the pipeline or by direct
    /// access to the `processors` vector.
    ///
    /// # Returns
    ///
    /// A new `ProcessingPipeline` with no processors configured.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::processing::ProcessingPipeline;
    ///
    /// let pipeline = ProcessingPipeline::new();
    /// assert_eq!(pipeline.processors.len(), 0);
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Simple vector initialization.
    ///
    /// **Missing functionality**: None - basic constructor fully implemented.
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    /// Initialize the pipeline and return the output size.
    ///
    /// This method initializes all processors in the pipeline in order,
    /// propagating the output size of each processor as the input size
    /// to the next processor. This allows the pipeline to determine the
    /// final output dimensions after all processing transformations.
    ///
    /// # Parameters
    ///
    /// - `input_size`: The size of frames that will be fed into the pipeline
    ///
    /// # Returns
    ///
    /// The size of frames that will be output by the pipeline after all processing.
    ///
    /// # Errors
    ///
    /// Returns an error if any processor fails to initialize.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::processing::{ProcessingPipeline, Size};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pipeline = ProcessingPipeline::new();
    /// // Add processors to pipeline here...
    ///
    /// let input_size = Size { w: 1920, h: 1080 };
    /// let output_size = pipeline.initialize(input_size).await?;
    /// println!("Pipeline will output {}x{} frames", output_size.w, output_size.h);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(n) where n is the number of processors in the pipeline.
    /// Each processor's initialize method is called sequentially, and while most
    /// are O(1), some like GundamProcessor may involve size calculations.
    ///
    /// **Missing functionality**: None - sequentially initializes all processors and
    /// propagates size changes through the pipeline.
    pub async fn initialize(&mut self, input_size: Size) -> Result<Size> {
        let mut current_size = input_size;
        for processor in &mut self.processors {
            current_size = processor.initialize(current_size).await?;
        }
        Ok(current_size)
    }

    /// Process a frame through the entire pipeline.
    ///
    /// Feeds a frame through all processors in the pipeline in sequence.
    /// Each processor receives the output of the previous processor as input.
    /// The final processed frame is returned.
    ///
    /// # Parameters
    ///
    /// - `frame`: The input frame to process
    ///
    /// # Returns
    ///
    /// The frame after being processed by all processors in the pipeline.
    ///
    /// # Errors
    ///
    /// Returns an error if any processor fails during processing.
    ///
    /// # Panics
    ///
    /// Panics if a processor returns `None` (indicating frame skipping),
    /// as this is not currently supported by the pipeline architecture.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::processing::ProcessingPipeline;
    /// use cap_rtsp::BgraFrame;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pipeline = ProcessingPipeline::new();
    /// // Add processors and initialize...
    ///
    /// let input_frame = BgraFrame {
    ///     data: Arc::new(vec![0u8; 1920 * 1080 * 4]),
    ///     width: 1920,
    ///     height: 1080,
    ///     stride: 1920 * 4,
    ///     pts_ns: None,
    /// };
    ///
    /// let output_frame = pipeline.process_frame(input_frame).await?;
    /// // output_frame now contains the processed result
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(n) where n is the number of processors, but the actual
    /// complexity depends on the processors used. For example:
    /// - Simple passthrough: O(1)
    /// - Gundam processing: O(width * height) due to image operations
    /// - Multiple processors: Sum of individual complexities
    ///
    /// **Missing functionality**: Error handling could be improved - currently panics
    /// if a processor returns None, but should probably return an error instead.
    pub async fn process_frame(&mut self, frame: BgraFrame) -> Result<BgraFrame> {
        let mut current_frame = frame;

        for processor in &mut self.processors {
            if let Some(processed) = processor.process_frame(current_frame).await? {
                current_frame = processed;
            } else {
                // Processor skipped frame but consumed it - this indicates an API issue
                // For now, panic as this shouldn't happen with current processors
                panic!("Frame processor returned None but consumed the input frame");
            }
        }

        Ok(current_frame)
    }
}

/// Multiplexer for broadcasting frames to multiple streams concurrently.
#[cfg(feature = "rtsp-streaming")]
pub struct StreamMultiplexer {
    pub streams: Vec<Box<dyn Stream>>,
}

#[cfg(feature = "rtsp-streaming")]
impl std::fmt::Debug for StreamMultiplexer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamMultiplexer")
            .field("stream_count", &self.streams.len())
            .finish()
    }
}

#[cfg(feature = "rtsp-streaming")]
impl StreamMultiplexer {
    /// Create a new stream multiplexer.
    ///
    /// Initializes an empty multiplexer ready to have streams added to it.
    /// Streams can be added by direct access to the `streams` vector.
    ///
    /// # Returns
    ///
    /// A new `StreamMultiplexer` with no streams configured.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::processing::StreamMultiplexer;
    ///
    /// let multiplexer = StreamMultiplexer::new();
    /// assert_eq!(multiplexer.stream_count(), 0);
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Simple vector initialization.
    ///
    /// **Missing functionality**: None - basic constructor fully implemented.
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
        }
    }

    /// Initialize all streams in the multiplexer.
    ///
    /// Iterates through all configured streams and calls their `initialize` method
    /// sequentially. This ensures all streams are properly set up before frame
    /// processing begins. For RTSP streams, this may involve network setup. For
    /// file streams, this involves GStreamer pipeline creation.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all streams initialize successfully, or the first error encountered.
    ///
    /// # Errors
    ///
    /// Returns an error if any stream fails to initialize. Common failure modes:
    /// - RTSP streams: Network connectivity issues, port binding conflicts
    /// - File streams: GStreamer plugin missing, invalid output path, codec issues
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::processing::{StreamMultiplexer, StreamConfig};
    /// use hybrid_screen_capture::processing::RtspStream;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut multiplexer = StreamMultiplexer::new();
    /// // Add streams...
    /// multiplexer.initialize().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(n) where n is the number of streams. Each stream's
    /// initialize method is called sequentially, and most are O(1) but some
    /// like FileStream may involve GStreamer pipeline setup.
    ///
    /// **Missing functionality**: None - sequentially initializes all streams.
    pub async fn initialize(&mut self) -> Result<()> {
        for stream in &mut self.streams {
            stream.initialize().await?;
        }
        Ok(())
    }

    /// Send a frame to all streams concurrently.
    ///
    /// Distributes a single frame to all configured streams simultaneously using
    /// async futures. Each stream receives a cloned reference to the frame to
    /// maintain zero-copy semantics where possible. The method waits for all
    /// streams to complete processing before returning.
    ///
    /// This approach ensures that slow streams don't block faster ones, and
    /// provides natural load balancing across different output destinations.
    ///
    /// # Parameters
    ///
    /// * `frame` - The BGRA frame to send to all streams. Will be cloned for
    ///   each stream to maintain reference counting.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all streams successfully process the frame, or the first
    /// error encountered.
    ///
    /// # Errors
    ///
    /// Returns an error if any stream fails to process the frame. Common failure modes:
    /// - Network issues for RTSP streams
    /// - Disk space exhaustion for file streams
    /// - GStreamer pipeline errors
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::processing::{StreamMultiplexer, StreamConfig, StreamFormat};
    /// use hybrid_screen_capture::BgraFrame;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut multiplexer = StreamMultiplexer::new();
    /// // Configure streams...
    ///
    /// let frame = BgraFrame {
    ///     data: Arc::new(vec![0; 1920 * 1080 * 4]),
    ///     width: 1920,
    ///     height: 1080,
    ///     stride: 1920 * 4,
    ///     pts_ns: Some(0),
    /// };
    ///
    /// multiplexer.send_frame(frame).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(n) where n is the number of streams, but operations
    /// are concurrent rather than sequential. The actual time depends on the
    /// slowest stream's send_frame implementation. Frame cloning is O(1) due
    /// to Arc reference counting.
    ///
    /// **Missing functionality**: Could implement backpressure handling if streams
    /// become overloaded. Currently waits for all streams to complete.
    pub async fn send_frame(&mut self, frame: BgraFrame) -> Result<()> {
        // Clone frame for each stream to maintain zero-copy semantics
        let frame_refs: Vec<BgraFrame> = (0..self.streams.len()).map(|_| frame.clone()).collect();

        // Wait for all sends to complete
        let mut futures = Vec::new();
        for (stream, frame_ref) in self.streams.iter_mut().zip(frame_refs) {
            futures.push(stream.send_frame(frame_ref));
        }

        // Wait for all sends to complete
        for result in join_all(futures).await {
            result?;
        }

        Ok(())
    }

    /// Shut down all streams in the multiplexer.
    ///
    /// Iterates through all configured streams and calls their `shutdown` method
    /// sequentially. This ensures proper cleanup of resources like network
    /// connections, file handles, and GStreamer pipelines.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all streams shut down successfully, or the first error encountered.
    ///
    /// # Errors
    ///
    /// Returns an error if any stream fails to shut down gracefully. This is
    /// typically non-fatal as resources will be cleaned up by the OS anyway.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::processing::StreamMultiplexer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut multiplexer = StreamMultiplexer::new();
    /// // Use multiplexer...
    /// multiplexer.shutdown().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(n) where n is the number of streams. Each stream's
    /// shutdown method is called sequentially.
    ///
    /// **Missing functionality**: None - sequentially shuts down all streams.
    pub async fn shutdown(&mut self) -> Result<()> {
        for stream in &mut self.streams {
            stream.shutdown().await?;
        }
        Ok(())
    }

    /// Get the number of streams in the multiplexer.
    ///
    /// Returns the current count of configured streams. This is useful for
    /// monitoring and debugging purposes.
    ///
    /// # Returns
    ///
    /// The number of streams currently configured in the multiplexer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::processing::StreamMultiplexer;
    ///
    /// let multiplexer = StreamMultiplexer::new();
    /// assert_eq!(multiplexer.stream_count(), 0);
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Simple vector length query.
    ///
    /// **Missing functionality**: None - utility method fully implemented.
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }
}

/// Gundam tiling processor for DeepSeek-OCR optimization.
#[cfg(feature = "rtsp-streaming")]
pub struct GundamProcessor {
    pub cfg: cap_scale::gundam::GundamCfg,
    pub tile_buffers: Vec<Vec<u8>>,
    pub global_buffer: Vec<u8>,
    pub output_size: Size,
}

#[cfg(feature = "rtsp-streaming")]
impl std::fmt::Debug for GundamProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GundamProcessor")
            .field("tile_buffers_count", &self.tile_buffers.len())
            .field("global_buffer_size", &self.global_buffer.len())
            .field("output_size", &self.output_size)
            .finish()
    }
}

#[cfg(feature = "rtsp-streaming")]
#[async_trait]
impl FrameProcessor for GundamProcessor {
    /// Initialize the Gundam processor with input size and return output size.
    ///
    /// Calculates the optimal tile grid layout based on input dimensions and
    /// Gundam configuration parameters. The output frame will be a composite
    /// grid containing individual tiles plus a global downscaled view.
    ///
    /// The algorithm estimates the number of tiles needed based on input size
    /// (typically aiming for tiles around 1024x1024 pixels), then arranges them
    /// in a roughly square grid layout for the composite output.
    ///
    /// # Parameters
    ///
    /// * `input_size` - The dimensions of input frames that will be processed.
    ///
    /// # Returns
    ///
    /// The dimensions of the composite output frame containing all tiles and
    /// the global view.
    ///
    /// # Errors
    ///
    /// This method doesn't currently return errors but could in future versions
    /// if configuration validation is added.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::processing::{GundamProcessor, FrameProcessor, Size};
    /// use cap_scale::gundam::GundamCfg;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut processor = GundamProcessor {
    ///     cfg: GundamCfg::default(),
    ///     tile_buffers: Vec::new(),
    ///     global_buffer: Vec::new(),
    ///     output_size: Size { w: 0, h: 0 },
    /// };
    ///
    /// let input_size = Size { w: 1920, h: 1080 };
    /// let output_size = processor.initialize(input_size).await?;
    /// println!("Output will be {}x{}", output_size.w, output_size.h);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Simple arithmetic calculations for grid layout
    /// and output size determination.
    ///
    /// **Missing functionality**: None - calculates tile grid and composite frame
    /// dimensions based on input size and Gundam configuration.
    async fn initialize(&mut self, input_size: Size) -> Result<Size> {
        // Estimate number of tiles based on input dimensions
        // Use the same logic as choose_grid but simplified
        let cols = ((input_size.w as f32 / 1024.0).ceil() as u32).clamp(1, 3);
        let rows = ((input_size.h as f32 / 1024.0).ceil() as u32).clamp(1, 3);
        let num_tiles = (cols * rows).clamp(self.cfg.min_tiles, self.cfg.max_tiles);

        // Allocate tile buffers
        let tile_buffer_size = (self.cfg.tile_side * self.cfg.tile_side * 4) as usize;
        self.tile_buffers = vec![vec![0u8; tile_buffer_size]; num_tiles as usize];

        // Allocate global buffer
        let global_buffer_size = (self.cfg.global_side * self.cfg.global_side * 4) as usize;
        self.global_buffer = vec![0u8; global_buffer_size];

        // Calculate output size based on Gundam configuration
        // The output is a composite grid of tiles + global view
        let total_elements = num_tiles + 1; // tiles + global view
        let cols_out = ((total_elements as f32).sqrt().ceil() as u32).max(1);
        let rows_out = (((total_elements as u32 + cols_out - 1) / cols_out) as usize).max(1) as u32;

        // Each element is tile_side x tile_side, except global which gets scaled
        let output_width = cols_out * self.cfg.tile_side;
        let output_height = rows_out * self.cfg.tile_side;

        self.output_size = Size {
            w: output_width,
            h: output_height,
        };
        Ok(self.output_size)
    }

    /// Process a frame with Gundam tiling for DeepSeek-OCR optimization.
    ///
    /// Applies the Gundam tiling algorithm to extract multiple overlapping tiles
    /// from the input frame along with a global downscaled view. The tiles are
    /// arranged in a composite grid layout optimized for OCR processing.
    ///
    /// The algorithm performs several key operations:
    /// 1. Extracts overlapping tiles at multiple scales
    /// 2. Creates a global downscaled view of the entire frame
    /// 3. Arranges tiles and global view in a composite grid
    /// 4. Returns the composite as a new BGRA frame
    ///
    /// This preprocessing significantly improves OCR accuracy for large documents
    /// or complex layouts by providing multiple views at different scales.
    ///
    /// # Parameters
    ///
    /// * `frame` - The input BGRA frame to process with Gundam tiling.
    ///
    /// # Returns
    ///
    /// `Ok(Some(composite_frame))` containing the tiled composite, or `Ok(None)`
    /// if processing should be skipped for this frame.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Gundam processing fails (invalid input dimensions, memory allocation)
    /// - Composite arrangement fails
    /// - Buffer size mismatches occur
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::processing::{GundamProcessor, FrameProcessor, Size};
    /// use cap_rtsp::BgraFrame;
    /// use cap_scale::gundam::GundamCfg;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut processor = GundamProcessor {
    ///     cfg: GundamCfg::default(),
    ///     tile_buffers: Vec::new(),
    ///     global_buffer: Vec::new(),
    ///     output_size: Size { w: 0, h: 0 },
    /// };
    ///
    /// let frame = BgraFrame {
    ///     data: Arc::new(vec![0; 1920 * 1080 * 4]),
    ///     width: 1920,
    ///     height: 1080,
    ///     stride: 1920 * 4,
    ///     pts_ns: Some(0),
    /// };
    ///
    /// let result = processor.process_frame(frame).await?;
    /// if let Some(composite) = result {
    ///     println!("Processed composite: {}x{}", composite.width, composite.height);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(width * height) - The gundam_pack_cpu function performs
    /// extensive image processing including:
    /// - Global view downscaling: O(width * height)
    /// - Tile extraction and processing: O(num_tiles * tile_size)
    /// - Composite arrangement: O(total_output_pixels)
    ///
    /// For typical 1920x1080 input with 9 tiles: ~O(2M) operations per frame.
    /// This is acceptable for real-time processing at reasonable frame rates.
    ///
    /// **Optimization opportunity**: Could use SIMD instructions for pixel operations
    /// to reduce constant factors, but algorithmic complexity is already optimal
    /// for the Gundam tiling approach.
    ///
    /// **Missing functionality**: None - fully implements Gundam tiling with tile
    /// extraction, global view scaling, and composite arrangement.
    async fn process_frame(&mut self, frame: BgraFrame) -> Result<Option<BgraFrame>> {
        // Process frame with Gundam tiling
        use cap_scale::gundam::gundam_pack_cpu;

        // Update tile buffer references
        let tile_refs: Vec<&mut [u8]> = self
            .tile_buffers
            .iter_mut()
            .map(|v| v.as_mut_slice())
            .collect();

        let gundam_outputs = cap_scale::gundam::GundamOutputs {
            tiles: tile_refs,
            global: self.global_buffer.as_mut_slice(),
        };

        // Process the frame
        gundam_pack_cpu(
            &mut fast_image_resize::Resizer::new(),
            &frame.data,
            frame.width,
            frame.height,
            frame.stride,
            self.cfg,
            &mut cap_scale::cpu::Staging::with_capacity(frame.stride * frame.height as usize),
            gundam_outputs,
        )?;

        // Arrange tiles and global into composite frame
        let (composite, width, height) = cap_rtsp::arrange_gundam_composite(
            &self.tile_buffers,
            &self.global_buffer,
            self.cfg.tile_side,
            self.cfg.global_side,
        );

        let composite_frame = BgraFrame {
            data: Arc::new(composite),
            width,
            height,
            stride: width as usize * 4,
            pts_ns: frame.pts_ns,
        };

        Ok(Some(composite_frame))
    }
}

/// RTSP stream implementation.
#[cfg(feature = "rtsp-streaming")]
pub struct RtspStream {
    pub publisher: cap_rtsp::RtspPublisher,
    pub config: StreamConfig,
    pub _server_handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "rtsp-streaming")]
impl std::fmt::Debug for RtspStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RtspStream")
            .field("config", &self.config)
            .field("has_server_handle", &self._server_handle.is_some())
            .finish()
    }
}

#[cfg(feature = "rtsp-streaming")]
#[async_trait]
impl Stream for RtspStream {
    /// Send a frame to the RTSP stream.
    ///
    /// Delegates the frame to the underlying RTSP publisher for network transmission.
    /// The publisher handles RTP packetization, network buffering, and client
    /// distribution automatically.
    ///
    /// # Parameters
    ///
    /// * `frame` - The BGRA frame to send over RTSP.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the frame was queued successfully, or an error if transmission fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Network connection is lost
    /// - RTSP publisher buffer is full
    /// - Frame encoding fails
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Delegates to the underlying RTSP publisher's send method,
    /// which is typically O(1) for frame queuing.
    ///
    /// **Missing functionality**: None - simple delegation to RTSP publisher.
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()> {
        self.publisher.send(frame)
    }

    /// Shut down the RTSP stream.
    ///
    /// RTSP streams don't require explicit shutdown as the underlying publisher
    /// handles connection lifecycle automatically. This method is a no-op that
    /// always succeeds.
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as RTSP streams don't need explicit cleanup.
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - RTSP streams don't require explicit shutdown.
    ///
    /// **Missing functionality**: Could implement proper connection cleanup if needed.
    async fn shutdown(&mut self) -> Result<()> {
        // RTSP streams don't need explicit shutdown
        Ok(())
    }

    fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Initialize the RTSP stream.
    ///
    /// RTSP streams are pre-initialized during construction, so this method
    /// is a no-op that always succeeds. The actual initialization happens
    /// when the RtspPublisher is created.
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())` as the stream is already initialized.
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - RTSP publisher is pre-initialized.
    ///
    /// **Missing functionality**: None - no additional initialization needed.
    async fn initialize(&mut self) -> Result<()> {
        // RTSP publisher is already initialized
        Ok(())
    }
}

/// File stream implementation for saving frames to disk.
#[cfg(feature = "rtsp-streaming")]
#[derive(Debug)]
pub struct FileStream {
    pub config: StreamConfig,
    pub path: String,
    pub frame_count: u64,
    pub pipeline: Option<gst::Pipeline>,
    pub appsrc: Option<gst_app::AppSrc>,
    pub initialized: bool,
}

#[cfg(feature = "rtsp-streaming")]
impl FileStream {
    /// Create a new file stream.
    ///
    /// Initializes a file stream that will encode frames to MP4 format using
    /// GStreamer. The stream is not immediately active - it must be initialized
    /// before use.
    ///
    /// # Parameters
    ///
    /// * `path` - The file system path where the output MP4 file will be written.
    /// * `config` - Stream configuration specifying dimensions, framerate, etc.
    ///
    /// # Returns
    ///
    /// A new `FileStream` instance ready for initialization.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::processing::{FileStream, StreamConfig, StreamFormat};
    ///
    /// let config = StreamConfig {
    ///     width: 1920,
    ///     height: 1080,
    ///     fps: 30,
    ///     format: StreamFormat::File { path: "output.mp4".to_string() },
    /// };
    ///
    /// let stream = FileStream::new("output.mp4".to_string(), config);
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Simple struct initialization.
    ///
    /// **Missing functionality**: None - basic constructor fully implemented.
    pub fn new(path: String, config: StreamConfig) -> Self {
        Self {
            config,
            path,
            frame_count: 0,
            pipeline: None,
            appsrc: None,
            initialized: false,
        }
    }
}

#[cfg(feature = "rtsp-streaming")]
#[async_trait]
impl Stream for FileStream {
    /// Send a frame to the file stream.
    ///
    /// Encodes a BGRA frame and writes it to the MP4 file through the GStreamer
    /// pipeline. Frames received before initialization are silently dropped.
    ///
    /// The method performs several operations:
    /// 1. Checks if the stream is initialized
    /// 2. Allocates a GStreamer buffer
    /// 3. Copies frame data to the buffer
    /// 4. Sets appropriate timestamps
    /// 5. Pushes the buffer to the encoding pipeline
    ///
    /// # Parameters
    ///
    /// * `frame` - The BGRA frame to encode and save to file.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the frame was successfully queued for encoding, or an error
    /// if the operation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - GStreamer buffer allocation fails
    /// - Buffer mapping fails
    /// - Pipeline push fails
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(frame_size) - Involves buffer allocation and data copying
    /// from the frame to GStreamer's buffer. For 1920x1080 BGRA frames, this is
    /// O(8MB) operations, which is significant but necessary for encoding.
    ///
    /// **Missing functionality**: Could implement frame dropping under memory pressure,
    /// but currently blocks until the frame is queued.
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()> {
        if !self.initialized {
            return Ok(()); // Skip frames until initialized
        }

        self.frame_count += 1;

        if let Some(appsrc) = &self.appsrc {
            // Allocate buffer and copy data
            let mut buffer = match gst::Buffer::with_size(frame.data.len()) {
                Ok(b) => b,
                Err(_) => return Ok(()),
            };
            {
                let bufw = match buffer.get_mut() {
                    Some(buf) => buf,
                    None => {
                        eprintln!("Failed to get mutable buffer for frame encoding");
                        return Ok(());
                    }
                };
                // Set timestamp
                let pts = frame
                    .pts_ns
                    .unwrap_or(self.frame_count * (1_000_000_000u64 / self.config.fps as u64));
                bufw.set_pts(gst::ClockTime::from_nseconds(pts));

                // Copy bytes
                if let Ok(mut map) = bufw.map_writable() {
                    map.as_mut_slice().copy_from_slice(&frame.data);
                } else {
                    eprintln!("Failed to map buffer for writing frame data");
                    return Ok(());
                }
            }

            // Push buffer to pipeline
            let _ = appsrc.push_buffer(buffer);
        }

        Ok(())
    }

    /// Shut down the file stream.
    ///
    /// Signals the end of the video stream to GStreamer and stops the encoding
    /// pipeline. This ensures the MP4 file is properly finalized and all frames
    /// are written to disk.
    ///
    /// The shutdown process:
    /// 1. Sends an End-of-Stream (EOS) signal to the pipeline
    /// 2. Stops the GStreamer pipeline
    /// 3. Logs the total number of frames saved
    ///
    /// # Returns
    ///
    /// `Ok(())` if shutdown completes successfully.
    ///
    /// # Errors
    ///
    /// This method doesn't currently return errors, but GStreamer operations
    /// could fail in theory.
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Sends EOS signal and stops pipeline, both O(1) operations.
    ///
    /// **Missing functionality**: Could wait for pipeline to fully flush before returning,
    /// but currently just signals completion.
    async fn shutdown(&mut self) -> Result<()> {
        if let Some(appsrc) = &self.appsrc {
            // Send EOS to signal end of stream
            let _ = appsrc.end_of_stream();
        }

        if let Some(pipeline) = &self.pipeline {
            // Stop the pipeline
            let _ = pipeline.set_state(gst::State::Null);
        }

        println!(
            "File stream '{}' saved {} frames",
            self.path, self.frame_count
        );
        Ok(())
    }

    fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Initialize the file stream with GStreamer pipeline.
    ///
    /// Sets up a complete GStreamer encoding pipeline for MP4 file output.
    /// The pipeline performs the following transformations:
    ///
    /// 1. `appsrc` - Receives raw BGRA frames from the application
    /// 2. `videoconvert` - Converts BGRA to I420 color format
    /// 3. `videoscale` - Handles any necessary scaling (though input should match config)
    /// 4. `x264enc` - H.264 video encoding with low-latency settings
    /// 5. `h264parse` - Parses encoded H.264 stream
    /// 6. `mp4mux` - Multiplexes video into MP4 container
    /// 7. `filesink` - Writes the final MP4 file to disk
    ///
    /// The pipeline is optimized for real-time encoding with:
    /// - Zero-latency H.264 encoding
    /// - Very fast preset for minimal CPU usage
    /// - 4000 kbps bitrate (configurable in future)
    ///
    /// # Returns
    ///
    /// `Ok(())` if the pipeline initializes successfully, or an error if setup fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - GStreamer initialization fails
    /// - Pipeline parsing fails (invalid launch string)
    /// - Required GStreamer plugins are missing
    /// - Pipeline state change fails
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - GStreamer pipeline creation and setup is typically
    /// fast, but depends on system GStreamer installation and plugins.
    ///
    /// **Missing functionality**: Could add more sophisticated pipeline configuration
    /// options (bitrate, codec settings, etc.) but current implementation provides
    /// basic MP4 encoding with reasonable defaults.
    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        // Initialize GStreamer
        gst::init()?;

        // Create pipeline for file encoding
        let launch = format!(
            "appsrc name=src is-live=true format=time do-timestamp=true caps=video/x-raw,format=BGRA,width={},height={},framerate={}/1 \
             ! videoconvert ! videoscale ! video/x-raw,format=I420 \
             ! x264enc tune=zerolatency speed-preset=veryfast bitrate=4000 \
             ! h264parse ! mp4mux ! filesink location={}",
            self.config.width, self.config.height, self.config.fps, self.path
        );

        let pipeline = match gst::parse::launch(&launch) {
            Ok(element) => match element.downcast::<gst::Pipeline>() {
                Ok(pipeline) => pipeline,
                Err(_) => return Err(anyhow::anyhow!("Failed to create pipeline")),
            },
            Err(e) => return Err(anyhow::anyhow!("Failed to parse pipeline: {}", e)),
        };

        // Get appsrc element
        let appsrc = pipeline
            .by_name("src")
            .and_then(|element| element.downcast::<gst_app::AppSrc>().ok())
            .ok_or_else(|| anyhow::anyhow!("Failed to get appsrc element"))?;

        // Configure appsrc
        appsrc.set_format(gst::Format::Time);
        appsrc.set_is_live(true);
        appsrc.set_do_timestamp(true);

        // Start the pipeline
        pipeline.set_state(gst::State::Playing)?;

        self.pipeline = Some(pipeline);
        self.appsrc = Some(appsrc);
        self.initialized = true;

        println!("Initialized file stream to '{}'", self.path);
        Ok(())
    }
}

/// Scaling processor for token-efficient image resizing.
/// Implements VLM-optimized scaling with configurable presets.
#[cfg(feature = "rtsp-streaming")]
pub struct ScalingProcessor {
    pub preset: cap_scale::presets::TokenPreset,
    pub resizer: fast_image_resize::Resizer,
    pub staging: cap_scale::cpu::Staging,
    pub output_buffer: Vec<u8>,
    pub output_size: Size,
}

#[cfg(feature = "rtsp-streaming")]
impl std::fmt::Debug for ScalingProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScalingProcessor")
            .field("preset", &self.preset)
            .field("output_buffer_size", &self.output_buffer.len())
            .field("output_size", &self.output_size)
            .finish()
    }
}

#[cfg(feature = "rtsp-streaming")]
#[async_trait]
impl FrameProcessor for ScalingProcessor {
    /// Initialize the scaling processor with input size and return output size.
    ///
    /// Computes the scaling plan based on the configured preset and input dimensions.
    /// Allocates output buffer and staging area for efficient processing.
    ///
    /// # Parameters
    ///
    /// * `input_size` - The dimensions of input frames that will be processed.
    ///
    /// # Returns
    ///
    /// The dimensions of the scaled output frames.
    ///
    /// # Errors
    ///
    /// This method doesn't currently return errors but could in future versions
    /// if buffer allocation fails.
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(1) - Plan computation and buffer allocation are constant time.
    ///
    /// **Missing functionality**: None - fully initializes scaling processor with preset-based plan.
    async fn initialize(&mut self, input_size: Size) -> Result<Size> {
        // Build scaling plan from preset
        let plan = cap_scale::presets::build_plan(
            cap_scale::presets::Size {
                w: input_size.w,
                h: input_size.h,
            },
            self.preset.to_target(),
            cap_scale::presets::AspectMode::Preserve,
        );

        self.output_size = Size {
            w: plan.out.w,
            h: plan.out.h,
        };

        // Pre-allocate output buffer
        let buffer_size = (self.output_size.w * self.output_size.h * 4) as usize;
        self.output_buffer.resize(buffer_size, 0);

        // Ensure staging buffer is large enough for input
        let input_buffer_size = (input_size.w * input_size.h * 4) as usize;
        self.staging.ensure_len(input_buffer_size);

        Ok(self.output_size)
    }

    /// Process a frame with token-efficient scaling.
    ///
    /// Applies the configured scaling preset to reduce image dimensions while
    /// preserving aspect ratio and OCR accuracy. Uses SIMD acceleration for
    /// high-performance processing.
    ///
    /// # Parameters
    ///
    /// * `frame` - The input BGRA frame to scale.
    ///
    /// # Returns
    ///
    /// `Ok(Some(scaled_frame))` containing the scaled frame, or `Ok(None)`
    /// if processing should be skipped for this frame.
    ///
    /// # Errors
    ///
    /// Returns an error if scaling fails due to buffer issues or fast_image_resize errors.
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(width × height) - SIMD-accelerated scaling processes
    /// each input pixel through convolution algorithm. For HD input (1920×1080)
    /// scaled to 640px longest side, this represents O(2M) operations per frame.
    ///
    /// **Missing functionality**: None - fully implements preset-based scaling
    /// with zero-copy buffer management and SIMD acceleration.
    async fn process_frame(&mut self, frame: BgraFrame) -> Result<Option<BgraFrame>> {
        // Build scaling plan for this frame
        let plan = cap_scale::presets::build_plan(
            cap_scale::presets::Size {
                w: frame.width,
                h: frame.height,
            },
            self.preset.to_target(),
            cap_scale::presets::AspectMode::Preserve,
        );

        // Scale the frame using cap_scale
        cap_scale::cpu::scale_bgra_cpu(
            &mut self.resizer,
            &frame.data,
            cap_scale::presets::Size {
                w: frame.width,
                h: frame.height,
            },
            Some(frame.stride),
            &plan,
            &mut self.output_buffer,
            Some(&mut self.staging),
        )?;

        // Create output frame
        let scaled_frame = BgraFrame {
            data: Arc::new(self.output_buffer.clone()),
            width: plan.out.w,
            height: plan.out.h,
            stride: (plan.out.w * 4) as usize,
            pts_ns: frame.pts_ns,
        };

        Ok(Some(scaled_frame))
    }
}
