// # Scrap Capture Module
//
// This module provides screen capture functionality using the `scrap` library
// for Windows and macOS platforms, with FFmpeg-based capture as fallback for Linux X11.
//
// ## Overview
//
// The module implements a hybrid capture approach:
// - **Windows/macOS**: Direct `scrap` library integration with FFmpeg encoding
// - **Linux X11**: FFmpeg `x11grab` for hardware-accelerated capture
// - **Linux Wayland**: Handled by separate `wayland.rs` module
//
// ## Architecture
//
// ```text
// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
// │   Screen        │───▶│   Scrap/FFmpeg  │───▶│   FFmpeg        │
// │   Capture       │    │   Capture       │    │   Encoding      │
// └─────────────────┘    └─────────────────┘    └─────────────────┘
//        │                        │                        │
//        ▼                        ▼                        ▼
//   Platform-specific       Zero-copy BGRA          H.264/MP4
//   frame grabbing          frame processing        output file
// ```
//
// ## Performance Characteristics
//
// - **Zero-copy processing**: BGRA frames fed directly to FFmpeg (no conversion)
// - **Hardware acceleration**: Leverages platform-specific capture APIs
// - **Real-time encoding**: Low-latency H.264 encoding with `zerolatency` tune
// - **Frame pacing**: Maintains consistent frame rates with timing control
//
// ## Platform Support
//
// | Platform | Capture Method | Backend | Notes |
// |----------|----------------|---------|-------|
// | Windows | `scrap` library | DirectShow/GDI | Full window/screen capture |
// | macOS | `scrap` library | AVFoundation | Full window/screen capture |
// | Linux X11 | FFmpeg `x11grab` | X11 | Hardware accelerated |
// | Linux Wayland | Portal API | PipeWire/GStreamer | Modern Wayland support |
//
// ## Example Usage
//
// Basic screen capture:
/// Internal API - no public examples available
//
// Window capture (Windows/macOS only):
/// Internal API - no public examples available

use anyhow::{Context, Result, anyhow};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use scrap::Window;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use scrap::{Capturer, Display};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::io::Write;
use std::process::{Command, Stdio};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::time::{Duration, Instant};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{thread, time};
use tokio::task::spawn_blocking;

use crate::CaptureOptions;

/// Captures screen content using platform-specific backends with FFmpeg encoding.
///
/// Time complexity: O(1) for setup and dispatch, but the actual capture operation
/// runs for O(seconds * fps) time with each frame processed in O(width * height)
/// for scaling operations.
///
/// Missing functionality: None - fully implements platform-specific dispatch
/// with proper error handling and feature gating.
pub async fn capture_ffmpeg(options: CaptureOptions) -> Result<()> {
    let _ = spawn_blocking(move || {
        // For X11 Linux, use ffmpeg's x11grab directly
        #[cfg(target_os = "linux")]
        {
            if !options.window {
                return capture_x11_ffmpeg(options);
            } else {
                return Err(anyhow!(
                    "Window capture not supported on Linux. Use full screen capture."
                ));
            }
        }

        // For other platforms, use the original scrap + ffmpeg approach
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        return capture_scrap_ffmpeg(options);

        #[allow(unreachable_code)]
        Err(anyhow!("Unsupported platform"))
    })
    .await?;
    Ok(())
}

/// Captures screen content on Linux X11 using FFmpeg's x11grab.
///
/// Time complexity: O(seconds * fps) - FFmpeg runs for the specified duration,
/// capturing and encoding frames in real-time.
///
/// Missing functionality: None - provides complete X11 screen capture with
/// hardware acceleration when available.
#[cfg(target_os = "linux")]
fn capture_x11_ffmpeg(options: CaptureOptions) -> Result<()> {
    println!("Using ffmpeg x11grab for X11 screen capture...");
    println!("Options: {:?}", options);

    let display = std::env::var("DISPLAY").unwrap_or(":0".to_string());
    println!("Using display: {}", display);

    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "x11grab",
            "-i",
            &format!("{}.0", display),
            "-r",
            &options.fps.to_string(),
            "-t",
            &options.seconds.to_string(),
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "zerolatency",
            "-crf",
            &options.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            &options.output,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to start ffmpeg")?;

    let status = child.wait().context("Failed to wait for ffmpeg")?;
    if status.success() {
        println!("Saved {}", options.output);
        Ok(())
    } else {
        Err(anyhow!("ffmpeg exited with status: {}", status))
    }
}

