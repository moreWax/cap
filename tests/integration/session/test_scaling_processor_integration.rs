//! Integration tests for Scaling Processor
//!
//! These tests validate that the scaling processor can be instantiated
//! and used correctly.

use cap_scale::presets::TokenPreset;
use cap_scale::cpu::Staging;
use fast_image_resize::Resizer;
use hybrid_screen_capture::processing::FrameProcessor;

// Test that we can create a scaling processor with different presets
#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_scaling_processor_creation() {
    // Test that we can create scaling processors with different presets
    let presets = vec![
        TokenPreset::P2_56_Long640,
        TokenPreset::P4_Long640,
        TokenPreset::P9_Long640,
        TokenPreset::P10_24_Long640,
    ];

    for preset in presets {
        // Create a scaling processor (this mirrors the implementation in session.rs)
        let mut processor = hybrid_screen_capture::processing::ScalingProcessor {
            preset,
            resizer: Resizer::new(),
            staging: Staging::with_capacity(1920 * 1080 * 4),
            output_buffer: Vec::new(),
            output_size: hybrid_screen_capture::processing::Size { w: 0, h: 0 },
        };

        // Test initialization
        let input_size = hybrid_screen_capture::processing::Size { w: 1920, h: 1080 };
        let output_size = processor.initialize(input_size).await.unwrap();

        // Verify output size is scaled down
        assert!(output_size.w <= 640, "Width should be scaled down to max 640px for preset {:?}", preset);
        assert!(output_size.h <= 360, "Height should be scaled down to max 360px for preset {:?}", preset);
        assert!(output_size.w > 0, "Width should be positive");
        assert!(output_size.h > 0, "Height should be positive");
    }
}

// Test that scaling processor maintains aspect ratio
#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_scaling_processor_aspect_ratio() {
    let preset = TokenPreset::P4_Long640;
    let mut processor = hybrid_screen_capture::processing::ScalingProcessor {
        preset,
        resizer: Resizer::new(),
        staging: Staging::with_capacity(1920 * 1080 * 4),
        output_buffer: Vec::new(),
        output_size: hybrid_screen_capture::processing::Size { w: 0, h: 0 },
    };

    // Test with 16:9 aspect ratio
    let input_size = hybrid_screen_capture::processing::Size { w: 1920, h: 1080 };
    let output_size = processor.initialize(input_size).await.unwrap();

    // Should maintain 16:9 aspect ratio (640:360 = 16:9)
    let expected_ratio = 16.0 / 9.0;
    let actual_ratio = output_size.w as f32 / output_size.h as f32;
    let ratio_diff = (actual_ratio - expected_ratio).abs();

    assert!(ratio_diff < 0.1, "Aspect ratio should be preserved (expected ~16:9, got {:.2}:{:.2})",
        output_size.w, output_size.h);
}