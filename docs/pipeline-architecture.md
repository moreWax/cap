# LLM Adaptor Architecture

## Current Architecture Overview

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Screen        │    │   RTSP Server    │    │   LLM Adaptor   │
│   Capture       │───▶│   (cap-rtsp)     │───▶│   (Future)      │
│   (cap)         │    │   H.264 Stream   │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
        │                        │                        │
        ▼                        ▼                        ▼
   Platform-specific        GStreamer Pipeline       LLM-specific
   capture backends         x264enc + rtph264pay     API integration
   (scrap, ashpd)           Fixed format            (OpenAI, Claude, etc.)
```

## Architecture Split

### 🎥 **Producer Side (Screen Capture → RTSP)**
**This stays the same for all LLMs!** ✅

- **Screen Capture**: Platform-specific backends (scrap, ashpd)
- **Frame Processing**: BGRA frames with optional token-efficient scaling
- **RTSP Server**: GStreamer pipeline with H.264 encoding
- **Output**: Standardized RTSP stream at `rtsp://127.0.0.1:8554/cap`

### 🤖 **Consumer Side (RTSP → LLM)**
**This is where we build adaptors for each LLM!** 🔧

Each LLM needs its own adaptor that:
1. **Connects to RTSP stream**
2. **Extracts frames** at appropriate intervals
3. **Formats data** for the specific LLM API
4. **Handles responses** and coordinates with the LLM

## LLM Adaptor Template

```rust
struct LlmAdaptor {
    rtsp_url: String,
    client: LlmClient,
    frame_interval: Duration,
}

impl LlmAdaptor {
    async fn run(&self) -> Result<()> {
        // 1. Connect to RTSP stream
        let rtsp_client = connect_to_rtsp(&self.rtsp_url)?;
        
        // 2. Extract frames periodically
        loop {
            let frame = rtsp_client.get_next_frame()?;
            
            // 3. Format for specific LLM
            let formatted_data = self.format_for_llm(frame)?;
            
            // 4. Send to LLM API
            let response = self.client.query(formatted_data).await?;
            
            // 5. Process response
            self.handle_response(response)?;
            
            tokio::time::sleep(self.frame_interval).await;
        }
    }
    
    fn format_for_llm(&self, frame: Frame) -> Result<LlmInput> {
        // LLM-specific formatting logic
        // Different for OpenAI, Claude, Gemini, etc.
    }
}
```

## Current Implementation Status

### ✅ **Completed (Producer Side)**
- RTSP server with GStreamer
- Cross-platform screen capture
- Token-efficient scaling presets
- H.264 streaming infrastructure

### 🚧 **In Progress (Consumer Side)**
- Qwen3-VL adaptor (basic implementation in `test_qwen_vl.rs`)

### 📋 **To Do (Consumer Side)**
- OpenAI GPT-4V adaptor
- Anthropic Claude 3 Vision adaptor  
- Google Gemini Vision adaptor
- DeepSeek OCR adaptor
- Frame extraction from RTSP stream
- Real-time streaming coordination

## Key Benefits of This Architecture

1. **🔄 Reusable Producer**: One RTSP pipeline serves all LLMs
2. **🎯 Focused Adaptors**: Each LLM adaptor handles only its specific API
3. **📈 Scalable**: Easy to add new LLM support without touching core capture code
4. **⚡ Performance**: RTSP provides efficient, low-latency frame delivery
5. **🛠️ Maintainable**: Clear separation of concerns between capture and AI integration

## Next Steps

1. **Extract RTSP client library** from current test code
2. **Create LLM adaptor trait/interface** for consistency
3. **Implement adaptors** for major LLMs (OpenAI, Claude, Gemini)
4. **Add frame buffering/coordination** for real-time streaming
5. **Test end-to-end** with actual screen capture

The RTSP "pipe" is indeed the stable, reusable component - we just need adaptors on the LLM side! 🎯

# Pipeline Architecture: Capture → Processing → Streaming

This document provides a comprehensive overview of the modular screen capture pipeline architecture, detailing the data flow from raw frame capture through processing to final output streaming.

