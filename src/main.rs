use anyhow::Result;
use clap::Parser;
use hybrid_screen_capture::config::CaptureConfig;

/// Minimal, human-friendly hybrid screen capture:
/// - Windows/macOS/X11: scrap + ffmpeg (subprocess)
/// - Wayland: xdg-desktop-portal + pipewire via ashpd + GStreamer pipeline
#[derive(Parser, Debug)]
#[command(name = "cap")]
#[command(about = "🎥 Record your screen to MP4 with automatic backend selection")]
#[command(long_about = "Record your screen to MP4 with automatic backend selection based on your platform.
Supports multiple quality presets and flexible duration formats for easy screen recording.")]
struct Args {
    /// Output MP4 file path (positional or use -o)
    #[arg(default_value = "capture.mp4", help = "Output file path (MP4 format)")]
    output: String,

    /// Output MP4 path
    #[arg(short, long, help = "Alternative way to specify output file")]
    output_flag: Option<String>,

    /// Recording duration (supports seconds, minutes, hours)
    #[arg(short, long, default_value = "10s",
          help = "How long to record: 30s (30 seconds), 2m (2 minutes), 1h (1 hour)")]
    duration: String,

    /// Video quality preset
    #[arg(short, long, default_value = "medium",
          help = "Video quality preset: low (small files), medium (balanced), high (better quality), ultra (best quality)")]
    quality: String,

    /// Frames per second
    #[arg(short = 'f', long, default_value_t = 30,
          help = "Frames per second (higher = smoother but larger files)")]
    fps: u32,

    /// Capture a specific window instead of full screen
    #[arg(long, help = "Capture a specific window instead of the entire screen")]
    window: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

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
    );

    config.validate().map_err(anyhow::Error::msg)?;
    let options = config.to_capture_options();
    hybrid_screen_capture::capture_screen(options).await
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
    let num: u32 = num_str.parse().map_err(|_| anyhow::anyhow!("Invalid number in duration: {}", num_str))?;

    match unit {
        "s" => Ok(num),
        "m" => Ok(num * 60),
        "h" => Ok(num * 3600),
        _ => Err(anyhow::anyhow!("Invalid duration unit: {}. Use 's' for seconds, 'm' for minutes, 'h' for hours", unit)),
    }
}

/// Parse quality preset into CRF value
fn parse_quality(quality: &str) -> Result<u8> {
    match quality.to_lowercase().as_str() {
        "low" => Ok(28),      // Lower quality, smaller files
        "medium" => Ok(23),   // Default quality/size balance
        "high" => Ok(20),     // High quality
        "ultra" => Ok(18),    // Very high quality, larger files
        _ => Err(anyhow::anyhow!("Invalid quality preset: {}. Use: low, medium, high, ultra", quality)),
    }
}
