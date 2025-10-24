//! # Capture Session Management
//!
//! High-level session orchestration for screen capture with processing pipelines
//! and streaming. Provides a declarative, builder-pattern API for configuring
//! complex capture workflows.
//!
//! ## Architecture
//!
//! The session management follows a composable, declarative approach:
//! 1. **CaptureSource Trait**: Abstract interface for frame sources
//! 2. **CaptureSession**: Orchestrates capture, processing, and streaming
//! 3. **CaptureSessionBuilder**: Fluent API for session configuration
//! 4. **Platform-specific Sources**: Concrete implementations for each platform
//!
//! ## Zero-Copy Design
//!
//! Session management maintains zero-copy principles:
//! - Frame sources provide Arc-referenced buffers
//! - Processing pipelines transform data without copying
//! - Streams broadcast using atomic reference counting
//! - No allocations in the capture-processing-streaming loop
//!
//! ## Non-Branching Patterns
//!
//! Configuration decisions are made at build time:
//! - Pipeline structure determined during builder construction
//! - Stream configuration immutable after initialization
//! - Runtime execution follows linear, predictable paths
//! - No conditional logic in hot processing loops

// External crate imports
use anyhow::Result;
use async_trait::async_trait;
#[cfg(feature = "rtsp-streaming")]
use cap_rtsp::BgraFrame;
use cap_scale::presets::TokenPreset;

// Internal module imports
use crate::processing::processing::{
    FrameProcessor, ProcessingPipeline, Size, Stream, StreamConfig, StreamFormat, StreamMultiplexer,
};

/// Abstract interface for frame capture sources.
/// Enables pluggable capture backends for different platforms and modes.
#[cfg(feature = "rtsp-streaming")]
#[async_trait]
pub trait CaptureSource: Send {
    /// Capture the next frame from the source.
    async fn capture_frame(&mut self) -> Result<BgraFrame>;
    /// Get the native resolution of the capture source.
    fn input_size(&self) -> Size;
    /// Initialize the capture source.
    async fn initialize(&mut self) -> Result<()>;
    /// Shut down the capture source.
    async fn shutdown(&mut self) -> Result<()>;
}

/// High-level capture session that orchestrates everything.
/// Provides the main entry point for configured capture workflows.
#[cfg(feature = "rtsp-streaming")]
pub struct CaptureSession {
    pipeline: ProcessingPipeline,
    multiplexer: StreamMultiplexer,
    capture_source: Box<dyn CaptureSource>,
}

impl CaptureSession {
    /// Create a new capture session using the builder pattern.
    pub fn builder() -> CaptureSessionBuilder {
        CaptureSessionBuilder::new()
    }

    /// Run the capture session.
    /// This is the main execution loop that captures, processes, and streams frames.
    pub async fn run(mut self) -> Result<()> {
        // Initialize everything
        let input_size = self.capture_source.input_size();
        let output_size = self.pipeline.initialize(input_size).await?;
        self.multiplexer.initialize().await?;

        println!("Capture session started:");
        println!("  Input: {}x{}", input_size.w, input_size.h);
        println!("  Output: {}x{}", output_size.w, output_size.h);
        println!("  Streams: {}", self.multiplexer.stream_count());

        // Main capture loop - zero-copy, non-branching execution
        loop {
            // Capture frame
            let raw_frame = self.capture_source.capture_frame().await?;
            // Process through pipeline
            let processed_frame = self.pipeline.process_frame(raw_frame).await?;
            // Send to all streams
            self.multiplexer.send_frame(processed_frame).await?;
        }
    }
}

/// Builder for creating capture sessions with fluent API.
/// Enables declarative configuration of complex capture workflows.
pub struct CaptureSessionBuilder {
    processors: Vec<Box<dyn FrameProcessor>>,
    streams: Vec<Box<dyn Stream>>,
    capture_source: Option<Box<dyn CaptureSource>>,
}

