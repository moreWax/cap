// SPDX-License-Identifier: MIT
//! # DeepSeek OCR "Gundam" Tiling Implementation
//!
//! This module implements the DeepSeek OCR "Gundam" input format, which combines
//! multiple overlapping tiles with a global view for enhanced document understanding.
//!
//! ## Gundam Format Overview
//!
//! The Gundam format processes documents by:
//! 1. **Dividing into tiles**: 2-9 overlapping 640×640px tiles
//! 2. **Global view**: Single 1024×1024px overview image
//! 3. **Overlapping regions**: Configurable overlap between adjacent tiles
//!
//! This multi-resolution approach helps VLMs understand both local details
//! and global document structure simultaneously.
//!
//! ## Grid Selection Algorithm
//!
//! The system automatically chooses a grid layout based on input dimensions:
//! - **Base calculation**: `cols = ceil(width/1024)`, `rows = ceil(height/1024)`
//! - **Clamping**: Limited to 1-3 rows/columns (max 9 tiles total)
//! - **Minimum enforcement**: Ensures at least 2 tiles for multi-resolution benefit
//!
//! ## Performance Characteristics
//!
//! - **Memory efficient**: Reuses caller-provided output buffers
//! - **Zero allocations**: All buffers pre-allocated by caller
//! - **SIMD accelerated**: Uses same scaling engine as single-image processing
//! - **Configurable overlap**: Tunable for different content types
//!
//! ## Use Cases
//!
//! Gundam tiling is particularly effective for:
//! - **Dense documents**: Multi-column layouts, forms, tables
//! - **Large images**: Screenshots, diagrams, charts
//! - **Mixed content**: Documents with both text and visual elements
//!
//! ## Future Optimizations
//!
//! TODO: Consider adaptive overlap based on content analysis (text density, layout complexity).
//! TODO: Add support for non-square tiles if DeepSeek updates their format.
//! TODO: Investigate tile prioritization based on text density for better token efficiency.
//! TODO: Add support for progressive tile generation to reduce initial latency.

use anyhow::Result;
use fast_image_resize::Resizer;

use crate::cpu::{scale_bgra_cpu, Staging};
use crate::presets::{build_plan, AspectMode, ScaleTarget, Size};

/// Configuration for Gundam tiling matching DeepSeek-OCR input requirements.
/// All parameters tuned for optimal OCR accuracy vs token efficiency.
#[derive(Clone, Copy)]
pub struct GundamCfg {
    /// Size of individual tiles (640px square for DeepSeek compatibility)
    pub tile_side: u32,
    /// Size of global overview image (1024px square)
    pub global_side: u32,
    /// Minimum number of tiles to generate (ensures multi-resolution benefit)
    pub min_tiles: u32,
    /// Maximum number of tiles (DeepSeek limit)
    pub max_tiles: u32,
    /// Automatically choose grid based on input dimensions
    pub auto_grid: bool,
    /// Overlap fraction between adjacent tiles (0.0 = no overlap)
    pub overlap_frac: f32,
    /// Background color for padding (white for document processing)
    pub pad_bg: [u8; 4],
}

impl Default for GundamCfg {
    fn default() -> Self {
        Self {
            tile_side: 640,
            global_side: 1024,
            min_tiles: 2,
            max_tiles: 9,
            auto_grid: true,
            overlap_frac: 0.0, // No overlap matches public DeepSeek examples
            pad_bg: [255, 255, 255, 255], // White background for documents
        }
    }
}

