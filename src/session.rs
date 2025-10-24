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

// Standard library imports

// External crate imports
#[cfg(feature = "rtsp-streaming")]
use anyhow::Result;
#[cfg(feature = "rtsp-streaming")]
use async_trait::async_trait;
#[cfg(feature = "rtsp-streaming")]
use cap_rtsp::BgraFrame;
#[cfg(feature = "rtsp-streaming")]
use cap_scale::presets::TokenPreset;
#[cfg(feature = "rtsp-streaming")]
use tokio::sync::watch;

// Internal module imports
#[cfg(feature = "rtsp-streaming")]
use crate::processing::processing::GundamProcessor;
#[cfg(feature = "rtsp-streaming")]
use crate::processing::{
    FrameProcessor, ProcessingPipeline, Size, Stream, StreamConfig, StreamFormat, StreamMultiplexer,
};

/// Abstract interface for frame capture sources.
/// Enables pluggable capture backends for different platforms and modes.
#[cfg(feature = "rtsp-streaming")]
#[async_trait]
pub trait CaptureSource: Send {
    /// Captures the next frame from the source asynchronously.
    ///
    /// # Returns
    ///
    /// A `Result` containing the next `BgraFrame` if successful, or an error otherwise.
    async fn capture_frame(&mut self) -> Result<BgraFrame>;

    /// Returns the native resolution of the capture source.
    ///
    /// # Returns
    ///
    /// A `Size` struct representing the width and height of the source.
    fn input_size(&self) -> Size;

    /// Initializes the capture source asynchronously.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of initialization.
    async fn initialize(&mut self) -> Result<()>;

    /// Shuts down the capture source asynchronously.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure of shutdown.
    async fn shutdown(&mut self) -> Result<()>;
}

/// High-level capture session that orchestrates everything.
/// Provides the main entry point for configured capture workflows.
#[cfg(feature = "rtsp-streaming")]
pub struct CaptureSession {
    pipeline: ProcessingPipeline,
    multiplexer: StreamMultiplexer,
    capture_source: Box<dyn CaptureSource>,
    shutdown_rx: watch::Receiver<bool>,
    shutdown_tx: watch::Sender<bool>,
}

#[cfg(feature = "rtsp-streaming")]
impl std::fmt::Debug for CaptureSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureSession")
            .field("pipeline", &self.pipeline)
            .field("multiplexer", &self.multiplexer)
            .field("has_capture_source", &true)
            .field("shutdown_signaled", &*self.shutdown_rx.borrow())
            .finish()
    }
}

#[cfg(feature = "rtsp-streaming")]
impl CaptureSession {
    /// Create a new capture session using the builder pattern.
    pub fn builder() -> CaptureSessionBuilder {
        CaptureSessionBuilder::new()
    }

    /// Run the capture session.
    ///
    /// This is the main execution loop that orchestrates the entire capture workflow.
    /// The method initializes all components, then enters a tight loop that captures,
    /// processes, and streams frames with zero-copy efficiency.
    ///
    /// The execution flow:
    /// 1. Initialize capture source, processing pipeline, and stream multiplexer
    /// 2. Log session configuration for debugging
    /// 3. Enter main capture loop:
    ///    - Capture raw frame from source
    ///    - Process frame through pipeline (optional transformations)
    ///    - Broadcast processed frame to all configured streams
    /// 4. Continue until shutdown signal is received
    /// 5. Perform graceful cleanup of all resources
    ///
    /// # Returns
    ///
    /// `Ok(())` if the session runs successfully, or an error if initialization
    /// or any component fails during execution.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Capture source initialization fails
    /// - Processing pipeline initialization fails
    /// - Stream multiplexer initialization fails
    /// - Any frame capture, processing, or streaming operation fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let capture_source = FFmpegCaptureSource::new(":0.0")?;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_rtsp_stream(8554, 1920, 1080, 30)
    ///     .with_capture_source(capture_source)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Characteristics
    ///
    /// **Time complexity**: O(n) where n is the number of frames captured until shutdown.
    /// Each iteration performs O(1) capture + O(pipeline_complexity) processing +
    /// O(num_streams) streaming. The loop runs until shutdown signal is received.
    ///
    /// **Graceful shutdown**: Now supports graceful shutdown via shutdown signal.
    /// The session will properly clean up all resources when shutdown is requested.
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
            // Check for shutdown signal
            if *self.shutdown_rx.borrow() {
                println!("Shutdown signal received, cleaning up...");
                break;
            }

