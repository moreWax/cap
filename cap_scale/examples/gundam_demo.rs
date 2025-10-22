use cap_scale::presets::{Size, AspectMode, ScaleTarget, build_plan, TokenPreset};
use cap_scale::cpu::{scale_bgra_cpu, Staging};
use cap_scale::gundam::{gundam_pack_cpu, GundamCfg, GundamOutputs};
use fast_image_resize::Resizer;

fn main() -> anyhow::Result<()> {
    // Fake 1920x1080 BGRA frame
    let src_w = 1920u32;
    let src_h = 1080u32;
    let stride = (src_w as usize) * 4;
    let mut src = vec![0u8; (stride * src_h as usize)];
    // draw a simple gradient
    for y in 0..src_h as usize {
        for x in 0..src_w as usize {
            let i = y*stride + x*4;
            src[i+0] = (x % 256) as u8;     // B
            src[i+1] = (y % 256) as u8;     // G
            src[i+2] = ((x+y) % 256) as u8; // R
            src[i+3] = 255;                 // A
        }
    }

    // Simple preset: clamp long side to 640, preserve aspect
    let plan = build_plan(Size{w:src_w,h:src_h}, TokenPreset::P9_Long640.to_target(), AspectMode::Preserve);
    let mut dst = vec![0u8; (plan.out.w as usize)*(plan.out.h as usize)*4];
    let mut resizer = Resizer::new();
    let mut staging = Staging::with_capacity((src_w as usize)*4*src_h as usize);
    scale_bgra_cpu(&mut resizer, &src, Size{w:src_w,h:src_h}, Some(stride), &plan, &mut dst, Some(&mut staging))?;
    println!("preset out: {}x{}", plan.out.w, plan.out.h);

    // Gundam: tiles 640 + global 1024
    let cfg = GundamCfg::default();
    let mut tiles: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; (cfg.tile_side as usize)*(cfg.tile_side as usize)*4]).collect();
    let mut tiles_slices: Vec<&mut [u8]> = tiles.iter_mut().map(|v| v.as_mut_slice()).collect();
    let mut global = vec![0u8; (cfg.global_side as usize)*(cfg.global_side as usize)*4];
    let outs = GundamOutputs { tiles: tiles_slices, global: global.as_mut_slice() };
    gundam_pack_cpu(&mut resizer, &src, src_w, src_h, stride, cfg, &mut staging, outs)?;
    println!("gundam produced {} tiles + 1 global", tiles.len());

    Ok(())
}
