# cap-rtsp: RTSP Streaming Server for BGRA Frames

This crate provides low-latency RTSP streaming capabilities for BGRA video frames, optimized for real-time screen capture and AI vision model pipelines.

## Architecture Overview

The RTSP server uses GStreamer for efficient video encoding and streaming:
- **Single shared pipeline**: One H.264 encoder shared across all RTSP clients
- **Low-latency encoding**: x264enc with zero-latency tuning by default
- **Back-pressure handling**: Bounded channel prevents memory ballooning
- **GLib integration**: Proper threading with GLib main loop for stability

## Key Design Decisions

### Threading Model
- **Main thread**: GLib main loop runs RTSP server and pipeline
- **Worker thread**: Polls for frames and pushes to GStreamer appsrc
- **Caller threads**: Non-blocking frame submission via crossbeam channel

### Latency Optimization
- Small bounded channel (capacity 3) minimizes queuing delay
- `appsrc block=true` provides back-pressure without dropping frames
- Configurable encoder settings for different latency vs quality trade-offs
- Optional caller-provided PTS timestamps for precise timing

### Memory Management
- Frames use `Arc<Vec<u8>>` for zero-copy sharing between threads
- Configurable `appsrc_max_bytes` prevents unbounded memory usage
- Stride-aware frame handling for screen capture compatibility

## Performance Characteristics

- **Encoding latency**: ~10-50ms depending on resolution and encoder settings
- **Network latency**: RTSP/RTP adds ~5-20ms depending on network conditions
- **CPU usage**: H.264 encoding scales with resolution (HD ~10-20% single core)
- **Memory usage**: Bounded by channel capacity + encoder buffers

## Supported Encoders

The crate supports any GStreamer H.264 encoder via configurable launch strings:
- **x264enc** (default): Software encoding, widely compatible
- **vtenc_h264**: macOS VideoToolbox hardware acceleration
- **nvh264enc**: NVIDIA hardware encoding
- **d3d11h264enc**: Windows DirectX hardware encoding

## Usage Patterns

### Basic Screen Capture Streaming
```rust,no_run
use cap_rtsp::{start_server, RtspConfig, frame_from_bgra};

let cfg = RtspConfig {
    width: 1920,
    height: 1080,
    framerate: 30,
    ..Default::default()
};

let (publisher, handle) = start_server(cfg)?;

// In your capture loop:
loop {
    let bgra_data = capture_screen_frame();
    let frame = frame_from_bgra(bgra_data, 1920, 1080, 30, frame_count);
    publisher.send(frame)?;
}
# Ok::<(), anyhow::Error>(())
```

### Hardware-Accelerated Streaming
```rust,no_run
let cfg = RtspConfig {
    encoder: Some("nvh264enc preset=low-latency-hq zerolatency=true bitrate=8000".into()),
    ..Default::default()
};
```

## Integration with cap-scale

This crate works seamlessly with `cap-scale` for AI-optimized streaming:
```rust,no_run
use cap_rtsp::{start_server, RtspConfig};
use cap_scale::{scale_bgra_cpu, presets::build_plan};

// Scale frame for AI model input
let scaled_frame = scale_bgra_cpu(/* ... */)?;

// Stream the scaled frame
let rtsp_frame = frame_from_bgra(scaled_frame, scaled_w, scaled_h, 10, idx);
publisher.send(rtsp_frame)?;
# Ok::<(), anyhow::Error>(())
```

## API Reference

### Core Types

- **`BgraFrame`**: Raw BGRA frame with metadata (width, height, stride, PTS)
- **`RtspPublisher`**: Thread-safe handle for sending frames to RTSP stream
- **`RtspConfig`**: Configuration for server setup and encoder settings
- **`RtspStreamer`**: High-level interface with optional frame processing

### Key Functions

- **`start_server(cfg: RtspConfig)`**: Initialize RTSP server and return publisher handle
- **`frame_from_bgra(bytes, width, height, fps, idx)`**: Create BGRA frame with automatic PTS
- **`arrange_gundam_composite(tiles, global, tile_side, global_side)`**: Composite Gundam tiles for streaming

### Traits

- **`FrameProcessor`**: Async trait for custom frame processing before streaming

## Pipeline Architecture

The GStreamer pipeline follows this structure:
```
appsrc → videoconvert → videoscale → encoder → rtph264pay → clients
```

- **appsrc**: Receives BGRA frames from application
- **videoconvert**: Converts BGRA to I420 colorspace
- **videoscale**: Handles any resolution adjustments
- **encoder**: H.264 encoding (configurable)
- **rtph264pay**: RTP packetization for network streaming

## Threading Behavior

- **Server thread**: Runs GLib main loop, handles RTSP protocol
- **Push worker**: Async task polls for frames and feeds appsrc
- **Caller threads**: Can call `send()` concurrently without blocking

## Resource Management

- Bounded crossbeam channel prevents memory ballooning
- GLib main context ensures proper cleanup on shutdown
- Appsrc back-pressure prevents frame dropping under load

## Future Optimizations

- Multiple simultaneous streams with different encodings
- Adaptive bitrate based on client capabilities
- Frame dropping strategies for sustained overload conditions
- H.265 encoding for better compression efficiency
- Metrics collection for latency and throughput monitoring
- Client connection limits and authentication</content>
<parameter name="filePath">/home/xor/cap/cap-rtsp/README.md