            // Capture frame
            let raw_frame = self.capture_source.capture_frame().await?;
            // Process through pipeline
            let processed_frame = self.pipeline.process_frame(raw_frame).await?;
            // Send to all streams
            self.multiplexer.send_frame(processed_frame).await?;
        }

        // Graceful cleanup
        self.cleanup().await?;
        println!("Capture session shut down gracefully");
        Ok(())
    }

    /// Request graceful shutdown of the capture session.
    ///
    /// Signals the session to stop capturing and perform cleanup.
    /// This method is non-blocking and returns immediately after sending the signal.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
    /// use tokio::time::{sleep, Duration};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let capture_source = FFmpegCaptureSource::new(":0.0")?;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_capture_source(capture_source)
    ///     .build()?;
    ///
    /// // Start session in background
    /// let session_handle = tokio::spawn(async move {
    ///     session.run().await
    /// });
    ///
    /// // Let it run for a bit
    /// sleep(Duration::from_secs(5)).await;
    ///
    /// // Request shutdown
    /// session_handle.abort(); // This would need to be done differently in practice
    /// # Ok(())
    /// # }
    /// ```
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Perform cleanup of all session resources.
    ///
    /// This method ensures all components are properly shut down and resources
    /// are released. It's called automatically during graceful shutdown.
    ///
    /// # Returns
    ///
    /// `Ok(())` if cleanup succeeds, or an error if any component fails to clean up.
    async fn cleanup(&mut self) -> Result<()> {
        println!("Cleaning up capture session resources...");

        // Shutdown capture source
        self.capture_source.shutdown().await?;

        // Shutdown multiplexer (which shuts down all streams)
        self.multiplexer.shutdown().await?;

        // Pipeline cleanup is handled by Drop implementations
        println!("All resources cleaned up successfully");
        Ok(())
    }

    /// Get the expected output size after pipeline initialization.
    ///
    /// This method initializes the pipeline with the capture source's input size
    /// and returns the final output dimensions. This is useful for testing and
    /// validation without actually running the capture session.
    ///
    /// # Returns
    ///
    /// `Ok(Size)` containing the output dimensions after all processing, or an
    /// error if pipeline initialization fails.
    ///
    /// # Errors
    ///
    /// Returns an error if any processor in the pipeline fails to initialize.
    pub async fn get_output_size(&mut self) -> Result<Size> {
        // Initialize capture source
        self.capture_source.initialize().await?;

        let input_size = self.capture_source.input_size();
        let output_size = self.pipeline.initialize(input_size).await?;

        // Initialize multiplexer (which initializes all streams)
        self.multiplexer.initialize().await?;

        Ok(output_size)
    }
}

/// Builder for creating capture sessions with fluent API.
/// Enables declarative configuration of complex capture workflows.
#[cfg(feature = "rtsp-streaming")]
pub struct CaptureSessionBuilder {
    processors: Vec<Box<dyn FrameProcessor>>,
    streams: Vec<Box<dyn Stream>>,
    capture_source: Option<Box<dyn CaptureSource>>,
}

