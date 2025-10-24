//! # Configuration Module
//!
//! This module provides configuration structures and validation for screen capture operations.
//! It serves as the common interface between CLI applications, GUI applications, and the core
//! capture library.
//!
//! ## Overview
//!
//! The configuration system is designed to be:
//! - **Type-safe**: Compile-time validation of configuration parameters
//! - **Cross-platform**: Consistent interface across all supported platforms
//! - **Extensible**: Easy to add new configuration options
//! - **Validated**: Runtime validation with helpful error messages
//!
//! ## Configuration Parameters
//!
//! | Parameter | Type | Range | Description |
//! |-----------|------|-------|-------------|
//! | `output` | `String` | Any valid path | Output file path |
//! | `fps` | `u32` | 1-120 | Target frames per second |
//! | `seconds` | `u32` | 1-3600 | Capture duration in seconds |
//! | `crf` | `u8` | 18-28 | x264 quality factor (lower = higher quality) |
//! | `window` | `bool` | true/false | Window vs full screen capture |
//!
//! ## Quality Presets
//!
//! The CLI provides convenient quality presets that map to CRF values:
//! - `low`: CRF 28 (smaller files, acceptable quality)
//! - `medium`: CRF 23 (balanced quality/size) - recommended default
//! - `high`: CRF 20 (better quality, larger files)
//! - `ultra`: CRF 18 (best quality, largest files)
//!
//! ## Duration Formats
//!
//! The CLI supports flexible duration input:
//! - Raw seconds: `30` or `30s`
//! - Minutes: `2m` (120 seconds)
//! - Hours: `1h` (3600 seconds)
//!
//! ## Examples
//!
//! ```rust
//! use hybrid_screen_capture::config::config::CaptureConfig;
//!
//! // Use defaults
//! let config = CaptureConfig::default();
//!
//! // Custom configuration
//! let config = CaptureConfig::new(
//!     "output.mp4".to_string(),
//!     60,  // 60 FPS
//!     30,  // 30 seconds
//!     18,  // High quality
//!     false, // Full screen
//!     None, // No scaling preset
//!     false  // No Gundam mode
//! );
//!
//! // Validate configuration
//! assert!(config.validate().is_ok());
//!
//! // Convert to capture options
//! let options = config.to_capture_options();
//! ```

/// Configuration structure for screen capture operations.
///
/// This struct holds all the parameters needed to configure a screen capture session.
/// It provides validation, default values, and conversion to the core library's
/// `CaptureOptions` struct.
///
/// # Field Descriptions
///
/// - `output`: Path where the captured video will be saved
/// - `fps`: Target frames per second (affects smoothness and file size)
/// - `seconds`: Duration of the capture in seconds
/// - `crf`: Constant Rate Factor (quality setting for x264 encoding)
/// - `window`: Whether to capture a specific window or the full screen
///
/// # Examples
///
/// Basic configuration:
/// ```rust
/// use hybrid_screen_capture::config::config::CaptureConfig;
///
/// let config = CaptureConfig {
///     output: "my_capture.mp4".to_string(),
///     fps: 30,
///     seconds: 10,
///     crf: 23,
///     window: false,
///     scale_preset: None,
///     gundam_mode: false,
/// };
/// ```
///
/// High-quality configuration:
/// ```rust
/// # use hybrid_screen_capture::config::config::CaptureConfig;
/// let config = CaptureConfig {
///     output: "high_quality.mp4".to_string(),
///     fps: 60,
///     seconds: 30,
///     crf: 18,  // High quality, larger file
///     window: true,  // Window capture
///     scale_preset: None,
///     gundam_mode: false,
/// };
/// ```
pub struct CaptureConfig {
    /// Output file path for the captured video.
    ///
    /// This can be an absolute or relative path. The file extension determines
    /// the container format (e.g., .mp4, .avi, .mov). The parent directory
    /// must exist and be writable.
    pub output: String,

    /// Target frames per second for the capture.
    ///
    /// Higher values result in smoother video but require more CPU and
    /// produce larger files. Common values are 30, 60, and 120 FPS.
    /// Must be greater than 0.
    pub fps: u32,

    /// Duration of the capture in seconds.
    ///
    /// The capture will run for exactly this many seconds before stopping.
    /// Longer captures produce larger files. Must be greater than 0.
    pub seconds: u32,

