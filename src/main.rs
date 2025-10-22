use anyhow::Result;
use clap::Parser;
use hybrid_screen_capture::config::CaptureConfig;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

#[cfg(feature = "rtsp-streaming")]
use cap_rtsp::{BgraFrame, RtspConfig, RtspPublisher, frame_from_bgra, start_server};

/// Minimal, human-friendly hybrid screen capture:
/// - Windows/macOS/X11: scrap + ffmpeg (subprocess)
/// - Wayland: xdg-desktop-portal + pipewire via ashpd + GStreamer pipeline
#[derive(Parser, Debug)]
#[command(name = "cap")]
#[command(about = "🎥 Record your screen to MP4 with automatic backend selection")]
#[command(
    long_about = "Record your screen to MP4 with automatic backend selection based on your platform.
Supports multiple quality presets and flexible duration formats for easy screen recording."
)]
struct Args {
    /// Output MP4 file path (positional or use -o)
    #[arg(default_value = "capture.mp4", help = "Output file path (MP4 format)")]
    output: String,

    /// Output MP4 path
    #[arg(short, long, help = "Alternative way to specify output file")]
    output_flag: Option<String>,

    /// Recording duration (supports seconds, minutes, hours)
    #[arg(
        short,
        long,
        default_value = "10s",
        help = "How long to record: 30s (30 seconds), 2m (2 minutes), 1h (1 hour)"
    )]
    duration: String,

    /// Video quality preset
    #[arg(
        short,
        long,
        default_value = "medium",
        help = "Video quality preset: low (small files), medium (balanced), high (better quality), ultra (best quality)"
    )]
    quality: String,

    /// Frames per second
    #[arg(
        short = 'f',
        long,
        default_value_t = 30,
        help = "Frames per second (higher = smoother but larger files)"
    )]
    fps: u32,

    /// Capture a specific window instead of full screen
    #[arg(long, help = "Capture a specific window instead of the entire screen")]
    window: bool,

    /// Token-efficient scaling preset for VLM input
    #[arg(
        long,
        value_enum,
        help = "Scale frames for token-efficient VLM processing: p2_56 (2.56x), p4 (4x), p6_9 (6.9x), p9 (9x), p10_24 (10.24x)"
    )]
    scale_preset: Option<cap_scale::presets::TokenPreset>,

    /// Enable DeepSeek-OCR Gundam tiling mode
    #[arg(
        long,
        help = "Enable Gundam tiling mode: produces n×640×640 tiles + 1024×1024 global view for DeepSeek-OCR"
    )]
    gundam: bool,

    /// Enable RTSP streaming mode
    #[arg(
        long,
        help = "Stream via RTSP instead of saving to file. Use with --rtsp-port to set port"
    )]
    rtsp: bool,

    /// RTSP server port
    #[arg(
        long,
        default_value_t = 8554,
        help = "RTSP server port when using --rtsp"
    )]
    rtsp_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle RTSP streaming mode
    #[cfg(feature = "rtsp-streaming")]
    if args.rtsp {
        return run_rtsp_mode(args).await;
    }

    // Parse duration string (e.g., "30s", "2m", "1h")
    let seconds = parse_duration(&args.duration)?;

    // Parse quality preset
    let crf = parse_quality(&args.quality)?;

    // Use output flag if provided, otherwise use positional argument
    let output = args.output_flag.unwrap_or(args.output);

    let config = CaptureConfig::new(
        output,
        args.fps,
        seconds,
        crf,
        args.window,
        args.scale_preset,
        args.gundam,
    );

    config.validate().map_err(anyhow::Error::msg)?;
    let options = config.to_capture_options();
    hybrid_screen_capture::capture_screen(options).await
}