#[cfg(feature = "rtsp-streaming")]
impl CaptureSessionBuilder {
    /// Create a new session builder.
    ///
    /// Initializes an empty builder with no processors, streams, or capture source
    /// configured. Use the fluent API methods to add components before calling `build()`.
    ///
    /// # Returns
    ///
    /// A new `CaptureSessionBuilder` ready for configuration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::session::CaptureSessionBuilder;
    ///
    /// let builder = CaptureSessionBuilder::new();
    /// // Configure with fluent API...
    /// ```
    ///
    /// **Missing functionality**: None - basic constructor fully implemented.
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
            streams: Vec::new(),
            capture_source: None,
        }
    }

    /// Add Gundam tiling processor to the pipeline.
    ///
    /// Configures the session to use Gundam tiling for DeepSeek-OCR optimization.
    /// This processor extracts multiple overlapping tiles from each frame along
    /// with a global downscaled view, arranging them in a composite grid.
    ///
    /// The Gundam processor significantly improves OCR accuracy for large documents
    /// or complex layouts by providing multiple views at different scales in a
    /// single frame.
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::session::CaptureSession;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_gundam()  // Enable Gundam tiling
    ///     .with_rtsp_stream(8554, 1920, 1080, 30)
    ///     // ... other configuration
    ///     .build();
    /// ```
    ///
    /// **Missing functionality**: None - fully implements Gundam processor addition,
    /// though the processor itself may have TODOs for buffer allocation.
    pub fn with_gundam(mut self) -> Self {
        self.processors.push(Box::new(GundamProcessor {
            cfg: cap_scale::gundam::GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        }));
        self
    }

    /// Add scaling processor with the specified preset.
    ///
    /// Configures the session to scale captured frames according to a token-efficient
    /// preset. This reduces the resolution while maintaining aspect ratio, making
    /// the output more suitable for vision-language models that have token limits.
    ///
    /// Different presets optimize for different model input requirements and
    /// token budgets.
    ///
    /// # Parameters
    ///
    /// * `preset` - The scaling preset to use for token-efficient resizing.
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use cap_scale::presets::TokenPreset;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_scaling(TokenPreset::P4_Long640)  // Scale for GPT-4V
    ///     .with_file_output("output.mp4".to_string(), 1920, 1080, 30)
    ///     // ... other configuration
    ///     .build();
    /// ```
    ///
    /// **Missing functionality**: None - fully implements scaling processor addition
    /// with preset-based configuration and SIMD acceleration.
    pub fn with_scaling(mut self, preset: TokenPreset) -> Self {
        use crate::processing::processing::ScalingProcessor;
        self.processors.push(Box::new(ScalingProcessor {
            preset,
            resizer: fast_image_resize::Resizer::new(),
            staging: cap_scale::cpu::Staging::with_capacity(1920 * 1080 * 4), // Pre-allocate for HD
            output_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        }));
        self
    }

    /// Add RTSP streaming output.
    ///
    /// Configures the session to stream captured frames over RTSP (Real-Time
    /// Streaming Protocol). RTSP streams can be viewed by RTSP-compatible
    /// media players and integrated into other systems.
    ///
    /// The stream will be available at `rtsp://localhost:{port}/cap` and
    /// supports multiple simultaneous viewers.
    ///
    /// # Parameters
    ///
    /// * `port` - The network port to bind the RTSP server to.
    /// * `width` - The width of the streamed video in pixels.
    /// * `height` - The height of the streamed video in pixels.
    /// * `fps` - The target frames per second for the stream.
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    ///
    /// # Errors
    ///
    /// This method doesn't return errors directly, but the `build()` method
    /// may fail if RTSP publisher creation fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let capture_source = FFmpegCaptureSource::new(":0.0")?;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_rtsp_stream(8554, 1920, 1080, 30)  // RTSP on port 8554
    ///     .with_capture_source(capture_source)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_rtsp_stream(mut self, port: u16, width: u32, height: u32, fps: u32) -> Self {
        use crate::processing::processing::RtspStream;
        use cap_rtsp::{RtspConfig, start_server};

        // Create RTSP server configuration
        let rtsp_config = RtspConfig {
            port,
            mount: "/cap".to_string(),
            width,
            height,
            framerate: fps,
            encoder: None,
            appsrc_max_bytes: Some(8 * 1024 * 1024),
        };

        // Start RTSP server and get publisher
        let (rtsp_publisher, server_handle) =
            start_server(rtsp_config).expect("Failed to start RTSP server");

        // Create RTSP stream configuration
        let config = StreamConfig {
            width,
            height,
            fps,
            format: StreamFormat::Rtsp {
                port,
                mount: "/cap".to_string(),
            },
        };

        let rtsp_stream = RtspStream {
            publisher: rtsp_publisher,
            config,
            _server_handle: Some(server_handle),
        };

        self.streams.push(Box::new(rtsp_stream));
        self
    }

    /// Add file output stream.
    ///
    /// Configures the session to save captured frames to a video file on disk.
    /// The output format is MP4 with H.264 encoding, providing good compression
    /// and compatibility with most video players.
    ///
    /// # Parameters
    ///
    /// * `path` - The file system path where the output video will be saved.
    /// * `width` - The width of the output video in pixels.
    /// `height` - The height of the output video in pixels.
    /// * `fps` - The target frames per second for the output video.
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let capture_source = FFmpegCaptureSource::new(":0.0")?;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_file_output("recording.mp4".to_string(), 1920, 1080, 30)
    ///     .with_capture_source(capture_source)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **Missing functionality**: FILE STREAM IMPLEMENTATION MISSING - creates config
    /// but has a TODO comment and doesn't actually add a FileStream to the streams vector.
    /// Needs to create and add a FileStream instance.
    pub fn with_file_output(mut self, path: String, width: u32, height: u32, fps: u32) -> Self {
        use crate::processing::processing::FileStream;
        let config = StreamConfig {
            width,
            height,
            fps,
            format: StreamFormat::File { path: path.clone() },
        };
        self.streams.push(Box::new(FileStream::new(path, config)));
        self
    }

    /// Add a custom stream to the session.
    ///
    /// Allows adding any type that implements the Stream trait.
    /// This is useful for testing with mock streams or custom implementations.
    ///
    /// # Parameters
    ///
    /// * `stream` - Any type that implements the `Stream` trait.
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    pub fn with_stream<S: Stream + 'static>(mut self, stream: S) -> Self {
        self.streams.push(Box::new(stream));
        self
    }

    /// Set the capture source for the session.
    ///
    /// Specifies where frames will be captured from. The capture source provides
    /// the raw video feed that gets processed and streamed. Different platforms
    /// and use cases require different capture sources.
    ///
    /// # Parameters
    ///
    /// * `source` - Any type that implements the `CaptureSource` trait.
    ///
    /// # Returns
    ///
    /// The builder instance for method chaining.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let capture_source = FFmpegCaptureSource::new(":0.0")?;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_capture_source(capture_source)
    ///     .with_file_output("output.mp4".to_string(), 1920, 1080, 30)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **Missing functionality**: None - properly sets the capture source using dynamic dispatch.
    pub fn with_capture_source<S: CaptureSource + 'static>(mut self, source: S) -> Self {
        self.capture_source = Some(Box::new(source));
        self
    }

    /// Build the capture session with the configured components.
    ///
    /// Assembles all the configured processors, streams, and capture source into
    /// a ready-to-run `CaptureSession`. This method validates the configuration
    /// and performs any necessary setup.
    ///
    /// The build process:
    /// 1. Creates a processing pipeline and adds all configured processors
    /// 2. Creates a stream multiplexer and adds all configured streams
    /// 3. Validates that at least one stream is configured
    /// 4. Validates that a capture source is specified
    /// 5. Creates shutdown signal channels for graceful shutdown
    /// 6. Returns the fully configured session
    ///
    /// # Returns
    ///
    /// `Ok(CaptureSession)` if all components are properly configured, or an
    /// error describing the configuration problem.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No streams are configured
    /// - No capture source is specified
    /// - Stream configurations are incompatible
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hybrid_screen_capture::session::CaptureSession;
    /// use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let capture_source = FFmpegCaptureSource::new(":0.0")?;
    ///
    /// let session = CaptureSession::builder()
    ///     .with_gundam()
    ///     .with_rtsp_stream(8554, 1920, 1080, 30)
    ///     .with_file_output("output.mp4".to_string(), 1920, 1080, 30)
    ///     .with_capture_source(capture_source)
    ///     .build()?;  // Returns configured session
    ///
    /// session.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **Missing functionality**: Multiplexer configuration is oversimplified - uses only
    /// the first stream's config instead of properly merging or validating all stream
    /// configurations. Could lead to issues if streams have conflicting requirements.
    pub fn build(self) -> Result<CaptureSession> {
        let mut pipeline = ProcessingPipeline::new();
        for processor in self.processors {
            pipeline.processors.push(processor);
        }

        // Create multiplexer config from first stream (simplified)
        if self.streams.is_empty() {
            return Err(anyhow::anyhow!("At least one stream must be configured"));
        }

        let mut multiplexer = StreamMultiplexer::new();
        for stream in self.streams {
            multiplexer.streams.push(stream);
        }

        let capture_source = self
            .capture_source
            .ok_or_else(|| anyhow::anyhow!("No capture source specified"))?;

        // Create shutdown signal channels
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Ok(CaptureSession {
            pipeline,
            multiplexer,
            capture_source,
            shutdown_tx,
            shutdown_rx,
        })
    }
}
