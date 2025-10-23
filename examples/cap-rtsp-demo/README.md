# cap-rtsp-demo

Demo application showing RTSP streaming from synthetic BGRA frames using the `cap-rtsp` crate.

## Usage

```bash
# Build and run the demo
cargo run --bin cap-rtsp-demo

# Or with custom options
cargo run --bin cap-rtsp-demo -- --width 1920 --height 1080 --fps 60 --port 8555

# View the stream with VLC
vlc rtsp://127.0.0.1:8554/cap
```

## What it does

- Generates synthetic moving color gradient frames
- Streams them via RTSP using H.264 encoding
- Shows low-latency streaming suitable for VLM input
- Demonstrates the `cap-rtsp` API usage

## Integration with cap

To use with real screen capture, replace the synthetic frame generation with:

```rust
use cap::scrap::get_next_frame;
use cap_scale::{scale_bgra_cpu, TokenPreset};

// In your capture loop:
let frame = get_next_frame()?;
// Optionally scale for token efficiency
let scaled = scale_bgra_cpu(&frame.data, frame.width, frame.height, TokenPreset::VLM_512x512)?;
// Send to RTSP
rtsp_publisher.send(frame_from_bgra(scaled, 512, 512, 30, frame_idx))?;
```

## Dependencies

Requires GStreamer plugins:
- `gst-plugins-base`
- `gst-plugins-good`
- `gst-plugins-bad`
- `gst-plugins-ugly`
- `gst-libav` (for H.264 encoding)

Install on Ubuntu:
```bash
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
    gstreamer1.0-libav
```