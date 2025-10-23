// SPDX-License-Identifier: MIT
//! # cap-scale: Token-Efficient Screen Scaling for VLM Input
//!
//! This crate provides high-performance, token-efficient image scaling optimized for Vision Language Models (VLMs).
//! It implements DeepSeek OCR-inspired compression techniques to reduce token usage while preserving visual quality.
//!
//! ## Architecture Overview
//!
//! The crate is designed around three core principles:
//! 1. **Zero-copy where possible**: Minimize memory allocations and copies
//! 2. **SIMD acceleration**: Use fast_image_resize for CPU-optimized scaling
//! 3. **VLM-optimized presets**: Token-saving scaling ratios based on empirical testing
//!
//! ## Key Components
//!
//! - [`presets`]: Scaling plan computation and token-efficient preset definitions
//! - [`cpu`]: CPU-based scaling implementation using SIMD acceleration
//! - [`gundam`]: DeepSeek OCR "Gundam" tiling for complex document layouts
//!
//! ## Performance Characteristics
//!
//! - **SIMD-accelerated**: Leverages AVX2/AVX-512 when available
//! - **Memory efficient**: Pre-allocated buffers and staging areas
//! - **Zero-allocation scaling**: Reuses buffers across frames
//! - **Stride-aware**: Handles both tightly-packed and strided input
//!
//! ## Token Efficiency
//!
//! The scaling presets are designed to minimize VLM token usage while preserving OCR accuracy:
//! - P2_56: ~2.56x token reduction (1024px → 640px longest side)
//! - P4: 4x token reduction (1280px → 640px longest side)
//! - P6_9: ~6.9x token reduction (1344px → 512px longest side)
//! - P9: 9x token reduction (1920px → 640px longest side)
//! - P10_24: ~10.24x token reduction (2048px → 640px longest side)
//!
//! ## Usage Example
//!
//! ```rust
//! use cap_scale::{cpu::scale_bgra_cpu, presets::{build_plan, ScaleTarget, AspectMode, Size}};
//!
//! // Create a scaling plan for VLM input
//! let input_size = Size { w: 1920, h: 1080 };
//! let plan = build_plan(
//!     input_size,
//!     ScaleTarget::MaxLongSide(640), // P4 preset equivalent
//!     AspectMode::Preserve // Maintain aspect ratio
//! );
//!
//! // Scale BGRA image data
//! let mut resizer = fast_image_resize::Resizer::new();
//! let mut output = vec![0u8; (plan.out.w * plan.out.h * 4) as usize];
//!
//! scale_bgra_cpu(
//!     &mut resizer,
//!     &input_bgra_data,
//!     input_size,
//!     Some(1920 * 4), // stride in bytes
//!     &plan,
//!     &mut output,
//!     None // no staging needed for tightly-packed input
//! )?;
//! ```
//!
//! ## Future Optimizations
//!
//! TODO: Consider GPU acceleration via wgpu for even faster scaling on supported hardware.
//! TODO: Investigate zero-copy GPU buffer sharing with Vulkan/DirectX capture backends.
//! TODO: Add support for progressive JPEG encoding to reduce network bandwidth.

pub mod cpu;
pub mod gundam;
pub mod presets;
