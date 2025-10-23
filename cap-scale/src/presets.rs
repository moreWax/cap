// SPDX-License-Identifier: MIT
//! # Scaling Presets and Plan Computation
//!
//! This module provides the core logic for computing scaling plans and token-efficient presets.
//! It implements aspect-ratio preserving scaling with multiple output strategies optimized for VLM input.
//!
//! ## Design Philosophy
//!
//! The scaling system is designed around three key concepts:
//! 1. **ScaleTarget**: What size constraint to apply (max side length vs exact dimensions)
//! 2. **AspectMode**: How to handle aspect ratio differences (preserve, distort, or pad)
//! 3. **ScalePlan**: The computed output parameters and ROI for actual scaling
//!
//! ## Token Efficiency Strategy
//!
//! VLM token usage scales with image pixel count, but OCR accuracy depends more on longest dimension.
//! Our presets clamp the longest side to efficient values while preserving aspect ratio:
//!
//! - **640px max**: Good balance for most content (P2_56, P4, P9, P10_24)
//! - **512px max**: Higher compression for dense text (P6_9)
//!
//! ## Performance Considerations
//!
//! - All computations use floating-point for precision but round to integers
//! - No upscaling: images smaller than target are left unchanged
//! - Clamp to minimum 1px to prevent division by zero
//!
//! ## Future Optimizations
//!
//! TODO: Consider caching ScalePlan objects for repeated same-size inputs to avoid recomputation.
//! TODO: Add support for non-square padding (letterbox vs pillarbox based on content analysis).

/// Represents a 2D size with width and height in pixels.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

/// Defines how aspect ratio differences are handled during scaling.
#[derive(Clone, Copy, Debug)]
pub enum AspectMode {
    /// Keep original aspect ratio; output fits entirely within target bounds.
    /// This is the recommended mode for VLM input to preserve content proportions.
    Preserve,
    /// Stretch/squeeze image to exactly match target dimensions.
    /// Distorts aspect ratio - use only when exact dimensions are required.
    Distort,
    /// Add padding to match exact target dimensions while preserving aspect ratio.
    /// Useful for creating consistent input sizes across varying source aspect ratios.
    Pad { bg_rgba: [u8; 4] },
}

/// Defines the target size constraint for scaling operations.
#[derive(Clone, Copy, Debug)]
pub enum ScaleTarget {
    /// Clamp the longest side to a maximum value, derive the other side proportionally.
    /// This is the primary mode for token-efficient VLM scaling.
    MaxLongSide(u32),
    /// Force output to exact dimensions (used with AspectMode::Distort/Pad).
    /// Less common for VLM input but useful for fixed-size model requirements.
    Exact(Size),
}

/// Complete scaling plan computed from input parameters.
/// Contains all information needed to perform the actual scaling operation.
#[derive(Clone, Copy, Debug)]
pub struct ScalePlan {
    /// Original input dimensions
    pub input: Size,
    /// Target size constraint used for planning
    pub target: ScaleTarget,
    /// Aspect ratio handling strategy
    pub aspect: AspectMode,
    /// Final computed output dimensions
    pub out: Size,
    /// If padding is used, specifies the sub-rectangle where scaled content is placed.
    /// Format: (x, y, width, height) in output coordinate space.
    pub dst_roi: Option<(u32, u32, u32, u32)>,
}

