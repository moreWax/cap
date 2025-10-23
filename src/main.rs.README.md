# src/main.rs: CLI Application and High-Level Orchestration

Minimal, human-friendly hybrid screen capture application with automatic backend selection. Provides CLI interface for screen recording and RTSP streaming with VLM-optimized processing options.

## Architecture Overview

The main application serves as the CLI entry point and high-level orchestrator:

1. **CLI Argument Parsing**: clap-based command-line interface
2. **Mode Dispatch**: File recording vs RTSP streaming modes
3. **Platform Detection**: Automatic backend selection (scrap, ffmpeg, GStreamer)
4. **Configuration Management**: Duration parsing, quality presets, scaling options
5. **Capture Orchestration**: Coordinates capture, processing, and output

## CLI Interface

### Basic Screen Recording
```bash
# Record 30 seconds to capture.mp4
cap -d 30s

# Record 2 minutes with high quality
cap -d 2m -q high output.mp4

# Record specific window at 60 FPS
cap --window -f 60 -d 1m window_capture.mp4
```

### VLM-Optimized Capture
```bash
# Token-efficient scaling for AI models
cap --scale-preset p4 -d 30s vlm_input.mp4

# DeepSeek OCR Gundam tiling mode
cap --gundam -d 1m gundam_capture.mp4

# Combined scaling + Gundam for complex documents
cap --scale-preset p2_56 --gundam -d 45s optimized_capture.mp4
```

### RTSP Streaming
```bash
# Stream screen via RTSP
cap --rtsp --fps 30

# Stream with VLM optimization
cap --rtsp --scale-preset p4 --rtsp-port 8555

# Stream Gundam tiles for OCR
cap --rtsp --gundam --fps 10
```

## Command-Line Arguments

### Core Options
- **`output`**: Output MP4 file path (positional)
- **`-o, --output-flag`**: Alternative output specification
- **`-d, --duration`**: Recording duration (`30s`, `2m`, `1h`)
- **`-q, --quality`**: Quality preset (`low`, `medium`, `high`, `ultra`)
- **`-f, --fps`**: Target frames per second

### Advanced Options
- **`--window`**: Capture specific window instead of full screen
- **`--scale-preset`**: VLM token-efficient scaling (`p2_56`, `p4`, `p6_9`, `p9`, `p10_24`)
- **`--gundam`**: Enable DeepSeek-OCR Gundam tiling mode
- **`--rtsp`**: Enable RTSP streaming mode
- **`--rtsp-port`**: RTSP server port (default: 8554)

## Application Flow

### File Recording Mode
1. **Parse CLI arguments** and validate configuration
2. **Create CaptureConfig** with output path and parameters
3. **Delegate to hybrid_screen_capture** library for actual recording
4. **Handle platform-specific capture** (scrap, ffmpeg, GStreamer)

### RTSP Streaming Mode
1. **Configure RTSP server** with appropriate dimensions
2. **Start RTSP server** in background thread
3. **Initialize capture resources** (scaling, Gundam buffers)
4. **Run capture loop** sending frames to RTSP publisher
5. **Handle graceful shutdown** on interruption

## Platform-Specific Behavior

### Windows/macOS
- **Primary**: scrap crate for direct screen capture
- **Fallback**: ffmpeg subprocess for compatibility
- **Features**: Window selection, high FPS capture

### Linux (X11)
- **Primary**: scrap crate for X11 capture
- **Fallback**: Synthetic frames for demonstration
- **Features**: X11 display integration

### Linux (Wayland)
- **Primary**: xdg-desktop-portal + pipewire + GStreamer
- **Features**: Modern Wayland compatibility

## Configuration Management

### Duration Parsing
Supports human-friendly duration formats:
- `30` → 30 seconds
- `30s` → 30 seconds
- `2m` → 120 seconds
- `1h` → 3600 seconds

### Quality Presets
Maps to ffmpeg CRF values:
- `low` → CRF 28 (smaller files)
- `medium` → CRF 23 (balanced)
- `high` → CRF 20 (better quality)
- `ultra` → CRF 18 (best quality)

### Scaling Integration
- **Token Presets**: Maps to cap-scale presets for VLM efficiency
- **Gundam Mode**: Enables DeepSeek OCR tiling with composite frames
- **Resolution Calculation**: Automatic dimension computation for streaming

## Error Handling

### Configuration Validation
- Invalid duration formats
- Unsupported quality presets
- Missing required arguments
- Platform capability checks

### Runtime Error Recovery
- Capture backend failures with fallbacks
- Frame processing errors with graceful degradation
- RTSP streaming errors with connection recovery

## Integration Points

### cap-rtsp Integration
- RTSP server initialization and configuration
- Frame publishing with back-pressure handling
- Composite frame arrangement for Gundam mode

### cap-scale Integration
- Scaling plan computation and execution
- Gundam tiling for OCR optimization
- Buffer management for zero-copy processing

### hybrid_screen_capture Integration
- Platform-specific capture delegation
- Configuration translation and validation
- File output handling

## Performance Considerations

- **Frame pacing**: Maintains target FPS with sleep-based timing
- **Memory management**: Bounded buffers prevent unbounded growth
- **Platform optimization**: Uses fastest available capture method
- **Processing efficiency**: Optional scaling/Gundam only when requested

## Future Extensions

- Additional capture sources (Android, iOS screen mirroring)
- Custom processing pipelines via plugins
- Multi-stream output (file + RTSP simultaneously)
- Configuration file support for complex setups
- GUI wrapper for easier configuration
- Cloud upload integration for recorded files</content>
<parameter name="filePath">/home/xor/cap/src/main.rs.README.md