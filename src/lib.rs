//! # Hybrid Screen Capture Library
//!
//! A high-performance, cross-platform screen capture library with advanced
//! processing capabilities for real-time streaming and recording.
//!
//! ## Architecture
//!
//! The library is organized into several key modules:
//! - `capture`: Platform-specific screen capture implementations
//! - `processing`: Frame processing pipeline with scaling and tiling
//! - `core`: Low-level utilities like buffer pools and ring buffers
//! - `config`: Configuration management and validation
//! - `session`: High-level session orchestration
//!
//! ## Features
//!
//! - **Zero-copy processing**: Frames flow through the pipeline without copying
//! - **Cross-platform**: Supports Windows, macOS, and Linux
//! - **Real-time streaming**: RTSP streaming with H.264 encoding
//! - **Advanced processing**: Token-efficient scaling and OCR-optimized tiling
//! - **Async/await**: Built on Tokio for high concurrency
//!
//! ## Example
//!
//! ```rust,no_run
//! use hybrid_screen_capture::capture_screen;
//! use hybrid_screen_capture::CaptureOptions;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let options = CaptureOptions {
//!     output: "output.mp4".to_string(),
//!     fps: 30,
//!     seconds: 10,
//!     crf: 23,
//!     window: false,
//!     scale_preset: None,
//!     gundam_mode: false,
//! };
//!
//! capture_screen(options).await?;
//! # Ok(())
//! # }
//! ```

// Standard library imports

// External crate imports
use anyhow::{Result, anyhow};

// Internal module imports
pub mod capture;
pub mod config;
pub mod core;
pub mod error;
pub mod processing;
pub mod session;

/// Re-export error types for convenience
pub use error::{
    CaptureError, CaptureResult, HasRecoverySuggestion, HasSeverity, Recoverable, Retryable,
};

/// Re-export commonly used types from dependencies
#[cfg(feature = "rtsp-streaming")]
pub use cap_rtsp::BgraFrame;

/// Configuration options for screen capture operations.
///
/// This struct encapsulates all parameters needed to configure a screen capture session,
/// including output settings, quality parameters, and capture mode selection.
///
/// # Examples
///
/// ```rust
/// use hybrid_screen_capture::CaptureOptions;
///
/// let options = CaptureOptions {
///     output: "output.mp4".to_string(),
///     fps: 60,
///     seconds: 30,
///     crf: 18,  // High quality
///     window: false,  // Full screen capture
///     scale_preset: None,
///     gundam_mode: false,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// Output file path for the captured video.
    ///
    /// Supports any format supported by FFmpeg (MP4, AVI, MOV, etc.).
    /// The file extension determines the container format.
    pub output: String,

    /// Target frames per second for the capture.
    ///
    /// Common values: 30, 60, 120. Higher values require more CPU
    /// and produce larger files. Must be greater than 0.
    pub fps: u32,

    /// Duration of the capture in seconds.
    ///
    /// The capture will run for exactly this many seconds, then stop.
    /// Must be greater than 0.
    pub seconds: u32,

    /// Constant Rate Factor for x264/x265 encoding.
    ///
    /// Lower values = higher quality but larger files.
    /// Recommended range: 18-28 (sane range for x264).
    /// - 18: Visually lossless
    /// - 23: Default (good quality/size balance)
    /// - 28: Smaller files, lower quality
    pub crf: u8,

    /// Whether to capture a specific window instead of the full screen.
    ///
    /// When `true`, the user will be prompted to select a window to capture.
    /// When `false`, captures the primary display.
    ///
    /// Note: Window capture is not supported on Linux (will fall back to full screen).
    pub window: bool,

    /// Optional scaling preset for token-efficient VLM input.
    ///
    /// When set, captured frames will be scaled down to reduce token usage
    /// while maintaining visual quality. Uses aspect-preserving scaling.
    pub scale_preset: Option<cap_scale::presets::TokenPreset>,

    /// Whether to enable DeepSeek-OCR Gundam tiling mode.
    ///
    /// When enabled, produces n×640×640 tiles + 1×1024×1024 global view
    /// exactly matching DeepSeek-OCR's input requirements.
    pub gundam_mode: bool,
}

/// Dispatch capture to the appropriate platform-specific implementation.
///
/// This internal function routes the capture request to the correct backend
/// based on compile-time platform detection. It uses conditional compilation
/// to ensure only supported platforms are targeted.
///
/// # Parameters
///
/// * `options` - Capture configuration to pass to the platform backend.
///
/// # Returns
///
/// Result from the platform-specific capture implementation.
///
/// # Errors
///
/// Returns an error if the target platform is not supported or if the
/// platform-specific implementation fails.
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - Simple compile-time platform detection and delegation.
///
/// **Missing functionality**: None - handles all supported platforms with appropriate
/// error messages for unsupported ones.
async fn dispatch_to_platform(options: CaptureOptions) -> Result<()> {
    #[cfg(target_os = "linux")]
    return dispatch_linux(options).await;

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    return dispatch_desktop(options).await;

    #[allow(unreachable_code)]
    Err(anyhow!("Unsupported OS"))
}

