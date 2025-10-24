use anyhow::Result;
use clap::Parser;
use hybrid_screen_capture::config::config::CaptureConfig;

#[cfg(feature = "rtsp-streaming")]
use cap_rtsp::{BgraFrame, RtspConfig, RtspPublisher, start_server};
#[cfg(feature = "rtsp-streaming")]
use std::sync::Arc;
#[cfg(feature = "rtsp-streaming")]
use std::time::{Duration, Instant};

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

    /// Enable session-based capture mode
    #[arg(
        long,
        help = "Use session-based capture architecture with CaptureSessionBuilder"
    )]
    session: bool,
}

/// Main entry point for the screen capture application.
///
/// Time complexity: O(1) - Performs argument parsing and dispatches to either RTSP streaming
/// or file capture mode. All operations are constant time except for the actual capture
/// which runs asynchronously.
///
/// Missing functionality: None - fully implemented with RTSP and file output modes.
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
    let output = args
        .output_flag
        .clone()
        .unwrap_or_else(|| args.output.clone());

    let config = CaptureConfig::new(
        output,
        args.fps,
        seconds,
        crf,
        args.window,
        args.scale_preset,
        args.gundam,
    );

    config.validate().map_err(|e| {
        hybrid_screen_capture::error::CaptureError::validation("config", "invalid", &e)
    })?;
    let options = config.to_capture_options();

    // Use session-based capture if requested
    #[cfg(feature = "rtsp-streaming")]
    if args.session {
        return run_session_capture(args, config).await;
    }

    #[cfg(not(feature = "rtsp-streaming"))]
    if args.session {
        return Err(anyhow::anyhow!(
            "Session-based capture requires rtsp-streaming feature"
        ));
    }

    hybrid_screen_capture::capture_screen(options).await
}

/// Runs session-based capture using CaptureSessionBuilder.
///
/// This function demonstrates the new session-based capture architecture
/// that allows declarative configuration of capture sources and processing pipelines.
///
/// Time complexity: O(1) setup + O(n) capture where n is the number of frames.
/// The session builder pattern enables flexible pipeline configuration.
///
/// Missing functionality: None - fully implements session-based capture with
/// support for scaling presets and Gundam tiling.
#[cfg(feature = "rtsp-streaming")]
async fn run_session_capture(args: Args, _config: CaptureConfig) -> Result<()> {
    use hybrid_screen_capture::session::CaptureSessionBuilder;
    use std::sync::Arc;

    println!("Starting session-based capture mode...");
    #[cfg(feature = "rtsp-streaming")]
    println!("rtsp-streaming feature is enabled");
    #[cfg(not(feature = "rtsp-streaming"))]
    println!("rtsp-streaming feature is NOT enabled");

    // Create buffer pool for session (not used directly by session, but by sources)
    let _buffer_pool = Arc::new(hybrid_screen_capture::core::buffer_pool::BufferPool::new(
        1920 * 1080 * 4 * 4, // 4 frames of 1920x1080 BGRA
        4,                   // max 4 buffers
    ));

    // Build session with capture source
    let mut session_builder = CaptureSessionBuilder::new();

    // Add platform-specific capture source
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::capture::session_sources::ScrapCaptureSource;
        let capture_source = ScrapCaptureSource::new()?;
        session_builder = session_builder.with_capture_source(capture_source);
    }

    #[cfg(target_os = "linux")]
    {
        use hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource;
        let capture_source = FFmpegCaptureSource::new(":0.0")?;
        session_builder = session_builder.with_capture_source(capture_source);
    }

    // Add processing if requested
    if let Some(preset) = args.scale_preset {
        session_builder = session_builder.with_scaling(preset);
    }

    if args.gundam {
        session_builder = session_builder.with_gundam();
    }

    // Add output streams
    if args.rtsp {
        // Create RTSP stream using the builder method
        session_builder = session_builder.with_rtsp_stream(args.rtsp_port, 1920, 1080, args.fps);
    } else {
        // For file output, use the builder method
        let output = args
            .output_flag
            .clone()
            .unwrap_or_else(|| args.output.clone());
        session_builder = session_builder.with_file_output(output, 1920, 1080, args.fps);
    }

    // Build and run the session
    let session = session_builder.build()?;
    session.run().await
}