#[cfg(feature = "rtsp-streaming")]
async fn run_rtsp_mode(args: Args) -> Result<()> {
    use tokio::task::spawn_blocking;

    println!("Starting RTSP streaming mode...");
    println!("Stream URL: rtsp://127.0.0.1:{}/cap", args.rtsp_port);
    println!(
        "Use VLC or similar to view: vlc rtsp://127.0.0.1:{}/cap",
        args.rtsp_port
    );

    // Configure RTSP server with scaling if requested
    let (width, height) = if let Some(preset) = args.scale_preset {
        // Use scaled resolution for VLM efficiency
        match preset {
            cap_scale::presets::TokenPreset::P2_56_Long640 => (640, 400), // Approximate scaled size
            cap_scale::presets::TokenPreset::P4_Long640 => (640, 480),
            cap_scale::presets::TokenPreset::P6_9_Long512 => (512, 384),
            cap_scale::presets::TokenPreset::P9_Long640 => (640, 427),
            cap_scale::presets::TokenPreset::P10_24_Long640 => (640, 400),
        }
    } else {
        (1920, 1080) // Default resolution
    };

    let rtsp_config = RtspConfig {
        port: args.rtsp_port,
        mount: "/cap".into(),
        width,
        height,
        framerate: args.fps,
        encoder: None,
        appsrc_max_bytes: Some(8 * 1024 * 1024),
    };

    // Start RTSP server
    let (rtsp_publisher, server_handle) = start_server(rtsp_config)?;

    println!("RTSP server started. Beginning screen capture streaming...");
    println!("Press Ctrl+C to stop streaming");

    // Run capture in blocking task
    let capture_result = spawn_blocking(move || capture_to_rtsp(rtsp_publisher, args)).await?;

    capture_result?;

    // Wait for server to shut down gracefully
    server_handle.join().expect("RTSP server thread panicked");
    Ok(())
}