/// Dispatch Linux capture based on session type (Wayland vs X11).
///
/// Linux systems can run either X11 or Wayland sessions, each requiring
/// different capture approaches. This function detects the session type
/// and routes to the appropriate backend.
///
/// The detection logic:
/// 1. Checks XDG_SESSION_TYPE environment variable
/// 2. Routes to Wayland if "wayland" (case-insensitive)
/// 3. Falls back to X11 for all other values
///
/// # Parameters
///
/// * `options` - Capture configuration to pass to the Linux backend.
///
/// # Returns
///
/// Result from the Linux-specific capture implementation.
///
/// # Errors
///
/// Returns an error if neither Wayland nor X11 backends are available
/// or if the selected backend fails.
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - Environment variable check and delegation.
///
/// **Missing functionality**: None - properly detects Wayland vs X11 and routes
/// to appropriate backend with fallback options.
#[cfg(target_os = "linux")]
async fn dispatch_linux(options: CaptureOptions) -> Result<()> {
    if is_wayland_session() {
        dispatch_wayland(options).await
    } else {
        dispatch_x11(options).await
    }
}

/// Dispatch to Wayland capture implementation.
///
/// Wayland requires special handling due to its security model. This function
/// attempts to use the native Wayland implementation if available, with
/// fallback logic for when the required features are not enabled.
///
/// The priority order:
/// 1. Use `wayland-pipe` feature (Portal + PipeWire + GStreamer) - preferred
/// 2. Fall back to `screen-capture` feature (scrap + FFmpeg) - may not work
/// 3. Return error if neither feature is available
///
/// # Parameters
///
/// * `options` - Capture configuration to pass to the Wayland backend.
///
/// # Returns
///
/// Result from the Wayland capture implementation.
///
/// # Errors
///
/// Returns an error if no suitable Wayland capture backend is available.
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - Feature-gated delegation with fallback logic.
///
/// **Missing functionality**: Could add more sophisticated fallback detection,
/// but current implementation provides clear error messages and fallbacks.
#[cfg(target_os = "linux")]
async fn dispatch_wayland(options: CaptureOptions) -> Result<()> {
    #[cfg(feature = "wayland-pipe")]
    {
        println!("Detected Wayland session → using Portal + PipeWire (ashpd) + GStreamer …");
        return capture::wayland::capture_gstreamer(&options).await;
    }
    #[cfg(not(feature = "wayland-pipe"))]
    {
        eprintln!(
            "Wayland session detected but 'wayland-pipe' feature is disabled. \
Enable it with: cargo run --release --features wayland-pipe\n\
Note: This requires GStreamer + dev headers (see README). Falling back to scrap + FFmpeg, which may not work under Wayland."
        );
        #[cfg(feature = "screen-capture")]
        return capture::scrap::capture_ffmpeg(options).await;
        #[cfg(not(feature = "screen-capture"))]
        return Err(anyhow!(
            "Screen capture not available - enable with: cargo run --features screen-capture"
        ));
    }
}

/// Dispatch to X11 capture implementation.
///
/// X11 sessions use the traditional Linux desktop environment. This function
/// delegates to the scrap-based capture implementation, which works reliably
/// on X11 systems.
///
/// The implementation uses:
/// - scrap library for screen capture
/// - FFmpeg for video encoding
/// - Standard X11 APIs for display access
///
/// # Parameters
///
/// * `options` - Capture configuration to pass to the X11 backend.
///
/// # Returns
///
/// Result from the X11 capture implementation.
///
/// # Errors
///
/// Returns an error if the screen-capture feature is not enabled or if
/// the capture operation fails.
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - Simple delegation to scrap-based capture.
///
/// **Missing functionality**: None - uses established scrap + FFmpeg backend for X11.
#[cfg(target_os = "linux")]
async fn dispatch_x11(options: CaptureOptions) -> Result<()> {
    println!("Detected X11 session → using scrap + FFmpeg …");
    #[cfg(feature = "screen-capture")]
    return capture::scrap::capture_ffmpeg(options).await;
    #[cfg(not(feature = "screen-capture"))]
    return Err(anyhow!(
        "Screen capture not available - enable with: cargo run --features screen-capture"
    ));
}

