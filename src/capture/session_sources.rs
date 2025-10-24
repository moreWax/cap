// # Session-Based Capture Sources
//
// This module provides CaptureSource implementations that can be used with
// CaptureSessionBuilder for declarative session-based capture workflows.
//
// These sources wrap the platform-specific capture backends (scrap, FFmpeg, etc.)
// in a unified CaptureSource trait interface that integrates with the session
// architecture.

#[cfg(feature = "rtsp-streaming")]
use crate::error::CaptureError;
#[cfg(feature = "rtsp-streaming")]
use crate::processing::Size;
#[cfg(feature = "rtsp-streaming")]
use crate::session::CaptureSource;
#[cfg(feature = "rtsp-streaming")]
use anyhow::Result;
#[cfg(feature = "rtsp-streaming")]
use async_trait::async_trait;
#[cfg(feature = "rtsp-streaming")]
use cap_rtsp::BgraFrame;
#[cfg(all(
    feature = "rtsp-streaming",
    any(target_os = "windows", target_os = "macos")
))]
use scrap;
#[cfg(feature = "rtsp-streaming")]
use std::sync::Arc;

#[cfg(feature = "rtsp-streaming")]
/// Scrap-based capture source for Windows and macOS platforms.
///
/// This implementation wraps the scrap library's Capturer in a CaptureSource
/// trait implementation that can be used with CaptureSessionBuilder.
#[cfg(all(
    feature = "rtsp-streaming",
    any(target_os = "windows", target_os = "macos")
))]
#[derive(Debug)]
pub struct ScrapCaptureSource {
    capturer: scrap::Capturer,
}

#[cfg(all(
    feature = "rtsp-streaming",
    any(target_os = "windows", target_os = "macos")
))]
impl ScrapCaptureSource {
    /// Create a new ScrapCaptureSource for the primary display.
    pub fn new() -> Result<Self> {
        let display = scrap::Display::primary()
            .map_err(|e| anyhow!("Failed to get primary display: {}", e))?;

        let capturer = scrap::Capturer::new(display)
            .map_err(|e| anyhow!("Failed to create capturer: {}", e))?;

        Ok(Self { capturer })
    }

    /// Create a ScrapCaptureSource for a specific display.
    pub fn with_display(display: scrap::Display) -> Result<Self> {
        let capturer = scrap::Capturer::new(display)
            .map_err(|e| anyhow!("Failed to create capturer: {}", e))?;

        Ok(Self { capturer })
    }
}

#[cfg(all(
    feature = "rtsp-streaming",
    any(target_os = "windows", target_os = "macos")
))]
#[async_trait]
impl CaptureSource for ScrapCaptureSource {
    fn input_size(&self) -> Size {
        let width = self.capturer.width() as u32;
        let height = self.capturer.height() as u32;
        Size {
            w: width,
            h: height,
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        // Scrap capturer is initialized in constructor, nothing additional needed
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<BgraFrame> {
        let frame = self
            .capturer
            .frame()
            .map_err(|e| anyhow!("Failed to capture frame: {}", e))?;

        // Create BgraFrame from the captured data
        let bgra_frame = BgraFrame {
            data: Arc::new(frame),
            width: self.capturer.width() as u32,
            height: self.capturer.height() as u32,
            stride: self.capturer.width() as usize * 4,
            pts_ns: None, // Let the session handle timing
        };

        Ok(bgra_frame)
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Scrap capturer doesn't require explicit shutdown
        Ok(())
    }
}

#[cfg(all(feature = "rtsp-streaming", target_os = "linux"))]
/// FFmpeg-based capture source for Linux X11.
///
/// This implementation wraps FFmpeg's x11grab in a CaptureSource trait
/// that integrates with the session architecture.
#[cfg(all(feature = "rtsp-streaming", target_os = "linux"))]
#[derive(Debug)]
pub struct FFmpegCaptureSource {
    // TODO: Implement FFmpeg x11grab wrapper
    width: u32,
    height: u32,
}

#[cfg(all(feature = "rtsp-streaming", target_os = "linux"))]
impl FFmpegCaptureSource {
    /// Create a new FFmpegCaptureSource for X11 display.
    pub fn new(_display: &str) -> Result<Self, CaptureError> {
        // TODO: Initialize FFmpeg x11grab context
        // For now, return placeholder dimensions
        Ok(Self {
            width: 1920,
            height: 1080,
        })
    }
}

#[cfg(all(feature = "rtsp-streaming", target_os = "linux"))]
#[async_trait]
impl CaptureSource for FFmpegCaptureSource {
    fn input_size(&self) -> Size {
        Size {
            w: self.width,
            h: self.height,
        }
    }

    async fn initialize(&mut self) -> Result<()> {
        // TODO: Initialize FFmpeg x11grab context
        // For now, just succeed to allow session initialization
        println!("FFmpeg capture source initialized (stub implementation)");
        Ok(())
    }

    async fn capture_frame(&mut self) -> Result<BgraFrame> {
        // TODO: Capture frame using FFmpeg
        // For now, return a synthetic frame to allow testing
        let width = self.width;
        let height = self.height;
        let mut data = vec![0u8; (width * height * 4) as usize];

        // Create a simple gradient pattern for testing
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let r = (x * 255 / width) as u8;
                let g = (y * 255 / height) as u8;
                let b = 128u8;
                let a = 255u8;

                // BGRA format
                data[idx] = b; // Blue
                data[idx + 1] = g; // Green
                data[idx + 2] = r; // Red
                data[idx + 3] = a; // Alpha
            }
        }

        Ok(BgraFrame {
            data: Arc::new(data),
            width,
            height,
            stride: width as usize * 4,
            pts_ns: None,
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        // TODO: Shutdown FFmpeg context
        Ok(())
    }
}
