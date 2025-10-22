// SPDX-License-Identifier: MIT
/// Simple geometry + preset logic for token-efficient scaling.
#[derive(Clone, Copy, Debug)]
pub struct Size { pub w: u32, pub h: u32 }

#[derive(Clone, Copy, Debug)]
pub enum AspectMode {
    /// Keep aspect; output size fits inside target box (no padding).
    Preserve,
    /// Stretch image to match target exactly (distorts aspect).
    Distort,
    /// Letterbox/pillarbox to exact target; preserves aspect, fills background.
    Pad { bg_rgba: [u8; 4] },
}

#[derive(Clone, Copy, Debug)]
pub enum ScaleTarget {
    /// Clamp the **longest** side to N (e.g., 640 or 512). The other side is derived.
    MaxLongSide(u32),
    /// Force an exact output canvas (use with Distort/Pad).
    Exact(Size),
}

#[derive(Clone, Copy, Debug)]
pub struct ScalePlan {
    pub input: Size,
    pub target: ScaleTarget,
    pub aspect: AspectMode,
    /// Final computed output dimensions.
    pub out: Size,
    /// If Pad, this is the sub-rect where the resized image is placed.
    pub dst_roi: Option<(u32, u32, u32, u32)>, // (x, y, w, h)
}

pub fn build_plan(input: Size, target: ScaleTarget, aspect: AspectMode) -> ScalePlan {
    match (target, aspect) {
        (ScaleTarget::MaxLongSide(max_side), AspectMode::Preserve) => {
            let (w, h) = fit_preserve(input, max_side);
            ScalePlan { input, target, aspect, out: Size { w, h }, dst_roi: None }
        }
        (ScaleTarget::MaxLongSide(max_side), AspectMode::Distort) => {
            let out = Size { w: max_side, h: max_side };
            ScalePlan { input, target, aspect, out, dst_roi: None }
        }
        (ScaleTarget::MaxLongSide(max_side), AspectMode::Pad { .. }) => {
            let out = Size { w: max_side, h: max_side }; // square canvas
            let (rw, rh) = fit_preserve(input, max_side);
            let x = (out.w - rw) / 2;
            let y = (out.h - rh) / 2;
            ScalePlan { input, target, aspect, out, dst_roi: Some((x, y, rw, rh)) }
        }
        (ScaleTarget::Exact(out), AspectMode::Distort) => {
            ScalePlan { input, target, aspect, out, dst_roi: None }
        }
        (ScaleTarget::Exact(out), AspectMode::Preserve) => {
            let (rw, rh) = fit_within(input, out);
            ScalePlan { input, target, aspect, out: Size { w: rw, h: rh }, dst_roi: None }
        }
        (ScaleTarget::Exact(out), AspectMode::Pad { .. }) => {
            let (rw, rh) = fit_within(input, out);
            let x = (out.w - rw) / 2;
            let y = (out.h - rh) / 2;
            ScalePlan { input, target, aspect, out, dst_roi: Some((x, y, rw, rh)) }
        }
    }
}

fn fit_preserve(input: Size, max_long: u32) -> (u32, u32) {
    let (w, h) = (input.w as f64, input.h as f64);
    let long = w.max(h);
    let s = (max_long as f64 / long).min(1.0); // don’t upscale
    (((w * s).round() as u32).max(1), ((h * s).round() as u32).max(1))
}

fn fit_within(input: Size, box_: Size) -> (u32, u32) {
    let (w, h) = (input.w as f64, input.h as f64);
    let (bw, bh) = (box_.w as f64, box_.h as f64);
    let s = (bw / w).min(bh / h).min(1.0);
    (((w * s).round() as u32).max(1), ((h * s).round() as u32).max(1))
}

/// The 5 recording "token-saver" presets from the plan.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum TokenPreset {
    /// 1024 → 640  ≈ 2.56× tokens saved
    #[clap(name = "p2_56")]
    P2_56_Long640,
    /// 1280 → 640  = 4×
    #[clap(name = "p4")]
    P4_Long640,
    /// 1344 → 512  ≈ 6.9×
    #[clap(name = "p6_9")]
    P6_9_Long512,
    /// 1920 → 640  = 9×
    #[clap(name = "p9")]
    P9_Long640,
    /// 2048 → 640  ≈ 10.24×
    #[clap(name = "p10_24")]
    P10_24_Long640,
}

impl TokenPreset {
    pub fn to_target(self) -> ScaleTarget {
        match self {
            TokenPreset::P6_9_Long512 => ScaleTarget::MaxLongSide(512),
            _ => ScaleTarget::MaxLongSide(640),
        }
    }
}