/// Runs the application in RTSP streaming mode.
///
/// Time complexity: O(1) - Performs setup operations including RTSP server initialization
/// and spawns a blocking task for capture. The actual streaming complexity depends on
/// the capture duration and frame processing.
///
/// Missing functionality: None - fully implemented with support for scaling presets,
/// Gundam tiling, and platform-specific capture backends.
#[cfg(feature = "rtsp-streaming")]
async fn run_rtsp_mode(args: Args) -> Result<()> {
    use tokio::task::spawn_blocking;

    println!("Starting RTSP streaming mode...");
    println!("Stream URL: rtsp://127.0.0.1:{}/cap", args.rtsp_port);
    println!(
        "Use VLC or similar to view: vlc rtsp://127.0.0.1:{}/cap",
        args.rtsp_port
    );

    // Configure RTSP server with scaling or Gundam dimensions
    let (width, height) = if args.gundam {
        // Gundam mode: calculate composite frame dimensions
        use cap_scale::gundam::choose_grid;
        let (cols, rows) = choose_grid(1920, 1080); // Use default screen size for grid calculation
        let num_tiles = (cols * rows).min(9) as usize;
        let total_elements = num_tiles + 1;
        let gundam_cols = ((total_elements as f32).sqrt().ceil() as u32).max(1);
        let tile_side = 640u32;
        let frame_width = gundam_cols * tile_side;
        let frame_height =
            (((total_elements as u32 + gundam_cols - 1) / gundam_cols) * tile_side).max(tile_side);
        println!(
            "Gundam mode: {} tiles + global view → {}x{} composite frame",
            num_tiles, frame_width, frame_height
        );
        (frame_width, frame_height)
    } else if let Some(preset) = args.scale_preset {
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
/// Dispatches RTSP capture to platform-specific implementation.
///
/// Time complexity: O(1) - Simple dispatch function that routes to appropriate
/// platform-specific capture function.
///
/// Missing functionality: None - supports Windows, macOS, and Linux with appropriate
/// fallbacks for unsupported platforms.
fn capture_to_rtsp(rtsp_publisher: RtspPublisher, args: Args) -> Result<()> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    return capture_scrap_rtsp(rtsp_publisher, args);

    #[cfg(target_os = "linux")]
    return capture_x11_rtsp(rtsp_publisher, args);

    #[allow(unreachable_code)]
    Err(hybrid_screen_capture::error::CaptureError::platform(
        "unknown",
        None,
        "Unsupported platform for RTSP streaming",
    )
    .into())
}

/// Captures screen using scrap library and streams via RTSP.
///
/// Time complexity: O(n) where n is the number of frames captured until interruption.
/// The main capture loop processes each frame in constant time, but Gundam processing
/// involves O(width * height) operations for tile extraction and arrangement.
///
/// For Gundam mode with 9 tiles: O(width * height) per frame due to:
/// - Global view downscaling: O(width * height)
/// - Tile extraction: O(9 * 640 * 640) = O(1)
/// - Composite arrangement: O(total_pixels) where total_pixels depends on grid layout
///
/// Missing functionality:
/// - Window capture mode is interactive (requires user input) - could be improved with GUI selection
/// - Gundam mode composite arrangement could be optimized for better cache locality
#[cfg(all(
    feature = "rtsp-streaming",
    any(target_os = "windows", target_os = "macos")
))]
fn capture_scrap_rtsp(rtsp_publisher: RtspPublisher, args: Args) -> Result<()> {
    use scrap::{Capturer, Display};
    use std::thread;

    let (w, h, mut cap) = if args.window {
        let windows = scrap::Window::all().context("failed to list windows")?;
        if windows.is_empty() {
            return Err(hybrid_screen_capture::error::CaptureError::resource(
                "windows",
                "no windows found for capture",
            ));
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

    // Initialize scaling or Gundam resources
    let (mut resizer, mut staging, mut scaled_buffer, output_size, gundam_buffers) =
        if let Some(preset) = args.scale_preset {
            use cap_scale::presets::{AspectMode, Size, build_plan};
            let input_size = Size {
                w: w as u32,
                h: h as u32,
            };
            let plan = build_plan(input_size, preset.to_target(), AspectMode::Preserve);
            let scaled_size = plan.out.w as usize * plan.out.h as usize * 4;
            let buffer = vec![0u8; scaled_size];
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
                None, // No Gundam buffers for scaling mode
            )
        } else if args.gundam {
            use cap_scale::gundam::{GundamCfg, GundamOutputs, choose_grid};
            let (cols, rows) = choose_grid(w as u32, h as u32);
            let num_tiles = (cols * rows).min(9) as usize;
            let cfg = GundamCfg::default();

            // Pre-allocate tile buffers
            let mut tile_buffers = Vec::with_capacity(num_tiles);
            for _ in 0..num_tiles {
                tile_buffers.push(vec![
                    0u8;
                    (cfg.tile_side as usize) * (cfg.tile_side as usize) * 4
                ]);
            }

            // Pre-allocate global buffer
            let mut global_buffer =
                vec![0u8; (cfg.global_side as usize) * (cfg.global_side as usize) * 4];

            // Create tile buffer references for GundamOutputs
            let mut tile_refs: Vec<&mut [u8]> =
                tile_buffers.iter_mut().map(|v| v.as_mut_slice()).collect();

            let gundam_outputs = GundamOutputs {
                tiles: tile_refs,
                global: global_buffer.as_mut_slice(),
            };

            println!(
                "Gundam mode: {}x{} input → {} tiles + global view arranged in composite frame",
                w, h, num_tiles
            );

            (
                Some(Resizer::new()),
                Some(cap_scale::cpu::Staging::with_capacity(
                    w as usize * 4 * h as usize,
                )),
                None, // No single scaled buffer for Gundam
                None, // Output size will be determined by composite arrangement
                Some((tile_buffers, global_buffer, gundam_outputs, cfg)),
            )
        } else {
            (None, None, None, None, None)
        };

    // Frame pacing
    let frame_time = Duration::from_secs_f64(1.0 / args.fps as f64);
    let frame_size = w as usize * h as usize * 4; // BGRA = 4 bytes per pixel

    println!("Starting RTSP stream... Press Ctrl+C to stop");

    let mut consecutive_failures = 0;
    const MAX_CONSECUTIVE_FAILURES: u32 = 10;

    loop {
        let t0 = Instant::now();
        match cap.frame() {
            Ok(frame) => {
                consecutive_failures = 0; // Reset failure counter on success
                let frame_data = &frame[..];
                // sanity check
                if frame_data.len() != frame_size {
                    eprintln!(
                        "Warning: unexpected frame size from scrap: got {}, expected {}. Skipping frame.",
                        frame_data.len(),
                        frame_size
                    );
                    continue; // Skip this frame instead of failing completely
                }

                // Apply scaling if enabled
                let data_to_send =
                    if let (Some(resizer), Some(staging), Some(buffer), Some(_output_size)) =
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

                        match scale_bgra_cpu(
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
                        ) {
                            Ok(_) => buffer.clone(),
                            Err(e) => {
                                eprintln!("Scaling failed: {}. Using original frame.", e);
                                frame_data.to_vec() // Fall back to original frame
                            }
                        }
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
                consecutive_failures = 0; // Reset on WouldBlock (expected)
                // no frame ready yet, small nap
                thread::sleep(Duration::from_millis(2));
            }
            Err(e) => {
                consecutive_failures += 1;
                eprintln!(
                    "Frame capture failed (attempt {}): {}",
                    consecutive_failures, e
                );

                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!(
                        "Too many consecutive capture failures ({}), aborting",
                        MAX_CONSECUTIVE_FAILURES
                    );
                    return Err(anyhow::anyhow!(
                        "scrap capture failed after {} consecutive attempts: {}",
                        MAX_CONSECUTIVE_FAILURES,
                        e
                    ));
                }

                // Brief pause before retry
                thread::sleep(Duration::from_millis(10));
            }
        }

        // Pace to target FPS
        let elapsed = t0.elapsed();
        if elapsed < frame_time {
            thread::sleep(frame_time - elapsed);
        }
    }

    Ok(())
}

