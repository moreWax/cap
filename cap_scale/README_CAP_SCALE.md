# cap_scale (drop-in module)

CPU-first, zero-churn (one write per resize) scaling and **DeepSeek-OCR Gundam** tiling for VLM input:

- Five **token-saver presets** (long-side clamp to 640/512) with aspect modes: Preserve, Distort, Pad.
- **Gundam**: n×640×640 tiles + 1×1024×1024 global, ROI cropping + square padding to match the DeepSeek-OCR input side exactly.
- Built on `fast_image_resize` (SIMD; rayon for multi-thread).

## Add to your workspace

```
# in Cargo.toml (workspace members)
members = ["cap_scale", ...]

# or as a path dependency in your recorder crate:
[dependencies]
cap_scale = { path = "./cap_scale" }
```

## Core APIs

- `presets::{TokenPreset, AspectMode, ScaleTarget, build_plan, Size}`
- `cpu::{scale_bgra_cpu, Staging}`
- `gundam::{GundamCfg, GundamOutputs, gundam_pack_cpu}`

## Example

```
cargo run --example gundam_demo
```

This prints the scaled size for the preset flow, and the number of Gundam tiles produced.

## Integration outline (recorder)

1. **Capture** BGRA frames plus stride (bytes/row).
2. Keep a **`Resizer`**, **`Staging`**, and **output buffers** (Vec<u8>) alive; reuse each frame.
3. For presets: compute `plan = build_plan(...)` then call `scale_bgra_cpu(...)`.
4. For Gundam: pre-allocate `N * (640*640*4)` tile buffers + `1 * (1024*1024*4)` global; call `gundam_pack_cpu(...)`.
5. Hand buffers to your encoder/uploader (RTSP/HLS or VLM connector).

> Note: If your source stride equals `width*4`, no compaction is needed; we write directly.
