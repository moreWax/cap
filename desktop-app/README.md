# Screen Capture Desktop App

A cross-platform screen recording application with both CLI and GUI interfaces using the hybrid screen capture library.

## Features

- **GUI Interface**: Native desktop application with visual controls
- Support for full screen or window capture
- Configurable FPS, duration, and quality (CRF)
- Cross-platform screen capture (X11, Wayland, Windows, macOS)

## Usage

### GUI Application

```bash
cd desktop-app
cargo run --bin desktop-gui
```

The GUI provides visual controls for all recording settings and shows recording status.

## Requirements

- FFmpeg must be installed on your system
- For Linux: X11 or Wayland session
- For window capture: Supported on Windows and macOS

## Building

```bash
cargo build --release
```

The binary will be available at `target/release/desktop-gui`.