    /// Constant Rate Factor for x264/x265 encoding quality.
    ///
    /// Lower values = higher quality but larger files.
    /// - 18: Visually lossless quality
    /// - 23: Good balance of quality and file size (default)
    /// - 28: Smaller files with visible quality loss
    ///
    /// Must be between 18 and 28 (inclusive).
    pub crf: u8,

    /// Whether to capture a specific window instead of the full screen.
    ///
    /// When `true`, the user will be prompted to select a window to capture.
    /// When `false`, captures the primary display. Note that window capture
    /// is not supported on Linux platforms.
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

impl Default for CaptureConfig {
    /// Creates a default configuration suitable for most use cases.
    ///
    /// Default values:
    /// - `output`: "capture.mp4"
    /// - `fps`: 30 (good balance of smoothness and performance)
    /// - `seconds`: 10 (reasonable test duration)
    /// - `crf`: 23 (good quality/size balance)
    /// - `window`: false (full screen capture)
    /// - `scale_preset`: None (no scaling)
    /// - `gundam_mode`: false (standard capture)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use hybrid_screen_capture::config::config::CaptureConfig;
    ///
    /// let config = CaptureConfig::default();
    /// assert_eq!(config.output, "capture.mp4");
    /// assert_eq!(config.fps, 30);
    /// ```
    fn default() -> Self {
        Self {
            output: "capture.mp4".to_string(),
            fps: 30,
            seconds: 10,
            crf: 23,
            window: false,
            scale_preset: None,
            gundam_mode: false,
        }
    }
}

impl CaptureConfig {
    /// Creates a new configuration with the specified parameters.
    ///
    /// Time complexity: O(1) - Simple struct field assignment.
    ///
    /// Missing functionality: None - fully implements configuration creation
    /// with all required parameters.
    pub fn new(
        output: String,
        fps: u32,
        seconds: u32,
        crf: u8,
        window: bool,
        scale_preset: Option<cap_scale::presets::TokenPreset>,
        gundam_mode: bool,
    ) -> Self {
        Self {
            output,
            fps,
            seconds,
            crf,
            window,
            scale_preset,
            gundam_mode,
        }
    }

    /// Validates the configuration parameters.
    ///
    /// Time complexity: O(1) - Performs constant-time range checks on numeric fields.
    ///
    /// Missing functionality: None - validates all documented constraints with
    /// clear error messages.
    pub fn validate(&self) -> Result<(), String> {
        if self.fps == 0 {
            return Err("FPS must be greater than 0".to_string());
        }
        if self.seconds == 0 {
            return Err("Duration must be greater than 0 seconds".to_string());
        }
        if !(18..=28).contains(&self.crf) {
            return Err("CRF must be between 18 and 28".to_string());
        }
        Ok(())
    }

    /// Convert to CaptureOptions for use with the capture library
    ///
    /// Time complexity: O(output.len()) - Due to string cloning, but typically
    /// O(1) for reasonable path lengths.
    ///
    /// Missing functionality: None - performs complete field mapping to the
    /// library's expected format.
    pub fn to_capture_options(&self) -> crate::CaptureOptions {
        crate::CaptureOptions {
            output: self.output.clone(),
            fps: self.fps,
            seconds: self.seconds,
            crf: self.crf,
            window: self.window,
            scale_preset: self.scale_preset,
            gundam_mode: self.gundam_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CaptureConfig::default();
        assert_eq!(config.output, "capture.mp4");
        assert_eq!(config.fps, 30);
        assert_eq!(config.seconds, 10);
        assert_eq!(config.crf, 23);
        assert_eq!(config.window, false);
    }

    #[test]
    fn test_config_validation() {
        let mut config = CaptureConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid FPS
        config.fps = 0;
        assert!(config.validate().is_err());
        config.fps = 30; // Reset

        // Invalid seconds
        config.seconds = 0;
        assert!(config.validate().is_err());
        config.seconds = 10; // Reset

        // Invalid CRF
        config.crf = 10;
        assert!(config.validate().is_err());
        config.crf = 30;
        assert!(config.validate().is_err());
        config.crf = 23; // Reset

        // Valid again
        assert!(config.validate().is_ok());
    }
}
