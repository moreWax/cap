# src/config/session.rs: Capture Session Management

High-level session orchestration for screen capture with processing pipelines and streaming. Provides a declarative, builder-pattern API for configuring complex capture workflows.

## Architecture Overview

The session management follows a composable, declarative approach:
1. **CaptureSource Trait**: Abstract interface for frame sources
2. **CaptureSession**: Orchestrates capture, processing, and streaming
3. **CaptureSessionBuilder**: Fluent API for session configuration
4. **Platform-specific Sources**: Concrete implementations for each platform

## Zero-Copy Design

Session management maintains zero-copy principles:
- Frame sources provide Arc-referenced buffers
- Processing pipelines transform data without copying
- Streams broadcast using atomic reference counting
- No allocations in the capture-processing-streaming loop

## Non-Branching Patterns

Configuration decisions are made at build time:
- Pipeline structure determined during builder construction
- Stream configuration immutable after initialization
- Runtime execution follows linear, predictable paths
- No conditional logic in hot processing loops

## Core Components

### CaptureSource Trait
```rust
#[async_trait]
pub trait CaptureSource: Send {
    async fn capture_frame(&mut self) -> Result<BgraFrame>;
    fn input_size(&self) -> Size;
    async fn initialize(&mut self) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}
```

Abstract interface for frame capture sources. Enables pluggable backends for different platforms (Linux/X11, Windows, macOS) and capture modes (screen, window, region).

### CaptureSession
```rust
pub struct CaptureSession {
    pipeline: ProcessingPipeline,
    multiplexer: StreamMultiplexer,
    capture_source: Box<dyn CaptureSource>,
}
```

Main orchestration component that coordinates capture, processing, and streaming. Provides the `run()` method for the main execution loop.

### CaptureSessionBuilder
```rust
pub struct CaptureSessionBuilder {
    processors: Vec<Box<dyn FrameProcessor>>,
    streams: Vec<Box<dyn Stream>>,
    capture_source: Option<Box<dyn CaptureSource>>,
}
```

Fluent builder API for declarative session configuration. Allows chaining method calls to configure processors, streams, and capture sources.

## Builder Pattern Usage

### Basic Screen Capture with RTSP Streaming
```rust,no_run
use cap::session::{CaptureSession, CaptureSource};

// Create session with builder pattern
let session = CaptureSession::builder()
    .with_gundam()  // Add Gundam tiling processor
    .with_rtsp_stream(8554, 1920, 1080, 30)  // RTSP output
    .with_capture_source(X11CaptureSource::new()?)  // Linux X11 capture
    .build()?;

// Run the capture session
session.run().await?;
```

### Advanced Configuration
```rust,no_run
let session = CaptureSession::builder()
    .with_scaling(TokenPreset::P4_Long640)  // Token-efficient scaling
    .with_gundam()  // DeepSeek OCR tiling
    .with_rtsp_stream(8554, 1280, 1280, 10)  // RTSP at reduced FPS
    .with_file_output("output.mp4".into(), 1280, 1280, 10)  // File recording
    .with_capture_source(WindowsCaptureSource::new(window_handle)?)
    .build()?;
```

## Session Lifecycle

### Initialization Phase
1. **Capture source initialization**: Set up screen capture backend
2. **Pipeline initialization**: Configure processing chain and determine output sizes
3. **Stream initialization**: Set up output destinations (RTSP servers, file writers)

### Runtime Phase
1. **Frame capture**: Get next frame from capture source
2. **Frame processing**: Apply processing pipeline transformations
3. **Frame streaming**: Broadcast to all configured output streams

### Shutdown Phase
1. **Stream shutdown**: Clean up output destinations
2. **Pipeline shutdown**: Release processing resources
3. **Capture shutdown**: Clean up capture backend

## Platform-Specific Capture Sources

### Linux (X11)
- Uses scrap crate for screen capture
- Handles X11 display server integration
- Supports full screen and window capture

### Windows
- Uses Windows Graphics Capture API
- Hardware-accelerated capture when available
- Supports HDR and high-refresh-rate displays

### macOS
- Uses ScreenCaptureKit (macOS 12.3+)
- Hardware-accelerated with AVFoundation
- Supports system audio capture

## Integration with Processing Pipeline

The session management integrates tightly with `src/processing/processing.rs`:

- **ProcessingPipeline**: Configured via builder methods (`with_gundam()`, `with_scaling()`)
- **StreamMultiplexer**: Automatically created from stream configurations
- **FrameProcessor/Stream traits**: Extended by session components

## Performance Characteristics

- **Zero-copy execution**: Arc-based frame sharing throughout pipeline
- **Predictable latency**: Linear processing without conditional branches
- **Concurrent streaming**: Parallel output to multiple destinations
- **Memory bounded**: Pre-allocated buffers prevent unbounded growth

## Error Handling

Comprehensive error propagation:
- **Initialization errors**: Configuration validation and resource setup
- **Runtime errors**: Frame capture failures, processing errors, streaming failures
- **Recovery strategies**: Graceful degradation and error recovery patterns

## Future Extensions

- Additional capture sources (Wayland, Android, iOS)
- Custom processor implementations
- Advanced stream types (WebRTC, HLS, RTMP)
- Session persistence and configuration serialization
- Metrics collection and performance monitoring
- Hot reconfiguration during runtime