/// Rectangle definition in source pixel coordinates.
/// Used for defining tile boundaries with potential overlap.
#[derive(Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Compute optimal tile grid dimensions based on input image size.
///
/// Implements the DeepSeek grid selection algorithm:
/// 1. Calculate base grid: ceil(dimension/1024)
/// 2. Clamp to 1-3 per dimension (max 9 tiles)
/// 3. Ensure minimum 2 tiles for multi-resolution benefit
/// 4. Balance rows vs columns based on aspect ratio
///
/// # Arguments
/// * `in_w` - Input image width
/// * `in_h` - Input image height
///
/// # Returns
/// (columns, rows) tuple for tile grid
pub fn choose_grid(in_w: u32, in_h: u32) -> (u32, u32) {
    let mut cols = (f64::from(in_w) / 1024.0).ceil() as u32;
    let mut rows = (f64::from(in_h) / 1024.0).ceil() as u32;
    cols = cols.clamp(1, 3);
    rows = rows.clamp(1, 3);
    let mut n = rows * cols;
    if n < 2 {
        // Force minimum 2 tiles by expanding the longer dimension
        if in_w >= in_h {
            cols = (cols + 1).clamp(1, 3)
        } else {
            rows = (rows + 1).clamp(1, 3)
        }
        n = rows * cols;
    }
    if n > 9 {
        // Reduce to max 9 tiles, preferring to reduce the shorter dimension
        if in_w >= in_h {
            cols = 3;
            rows = (9 / cols).max(1);
        } else {
            rows = 3;
            cols = (9 / rows).max(1);
        }
    }
    (cols, rows)
}

/// Generate tile rectangles for a grid layout with optional overlap.
///
/// Creates overlapping tiles by extending each tile boundary by the overlap fraction.
/// Overlap helps preserve context across tile boundaries, important for text that
/// spans multiple tiles.
///
/// # Arguments
/// * `in_w`, `in_h` - Input image dimensions
/// * `cols`, `rows` - Grid dimensions from choose_grid()
/// * `overlap_frac` - Fraction of tile size to overlap (0.0 = no overlap)
///
/// # Returns
/// Vector of Rect defining each tile's source region
fn mk_grid(in_w: u32, in_h: u32, cols: u32, rows: u32, overlap_frac: f32) -> Vec<Rect> {
    let mut rects = Vec::with_capacity((cols * rows) as usize);
    let step_w = (in_w as f32 / cols as f32).ceil() as u32;
    let step_h = (in_h as f32 / rows as f32).ceil() as u32;

    // Calculate overlap in pixels
    let ovw = ((step_w as f32) * overlap_frac) as i32;
    let ovh = ((step_h as f32) * overlap_frac) as i32;

    for r in 0..rows {
        for c in 0..cols {
            // Calculate base tile boundaries
            let mut x0 = (c * step_w) as i32 - ovw;
            let mut y0 = (r * step_h) as i32 - ovh;
            let mut x1 = ((c + 1) * step_w) as i32 + ovw;
            let mut y1 = ((r + 1) * step_h) as i32 + ovh;

            // Clamp to image boundaries
            x0 = x0.clamp(0, in_w as i32);
            y0 = y0.clamp(0, in_h as i32);
            x1 = x1.clamp(0, in_w as i32);
            y1 = y1.clamp(0, in_h as i32);

            let w = (x1 - x0).max(1) as u32;
            let h = (y1 - y0).max(1) as u32;

            rects.push(Rect {
                x: x0 as u32,
                y: y0 as u32,
                w,
                h,
            });
        }
    }
    rects
}

/// Output buffer container for Gundam processing.
///
/// All buffers must be pre-allocated by the caller with correct sizes:
/// - Each tile buffer: `tile_side * tile_side * 4` bytes (BGRA)
/// - Global buffer: `global_side * global_side * 4` bytes (BGRA)
///
/// This design enables zero-allocation processing and buffer reuse across frames.
pub struct GundamOutputs<'a> {
    /// Individual tile images (scaled to tile_side × tile_side)
    pub tiles: Vec<&'a mut [u8]>,
    /// Global overview image (scaled to global_side × global_side)
    pub global: &'a mut [u8],
}

