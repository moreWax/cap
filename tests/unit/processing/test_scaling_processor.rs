//! Unit tests for the ScalingProcessor
//!
//! These tests validate the scaling processor functionality.

use cap_scale::cpu::Staging;
use cap_scale::presets::TokenPreset;
use fast_image_resize::Resizer;
use std::sync::Arc;

// Define the types we need for testing
#[derive(Debug, Clone, Copy)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug)]
pub struct BgraFrame {
    pub data: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub pts_ns: Option<u64>,
}

#[async_trait::async_trait]
pub trait FrameProcessor: Send + Sync {
    async fn initialize(&mut self, input_size: Size) -> Result<Size, anyhow::Error>;
    async fn process_frame(&mut self, frame: BgraFrame)
    -> Result<Option<BgraFrame>, anyhow::Error>;
}

// Test version of ScalingProcessor that matches the actual implementation
pub struct ScalingProcessor {
    pub preset: TokenPreset,
    pub resizer: Resizer,
    pub staging: Staging,
    pub output_buffer: Vec<u8>,
    pub output_size: Size,
}

#[async_trait::async_trait]
impl FrameProcessor for ScalingProcessor {
    async fn initialize(&mut self, input_size: Size) -> Result<Size, anyhow::Error> {
        // Build scaling plan from preset
        let plan = cap_scale::presets::build_plan(
            cap_scale::presets::Size {
                w: input_size.w,
                h: input_size.h,
            },
            self.preset.to_target(),
            cap_scale::presets::AspectMode::Preserve,
        );

        self.output_size = Size {
            w: plan.out.w,
            h: plan.out.h,
        };

        // Pre-allocate output buffer
        let buffer_size = (self.output_size.w * self.output_size.h * 4) as usize;
        self.output_buffer.resize(buffer_size, 0);

        // Ensure staging buffer is large enough for input
        let input_buffer_size = (input_size.w * input_size.h * 4) as usize;
        self.staging.ensure_len(input_buffer_size);

        Ok(self.output_size)
    }

    async fn process_frame(
        &mut self,
        frame: BgraFrame,
    ) -> Result<Option<BgraFrame>, anyhow::Error> {
        // Build scaling plan for this frame
        let plan = cap_scale::presets::build_plan(
            cap_scale::presets::Size {
                w: frame.width,
                h: frame.height,
            },
            self.preset.to_target(),
            cap_scale::presets::AspectMode::Preserve,
        );

        // Scale the frame using cap_scale
        cap_scale::cpu::scale_bgra_cpu(
            &mut self.resizer,
            &frame.data,
            cap_scale::presets::Size {
                w: frame.width,
                h: frame.height,
            },
            Some(frame.stride),
            &plan,
            &mut self.output_buffer,
            Some(&mut self.staging),
        )?;

        // Create output frame
        let scaled_frame = BgraFrame {
            data: Arc::new(self.output_buffer.clone()),
            width: plan.out.w,
            height: plan.out.h,
            stride: (plan.out.w * 4) as usize,
            pts_ns: frame.pts_ns,
        };

        Ok(Some(scaled_frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_frame(width: u32, height: u32) -> BgraFrame {
        let pixel_count = (width * height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        // Create a simple gradient pattern for testing
        for y in 0..height {
            for x in 0..width {
                let pixel_index = ((y * width + x) * 4) as usize;
                let r = ((x as f32 / width as f32) * 255.0) as u8;
                let g = ((y as f32 / height as f32) * 255.0) as u8;
                let b = 128u8;

                data[pixel_index] = b; // B
                data[pixel_index + 1] = g; // G
                data[pixel_index + 2] = r; // R
                data[pixel_index + 3] = 255; // A
            }
        }

        BgraFrame {
            data: Arc::new(data),
            width,
            height,
            stride: (width * 4) as usize,
            pts_ns: Some(1000000), // 1ms
        }
    }

    #[tokio::test]
    async fn test_scaling_processor_low_detail() {
        let mut processor = ScalingProcessor {
            preset: TokenPreset::P2_56_Long640, // Light compression
            resizer: Resizer::new(),
            staging: Staging::with_capacity(1920 * 1080 * 4),
            output_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 1920, h: 1080 };
        let output_size = processor.initialize(input_size).await.unwrap();

        // P2_56_Long640 should scale to max 640px on longest side
        assert_eq!(output_size.w, 640);
        assert_eq!(output_size.h, 360);

        // Test processing a frame
        let input_frame = create_test_frame(1920, 1080);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        assert_eq!(output_frame.width, 640);
        assert_eq!(output_frame.height, 360);
        assert_eq!(output_frame.stride, 640 * 4);
        assert_eq!(output_frame.data.len(), 640 * 360 * 4);
        assert_eq!(output_frame.pts_ns, Some(1000000));
    }

    #[tokio::test]
    async fn test_scaling_processor_balanced() {
        let mut processor = ScalingProcessor {
            preset: TokenPreset::P4_Long640, // Balanced compression
            resizer: Resizer::new(),
            staging: Staging::with_capacity(1024 * 768 * 4),
            output_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 1024, h: 768 };
        let output_size = processor.initialize(input_size).await.unwrap();

        // P4_Long640 should scale to max 640px on longest side
        assert_eq!(output_size.w, 640);
        assert_eq!(output_size.h, 480);

        // Test processing a frame
        let input_frame = create_test_frame(1024, 768);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        assert_eq!(output_frame.width, 640);
        assert_eq!(output_frame.height, 480);
        assert_eq!(output_frame.stride, 640 * 4);
        assert_eq!(output_frame.data.len(), 640 * 480 * 4);
    }

    #[tokio::test]
    async fn test_scaling_processor_high_detail() {
        let mut processor = ScalingProcessor {
            preset: TokenPreset::P9_Long640, // Aggressive compression
            resizer: Resizer::new(),
            staging: Staging::with_capacity(1344 * 756 * 4),
            output_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 1344, h: 756 };
        let output_size = processor.initialize(input_size).await.unwrap();

        // P9_Long640 should scale to max 640px on longest side
        assert_eq!(output_size.w, 640);
        assert_eq!(output_size.h, 360);

        // Test processing a frame
        let input_frame = create_test_frame(1344, 756);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        assert_eq!(output_frame.width, 640);
        assert_eq!(output_frame.height, 360);
        assert_eq!(output_frame.stride, 640 * 4);
        assert_eq!(output_frame.data.len(), 640 * 360 * 4);
    }

    #[tokio::test]
    async fn test_scaling_processor_uninitialized_error() {
        let mut processor = ScalingProcessor {
            preset: TokenPreset::P4_Long640,
            resizer: Resizer::new(),
            staging: Staging::with_capacity(640 * 480 * 4),
            output_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_frame = create_test_frame(640, 480);
        let result = processor.process_frame(input_frame).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        println!("Actual error: {}", error_msg);
        // The error should be related to uninitialized state
        assert!(
            error_msg.contains("Output buffer too small") || error_msg.contains("uninitialized")
        );
    }

    #[tokio::test]
    async fn test_scaling_processor_preserves_timestamp() {
        let mut processor = ScalingProcessor {
            preset: TokenPreset::P4_Long640,
            resizer: Resizer::new(),
            staging: Staging::with_capacity(800 * 600 * 4),
            output_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 800, h: 600 };
        processor.initialize(input_size).await.unwrap();

        let input_frame = create_test_frame(800, 600);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        // Timestamp should be preserved
        assert_eq!(output_frame.pts_ns, Some(1000000));
    }
}
