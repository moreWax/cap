# Modularized Gundam RTSP Streaming Architecture

## Overview

This document describes the newly modularized architecture for the Gundam RTSP streaming feature in the `cap` project. The refactoring follows first principles of extensibility, modularity, and performance, implementing a zero-copy, non-branching design optimized for real-time video processing.

## Architecture Principles

### Zero-Copy Design
- **Atomic Reference Counting**: All frame data uses `Arc<[u8]>` to eliminate copying
- **Buffer Pooling**: Reusable memory regions prevent allocation overhead
- **Direct Buffer Manipulation**: Processing operates on existing buffers without duplication

### Non-Branching Code
- **Declarative Configuration**: All conditional logic resolved at initialization
- **Functional Composition**: Pipeline structure determined at build time
- **Predictable Execution**: Linear processing paths in hot loops
- **Immutable State**: No runtime flags controlling behavior

### Modular Architecture
- **Trait-Based Abstractions**: Extensible interfaces for processing and streaming
- **Composable Pipelines**: Chainable processing operations
- **Concurrent Streaming**: Lock-free broadcasting to multiple outputs

## Module Structure

### `src/processing/processing.rs` - Frame Processing Architecture

#### Core Traits
```rust
#[async_trait]
pub trait FrameProcessor: Send + Sync {
    async fn process_frame(&mut self, frame: &BgraFrame, resources: &mut ProcessingResources) -> Result<BgraFrame>;
    fn output_size(&self) -> Size;
    async fn initialize(&mut self, input_size: Size) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}

#[async_trait]
pub trait Stream: Send + Sync {
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()>;
    fn config(&self) -> &StreamConfig;
    async fn initialize(&mut self) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}
```

#### Key Components
- **`ProcessingResources`**: Shared resizer, staging buffers, and buffer pool
- **`BufferPool`**: Efficient memory reuse with configurable limits
- **`GundamProcessor`**: Implements DeepSeek-OCR tiling with zero-copy operations
- **`ProcessingPipeline`**: Composable chain of frame processors
- **`StreamMultiplexer`**: Concurrent broadcasting to multiple stream outputs
- **`RtspStream`**: RTSP streaming implementation

#### Zero-Copy Operations
```rust
// Frame data flows through Arc references
let frame_clone = BgraFrame {
    data: Arc::clone(&frame.data),  // Zero-copy cloning
    width: frame.width,
    height: frame.height,
    stride: frame.stride,
    pts_ns: frame.pts_ns,
};
```

### `src/config/session.rs` - High-Level Orchestration

#### Core Traits
```rust
#[async_trait]
pub trait CaptureSource: Send {
    async fn capture_frame(&mut self) -> Result<BgraFrame>;
    fn input_size(&self) -> Size;
    async fn initialize(&mut self) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}
```

#### Key Components
- **`CaptureSession`**: Main orchestration with capture-processing-streaming loop
- **`CaptureSessionBuilder`**: Fluent API for declarative session configuration
- **Platform-specific Sources**: Concrete capture implementations

#### Builder Pattern
```rust
let session = CaptureSession::builder()
    .with_gundam()                    // Add Gundam processing
    .with_rtsp_stream(8554, 1920, 1080, 30)  // Add RTSP output
    .with_capture_source(source)      // Set capture source
    .build()?;
```

## Implementation Phases

### Phase 1: Extract FrameProcessor Trait ✅
- **Location**: `src/processing.rs`
- **Purpose**: Define extensible frame processing interface
- **Zero-Copy**: Uses `&BgraFrame` references and `Arc` cloning
- **Non-Branching**: Trait dispatch instead of runtime conditionals

### Phase 2: Implement ProcessingPipeline and StreamMultiplexer ✅
- **Location**: `src/processing.rs`
- **Purpose**: Enable composable processing and concurrent streaming
- **Zero-Copy**: Pipeline processes frames through Arc references
- **Non-Branching**: Fixed pipeline structure, concurrent futures

### Phase 3: Add BufferPool and Resource Management ✅
- **Location**: `src/processing.rs::ProcessingResources`
- **Purpose**: Eliminate allocation overhead in processing loops
- **Zero-Copy**: Reuses buffers across frames
- **Performance**: Configurable pool sizes prevent memory pressure

### Phase 4: Create Builder API for Intuitive Configuration ✅
- **Location**: `src/session.rs::CaptureSessionBuilder`
- **Purpose**: Declarative session setup with fluent interface
- **Non-Branching**: Configuration decisions made at build time
- **Extensibility**: Easy addition of new processors and streams