#[cfg(feature = "rtsp-streaming")]
fn capture_to_rtsp(rtsp_publisher: RtspPublisher, args: Args) -> Result<()> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    return capture_scrap_rtsp(rtsp_publisher, args);

    #[cfg(target_os = "linux")]
    return capture_x11_rtsp(rtsp_publisher, args);

    #[allow(unreachable_code)]
    Err(anyhow::anyhow!("Unsupported platform for RTSP streaming"))
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_scrap_rtsp(rtsp_publisher: RtspPublisher, args: Args) -> Result<()> {
    use scrap::{Capturer, Display};
    use std::thread;

    let (w, h, mut cap) = if args.window {
        let windows = scrap::Window::all().context("failed to list windows")?;
        if windows.is_empty() {
            return Err(anyhow::anyhow!("no windows found"));
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
            return Err(anyhow::anyhow!("invalid window index"));
        }
        let window = windows
            .into_iter()
            .nth(index)
            .ok_or_else(|| anyhow::anyhow!("window index {} out of bounds", index))?;
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

    // Initialize scaling resources if needed
    let (mut resizer, mut staging, mut scaled_buffer, output_size) =
        if let Some(preset) = args.scale_preset {
            use cap_scale::presets::{AspectMode, Size, build_plan};
            let input_size = Size {
                w: w as u32,
                h: h as u32,
            };
            let plan = build_plan(input_size, preset.to_target(), AspectMode::Preserve);
            let scaled_size = plan.out.w as usize * plan.out.h as usize * 4;
            let mut buffer = vec![0u8; scaled_size];
            println!(
                "Scaling enabled: {}x{} → {}x{} (scaling preset)",
                w, h, plan.out.w, plan.out.h
            );
            (
                Some(Resizer::new()),
                Some(cap_scale::cpu::Staging::with_capacity(
                    w as usize * 4 * h as usize,
                )),
                Some(buffer),
                Some(plan.out),
            )
        } else if args.gundam {
            return Err(anyhow::anyhow!(
                "Gundam mode not yet implemented for RTSP streaming"
            ));
        } else {
            (None, None, None, None)
        };

    // Frame pacing
    let frame_time = Duration::from_secs_f64(1.0 / args.fps as f64);
    let frame_size = w as usize * h as usize * 4; // BGRA = 4 bytes per pixel

    println!("Starting RTSP stream... Press Ctrl+C to stop");

    loop {
        let t0 = Instant::now();
        match cap.frame() {
            Ok(frame) => {
                let frame_data = &frame[..];
                // sanity check
                if frame_data.len() != frame_size {
                    return Err(anyhow::anyhow!(
                        "unexpected frame size from scrap: got {}, expected {}",
                        frame_data.len(),
                        frame_size
                    ));
                }

                // Apply scaling if enabled
                let data_to_send =
                    if let (Some(resizer), Some(staging), Some(buffer), Some(output_size)) =
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
                            args.scale_preset.as_ref().unwrap().to_target(),
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
                        buffer.clone()
                    } else {
                        frame_data.to_vec()
                    };

                // Create RTSP frame
                let rtsp_frame = BgraFrame {
                    data: Arc::new(data_to_send),
                    width: output_size.map(|s| s.w).unwrap_or(w as u32),
                    height: output_size.map(|s| s.h).unwrap_or(h as u32),
                    stride: output_size
                        .map(|s| s.w as usize * 4)
                        .unwrap_or(w as usize * 4),
                    pts_ns: None, // Let RTSP handle timing
                };

                // Send to RTSP (non-blocking)
                if let Err(e) = rtsp_publisher.send(rtsp_frame) {
                    eprintln!("Failed to send frame to RTSP: {}", e);
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // no frame ready yet, small nap
                thread::sleep(Duration::from_millis(2));
            }
            Err(e) => return Err(anyhow::anyhow!("scrap frame error: {}", e)),
        }

        // Pace to target FPS
        let elapsed = t0.elapsed();
        if elapsed < frame_time {
            thread::sleep(frame_time - elapsed);
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn capture_x11_rtsp(rtsp_publisher: RtspPublisher, args: Args) -> Result<()> {
    // Try to use scrap for Linux X11 capture
    #[cfg(feature = "screen-capture")]
    {
        use scrap::{Capturer, Display};
        use std::thread;

        println!("Attempting to use scrap for X11 RTSP streaming...");

        let display = match Display::primary() {
            Ok(display) => display,
            Err(e) => {
                eprintln!(
                    "Scrap not available for X11 ({}), falling back to synthetic frames",
                    e
                );
                return capture_x11_synthetic_rtsp(rtsp_publisher, args);
            }
        };

        let w = display.width();
        let h = display.height();
        let mut cap = match Capturer::new(display) {
            Ok(cap) => cap,
            Err(e) => {
                eprintln!(
                    "Failed to create scrap capturer ({}), falling back to synthetic frames",
                    e
                );
                return capture_x11_synthetic_rtsp(rtsp_publisher, args);
            }
        };

        println!("Using scrap for X11 capture: {}x{}", w, h);

        // Initialize scaling resources if needed
        let (mut resizer, mut staging, mut scaled_buffer, output_size) =
            if let Some(preset) = args.scale_preset {
                use cap_scale::presets::{AspectMode, Size, build_plan};
                let input_size = Size {
                    w: w as u32,
                    h: h as u32,
                };
                let plan = build_plan(input_size, preset.to_target(), AspectMode::Preserve);
                let scaled_size = plan.out.w as usize * plan.out.h as usize * 4;
                let mut buffer = vec![0u8; scaled_size];
                println!(
                    "Scaling enabled: {}x{} → {}x{} (scaling preset)",
                    w, h, plan.out.w, plan.out.h
                );
                (
                    Some(fast_image_resize::Resizer::new()),
                    Some(cap_scale::cpu::Staging::with_capacity(
                        w as usize * 4 * h as usize,
                    )),
                    Some(buffer),
                    Some(plan.out),
                )
            } else if args.gundam {
                return Err(anyhow::anyhow!(
                    "Gundam mode not yet implemented for RTSP streaming"
                ));
            } else {
                (None, None, None, None)
            };

        // Frame pacing
        let frame_time = Duration::from_secs_f64(1.0 / args.fps as f64);
        let frame_size = w as usize * h as usize * 4; // BGRA = 4 bytes per pixel

        println!("Starting RTSP stream... Press Ctrl+C to stop");

        loop {
            let t0 = Instant::now();
            match cap.frame() {
                Ok(frame) => {
                    let frame_data = &frame[..];
                    // sanity check
                    if frame_data.len() != frame_size {
                        return Err(anyhow::anyhow!(
                            "unexpected frame size from scrap: got {}, expected {}",
                            frame_data.len(),
                            frame_size
                        ));
                    }

                    // Apply scaling if enabled
                    let data_to_send =
                        if let (Some(resizer), Some(staging), Some(buffer), Some(output_size)) =
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
                                args.scale_preset.as_ref().unwrap().to_target(),
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
                            buffer.clone()
                        } else {
                            frame_data.to_vec()
                        };

                    // Create RTSP frame
                    let rtsp_frame = BgraFrame {
                        data: Arc::new(data_to_send),
                        width: output_size.map(|s| s.w).unwrap_or(w as u32),
                        height: output_size.map(|s| s.h).unwrap_or(h as u32),
                        stride: output_size
                            .map(|s| s.w as usize * 4)
                            .unwrap_or(w as usize * 4),
                        pts_ns: None, // Let RTSP handle timing
                    };

                    // Send to RTSP (non-blocking)
                    if let Err(e) = rtsp_publisher.send(rtsp_frame) {
                        eprintln!("Failed to send frame to RTSP: {}", e);
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // no frame ready yet, small nap
                    thread::sleep(Duration::from_millis(2));
                }
                Err(e) => return Err(anyhow::anyhow!("scrap frame error: {}", e)),
            }

            // Pace to target FPS
            let elapsed = t0.elapsed();
            if elapsed < frame_time {
                thread::sleep(frame_time - elapsed);
            }
        }

        Ok(())
    }

    #[cfg(not(feature = "screen-capture"))]
    {
        capture_x11_synthetic_rtsp(rtsp_publisher, args)
    }
}

#[cfg(target_os = "linux")]
fn capture_x11_synthetic_rtsp(rtsp_publisher: RtspPublisher, args: Args) -> Result<()> {
    // Fallback to synthetic frames when scrap is not available
    println!("X11 RTSP streaming not available, using synthetic frames for demonstration...");

    let frame_time = Duration::from_secs_f64(1.0 / args.fps as f64);
    let mut frame_idx = 0u64;

    // Generate synthetic BGRA frames directly
    loop {
        let width = 1920u32;
        let height = 1080u32;
        let mut data = vec![0u8; (width * height * 4) as usize];

        // Create a moving gradient pattern
        let time = frame_idx as f32 / args.fps as f32;
        let wave_speed = 1.0;

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;

                // Moving wave pattern
                let wave =
                    ((x as f32 * 0.005 + y as f32 * 0.005 + time * wave_speed).sin() + 1.0) * 0.5;

                // BGRA format
                data[idx] = (wave * 255.0) as u8; // Blue
                data[idx + 1] = ((1.0 - wave) * 255.0) as u8; // Green
                data[idx + 2] = (wave * 0.5 * 255.0) as u8; // Red
                data[idx + 3] = 255; // Alpha
            }
        }

        let frame = BgraFrame {
            data: Arc::new(data),
            width,
            height,
            stride: width as usize * 4,
            pts_ns: Some(frame_idx * (1_000_000_000u64 / args.fps as u64)),
        };

        if let Err(e) = rtsp_publisher.send(frame) {
            eprintln!("Failed to send frame to RTSP: {}", e);
            break;
        }
        frame_idx += 1;
        std::thread::sleep(frame_time);
    }

    Ok(())
}

#[cfg(feature = "rtsp-streaming")]
fn generate_synthetic_frame(width: u32, height: u32, frame_idx: u64, fps: u32) -> BgraFrame {
    let mut data = vec![0u8; (width * height * 4) as usize];

    // Create a moving gradient pattern
    let time = frame_idx as f32 / fps as f32;
    let wave_speed = 1.0;

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;

            // Moving wave pattern
            let wave =
                ((x as f32 * 0.005 + y as f32 * 0.005 + time * wave_speed).sin() + 1.0) * 0.5;

            // BGRA format
            data[idx] = (wave * 255.0) as u8; // Blue
            data[idx + 1] = ((1.0 - wave) * 255.0) as u8; // Green
            data[idx + 2] = (wave * 0.5 * 255.0) as u8; // Red
            data[idx + 3] = 255; // Alpha
        }
    }

    frame_from_bgra(data, width, height, fps, frame_idx)
}