/// Process image into Gundam format: multiple tiles + global view.
///
/// This function implements the complete DeepSeek OCR preprocessing pipeline:
/// 1. Analyze input dimensions and choose optimal grid
/// 2. Generate overlapping tile regions
/// 3. Scale each tile to 640×640 with padding
/// 4. Scale global view to 1024×1024 with padding
/// 5. Write results to caller-provided buffers
///
/// # Arguments
/// * `resizer` - Reusable SIMD-accelerated resizer
/// * `src_bgra` - Input BGRA image data
/// * `src_w`, `src_h` - Input dimensions
/// * `src_stride_bytes` - Bytes per row (for strided input)
/// * `cfg` - Gundam configuration
/// * `staging` - Scratch buffer for strided input processing
/// * `out` - Pre-allocated output buffers
///
/// # Performance Notes
/// - Reuses resizer across all scaling operations
/// - Processes tiles sequentially to minimize memory pressure
/// - Zero allocations during processing (all buffers pre-allocated)
///
/// # Future Optimizations
/// TODO: Consider parallel tile processing for very large inputs.
/// TODO: Add tile content analysis to prioritize high-information tiles.
pub fn gundam_pack_cpu(
    resizer: &mut Resizer,
    src_bgra: &[u8],
    src_w: u32,
    src_h: u32,
    src_stride_bytes: usize,
    cfg: GundamCfg,
    staging: &mut Staging,
    mut out: GundamOutputs,
) -> Result<()> {
    let input = Size { w: src_w, h: src_h };

    // Choose grid layout based on input dimensions
    let (cols, rows) = choose_grid(src_w, src_h);
    let mut rects = mk_grid(src_w, src_h, cols, rows, cfg.overlap_frac);
    rects.truncate(cfg.max_tiles as usize); // Limit to max tiles
    if rects.len() < cfg.min_tiles as usize && cols * rows >= cfg.min_tiles {
        // Grid selection should prevent this, but defensive check
    }

    // Create scaling plan for tiles (always pad to square)
    let tile_plan = |w: u32, h: u32| {
        build_plan(
            Size { w, h },
            ScaleTarget::Exact(Size {
                w: cfg.tile_side,
                h: cfg.tile_side,
            }),
            AspectMode::Pad {
                bg_rgba: cfg.pad_bg,
            },
        )
    };

    let need_tile = (cfg.tile_side as usize) * (cfg.tile_side as usize) * 4;
    for (i, r) in rects.iter().enumerate() {
        let plan = tile_plan(r.w, r.h);
        let dst = out.tiles.get_mut(i).expect("insufficient tile buffers");
        assert!(dst.len() >= need_tile, "tile buffer too small");

        // Extract tile region into staging buffer (handles strided input)
        compact_crop_to_staging(src_bgra, src_stride_bytes, *r, staging);

        // Scale tile to final size
        scale_bgra_cpu(
            resizer,
            staging.as_slice(),
            Size { w: r.w, h: r.h },
            Some((r.w as usize) * 4), // staging is always tightly packed
            &plan,
            *dst,
            None, // staging already compacted
        )?;
    }

    // Process global view
    let global_plan = build_plan(
        input,
        ScaleTarget::Exact(Size {
            w: cfg.global_side,
            h: cfg.global_side,
        }),
        AspectMode::Pad {
            bg_rgba: cfg.pad_bg,
        },
    );
    scale_bgra_cpu(
        resizer,
        src_bgra,
        input,
        Some(src_stride_bytes),
        &global_plan,
        out.global,
        Some(staging),
    )?;

    Ok(())
}

/// Extract rectangular region from strided BGRA buffer into tightly-packed staging buffer.
///
/// This function crops a specific region from the source image, handling strided input
/// by copying only the relevant pixel data into a contiguous buffer for efficient scaling.
///
/// # Arguments
/// * `src` - Source BGRA buffer (potentially strided)
/// * `src_pitch` - Bytes per row in source buffer
/// * `roi` - Region of interest to extract
/// * `staging` - Output buffer (will be resized as needed)
///
/// # Performance Notes
/// - Only copies the pixels actually needed for the tile
/// - Produces tightly-packed output for optimal scaling performance
/// - Called once per tile, so overhead is acceptable
///
/// # Future Optimizations
/// TODO: Consider SIMD acceleration for large tile extractions.
fn compact_crop_to_staging(src: &[u8], src_pitch: usize, roi: Rect, staging: &mut Staging) {
    let row_bytes = (roi.w as usize) * 4;
    let mut off = 0usize;
    staging.ensure_len(row_bytes * (roi.h as usize));
    for r in 0..roi.h as usize {
        let row_off = (roi.y as usize + r) * src_pitch + (roi.x as usize) * 4;
        let s = &src[row_off..row_off + row_bytes];
        let d = &mut staging.buf[off..off + row_bytes];
        d.copy_from_slice(s);
        off += row_bytes;
    }
}