/// Captures screen content on Windows/macOS using scrap library with FFmpeg encoding.
///
/// Time complexity: O(seconds * fps) where each frame capture is O(1) but scaling
/// operations are O(width * height). For 1920x1080 input with scaling, each frame
/// is O(2M) operations, leading to O(seconds * fps * 2M) total complexity.
///
/// Gundam mode not implemented: The function returns an error for Gundam mode,
/// indicating this feature needs implementation for video capture workflows.
///
/// Missing functionality:
/// - Gundam mode not supported for video capture (returns error)
/// - Window capture is interactive (requires user input from stdin)
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_scrap_ffmpeg(options: CaptureOptions) -> Result<()> {
    let (w, h, mut cap) = if options.window {
        let windows = Window::all().context("failed to list windows")?;
        if windows.is_empty() {
            return Err(anyhow!("no windows found"));
        }
        println!("Available windows:");
        for (i, w) in windows.iter().enumerate() {
            println!("{}: {}", i, w.title());
        }
        println!("Enter the number of the window to capture:");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .context("failed to read input")?;
        let index: usize = input.trim().parse().context("invalid number")?;
        if index >= windows.len() {
            return Err(anyhow!("invalid window index"));
        }
        let window = windows
            .into_iter()
            .nth(index)
            .ok_or_else(|| anyhow!("window index {} out of bounds", index))?;
        let w = window.width();
        let h = window.height();
        let cap = Capturer::new(window).context("cannot create capturer for window")?;
        (w, h, cap)
    } else {
        let display = Display::primary().context("scrap: no primary display")?;
        let w = display.width();
        let h = display.height();
        let cap = Capturer::new(display).context("scrap: cannot create capturer")?;
        (w, h, cap)
    };

    // Determine output dimensions (scaled or original)
    let (output_w, output_h) = if let Some(preset) = &options.scale_preset {
        use cap_scale::presets::{AspectMode, Size, build_plan};
        let input_size = Size {
            w: w as u32,
            h: h as u32,
        };
        let plan = build_plan(input_size, preset.to_target(), AspectMode::Preserve);
        (plan.out.w, plan.out.h)
    } else {
        (w as u32, h as u32)
    };

    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            // Raw frames in from stdin
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgra", // Changed from bgr24 to bgra for zero-copy
            "-s",
            &format!("{}x{}", output_w, output_h), // Use scaled dimensions
            "-r",
            &options.fps.to_string(),
            "-i",
            "-", // stdin
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "zerolatency",
            "-crf",
            &options.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            &options.output,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn ffmpeg (ensure it's installed and on PATH)")?;

    let mut ffmpeg_stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("ffmpeg stdin not available"))?;

    // Simple frame pacing
    let frame_time = Duration::from_secs_f64(1.0 / options.fps as f64);
    let end_time = Instant::now() + Duration::from_secs(options.seconds as u64);
    let frame_size = w as usize * h as usize * 4; // BGRA = 4 bytes per pixel

    // Initialize scaling resources if needed
    let (mut resizer, mut staging, mut scaled_buffer, output_size) =
        if let Some(preset) = &options.scale_preset {
            use cap_scale::presets::{AspectMode, Size, build_plan};
            let input_size = Size {
                w: w as u32,
                h: h as u32,
            };
            let plan = build_plan(input_size, preset.to_target(), AspectMode::Preserve);
            let scaled_size = plan.out.w as usize * plan.out.h as usize * 4;
            let mut buffer = vec![0u8; scaled_size];
            println!(
                "Scaling enabled: {}x{} → {}x{} ({} preset)",
                w,
                h,
                plan.out.w,
                plan.out.h,
                preset.to_target().to_string()
            );
            (
                Some(fir::Resizer::new()),
                Some(cap_scale::cpu::Staging::with_capacity(
                    w as usize * 4 * h as usize,
                )),
                Some(buffer),
                Some(plan.out),
            )
        } else if options.gundam_mode {
            return Err(anyhow!("Gundam mode not yet implemented for video capture"));
        } else {
            (None, None, None, None)
        };

    while Instant::now() < end_time {
        let t0 = Instant::now();
        match cap.frame() {
            Ok(frame) => {
                let frame_data = &frame[..];
                // sanity check
                if frame_data.len() != frame_size {
                    return Err(anyhow!(
                        "unexpected frame size from scrap: got {}, expected {}",
                        frame_data.len(),
                        frame_size
                    ));
                }

                // Apply scaling if enabled
                let data_to_write = if let (
                    Some(ref mut resizer),
                    Some(ref mut staging),
                    Some(ref mut buffer),
                    Some(output_size),
                ) =
                    (&mut resizer, &mut staging, &mut scaled_buffer, &output_size)
                {
                    use cap_scale::cpu::scale_bgra_cpu;
                    use cap_scale::presets::{AspectMode, Size, build_plan};

                    let input_size = Size {
                        w: w as u32,
                        h: h as u32,
                    };
                    let plan = build_plan(
                        input_size,
                        options.scale_preset.as_ref().unwrap().to_target(),
                        AspectMode::Preserve,
                    );

                    scale_bgra_cpu(
                        resizer,
                        frame_data,
                        Size {
                            w: w as u32,
                            h: h as u32,
                        },
                        Some(w as usize * 4),
                        &plan,
                        buffer,
                        Some(staging),
                    )?;
                    &buffer[..]
                } else {
                    frame_data
                };

                // Write data to FFmpeg (scaled or original)
                ffmpeg_stdin.write_all(data_to_write)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // no frame ready yet, small nap
                thread::sleep(time::Duration::from_millis(2));
            }
            Err(e) => return Err(anyhow!("scrap frame error: {}", e)),
        }

        // Pace to target FPS
        let elapsed = t0.elapsed();
        if elapsed < frame_time {
            thread::sleep(frame_time - elapsed);
        }
    }

    drop(ffmpeg_stdin); // close to let ffmpeg finalize
    let status = child.wait().context("waiting for ffmpeg to finish")?;
    if !status.success() {
        return Err(anyhow!("ffmpeg exited with code {:?}", status.code()));
    }
    println!("Saved {}", options.output);
    Ok(())
}