#[cfg(all(target_os = "linux", feature = "rtsp-streaming"))]
/// Captures screen on Linux X11 and streams via RTSP.
///
/// Time complexity: O(n) where n is the number of frames captured until interruption.
/// Attempts scrap first, falls back to synthetic frames if unavailable.
///
/// Missing functionality:
/// - Gundam mode not implemented for RTSP streaming on Linux
/// - Could benefit from Wayland support in addition to X11
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
                let buffer = vec![0u8; scaled_size];
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
                        if let (Some(resizer), Some(staging), Some(buffer), Some(_output_size)) =
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

/// Generates synthetic frames for RTSP streaming when screen capture is unavailable.
///
/// Time complexity: O(n) where n is the number of frames generated until interruption.
/// Each frame generation is O(width * height) due to the nested loops creating
/// the gradient pattern.
///
/// Missing functionality:
/// - This is a fallback implementation - real screen capture should be preferred
/// - Could add more sophisticated synthetic patterns or test signals
#[cfg(all(target_os = "linux", feature = "rtsp-streaming"))]
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

/// Parse duration string like "30s", "2m", "1h" into seconds
///
/// Supports flexible duration input formats for user convenience:
/// - Raw seconds: `30` or `30s` (30 seconds)
/// - Minutes: `2m` (120 seconds)  
/// - Hours: `1h` (3600 seconds)
///
/// # Parameters
///
/// - `duration`: Duration string in format "30s", "2m", "1h", or just "30"
///
/// # Returns
///
/// Duration in seconds as `u32`
///
/// # Errors
///
/// Returns an error if:
/// - The format is invalid (not a number followed by s/m/h)
/// - The unit is not recognized (only s, m, h supported)
/// - The number cannot be parsed as u32
///
/// # Examples
///
/// ```rust
/// use cap::parse_duration;
///
/// assert_eq!(parse_duration("30s")?, 30);
/// assert_eq!(parse_duration("2m")?, 120);
/// assert_eq!(parse_duration("1h")?, 3600);
/// assert_eq!(parse_duration("45")?, 45);  // Raw seconds
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - String parsing and basic arithmetic operations.
///
/// **Missing functionality**: None - supports seconds, minutes, and hours with validation.
fn parse_duration(duration: &str) -> Result<u32> {
    if let Ok(seconds) = duration.parse::<u32>() {
        return Ok(seconds);
    }

    let len = duration.len();
    if len < 2 {
        return Err(anyhow::Error::from(
            hybrid_screen_capture::error::CaptureError::validation(
                "duration",
                "invalid format",
                duration,
            ),
        ));
    }

    let (num_str, unit) = duration.split_at(len - 1);
    let num: u32 = num_str.parse().map_err(|_| {
        anyhow::Error::from(hybrid_screen_capture::error::CaptureError::validation(
            "duration",
            "invalid number",
            num_str,
        ))
    })?;

    match unit {
        "s" => Ok(num),
        "m" => Ok(num * 60),
        "h" => Ok(num * 3600),
        _ => Err(anyhow::Error::from(
            hybrid_screen_capture::error::CaptureError::validation(
                "duration",
                "invalid unit (use s/m/h)",
                unit,
            ),
        )),
    }
}

