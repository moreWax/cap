// SPDX-License-Identifier: MIT
//! Demo: RTSP streaming from synthetic BGRA frames.
//!
//! Run: cargo run --bin cap-rtsp-demo -- --help
//! Then: vlc rtsp://127.0.0.1:8554/cap
//!
//! This demo generates a moving color gradient and streams it via RTSP.
//! In a real app, you'd replace the synthetic frames with cap::scrap frames.

use anyhow::Result;
use cap_rtsp::{frame_from_bgra, start_server, BgraFrame, RtspConfig, RtspPublisher};
use clap::Parser;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "1280")]
    width: u32,
    #[arg(long, default_value = "720")]
    height: u32,
    #[arg(long, default_value = "30")]
    fps: u32,
    #[arg(long, default_value = "8554")]
    port: u16,
    #[arg(long, default_value = "/cap")]
    mount: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Configure RTSP server
    let cfg = RtspConfig {
        port: args.port,
        mount: args.mount.clone(),
        width: args.width,
        height: args.height,
        framerate: args.fps,
        encoder: Some(
            "x264enc tune=zerolatency speed-preset=veryfast bitrate=4000 ! h264parse".to_string(),
        ), // Remove profile=main
        appsrc_max_bytes: Some(4 * 1024 * 1024),
    };

    // Start server
    let (publisher, server_handle) = start_server(cfg)?;

    println!("RTSP demo starting...");
    println!("Stream URL: rtsp://127.0.0.1:{}{}", args.port, args.mount);
    println!(
        "Open with: vlc rtsp://127.0.0.1:{}{}",
        args.port, args.mount
    );
    println!("Press Ctrl+C to stop");

    // Generate and stream synthetic frames
    let start = Instant::now();
    let mut frame_idx = 0u64;

    loop {
        let elapsed = start.elapsed();
        let should_exit = elapsed.as_secs() > 300; // Run for 5 minutes max

        if should_exit {
            break;
        }

        // Generate a moving gradient frame
        let frame = generate_gradient_frame(args.width, args.height, frame_idx, args.fps);

        // Send to RTSP (non-blocking)
        if let Err(e) = publisher.send(frame) {
            eprintln!("Failed to send frame: {}", e);
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        frame_idx += 1;

        // Sleep to maintain framerate
        let target_duration = Duration::from_nanos(1_000_000_000 / args.fps as u64);
        let frame_time = start.elapsed() - elapsed;
        if frame_time < target_duration {
            thread::sleep(target_duration - frame_time);
        }
    }

    println!("Demo finished. Server thread will exit gracefully.");
    drop(publisher); // This will cause the server thread to exit
    server_handle.join().expect("Server thread panicked");

    Ok(())
}

/// Generate a synthetic BGRA frame with a moving color gradient.
/// Real apps would use cap::scrap::get_next_frame() instead.
fn generate_gradient_frame(width: u32, height: u32, frame_idx: u64, fps: u32) -> BgraFrame {
    let mut data = vec![0u8; (width * height * 4) as usize];

    // Create a moving gradient based on frame index
    let time = frame_idx as f32 / fps as f32;
    let wave_speed = 2.0;

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;

            // Moving wave pattern
            let wave = ((x as f32 * 0.01 + y as f32 * 0.01 + time * wave_speed).sin() + 1.0) * 0.5;

            // BGRA format: Blue, Green, Red, Alpha
            data[idx] = (wave * 255.0) as u8; // Blue
            data[idx + 1] = ((1.0 - wave) * 255.0) as u8; // Green
            data[idx + 2] = (wave * 0.5 * 255.0) as u8; // Red
            data[idx + 3] = 255; // Alpha
        }
    }

    frame_from_bgra(data, width, height, fps, frame_idx)
}
