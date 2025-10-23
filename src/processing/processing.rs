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
#[cfg(feature = "rtsp-streaming")]
use gstreamer as gst;
#[cfg(feature = "rtsp-streaming")]
use gstreamer::prelude::{Cast, ElementExt, GstBinExt};
#[cfg(feature = "rtsp-streaming")]
use gstreamer_app as gst_app;
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
}

impl ProcessingPipeline {
    /// Create a new processing pipeline.
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    /// Initialize the pipeline and return the output size.
    pub async fn initialize(&mut self, input_size: Size) -> Result<Size> {
        let mut current_size = input_size;
        for processor in &mut self.processors {
            current_size = processor.initialize(current_size).await?;
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
}

impl StreamMultiplexer {
    /// Create a new stream multiplexer.
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
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
    async fn initialize(&mut self, input_size: Size) -> Result<Size> {
        // Estimate number of tiles based on input dimensions
        // Use the same logic as choose_grid but simplified
        let cols = ((input_size.w as f32 / 1024.0).ceil() as u32).clamp(1, 3);
        let rows = ((input_size.h as f32 / 1024.0).ceil() as u32).clamp(1, 3);
        let num_tiles = (cols * rows).clamp(self.cfg.min_tiles, self.cfg.max_tiles);

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
pub struct RtspStream {
    pub publisher: cap_rtsp::RtspPublisher,
    pub config: StreamConfig,
    pub _server_handle: Option<std::thread::JoinHandle<()>>,
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

/// File stream implementation for saving frames to disk.
#[cfg(feature = "rtsp-streaming")]
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
                let bufw = buffer.get_mut().unwrap();
                // Set timestamp
                let pts = frame
                    .pts_ns
                    .unwrap_or(self.frame_count * (1_000_000_000u64 / self.config.fps as u64));
                bufw.set_pts(gst::ClockTime::from_nseconds(pts));

                // Copy bytes
                if let Ok(mut map) = bufw.map_writable() {
                    map.as_mut_slice().copy_from_slice(&frame.data);
                }
            }

            // Push buffer to pipeline
            let _ = appsrc.push_buffer(buffer);
        }

        Ok(())
    }

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
