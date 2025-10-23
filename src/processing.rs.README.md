# src/processing/processing.rs: Frame Processing Pipeline

Zero-copy, non-branching frame processing pipeline for real-time video streaming. Provides composable processors and stream multiplexing for high-performance capture-processing-streaming workflows.

## Architecture Overview

The processing pipeline follows a linear, zero-copy design:
1. **FrameProcessor Trait**: Extensible processing interface
2. **ProcessingPipeline**: Composable processor chain
3. **Stream Trait**: Abstract output destination interface
4. **StreamMultiplexer**: Concurrent broadcasting to multiple streams

## Zero-Copy Design

All processing maintains zero-copy principles:
- Frames use Arc<Vec<u8>> for atomic reference counting
- Processors transform data without copying when possible
- Streams broadcast using shared references
- No allocations in the processing hot path

## Non-Branching Patterns

Runtime execution avoids CPU pipeline stalls:
- Configuration decisions made at build time
- Linear processor chains with predictable execution
- No conditional logic in processing loops
- Declarative stream configuration

## Core Components

### FrameProcessor Trait
```rust
#[async_trait]
pub trait FrameProcessor: Send + Sync {
    async fn process_frame(&mut self, frame: BgraFrame) -> Result<Option<BgraFrame>>;
}
```

Extensible interface for frame transformations. Implement this trait to create custom processors for scaling, filtering, effects, or AI processing.

### ProcessingPipeline
```rust
pub struct ProcessingPipeline {
    pub processors: Vec<Box<dyn FrameProcessor>>,
    buffer_size: usize,
}
```

Composable chain of processors. Frames flow through each processor sequentially, with each processor potentially transforming the frame data.

### Stream Trait
```rust
#[async_trait]
pub trait Stream: Send + Sync {
    async fn send_frame(&mut self, frame: BgraFrame) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
    fn config(&self) -> &StreamConfig;
    async fn initialize(&mut self) -> Result<()>;
}
```

Abstract interface for output destinations. Supports RTSP streaming, file output, and custom stream types.

### StreamMultiplexer
```rust
pub struct StreamMultiplexer {
    pub streams: Vec<Box<dyn Stream>>,
    config: StreamConfig,
}
```

Broadcasts frames to multiple streams concurrently. Maintains zero-copy semantics by cloning Arc references rather than data.

## Built-in Processors

### GundamProcessor
Implements DeepSeek OCR "Gundam" tiling for complex document layouts:
- Divides input into 2-9 overlapping 640×640px tiles
- Creates 1024×1024px global overview
- Arranges tiles + global view into composite frame
- Optimized for VLM token efficiency

## Built-in Streams

### RtspStream
RTSP network streaming implementation:
- Wraps cap-rtsp::RtspPublisher
- Handles frame broadcasting with back-pressure
- Supports configurable RTSP endpoints

## Usage Patterns

### Basic Processing Pipeline
```rust,no_run
use cap::processing::{ProcessingPipeline, GundamProcessor};

// Create pipeline
let mut pipeline = ProcessingPipeline::new(1024 * 1024); // 1MB buffer

// Add Gundam tiling processor
let gundam = GundamProcessor {
    cfg: cap_scale::gundam::GundamCfg::default(),
    tile_buffers: vec![vec![0u8; 640*640*4]; 4],
    global_buffer: vec![0u8; 1024*1024*4],
    output_size: Size { w: 1280, h: 1280 },
};
pipeline.processors.push(Box::new(gundam));

// Initialize pipeline
let output_size = pipeline.initialize(Size { w: 1920, h: 1080 }).await?;

// Process frames
let processed_frame = pipeline.process_frame(input_frame).await?;
```

### Multi-Stream Broadcasting
```rust,no_run
use cap::processing::{StreamMultiplexer, RtspStream, StreamConfig, StreamFormat};

// Create multiplexer
let config = StreamConfig {
    width: 1920,
    height: 1080,
    fps: 30,
    format: StreamFormat::Rtsp { port: 8554, mount: "/stream".into() },
};
let mut multiplexer = StreamMultiplexer::new(config);

// Add RTSP stream
let rtsp_publisher = cap_rtsp::start_server(rtsp_config)?;
let rtsp_stream = RtspStream {
    publisher: rtsp_publisher,
    config: config.clone(),
};
multiplexer.streams.push(Box::new(rtsp_stream));

// Initialize and broadcast
multiplexer.initialize().await?;
multiplexer.send_frame(frame).await?;
```

## Pipeline Flow

```
Capture → ProcessingPipeline → StreamMultiplexer → Multiple Streams
    ↓           ↓                        ↓
Raw Frame → [Processor1 → Processor2] → [Stream1, Stream2, Stream3]
```

## Performance Characteristics

- **Zero-copy**: Arc-based frame sharing between threads
- **Concurrent broadcasting**: Parallel stream output
- **Linear processing**: Predictable execution without branching
- **Async processing**: Non-blocking frame transformations

## Integration with Session Management

The processing pipeline integrates with `src/config/session.rs` for high-level orchestration:

- **CaptureSession**: Uses ProcessingPipeline for frame processing
- **CaptureSessionBuilder**: Declarative pipeline configuration
- **CaptureSource**: Abstracts different capture backends

## Future Extensions

- Custom processor implementations (effects, compression, AI)
- Additional stream types (WebRTC, HLS, file recording)
- Pipeline branching and conditional processing
- Metrics collection and performance monitoring
- GPU-accelerated processing support