### Phase 5: Add Comprehensive Error Handling and Logging ✅
- **Error Propagation**: Uses `anyhow::Result` throughout
- **Resource Cleanup**: Proper shutdown sequences
- **Logging**: Informative progress and error messages

## Performance Characteristics

### CPU Branching Avoidance
- **Sequential Execution**: No `if` statements in processing hot paths
- **Predictable Branches**: All conditionals resolved at initialization
- **Branch Prediction Friendly**: Consistent execution patterns

### Memory Efficiency
- **Zero Allocations**: Buffer pools eliminate per-frame allocations
- **Reference Counting**: `Arc` provides thread-safe sharing without copying
- **Memory Mapping**: Direct access to frame data without intermediate buffers

### Concurrent Streaming
- **Lock-Free Broadcasting**: Atomic reference counting for frame distribution
- **Async Composition**: `futures_util::join_all` for concurrent sends
- **Resource Sharing**: Single frame data shared across multiple streams

## Usage Examples

### Basic Gundam RTSP Streaming
```rust
use cap::processing::{ProcessingPipeline, GundamProcessor, StreamMultiplexer, RtspStream};
use cap::session::CaptureSession;

// Create processing pipeline
let pipeline = ProcessingPipeline::new(10)
    .add_processor(GundamProcessor::new());

// Create RTSP stream
let rtsp_stream = RtspStream::new(8554, 1920, 1080, 30)?;
let multiplexer = StreamMultiplexer::new()
    .add_stream(rtsp_stream);

// Build and run session
let session = CaptureSession::builder()
    .with_pipeline(pipeline)
    .with_multiplexer(multiplexer)
    .with_capture_source(capture_source)
    .build()?;

session.run().await?;
```

### Multiple Concurrent Streams
```rust
let multiplexer = StreamMultiplexer::new()
    .add_stream(RtspStream::new(8554, 1920, 1080, 30)?)
    .add_stream(FileStream::new("output.mp4", 1920, 1080, 30)?)
    .add_stream(MemoryStream::new()?);
```

## Future Extensibility

### Adding New Processors
```rust
#[async_trait]
impl FrameProcessor for CustomProcessor {
    async fn process_frame(&mut self, frame: &BgraFrame, resources: &mut ProcessingResources) -> Result<BgraFrame> {
        // Custom zero-copy processing logic
        Ok(processed_frame)
    }
    // ... other trait methods
}
```

### Adding New Stream Types
```rust
#[async_trait]
impl Stream for CustomStream {
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()> {
        // Custom streaming logic
        Ok(())
    }
    // ... other trait methods
}
```

## Migration from Legacy Code

### Before (Embedded in main.rs)
```rust
// Processing logic embedded in capture loop
if args.gundam {
    // Complex inline Gundam processing
    gundam_pack_cpu(/* ... */);
    arrange_gundam_composite(/* ... */);
} else if args.scale_preset {
    // Inline scaling logic
    scale_bgra_cpu(/* ... */);
}
```

### After (Modular Architecture)
```rust
// Declarative pipeline configuration
let pipeline = ProcessingPipeline::new(10)
    .add_processor(GundamProcessor::new())
    .add_processor(ScalingProcessor::new(preset));

// Clean capture loop
let processed_frame = pipeline.process_frame(raw_frame).await?;
multiplexer.send_frame(processed_frame).await?;
```

## Testing and Validation

### Unit Tests
- **Processor Tests**: Validate individual frame transformations
- **Pipeline Tests**: Ensure correct composition and data flow
- **Stream Tests**: Verify output delivery and error handling

### Integration Tests
- **End-to-End Streaming**: Full capture-to-stream pipelines
- **Performance Benchmarks**: Measure latency and throughput
- **Resource Usage**: Monitor memory and CPU utilization

### Performance Validation
- **Branch Analysis**: Use CPU profilers to verify branch elimination
- **Memory Profiling**: Confirm zero-copy behavior
- **Latency Measurements**: Validate real-time performance requirements

## Conclusion

The modularized architecture provides:
- **Extensibility**: Easy addition of new processors and streams
- **Performance**: Zero-copy, non-branching execution
- **Maintainability**: Clean separation of concerns
- **Reliability**: Comprehensive error handling and resource management
- **Future-Proofing**: Support for multiple concurrent streams

This design enables the Gundam RTSP feature to scale from simple single-stream use cases to complex multi-stream, multi-processor workflows while maintaining the high-performance characteristics required for real-time video streaming.