/// Compute a complete scaling plan from input parameters.
///
/// This function implements the core scaling logic, determining output dimensions
/// and ROI placement based on the chosen target and aspect mode strategy.
///
/// # Arguments
/// * `input` - Source image dimensions
/// * `target` - Size constraint to apply
/// * `aspect` - How to handle aspect ratio differences
///
/// # Returns
/// A ScalePlan containing all parameters needed for scaling execution
///
/// # Performance
/// O(1) computation with minimal floating-point operations
pub fn build_plan(input: Size, target: ScaleTarget, aspect: AspectMode) -> ScalePlan {
    match (target, aspect) {
        (ScaleTarget::MaxLongSide(max_side), AspectMode::Preserve) => {
            let (w, h) = fit_preserve(input, max_side);
            ScalePlan {
                input,
                target,
                aspect,
                out: Size { w, h },
                dst_roi: None,
            }
        }
        (ScaleTarget::MaxLongSide(max_side), AspectMode::Distort) => {
            let out = Size {
                w: max_side,
                h: max_side,
            };
            ScalePlan {
                input,
                target,
                aspect,
                out,
                dst_roi: None,
            }
        }
        (ScaleTarget::MaxLongSide(max_side), AspectMode::Pad { .. }) => {
            let out = Size {
                w: max_side,
                h: max_side,
            }; // square canvas
            let (rw, rh) = fit_preserve(input, max_side);
            let x = (out.w - rw) / 2;
            let y = (out.h - rh) / 2;
            ScalePlan {
                input,
                target,
                aspect,
                out,
                dst_roi: Some((x, y, rw, rh)),
            }
        }
        (ScaleTarget::Exact(out), AspectMode::Distort) => ScalePlan {
            input,
            target,
            aspect,
            out,
            dst_roi: None,
        },
        (ScaleTarget::Exact(out), AspectMode::Preserve) => {
            let (rw, rh) = fit_within(input, out);
            ScalePlan {
                input,
                target,
                aspect,
                out: Size { w: rw, h: rh },
                dst_roi: None,
            }
        }
        (ScaleTarget::Exact(out), AspectMode::Pad { .. }) => {
            let (rw, rh) = fit_within(input, out);
            let x = (out.w - rw) / 2;
            let y = (out.h - rh) / 2;
            ScalePlan {
                input,
                target,
                aspect,
                out,
                dst_roi: Some((x, y, rw, rh)),
            }
        }
    }
}

/// Fit image within max_side constraint while preserving aspect ratio.
/// Returns (width, height) that fit within max_side on longest dimension.
///
/// This implements the core token-saving logic: clamp longest side, scale proportionally.
/// Never upscales - returns original dimensions if already smaller than max_side.
fn fit_preserve(input: Size, max_long: u32) -> (u32, u32) {
    let (w, h) = (input.w as f64, input.h as f64);
    let long = w.max(h);
    let s = (max_long as f64 / long).min(1.0); // don't upscale
    (
        ((w * s).round() as u32).max(1),
        ((h * s).round() as u32).max(1),
    )
}

/// Fit image within a bounding box while preserving aspect ratio.
/// Returns (width, height) that fit entirely within the box.
///
/// Used for exact target sizing with aspect preservation.
fn fit_within(input: Size, box_: Size) -> (u32, u32) {
    let (w, h) = (input.w as f64, input.h as f64);
    let (bw, bh) = (box_.w as f64, box_.h as f64);
    let s = (bw / w).min(bh / h).min(1.0);
    (
        ((w * s).round() as u32).max(1),
        ((h * s).round() as u32).max(1),
    )
}

/// Token-efficient scaling presets optimized for Vision Language Models.
///
/// These presets are designed based on empirical testing of token usage vs OCR accuracy.
/// Each preset clamps the longest image side to an efficient value, reducing tokens
/// while preserving enough resolution for accurate text recognition.
///
/// The naming convention (P2_56, P4, etc.) indicates the approximate token reduction factor.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TokenPreset {
    /// 1024px → 640px longest side ≈ 2.56× token reduction
    /// Good for high-quality content that needs minimal compression
    #[clap(name = "p2_56")]
    P2_56_Long640,
    /// 1280px → 640px longest side = 4× token reduction
    /// Balanced preset for most screen capture content
    #[clap(name = "p4")]
    P4_Long640,
    /// 1344px → 512px longest side ≈ 6.9× token reduction
    /// Higher compression, uses 512px max for dense text
    #[clap(name = "p6_9")]
    P6_9_Long512,
    /// 1920px → 640px longest side = 9× token reduction
    /// Aggressive compression for simple content
    #[clap(name = "p9")]
    P9_Long640,
    /// 2048px → 640px longest side ≈ 10.24× token reduction
    /// Maximum compression for basic OCR tasks
    #[clap(name = "p10_24")]
    P10_24_Long640,
}

impl TokenPreset {
    /// Convert preset to the corresponding ScaleTarget for plan computation.
    ///
    /// Most presets use 640px max side, but P6_9 uses 512px for higher compression.
    pub fn to_target(self) -> ScaleTarget {
        match self {
            TokenPreset::P6_9_Long512 => ScaleTarget::MaxLongSide(512),
            _ => ScaleTarget::MaxLongSide(640),
        }
    }
}