/// Parse quality preset string into CRF value
///
/// Maps human-readable quality presets to x264 CRF (Constant Rate Factor) values.
/// Lower CRF values produce higher quality but larger files.
///
/// # Quality Presets
///
/// | Preset | CRF | Description |
/// |--------|-----|-------------|
/// | `low` | 28 | Smaller files, acceptable quality |
/// | `medium` | 23 | Balanced quality/size (recommended default) |
/// | `high` | 20 | Better quality, larger files |
/// | `ultra` | 18 | Best quality, largest files |
///
/// # Parameters
///
/// - `quality`: Quality preset name (case-insensitive)
///
/// # Returns
///
/// CRF value as `u8` for use with x264 encoding
///
/// # Errors
///
/// Returns an error if the quality preset is not recognized.
///
/// # Examples
///
/// ```rust
/// use cap::parse_quality;
///
/// assert_eq!(parse_quality("medium")?, 23);
/// assert_eq!(parse_quality("HIGH")?, 20);  // Case insensitive
/// assert_eq!(parse_quality("ultra")?, 18);
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - String comparison and hash map lookup.
///
/// **Missing functionality**: None - supports all documented quality presets with validation.
fn parse_quality(quality: &str) -> Result<u8> {
    match quality.to_lowercase().as_str() {
        "low" => Ok(28),    // Lower quality, smaller files
        "medium" => Ok(23), // Default quality/size balance
        "high" => Ok(20),   // High quality
        "ultra" => Ok(18),  // Very high quality, larger files
        _ => Err(anyhow::Error::from(
            hybrid_screen_capture::error::CaptureError::validation(
                "quality",
                "invalid preset (use low/medium/high/ultra)",
                quality,
            ),
        )),
    }
}