/// Dispatch to Windows/macOS capture implementation.
///
/// Desktop platforms (Windows and macOS) use the scrap library for screen
/// capture with FFmpeg encoding. This provides consistent behavior across
/// both platforms with a single implementation.
///
/// The implementation uses:
/// - scrap library for cross-platform screen capture
/// - FFmpeg for video encoding and file output
/// - Platform-specific APIs for display access
///
/// # Parameters
///
/// * `options` - Capture configuration to pass to the desktop backend.
///
/// # Returns
///
/// Result from the desktop capture implementation.
///
/// # Errors
///
/// Returns an error if the screen-capture feature is not enabled or if
/// the capture operation fails.
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - Simple delegation to scrap-based capture.
///
/// **Missing functionality**: None - uses established scrap + FFmpeg backend for desktop platforms.
#[cfg(any(target_os = "windows", target_os = "macos"))]
async fn dispatch_desktop(options: CaptureOptions) -> Result<()> {
    println!("Using scrap + FFmpeg …");
    #[cfg(feature = "screen-capture")]
    return capture::scrap::capture_ffmpeg(options).await;
    #[cfg(not(feature = "screen-capture"))]
    return Err(anyhow!(
        "Screen capture not available - enable with: cargo run --features screen-capture"
    ));
}

/// Returns true if XDG_SESSION_TYPE indicates 'wayland'.
///
/// This function checks the standard Linux environment variable that indicates
/// the current desktop session type. Wayland sessions require different capture
/// approaches than X11 sessions due to Wayland's security model.
///
/// The check is case-insensitive and returns false if the environment variable
/// is not set (defaults to assuming X11).
///
/// # Returns
///
/// `true` if the session type is "wayland", `false` otherwise.
///
/// # Examples
///
/// ```rust
/// #[cfg(target_os = "linux")]
/// {
///     // On a Wayland system
///     unsafe { std::env::set_var("XDG_SESSION_TYPE", "wayland"); }
///     assert!(hybrid_screen_capture::is_wayland_session());
///
///     // On an X11 system
///     unsafe { std::env::set_var("XDG_SESSION_TYPE", "x11"); }
///     assert!(!hybrid_screen_capture::is_wayland_session());
/// }
/// ```
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) - Simple environment variable lookup and string comparison.
///
/// **Missing functionality**: Could check additional indicators like WAYLAND_DISPLAY,
/// but XDG_SESSION_TYPE is the standard way to detect session type.
#[cfg(target_os = "linux")]
pub fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
}

/// Main entry point for screen capture operations.
///
/// This is the primary API function that initiates screen capture with the specified
/// options. It handles platform detection, feature validation, and delegates to
/// the appropriate capture backend based on the target platform and available features.
///
/// The function performs several key operations:
/// 1. Validates the target platform (rejects WASM builds)
/// 2. Logs capture parameters for user feedback
/// 3. Dispatches to platform-specific capture implementation
/// 4. Returns success or detailed error information
///
/// # Parameters
///
/// * `options` - Configuration specifying output path, quality settings, duration, etc.
///
/// # Returns
///
/// `Ok(())` if capture completes successfully, or an error describing what failed.
///
/// # Errors
///
/// Returns an error if:
/// - Running in unsupported environment (WASM)
/// - Platform-specific capture backend fails
/// - Required features are not enabled
/// - Output path is invalid or inaccessible
///
/// # Examples
///
/// ```rust,no_run
/// use hybrid_screen_capture::{CaptureOptions, capture_screen};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let options = CaptureOptions {
///         output: "recording.mp4".to_string(),
///         fps: 30,
///         seconds: 60,
///         crf: 23,
///         window: false,
///         scale_preset: None,
///         gundam_mode: false,
///     };
///
///     capture_screen(options).await?;
///     println!("Screen capture completed successfully!");
///     Ok(())
/// }
/// ```
///
/// # Platform Support
///
/// - **Windows/macOS**: Uses scrap library with FFmpeg encoding
/// - **Linux X11**: Uses scrap library with FFmpeg encoding
/// - **Linux Wayland**: Uses XDG Portal + PipeWire + GStreamer (requires `wayland-pipe` feature)
/// - **WASM**: Not supported (returns error)
///
/// # Performance Characteristics
///
/// **Time complexity**: O(1) for setup and dispatch, but the actual capture operation
/// runs for O(seconds * fps) time. The dispatch itself is O(1) - just platform
/// detection and delegation to appropriate backend.
///
/// **Missing functionality**: None - fully implements platform detection and routing
/// to appropriate capture backends with proper feature gating.
pub async fn capture_screen(options: CaptureOptions) -> Result<()> {
    // WASM builds cannot capture screens - this is a configurator only
    #[cfg(target_arch = "wasm32")]
    {
        return Err(anyhow!(
            "Screen capture is not available in web browsers. Use the generated CLI command instead."
        ));
    }

    println!("Output: {}", options.output);
    println!(
        "FPS: {}, Duration: {}s, CRF: {}",
        options.fps, options.seconds, options.crf
    );

    dispatch_to_platform(options).await
}