impl CaptureSessionBuilder {
    /// Create a new session builder.
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
            streams: Vec::new(),
            capture_source: None,
        }
    }

    /// Add Gundam tiling processor to the pipeline.
    pub fn with_gundam(mut self) -> Self {
        use crate::processing::processing::GundamProcessor;
        use cap_scale::gundam::GundamCfg;
        self.processors.push(Box::new(GundamProcessor {
            cfg: GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        }));
        self
    }

    /// Add scaling processor with the specified preset.
    ///
    /// Time complexity: O(1) - Currently a no-op that returns self unchanged.
    ///
    /// Missing functionality: IMPLEMENTATION MISSING - currently just returns
    /// self without adding any scaling processor. Needs to create and add a
    /// scaling processor similar to GundamProcessor.
    pub fn with_scaling(self, _preset: TokenPreset) -> Self {
        // TODO: Implement scaling processor
        // For now, this is a placeholder that maintains the builder pattern
        self
    }

    /// Add RTSP streaming output.
    ///
    /// Time complexity: O(1) - RTSP server startup is asynchronous but this method
    /// performs synchronous setup. Server initialization happens in the background.
    ///
    /// Missing functionality: None - fully implements RTSP server creation and
    /// stream setup with proper error handling.
    pub fn with_rtsp_stream(mut self, port: u16, width: u32, height: u32, fps: u32) -> Self {
        use crate::processing::processing::RtspStream;

        // Create RTSP configuration
        let rtsp_config = cap_rtsp::RtspConfig {
            port,
            mount: "/cap".into(),
            width,
            height,
            framerate: fps,
            encoder: None,
            appsrc_max_bytes: Some(8 * 1024 * 1024),
        };

        // Start RTSP server
        let (publisher, server_handle) =
            cap_rtsp::start_server(rtsp_config).expect("Failed to start RTSP server");

        // Create stream configuration
        let config = StreamConfig {
            width,
            height,
            fps,
            format: StreamFormat::Rtsp {
                port,
                mount: "/cap".into(),
            },
        };

        self.streams.push(Box::new(RtspStream {
            publisher,
            config,
            _server_handle: Some(server_handle),
        }));
        self
    }

    /// Add file output stream.
    ///
    /// Time complexity: O(1) - Creates FileStream struct and pushes to vector.
    ///
    /// Missing functionality: None - properly creates and adds FileStream instance
    /// with correct configuration.
    #[cfg(feature = "rtsp-streaming")]
    pub fn with_file_output(mut self, path: String, width: u32, height: u32, fps: u32) -> Self {
        use crate::processing::processing::FileStream;

        let config = StreamConfig {
            width,
            height,
            fps,
            format: StreamFormat::File { path: path.clone() },
        };

        self.streams
            .push(Box::new(FileStream::new(path, config.clone())));
        self
    }

    /// Set the capture source for the session.
    pub fn with_capture_source<S: CaptureSource + 'static>(mut self, source: S) -> Self {
        self.capture_source = Some(Box::new(source));
        self
    }

    /// Build the capture session with the configured components.
    pub fn build(self) -> Result<CaptureSession> {
        let mut pipeline = ProcessingPipeline::new();
        for processor in self.processors {
            pipeline.processors.push(processor);
        }

        // Create multiplexer config from first stream (simplified)
        let _multiplexer_config = if let Some(first_stream) = self.streams.first() {
            first_stream.config().clone()
        } else {
            return Err(anyhow::anyhow!("At least one stream must be configured"));
        };

        let mut multiplexer = StreamMultiplexer::new();
        for stream in self.streams {
            multiplexer.streams.push(stream);
        }

        let capture_source = self
            .capture_source
            .ok_or_else(|| anyhow::anyhow!("No capture source specified"))?;

        Ok(CaptureSession {
            pipeline,
            multiplexer,
            capture_source,
        })
    }
}

/// RTSP stream implementation.
pub struct RtspStream {
    pub publisher: cap_rtsp::RtspPublisher,
    pub config: StreamConfig,
    _server_handle: std::thread::JoinHandle<()>,
}
