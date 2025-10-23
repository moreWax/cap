# cap-scale: Token-Efficient Screen Scaling for VLM Input

This crate provides high-performance, token-efficient image scaling optimized for Vision Language Models (VLMs). It implements DeepSeek OCR-inspired compression techniques to reduce token usage while preserving visual quality.

## Architecture Overview

The crate is designed around three core principles:
1. **Zero-copy where possible**: Minimize memory allocations and copies
2. **SIMD acceleration**: Use fast_image_resize for CPU-optimized scaling
3. **VLM-optimized presets**: Token-saving scaling ratios based on empirical testing

## Key Components

- **`presets`**: Scaling plan computation and token-efficient preset definitions
- **`cpu`**: CPU-based scaling implementation using SIMD acceleration
- **`gundam`**: DeepSeek OCR "Gundam" tiling for complex document layouts

## Performance Characteristics

- **SIMD-accelerated**: Leverages AVX2/AVX-512 when available
- **Memory efficient**: Pre-allocated buffers and staging areas
- **Zero-allocation scaling**: Reuses buffers across frames
- **Stride-aware**: Handles both tightly-packed and strided input

## Token Efficiency

The scaling presets are designed to minimize VLM token usage while preserving OCR accuracy:
- P2_56: ~2.56x token reduction (1024px → 640px longest side)
- P4: 4x token reduction (1280px → 640px longest side)
- P6_9: ~6.9x token reduction (1344px → 512px longest side)
- P9: 9x token reduction (1920px → 640px longest side)
- P10_24: ~10.24x token reduction (2048px → 640px longest side)

## Usage Examples

### Basic Single-Image Scaling
```rust,no_run
use cap_scale::{cpu::scale_bgra_cpu, presets::{build_plan, ScaleTarget, AspectMode, Size}};

// Create a scaling plan for VLM input
let input_size = Size { w: 1920, h: 1080 };
let plan = build_plan(
    input_size,
    ScaleTarget::MaxLongSide(640), // P4 preset equivalent
    AspectMode::Preserve // Maintain aspect ratio
);

// Scale BGRA image data
let mut resizer = fast_image_resize::Resizer::new();
let mut output = vec![0u8; (plan.out.w * plan.out.h * 4) as usize];

scale_bgra_cpu(
    &mut resizer,
    &input_bgra_data,
    input_size,
    Some(1920 * 4), // stride in bytes
    &plan,
    &mut output,
    None // no staging needed for tightly-packed input
)?;
```

### Gundam Tiling for Complex Documents
```rust,no_run
use cap_scale::gundam::{gundam_pack_cpu, GundamCfg, GundamOutputs};

// Configure Gundam tiling
let cfg = GundamCfg::default();

// Prepare output buffers
let mut tiles = vec![vec![0u8; 640 * 640 * 4]; 4]; // 4 tiles
let mut global = vec![0u8; 1024 * 1024 * 4]; // global view

let mut outputs = GundamOutputs {
    tiles: tiles.iter_mut().map(|v| v.as_mut_slice()).collect(),
    global: global.as_mut_slice(),
};

// Process image into tiles + global view
gundam_pack_cpu(
    &mut resizer,
    &input_bgra,
    1920, 1080,
    1920 * 4, // stride
    cfg,
    &mut staging,
    outputs
)?;
```

## API Reference

### Core Types

- **`Size`**: 2D dimensions (width, height)
- **`ScaleTarget`**: Size constraint (max side length vs exact dimensions)
- **`AspectMode`**: Aspect ratio handling (preserve, distort, pad)
- **`ScalePlan`**: Complete scaling plan with computed parameters
- **`TokenPreset`**: VLM-optimized scaling presets (P2_56, P4, P6_9, P9, P10_24)

### Key Functions

- **`build_plan(input, target, aspect)`**: Compute scaling plan
- **`scale_bgra_cpu(resizer, src, src_size, stride, plan, dst, staging)`**: Scale BGRA image
- **`gundam_pack_cpu(resizer, src, w, h, stride, cfg, staging, outputs)`**: Create Gundam tiles

### Gundam Types

- **`GundamCfg`**: Configuration for tiling (tile size, overlap, grid selection)
- **`GundamOutputs`**: Pre-allocated buffers for tiles and global view
- **`choose_grid(w, h)`**: Automatically select optimal tile grid

## Scaling Pipeline

The scaling system handles multiple scenarios:

### Single Image Scaling
1. **Plan computation**: Determine output dimensions and ROI
2. **Stride handling**: Compact strided input if needed
3. **SIMD scaling**: fast_image_resize with CatmullRom filter
4. **Background fill**: Apply padding color for Pad mode

### Gundam Tiling Pipeline
1. **Grid selection**: Choose optimal tile layout (2-9 tiles)
2. **Region extraction**: Extract overlapping tile regions
3. **Individual scaling**: Scale each tile to 640×640 with padding
4. **Global scaling**: Scale overview to 1024×1024 with padding

## Memory Management

- **Zero-copy input**: Direct processing when possible
- **Staging buffers**: Compact strided data for efficient scaling
- **Pre-allocated output**: Caller provides exact-sized buffers
- **Buffer reuse**: Resizer and staging buffers persist across frames

## Token Efficiency Strategy

VLM token usage scales with pixel count, but OCR accuracy depends more on longest dimension. Presets clamp the longest side to efficient values while preserving aspect ratio:

- **640px max**: Good balance for most content (P2_56, P4, P9, P10_24)
- **512px max**: Higher compression for dense text (P6_9)

## Future Optimizations

- GPU acceleration via wgpu for faster scaling
- Zero-copy GPU buffer sharing with Vulkan/DirectX capture backends
- Progressive JPEG encoding for reduced network bandwidth
- Adaptive overlap in Gundam tiling based on content analysis
- Parallel tile processing for large inputs</content>
<parameter name="filePath">/home/xor/cap/cap-scale/README.md