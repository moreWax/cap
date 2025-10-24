//! Unit tests for the GundamProcessor
//!
//! These tests validate the Gundam tiling processor functionality.

use cap_scale::gundam::GundamCfg;
use std::sync::Arc;

// Define the types we need for testing
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone)]
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

// Test version of GundamProcessor that matches the implementation
pub struct GundamProcessor {
    pub cfg: GundamCfg,
    pub tile_buffers: Vec<Vec<u8>>,
    pub global_buffer: Vec<u8>,
    pub output_size: Size,
}

#[async_trait::async_trait]
impl FrameProcessor for GundamProcessor {
    async fn initialize(&mut self, input_size: Size) -> Result<Size, anyhow::Error> {
        // Estimate number of tiles based on input dimensions
        // Use the same logic as choose_grid but simplified
        let cols = ((input_size.w as f32 / 1024.0).ceil() as u32).clamp(1, 3);
        let rows = ((input_size.h as f32 / 1024.0).ceil() as u32).clamp(1, 3);
        let num_tiles = (cols * rows).clamp(self.cfg.min_tiles, self.cfg.max_tiles);

        // Allocate tile buffers
        let tile_buffer_size = (self.cfg.tile_side * self.cfg.tile_side * 4) as usize;
        self.tile_buffers = vec![vec![0u8; tile_buffer_size]; num_tiles as usize];

        // Allocate global buffer
        let global_buffer_size = (self.cfg.global_side * self.cfg.global_side * 4) as usize;
        self.global_buffer = vec![0u8; global_buffer_size];

        // Calculate output size based on Gundam configuration
        // The output is a composite grid of tiles + global view
        let total_elements = num_tiles + 1; // tiles + global view
        let cols_out = ((total_elements as f32).sqrt().ceil() as u32).max(1);
        let rows_out = (((total_elements as u32 + cols_out - 1) / cols_out) as usize).max(1) as u32;

        // Each element is tile_side x tile_side, except global which gets scaled
        let output_width = cols_out * self.cfg.tile_side;
        let output_height = rows_out * self.cfg.tile_side;

        self.output_size = Size {
            w: output_width,
            h: output_height,
        };
        Ok(self.output_size)
    }

    async fn process_frame(
        &mut self,
        frame: BgraFrame,
    ) -> Result<Option<BgraFrame>, anyhow::Error> {
        // Process frame with Gundam tiling
        use cap_scale::gundam::gundam_pack_cpu;

        // Update tile buffer references
        let tile_refs: Vec<&mut [u8]> = self
            .tile_buffers
            .iter_mut()
            .map(|v| v.as_mut_slice())
            .collect();

        let gundam_outputs = cap_scale::gundam::GundamOutputs {
            tiles: tile_refs,
            global: self.global_buffer.as_mut_slice(),
        };

        // Process the frame
        gundam_pack_cpu(
            &mut fast_image_resize::Resizer::new(),
            &frame.data,
            frame.width,
            frame.height,
            frame.stride,
            self.cfg,
            &mut cap_scale::cpu::Staging::with_capacity(frame.stride * frame.height as usize),
            gundam_outputs,
        )?;

        // Arrange tiles and global into composite frame
        let (composite, width, height) = cap_rtsp::arrange_gundam_composite(
            &self.tile_buffers,
            &self.global_buffer,
            self.cfg.tile_side,
            self.cfg.global_side,
        );

        let composite_frame = BgraFrame {
            data: Arc::new(composite),
            width,
            height,
            stride: width as usize * 4,
            pts_ns: frame.pts_ns,
        };

        Ok(Some(composite_frame))
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
    async fn test_gundam_processor_initialization() {
        let mut processor = GundamProcessor {
            cfg: GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 1920, h: 1080 };
        let output_size = processor.initialize(input_size).await.unwrap();

        // Should have allocated buffers
        assert!(!processor.tile_buffers.is_empty());
        assert!(!processor.global_buffer.is_empty());

        // Output size should be reasonable for composite layout
        assert!(output_size.w > 0);
        assert!(output_size.h > 0);
        assert_eq!(processor.output_size, output_size);
    }

    #[tokio::test]
    async fn test_gundam_processor_frame_processing() {
        let mut processor = GundamProcessor {
            cfg: GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 1920, h: 1080 };
        processor.initialize(input_size).await.unwrap();

        // Test processing a frame
        let input_frame = create_test_frame(1920, 1080);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        // Output should be a composite frame
        assert!(output_frame.width > 0);
        assert!(output_frame.height > 0);
        assert_eq!(output_frame.stride, (output_frame.width * 4) as usize);
        assert_eq!(
            output_frame.data.len(),
            (output_frame.width * output_frame.height * 4) as usize
        );
        assert_eq!(output_frame.pts_ns, Some(1000000));
    }

    #[tokio::test]
    async fn test_gundam_processor_small_input() {
        let mut processor = GundamProcessor {
            cfg: GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 640, h: 480 };
        let output_size = processor.initialize(input_size).await.unwrap();

        // Should still work with small inputs
        assert!(output_size.w > 0);
        assert!(output_size.h > 0);

        let input_frame = create_test_frame(640, 480);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        assert!(output_frame.width > 0);
        assert!(output_frame.height > 0);
    }

    #[tokio::test]
    async fn test_gundam_processor_preserves_timestamp() {
        let mut processor = GundamProcessor {
            cfg: GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        let input_size = Size { w: 1024, h: 768 };
        processor.initialize(input_size).await.unwrap();

        let input_frame = create_test_frame(1024, 768);
        let result = processor.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        // Timestamp should be preserved
        assert_eq!(output_frame.pts_ns, Some(1000000));
    }
}
