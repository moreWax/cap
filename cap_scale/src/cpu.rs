// SPDX-License-Identifier: MIT
// CPU scaler built on fast_image_resize (SIMD-accelerated).
// BGRA8 in → BGRA8 out, direct write into caller-provided dst buffer.

use fast_image_resize as fir;
use fir::{Resizer, ResizeOptions};
use fir::images::{TypedImage, TypedImageRef, TypedCroppedImageMut};
use fir::pixels::U8x4;

use crate::presets::{AspectMode, ScalePlan, Size};

#[derive(Debug)]
pub enum ScaleError {
    BufferTooSmall,
    StrideMismatchAndNoStaging,
    Fir(fir::ResizeError),
    ImageBuf(fir::ImageBufferError),
    Crop(fir::CropBoxError),
}

impl From<fir::ResizeError> for ScaleError { fn from(e: fir::ResizeError) -> Self { Self::Fir(e) } }
impl From<fir::ImageBufferError> for ScaleError { fn from(e: fir::ImageBufferError) -> Self { Self::ImageBuf(e) } }
impl From<fir::CropBoxError> for ScaleError { fn from(e: fir::CropBoxError) -> Self { Self::Crop(e) } }

impl std::fmt::Display for ScaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScaleError::BufferTooSmall => write!(f, "Output buffer too small"),
            ScaleError::StrideMismatchAndNoStaging => write!(f, "Stride mismatch but no staging buffer provided"),
            ScaleError::Fir(e) => write!(f, "Fast image resize error: {}", e),
            ScaleError::ImageBuf(e) => write!(f, "Image buffer error: {}", e),
            ScaleError::Crop(e) => write!(f, "Crop error: {}", e),
        }
    }
}

impl std::error::Error for ScaleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScaleError::Fir(e) => Some(e),
            ScaleError::ImageBuf(e) => Some(e),
            ScaleError::Crop(e) => Some(e),
            _ => None,
        }
    }
}

/// Pre-allocated scratch to compact strided input to tightly packed rows (only if needed).
pub struct Staging {
    pub(crate) buf: Vec<u8>,
}
impl Staging {
    pub fn with_capacity(cap: usize) -> Self { Self { buf: Vec::with_capacity(cap) } }
    pub fn ensure_len(&mut self, len: usize) { if self.buf.len() < len { self.buf.resize(len, 0); } }
    pub fn as_slice(&self) -> &[u8] { &self.buf }
}

/// Main scaling entry point.
/// `src_stride_bytes`: bytes per row of source. If `Some(stride) != width*4`, we compact per-row into staging.
/// `dst` must be exactly `plan.out.w * plan.out.h * 4` bytes (BGRA).
pub fn scale_bgra_cpu(
    resizer: &mut Resizer,
    src_bgra: &[u8],
    src: Size,
    src_stride_bytes: Option<usize>,
    plan: &ScalePlan,
    dst: &mut [u8],
    mut staging: Option<&mut Staging>,
) -> Result<(), ScaleError> {
    let dst_len = (plan.out.w as usize) * (plan.out.h as usize) * 4;
    if dst.len() < dst_len {
        return Err(ScaleError::BufferTooSmall);
    }

    // --- Build source view (tightly packed) ---
    let tight_row_bytes = (src.w as usize) * 4;
    let src_view: TypedImageRef<U8x4>;
    if let Some(pitch) = src_stride_bytes {
        if pitch == tight_row_bytes {
            src_view = TypedImageRef::<U8x4>::from_buffer(src.w, src.h, src_bgra)?;
        } else {
            let st = staging.as_deref_mut().ok_or(ScaleError::StrideMismatchAndNoStaging)?;
            st.ensure_len(tight_row_bytes * (src.h as usize));
            compact_rows(src_bgra, pitch, st.buf.as_mut_slice(), tight_row_bytes, src.h as usize);
            src_view = TypedImageRef::<U8x4>::from_buffer(src.w, src.h, st.as_slice())?;
        }
    } else {
        src_view = TypedImageRef::<U8x4>::from_buffer(src.w, src.h, src_bgra)?;
    }

    // --- Build destination view (exact canvas) ---
    // Optional: letterbox background fill (must do before TypedImage creation)
    if let AspectMode::Pad { bg_rgba } = plan.aspect {
        fill_bgra(&mut dst[..dst_len], bg_rgba);
    }
    let mut dst_image = TypedImage::<U8x4>::from_buffer(plan.out.w, plan.out.h, dst)?;

    // --- Choose the destination subview (for Pad) ---
    let mut dst_view_any = if let Some((x, y, w, h)) = plan.dst_roi {
        let cropped = TypedCroppedImageMut::from_ref(&mut dst_image, x, y, w, h)?;
        CroppedOrFull::Cropped(cropped)
    } else {
        CroppedOrFull::Full(dst_image)
    };

    // --- Resize ---
    let opts = ResizeOptions::new()
        // For even more speed, switch to Bilinear:
        //.resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Bilinear))
        .use_alpha(false);

    match &mut dst_view_any {
        CroppedOrFull::Full(ref mut full) => resizer.resize_typed::<U8x4>(&src_view, full, &opts)?,
        CroppedOrFull::Cropped(ref mut roi) => resizer.resize_typed::<U8x4>(&src_view, roi, &opts)?,
    }

    Ok(())
}

enum CroppedOrFull<'a> {
    Full(TypedImage<'a, U8x4>),
    Cropped(TypedCroppedImageMut<'a, TypedImage<'a, U8x4>>),
}

#[inline]
fn fill_bgra(dst: &mut [u8], bg: [u8; 4]) {
    let mut i = 0;
    while i + 4 <= dst.len() {
        dst[i..i + 4].copy_from_slice(&bg);
        i += 4;
    }
}

#[inline]
fn compact_rows(src: &[u8], src_pitch: usize, dst: &mut [u8], row_bytes: usize, rows: usize) {
    for r in 0..rows {
        let s = &src[r * src_pitch .. r * src_pitch + row_bytes];
        let d = &mut dst[r * row_bytes .. (r + 1) * row_bytes];
        d.copy_from_slice(s);
    }
}
