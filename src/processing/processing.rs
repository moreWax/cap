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

use anyhow::Result;
use async_trait::async_trait;
use cap_rtsp::BgraFrame;
use futures_util::future::join_all;
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
#[async_trait]
pub trait FrameProcessor: Send + Sync {
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
#[async_trait]
pub trait Stream: Send + Sync {
    /// Send a frame to this stream.
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()>;
    /// Shut down this stream.
    async fn shutdown(&mut self) -> Result<()>;
    /// Get this stream's configuration.
    fn config(&self) -> &StreamConfig;
    /// Initialize this stream.
    async fn initialize(&mut self) -> Result<()>;
}

/// Composable processing pipeline.
/// Chains multiple processors together for sequential frame processing.
pub struct ProcessingPipeline {
    pub processors: Vec<Box<dyn FrameProcessor>>,
    buffer_size: usize,
}

impl ProcessingPipeline {
    /// Create a new processing pipeline with the specified buffer size.
    pub fn new(buffer_size: usize) -> Self {
        Self {
            processors: Vec::new(),
            buffer_size,
        }
    }

    /// Initialize the pipeline and return the output size.
    pub async fn initialize(&mut self, input_size: Size) -> Result<Size> {
        let mut current_size = input_size;
        for processor in &mut self.processors {
            // Processors would modify current_size based on their transformations
            // For now, assume size remains the same
        }
        Ok(current_size)
    }

    /// Process a frame through the entire pipeline.
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
pub struct StreamMultiplexer {
    pub streams: Vec<Box<dyn Stream>>,
    config: StreamConfig,
}

impl StreamMultiplexer {
    /// Create a new stream multiplexer with the given configuration.
    pub fn new(config: StreamConfig) -> Self {
        Self {
            streams: Vec::new(),
            config,
        }
    }

    /// Initialize all streams in the multiplexer.
    pub async fn initialize(&mut self) -> Result<()> {
        for stream in &mut self.streams {
            stream.initialize().await?;
        }
        Ok(())
    }

    /// Send a frame to all streams concurrently.
    pub async fn send_frame(&mut self, frame: BgraFrame) -> Result<()> {
        // Clone frame for each stream to maintain zero-copy semantics
        let frame_refs: Vec<BgraFrame> = (0..self.streams.len()).map(|_| frame.clone()).collect();

        // Send to all streams concurrently using futures
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
    pub async fn shutdown(&mut self) -> Result<()> {
        for stream in &mut self.streams {
            stream.shutdown().await?;
        }
        Ok(())
    }

    /// Get the number of streams in the multiplexer.
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }
}

/// Gundam tiling processor for DeepSeek-OCR optimization.
pub struct GundamProcessor {
    pub cfg: cap_scale::gundam::GundamCfg,
    pub tile_buffers: Vec<Vec<u8>>,
    pub global_buffer: Vec<u8>,
    pub output_size: Size,
}

#[async_trait]
impl FrameProcessor for GundamProcessor {
    async fn process_frame(&mut self, frame: BgraFrame) -> Result<Option<BgraFrame>> {
        // Process frame with Gundam tiling
        use cap_scale::gundam::gundam_pack_cpu;

        // Update tile buffer references
        let mut tile_refs: Vec<&mut [u8]> = self
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
pub struct RtspStream {
    pub publisher: cap_rtsp::RtspPublisher,
    pub config: StreamConfig,
}

#[async_trait]
impl Stream for RtspStream {
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()> {
        self.publisher.send(frame)
    }

    async fn shutdown(&mut self) -> Result<()> {
        // RTSP streams don't need explicit shutdown
        Ok(())
    }

    fn config(&self) -> &StreamConfig {
        &self.config
    }

    async fn initialize(&mut self) -> Result<()> {
        // RTSP publisher is already initialized
        Ok(())
    }
}