## High-Level Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   CAPTURE       │ -> │   PROCESSING    │ -> │   STREAMING     │
│   (Platform)    │    │   (Pipeline)    │    │   (Output)      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
   Raw Frames             Processed Frames          Output Streams
   (BGRA, Native)         (BGRA, Transformed)      (RTSP, File, etc.)
```

## Detailed Data Flow

### Phase 1: Frame Capture

#### Input Sources
- **Platform-specific backends**:
  - Windows/macOS: `scrap` crate (direct capture)
  - Linux X11: `scrap` crate (X11 integration)
  - Linux Wayland: `xdg-desktop-portal` + `pipewire` + `GStreamer`
  - Fallback: Synthetic frames for testing

#### Frame Format
- **Pixel format**: BGRA8 (4 bytes per pixel)
- **Memory layout**: Row-major, potentially strided
- **Reference counting**: `Arc<Vec<u8>>` for zero-copy sharing
- **Metadata**: Width, height, stride, optional PTS timestamp

#### Capture Characteristics
- **Resolution**: Native display resolution (1920x1080, 2560x1440, etc.)
- **Frame rate**: Configurable (10-60 FPS typical)
- **Latency**: Platform-dependent (10-50ms capture latency)
- **Threading**: Blocking capture calls with timeout handling

### Phase 2: Frame Processing

#### Processing Pipeline Architecture

```
Raw Frame → [Processor Chain] → Processed Frame
               │
               ├── GundamProcessor (OCR tiling)
               ├── ScalingProcessor (VLM optimization)
               └── CustomProcessor (extensible)
```

#### Gundam Processing (DeepSeek OCR Mode)
```
Input Frame (1920x1080)
    │
    ├── Grid Analysis: choose_grid() → (cols=2, rows=2)
    │
    ├── Tile Extraction: 4× 640×640 tiles + 1× 1024×1024 global
    │       │
    │       ├── Overlapping regions with configurable overlap
    │       └── Padding to square tiles (white background)
    │
    └── Composite Arrangement: arrange_gundam_composite()
            │
            └── Output: Single composite frame (1280×1280)
```

#### Scaling Processing (VLM Mode)
```
Input Frame (1920x1080)
    │
    ├── Plan Computation: build_plan() with TokenPreset
    │       │
    │       ├── Target: MaxLongSide(640) for P4 preset
    │       └── Aspect: Preserve (maintain proportions)
    │
    ├── SIMD Scaling: scale_bgra_cpu() with CatmullRom
    │       │
    │       ├── fast_image_resize with AVX2/AVX-512
    │       └── Zero-copy when possible, staging for strided input
    │
    └── Output: Scaled frame (640×480 for 4:3 aspect)
```

#### Processing Characteristics
- **Zero-copy design**: Arc references minimize allocations
- **SIMD acceleration**: CPU vector instructions for scaling
- **Memory bounded**: Pre-allocated buffers prevent ballooning
- **Configurable quality**: Lanczos3/CatmullRom filter selection

### Phase 3: Frame Streaming

#### Output Multiplexing

```
Processed Frame → StreamMultiplexer → Multiple Outputs
                        │
                        ├── RTSP Stream (real-time)
                        ├── File Stream (recording)
                        └── Custom Stream (extensible)
```

#### RTSP Streaming Architecture

```
GStreamer Pipeline: appsrc → videoconvert → encoder → rtph264pay → RTP/RTSP
                              │              │              │
                              ▼              ▼              ▼
                        BGRA→I420    H.264 encoding   RTP packetization
                        (colorspace)   (configurable)   (network streaming)
