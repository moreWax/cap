//! Common test utilities and helpers for the cap library tests
//!
//! This module provides shared utilities for testing various components
//! of the screen capture library.

pub mod assertions;
pub mod mock_capture;
pub mod test_frames;

/// Mock capture source for testing without actual screen capture
pub mod mock_capture {
    use cap::core::{BgraFrame, Size};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// Mock capture source that generates test frames
    pub struct MockCapture {
        tx: mpsc::UnboundedSender<BgraFrame>,
        size: Size,
        frame_count: usize,
    }

    impl MockCapture {
        /// Create a new mock capture source
        pub fn new(tx: mpsc::UnboundedSender<BgraFrame>, size: Size) -> Self {
            Self {
                tx,
                size,
                frame_count: 0,
            }
        }

        /// Generate and send a test frame
        pub fn send_test_frame(&mut self) -> Result<(), Box<dyn std::error::Error>> {
            let frame = self.create_test_frame();
            self.tx.send(frame)?;
            self.frame_count += 1;
            Ok(())
        }

        /// Create a test frame with a pattern for verification
        fn create_test_frame(&self) -> BgraFrame {
            let pixel_count = (self.size.width * self.size.height) as usize;
            let mut data = vec![0u8; pixel_count * 4]; // BGRA = 4 bytes per pixel

            // Create a simple pattern: alternating colors based on frame count
            let color = match self.frame_count % 3 {
                0 => [255, 0, 0, 255], // Red
                1 => [0, 255, 0, 255], // Green
                _ => [0, 0, 255, 255], // Blue
            };

            for pixel in data.chunks_exact_mut(4) {
                pixel.copy_from_slice(&color);
            }

            BgraFrame {
                data: Arc::new(data),
                size: self.size,
            }
        }

        /// Get the number of frames sent
        pub fn frame_count(&self) -> usize {
            self.frame_count
        }
    }
}

/// Test frame utilities and constants
pub mod test_frames {
    use cap::core::{BgraFrame, Size};
    use std::sync::Arc;

    /// Standard test sizes
    pub const FHD_SIZE: Size = Size {
        width: 1920,
        height: 1080,
    };
    pub const HD_SIZE: Size = Size {
        width: 1280,
        height: 720,
    };
    pub const QHD_SIZE: Size = Size {
        width: 2560,
        height: 1440,
    };
    pub const UHD_SIZE: Size = Size {
        width: 3840,
        height: 2160,
    };

    /// Create a solid color test frame
    pub fn create_solid_frame(size: Size, r: u8, g: u8, b: u8, a: u8) -> BgraFrame {
        let pixel_count = (size.width * size.height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = b; // BGRA order
            pixel[1] = g;
            pixel[2] = r;
            pixel[3] = a;
        }

        BgraFrame {
            data: Arc::new(data),
            size,
        }
    }

    /// Create a checkerboard pattern frame for testing
    pub fn create_checkerboard_frame(size: Size) -> BgraFrame {
        let pixel_count = (size.width * size.height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        for y in 0..size.height {
            for x in 0..size.width {
                let pixel_index = ((y * size.width + x) * 4) as usize;
                let is_black = (x / 32 + y / 32) % 2 == 0;

                if is_black {
                    data[pixel_index..pixel_index + 4].copy_from_slice(&[0, 0, 0, 255]); // Black
                } else {
                    data[pixel_index..pixel_index + 4].copy_from_slice(&[255, 255, 255, 255]); // White
                }
            }
        }

        BgraFrame {
            data: Arc::new(data),
            size,
        }
    }

    /// Create a gradient frame for testing scaling
    pub fn create_gradient_frame(size: Size) -> BgraFrame {
        let pixel_count = (size.width * size.height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        for y in 0..size.height {
            for x in 0..size.width {
                let pixel_index = ((y * size.width + x) * 4) as usize;
                let r = ((x as f32 / size.width as f32) * 255.0) as u8;
                let g = ((y as f32 / size.height as f32) * 255.0) as u8;
                let b = 128u8; // Constant blue channel

                data[pixel_index..pixel_index + 4].copy_from_slice(&[b, g, r, 255]);
            }
        }

        BgraFrame {
            data: Arc::new(data),
            size,
        }
    }
}

/// Custom assertions for testing
pub mod assertions {
    use cap::core::{BgraFrame, Size};

    /// Assert that two frames have the same dimensions
    pub fn assert_frame_sizes_equal(left: &BgraFrame, right: &BgraFrame) {
        assert_eq!(
            left.size, right.size,
            "Frame sizes don't match: {}x{} vs {}x{}",
            left.size.width, left.size.height, right.size.width, right.size.height
        );
    }

    /// Assert that a frame has the expected size
    pub fn assert_frame_size(frame: &BgraFrame, expected: Size) {
        assert_eq!(
            frame.size, expected,
            "Frame size mismatch: expected {}x{}, got {}x{}",
            expected.width, expected.height, frame.size.width, frame.size.height
        );
    }

    /// Assert that frame data is not empty
    pub fn assert_frame_not_empty(frame: &BgraFrame) {
        assert!(!frame.data.is_empty(), "Frame data is empty");
        let expected_len = (frame.size.width * frame.size.height * 4) as usize;
        assert_eq!(
            frame.data.len(),
            expected_len,
            "Frame data length mismatch: expected {}, got {}",
            expected_len,
            frame.data.len()
        );
    }

    /// Assert that two frames have identical pixel data
    pub fn assert_frames_equal(left: &BgraFrame, right: &BgraFrame) {
        assert_frame_sizes_equal(left, right);
        assert_eq!(
            left.data.as_ref(),
            right.data.as_ref(),
            "Frame pixel data doesn't match"
        );
    }

    /// Assert that a frame contains a solid color
    pub fn assert_solid_color(
        frame: &BgraFrame,
        expected_b: u8,
        expected_g: u8,
        expected_r: u8,
        expected_a: u8,
    ) {
        assert_frame_not_empty(frame);

        for pixel in frame.data.chunks_exact(4) {
            assert_eq!(pixel[0], expected_b, "Blue channel mismatch");
            assert_eq!(pixel[1], expected_g, "Green channel mismatch");
            assert_eq!(pixel[2], expected_r, "Red channel mismatch");
            assert_eq!(pixel[3], expected_a, "Alpha channel mismatch");
        }
    }
}