/// Parse duration string like "30s", "2m", "1h" into seconds
fn parse_duration(duration: &str) -> Result<u32> {
    if let Ok(seconds) = duration.parse::<u32>() {
        return Ok(seconds);
    }

    let len = duration.len();
    if len < 2 {
        return Err(anyhow::anyhow!("Invalid duration format: {}", duration));
    }

    let (num_str, unit) = duration.split_at(len - 1);
    let num: u32 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number in duration: {}", num_str))?;

    match unit {
        "s" => Ok(num),
        "m" => Ok(num * 60),
        "h" => Ok(num * 3600),
        _ => Err(anyhow::anyhow!(
            "Invalid duration unit: {}. Use 's' for seconds, 'm' for minutes, 'h' for hours",
            unit
        )),
    }
}

/// Parse quality preset into CRF value
fn parse_quality(quality: &str) -> Result<u8> {
    match quality.to_lowercase().as_str() {
        "low" => Ok(28),    // Lower quality, smaller files
        "medium" => Ok(23), // Default quality/size balance
        "high" => Ok(20),   // High quality
        "ultra" => Ok(18),  // Very high quality, larger files
        _ => Err(anyhow::anyhow!(
            "Invalid quality preset: {}. Use: low, medium, high, ultra",
            quality
        )),
    }
}
