//! Integration tests for GundamProcessor in the processing pipeline
//!
//! These tests validate GundamProcessor integration with the full processing pipeline.

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

#[async_trait::async_trait]
pub trait CaptureSource: Send + Sync {
    async fn initialize(&mut self) -> Result<Size, anyhow::Error>;
    async fn capture_frame(&mut self) -> Result<BgraFrame, anyhow::Error>;
    async fn cleanup(&mut self) -> Result<(), anyhow::Error>;
}

// Test version of GundamProcessor that matches the implementation
pub struct GundamProcessor {
    pub cfg: cap_scale::gundam::GundamCfg,
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

// Mock capture source for testing
pub struct MockCaptureSource {
    pub frame_count: usize,
    pub width: u32,
    pub height: u32,
}

#[async_trait::async_trait]
impl CaptureSource for MockCaptureSource {
    async fn initialize(&mut self) -> Result<Size, anyhow::Error> {
        Ok(Size {
            w: self.width,
            h: self.height,
        })
    }

    async fn capture_frame(&mut self) -> Result<BgraFrame, anyhow::Error> {
        self.frame_count += 1;

        // Create a test frame with a simple pattern
        let pixel_count = (self.width * self.height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        // Create a pattern that changes based on frame count
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel_index = ((y * self.width + x) * 4) as usize;
                let r = ((x as f32 / self.width as f32) * 255.0) as u8;
                let g = ((y as f32 / self.height as f32) * 255.0) as u8;
                let b = (self.frame_count as u8 * 10) % 255;

                data[pixel_index] = b; // B
                data[pixel_index + 1] = g; // G
                data[pixel_index + 2] = r; // R
                data[pixel_index + 3] = 255; // A
            }
        }

        Ok(BgraFrame {
            data: Arc::new(data),
            width: self.width,
            height: self.height,
            stride: (self.width * 4) as usize,
            pts_ns: Some(self.frame_count as u64 * 1000000), // 1ms per frame
        })
    }

    async fn cleanup(&mut self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

// Processing pipeline for integration testing
pub struct ProcessingPipeline {
    pub processors: Vec<Box<dyn FrameProcessor>>,
}

impl ProcessingPipeline {
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    pub fn add_processor(&mut self, processor: Box<dyn FrameProcessor>) {
        self.processors.push(processor);
    }

    pub async fn initialize(&mut self, input_size: Size) -> Result<Size, anyhow::Error> {
        let mut current_size = input_size;
        for processor in &mut self.processors {
            current_size = processor.initialize(current_size).await?;
        }
        Ok(current_size)
    }

    pub async fn process_frame(
        &mut self,
        frame: BgraFrame,
    ) -> Result<Option<BgraFrame>, anyhow::Error> {
        let mut current_frame = Some(frame);
        for processor in &mut self.processors {
            if let Some(frame) = current_frame {
                current_frame = processor.process_frame(frame).await?;
            } else {
                break;
            }
        }
        Ok(current_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gundam_processor_pipeline_integration() {
        // Create a GundamProcessor
        let gundam_processor = GundamProcessor {
            cfg: cap_scale::gundam::GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        // Create processing pipeline
        let mut pipeline = ProcessingPipeline::new();
        pipeline.add_processor(Box::new(gundam_processor));

        // Initialize pipeline
        let input_size = Size { w: 1920, h: 1080 };
        let output_size = pipeline.initialize(input_size).await.unwrap();

        // Verify output size is reasonable
        assert!(output_size.w > 0);
        assert!(output_size.h > 0);

        // Create mock capture source
        let mut capture_source = MockCaptureSource {
            frame_count: 0,
            width: 1920,
            height: 1080,
        };

        // Initialize capture source
        let capture_size = capture_source.initialize().await.unwrap();
        assert_eq!(capture_size, input_size);

        // Capture a frame
        let input_frame = capture_source.capture_frame().await.unwrap();
        assert_eq!(input_frame.width, 1920);
        assert_eq!(input_frame.height, 1080);

        // Process through pipeline
        let result = pipeline.process_frame(input_frame).await.unwrap();
        let output_frame = result.unwrap();

        // Verify output frame
        assert!(output_frame.width > 0);
        assert!(output_frame.height > 0);
        assert_eq!(output_frame.stride, (output_frame.width * 4) as usize);
        assert_eq!(
            output_frame.data.len(),
            (output_frame.width * output_frame.height * 4) as usize
        );
        assert_eq!(output_frame.pts_ns, Some(1000000)); // First frame timestamp
    }

    #[tokio::test]
    async fn test_gundam_processor_multiple_frames() {
        // Create a GundamProcessor
        let gundam_processor = GundamProcessor {
            cfg: cap_scale::gundam::GundamCfg::default(),
            tile_buffers: Vec::new(),
            global_buffer: Vec::new(),
            output_size: Size { w: 0, h: 0 },
        };

        // Create processing pipeline
        let mut pipeline = ProcessingPipeline::new();
        pipeline.add_processor(Box::new(gundam_processor));

        // Initialize pipeline
        let input_size = Size { w: 1280, h: 720 };
        pipeline.initialize(input_size).await.unwrap();

        // Create mock capture source
        let mut capture_source = MockCaptureSource {
            frame_count: 0,
            width: 1280,
            height: 720,
        };
        capture_source.initialize().await.unwrap();

        // Process multiple frames
        for i in 0..5 {
            let input_frame = capture_source.capture_frame().await.unwrap();
            let result = pipeline.process_frame(input_frame).await.unwrap();
            let output_frame = result.unwrap();

            // Verify each frame
            assert!(output_frame.width > 0);
            assert!(output_frame.height > 0);
            assert_eq!(output_frame.pts_ns, Some((i + 1) as u64 * 1000000));
        }
    }

    #[tokio::test]
    async fn test_gundam_processor_different_resolutions() {
        let resolutions = vec![
            Size { w: 640, h: 480 },
            Size { w: 1024, h: 768 },
            Size { w: 1920, h: 1080 },
            Size { w: 2560, h: 1440 },
        ];

        for resolution in resolutions {
            // Create a GundamProcessor for each resolution
            let gundam_processor = GundamProcessor {
                cfg: cap_scale::gundam::GundamCfg::default(),
                tile_buffers: Vec::new(),
                global_buffer: Vec::new(),
                output_size: Size { w: 0, h: 0 },
            };

            // Create processing pipeline
            let mut pipeline = ProcessingPipeline::new();
            pipeline.add_processor(Box::new(gundam_processor));

            // Initialize pipeline
            let output_size = pipeline.initialize(resolution).await.unwrap();
            assert!(output_size.w > 0);
            assert!(output_size.h > 0);

            // Create mock capture source
            let mut capture_source = MockCaptureSource {
                frame_count: 0,
                width: resolution.w,
                height: resolution.h,
            };
            capture_source.initialize().await.unwrap();

            // Process a frame
            let input_frame = capture_source.capture_frame().await.unwrap();
            let result = pipeline.process_frame(input_frame).await.unwrap();
            let output_frame = result.unwrap();

            // Verify output
            assert!(output_frame.width > 0);
            assert!(output_frame.height > 0);
            assert_eq!(output_frame.width, output_size.w);
            assert_eq!(output_frame.height, output_size.h);
        }
    }
}