```

#### RTSP Server Components
- **Threading model**:
  - Main thread: GLib main loop + RTSP protocol
  - Worker thread: Frame polling + GStreamer feeding
  - Publisher threads: Non-blocking frame submission

- **Back-pressure handling**:
  - Bounded channel (capacity 3) prevents memory issues
  - `appsrc block=true` provides natural back-pressure
  - Non-blocking send with brief retry for spikes

- **Encoding options**:
  - x264enc (software, default): Zerolatency tuning
  - Hardware encoders: nvh264enc, vtenc_h264, d3d11h264enc
  - Configurable bitrate, preset, and quality settings

#### File Recording
- **Format**: MP4 container with H.264 video
- **Quality control**: CRF-based quality presets
- **Platform integration**: ffmpeg subprocess or GStreamer
- **Metadata**: Duration, FPS, resolution tracking

## Module Interactions

### Core Modules

#### cap-rtsp (Streaming)
```
Responsible: RTSP server, GStreamer pipeline, frame publishing
Inputs: BgraFrame from processing pipeline
Outputs: RTP streams to RTSP clients
Dependencies: gstreamer, gstreamer-rtsp-server
```

#### cap-scale (Processing)
```
Responsible: Image scaling, Gundam tiling, VLM optimization
Inputs: Raw BGRA frames from capture
Outputs: Processed BGRA frames for streaming
Dependencies: fast_image_resize, custom presets
```

#### src/processing/processing.rs (Pipeline)
```
Responsible: Frame processor orchestration, stream multiplexing
Inputs: Frames from capture sources
Outputs: Frames to multiple stream destinations
Dependencies: cap-rtsp, cap-scale, async-trait
```

#### src/config/session.rs (Orchestration)
```
Responsible: High-level session management, builder patterns
Inputs: Configuration from CLI/main
Outputs: Running capture sessions
Dependencies: processing, async-trait
```

#### src/main.rs (CLI)
```
Responsible: Command-line interface, mode dispatch
Inputs: CLI arguments from user
Outputs: Configured capture sessions
Dependencies: clap, session management
```

### Data Flow Sequence

1. **Configuration** (`main.rs`):
   - Parse CLI arguments
   - Select capture mode (file/RTSP)
   - Configure processing options

2. **Session Setup** (`session.rs`):
   - Create capture session with builder
   - Initialize processing pipeline
   - Set up stream multiplexer

3. **Initialization** (`processing.rs` + `cap-rtsp`):
   - Start RTSP server (if streaming)
   - Initialize Gundam/scaling resources
   - Begin capture source setup

4. **Runtime Loop**:
   ```
   Capture Frame → Process Frame → Stream Frame
   (platform)     (pipeline)      (multiplexer)
   ```

5. **Frame Processing** (`cap-scale`):
   - Apply scaling/Gundam transformations
   - Maintain zero-copy semantics
   - Handle memory management

6. **Streaming** (`cap-rtsp`):
   - Encode frames with GStreamer
   - Handle RTSP protocol
   - Manage client connections

## Performance Characteristics

### Latency Budget (Target: <100ms end-to-end)
- **Capture**: 10-30ms (platform dependent)
- **Processing**: 5-20ms (scaling/Gundam)
- **Encoding**: 10-50ms (H.264 complexity)
- **Network**: 5-20ms (RTSP/RTP overhead)

### Throughput Scaling
- **CPU usage**: Linear with resolution and FPS
- **Memory usage**: Bounded by buffer sizes
- **Network bandwidth**: ~1-10 Mbps depending on quality

### Optimization Strategies
- **SIMD acceleration**: CPU vector instructions
- **Zero-copy design**: Minimize memory operations
- **Back-pressure**: Prevent resource exhaustion
- **Configurable quality**: Trade quality for performance

## Error Handling and Recovery

### Failure Modes
- **Capture failures**: Fallback to synthetic frames
- **Processing errors**: Skip frame, continue streaming
- **Streaming failures**: Log errors, attempt recovery
- **Resource exhaustion**: Bounded buffers prevent crashes

### Recovery Strategies
- **Graceful degradation**: Continue with reduced functionality
- **Automatic restart**: Reinitialize failed components
- **User notification**: Clear error messages and suggestions

## Future Architecture Extensions

### Planned Enhancements
- **GPU acceleration**: Vulkan/DirectX buffer sharing
- **Multi-stream**: Simultaneous file + RTSP output
- **Custom processors**: Plugin system for effects
- **Adaptive quality**: Dynamic bitrate adjustment
- **Metrics collection**: Performance monitoring and optimization

### Scalability Considerations
- **Horizontal scaling**: Multiple capture instances
- **Cloud integration**: Distributed processing pipelines
- **Edge computing**: Local AI processing capabilities

This architecture provides a solid foundation for real-time screen capture with AI-optimized processing, maintaining clean separation of concerns while enabling high-performance, zero-copy data flow throughout the pipeline.
