// SPDX-License-Identifier: MIT
// DeepSeek-OCR "Gundam" input semantics: n×640×640 tiles + 1×1024×1024 global.
use anyhow::Result;
use fast_image_resize::Resizer;

use crate::cpu::{scale_bgra_cpu, Staging};
use crate::presets::{AspectMode, ScaleTarget, Size, build_plan};

/// Config matching DeepSeek-OCR input side.
#[derive(Clone, Copy)]
pub struct GundamCfg {
    pub tile_side: u32,   // 640
    pub global_side: u32, // 1024
    pub min_tiles: u32,   // 2
    pub max_tiles: u32,   // 9
    /// Choose grid automatically by splitting up to 3×3 based on source size.
    pub auto_grid: bool,  // true
    /// 0.0 = no overlap (closest to public examples).
    pub overlap_frac: f32,
    /// Pad tiles/global to exact squares (BGRA color).
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
            overlap_frac: 0.0,
            pad_bg: [255, 255, 255, 255],
        }
    }
}

/// Rect in source pixels
#[derive(Clone, Copy)]
pub struct Rect { pub x: u32, pub y: u32, pub w: u32, pub h: u32 }

/// Compute a 2..9 tile grid. cols = ceil(w/1024), rows = ceil(h/1024), clamped to 1..3
pub fn choose_grid(in_w: u32, in_h: u32) -> (u32, u32) {
    let mut cols = (f64::from(in_w) / 1024.0).ceil() as u32;
    let mut rows = (f64::from(in_h) / 1024.0).ceil() as u32;
    cols = cols.clamp(1, 3);
    rows = rows.clamp(1, 3);
    let mut n = rows * cols;
    if n < 2 {
        if in_w >= in_h { cols = (cols + 1).clamp(1, 3) } else { rows = (rows + 1).clamp(1, 3) }
        n = rows * cols;
    }
    if n > 9 {
        if in_w >= in_h { cols = 3; rows = (9 / cols).max(1); } else { rows = 3; cols = (9 / rows).max(1); }
    }
    (cols, rows)
}

fn mk_grid(in_w: u32, in_h: u32, cols: u32, rows: u32, overlap_frac: f32) -> Vec<Rect> {
    let mut rects = Vec::with_capacity((cols * rows) as usize);
    let step_w = (in_w as f32 / cols as f32).ceil() as u32;
    let step_h = (in_h as f32 / rows as f32).ceil() as u32;

    let ovw = ((step_w as f32) * overlap_frac) as i32;
    let ovh = ((step_h as f32) * overlap_frac) as i32;

    for r in 0..rows {
        for c in 0..cols {
            let mut x0 = (c * step_w) as i32 - ovw;
            let mut y0 = (r * step_h) as i32 - ovh;
            let mut x1 = ((c + 1) * step_w) as i32 + ovw;
            let mut y1 = ((r + 1) * step_h) as i32 + ovh;

            x0 = x0.clamp(0, in_w as i32);
            y0 = y0.clamp(0, in_h as i32);
            x1 = x1.clamp(0, in_w as i32);
            y1 = y1.clamp(0, in_h as i32);

            let w = (x1 - x0).max(1) as u32;
            let h = (y1 - y0).max(1) as u32;

            rects.push(Rect { x: x0 as u32, y: y0 as u32, w, h });
        }
    }
    rects
}

/// Output buffers (caller-owned)
pub struct GundamOutputs<'a> {
    pub tiles: Vec<&'a mut [u8]>, // each len = tile_side*tile_side*4
    pub global: &'a mut [u8],     // len = global_side*global_side*4
}

/// Pack tiles (scaled to `tile_side` square) + global (scaled to `global_side` square).
/// Writes into caller-provided buffers (reused ring), no per-frame allocations.
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

    let (cols, rows) = choose_grid(src_w, src_h);
    let mut rects = mk_grid(src_w, src_h, cols, rows, cfg.overlap_frac);
    rects.truncate(cfg.max_tiles as usize);
    if rects.len() < cfg.min_tiles as usize && cols*rows >= cfg.min_tiles {
        // already handled by choose_grid (>=2)
    }

    let tile_plan = |w: u32, h: u32| {
        build_plan(
            Size { w, h },
            ScaleTarget::Exact(Size { w: cfg.tile_side, h: cfg.tile_side }),
            AspectMode::Pad { bg_rgba: cfg.pad_bg },
        )
    };

    // tiles
    let need_tile = (cfg.tile_side as usize) * (cfg.tile_side as usize) * 4;
    for (i, r) in rects.iter().enumerate() {
        let plan = tile_plan(r.w, r.h);
        let dst = out.tiles.get_mut(i).expect("insufficient tile buffers");
        assert!(dst.len() >= need_tile, "tile buffer too small");

        // Crop ROI into staging as tightly-packed BGRA
        compact_crop_to_staging(src_bgra, src_stride_bytes, *r, staging);

        // Resize from staging -> dst
        scale_bgra_cpu(
            resizer,
            staging.as_slice(),
            Size { w: r.w, h: r.h },
            Some((r.w as usize) * 4),
            &plan,
            *dst,
            None,
        )?;
    }

    // global
    let global_plan = build_plan(
        input,
        ScaleTarget::Exact(Size { w: cfg.global_side, h: cfg.global_side }),
        AspectMode::Pad { bg_rgba: cfg.pad_bg },
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

/// Copy ROI rows into `staging` as tightly packed BGRA.
fn compact_crop_to_staging(src: &[u8], src_pitch: usize, roi: Rect, staging: &mut Staging) {
    let row_bytes = (roi.w as usize) * 4;
    let mut off = 0usize;
    staging.ensure_len(row_bytes * (roi.h as usize));
    for r in 0..roi.h as usize {
        let row_off = (roi.y as usize + r) * src_pitch + (roi.x as usize) * 4;
        let s = &src[row_off .. row_off + row_bytes];
        let d = &mut staging.buf[off .. off + row_bytes];
        d.copy_from_slice(s);
        off += row_bytes;
    }
}
