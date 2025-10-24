//! Integration tests for Gundam Processor in CaptureSessionBuilder
//!
//! These tests validate that the Gundam processor can be added to a capture session
//! and works correctly with the session builder pattern.

#[cfg(feature = "rtsp-streaming")]
use hybrid_screen_capture::processing::Stream;
use hybrid_screen_capture::processing::{StreamConfig, StreamFormat};
#[cfg(feature = "rtsp-streaming")]
use hybrid_screen_capture::session::*;
use std::sync::Arc;

// Mock stream for testing that just captures output frames
#[derive(Clone)]
struct MockStream {
    frames_received: Arc<std::sync::Mutex<Vec<cap_rtsp::BgraFrame>>>,
    config: StreamConfig,
}

impl MockStream {
    fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            frames_received: Arc::new(std::sync::Mutex::new(Vec::new())),
            config: StreamConfig {
                width,
                height,
                fps,
                format: StreamFormat::File {
                    path: "test.mp4".into(),
                },
            },
        }
    }

    fn frame_count(&self) -> usize {
        self.frames_received.lock().unwrap().len()
    }

    fn get_last_frame(&self) -> Option<cap_rtsp::BgraFrame> {
        self.frames_received.lock().unwrap().last().cloned()
    }
}

#[async_trait::async_trait]
#[cfg(feature = "rtsp-streaming")]
impl Stream for MockStream {
    async fn send_frame(&mut self, frame: cap_rtsp::BgraFrame) -> Result<(), anyhow::Error> {
        self.frames_received.lock().unwrap().push(frame);
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn config(&self) -> &StreamConfig {
        &self.config
    }

    async fn initialize(&mut self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

// Mock capture source for testing
struct MockCaptureSource {
    frame_count: std::sync::Mutex<usize>,
    width: u32,
    height: u32,
}

impl MockCaptureSource {
    fn new(width: u32, height: u32) -> Self {
        Self {
            frame_count: std::sync::Mutex::new(0),
            width,
            height,
        }
    }
}

#[async_trait::async_trait]
#[cfg(feature = "rtsp-streaming")]
impl CaptureSource for MockCaptureSource {
    async fn capture_frame(&mut self) -> Result<cap_rtsp::BgraFrame, anyhow::Error> {
        let mut count = self.frame_count.lock().unwrap();
        *count += 1;

        // Create a test frame
        let pixel_count = (self.width * self.height) as usize;
        let mut data = vec![0u8; pixel_count * 4];

        // Simple pattern
        for i in 0..pixel_count {
            data[i * 4] = (i % 256) as u8; // B
            data[i * 4 + 1] = ((i / 256) % 256) as u8; // G
            data[i * 4 + 2] = 128; // R
            data[i * 4 + 3] = 255; // A
        }

        Ok(cap_rtsp::BgraFrame {
            data: Arc::new(data),
            width: self.width,
            height: self.height,
            stride: (self.width * 4) as usize,
            pts_ns: Some(*count as u64 * 1000000), // 1ms per frame
        })
    }

    fn input_size(&self) -> hybrid_screen_capture::processing::Size {
        hybrid_screen_capture::processing::Size {
            w: self.width,
            h: self.height,
        }
    }

    async fn initialize(&mut self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_capture_session_builder_with_gundam() {
    // Test that the session initializes correctly with Gundam
    // We can verify Gundam is working by checking the output size
    let mock_stream1 = MockStream::new(1920, 1080, 30);
    let mock_source1 = MockCaptureSource::new(1920, 1080);

    let mut session_with_gundam = CaptureSession::builder()
        .with_gundam()
        .with_stream(mock_stream1.clone())
        .with_capture_source(mock_source1)
        .build()
        .unwrap();

    let output_size_gundam = session_with_gundam.get_output_size().await.unwrap();

    // Create comparison session without Gundam
    let mock_stream2 = MockStream::new(1920, 1080, 30);
    let mock_source2 = MockCaptureSource::new(1920, 1080);

    let mut session_no_gundam = CaptureSession::builder()
        .with_stream(mock_stream2.clone())
        .with_capture_source(mock_source2)
        .build()
        .unwrap();

    let output_size_no_gundam = session_no_gundam.get_output_size().await.unwrap();

    // Gundam should produce different output dimensions
    assert!(
        output_size_gundam.w != output_size_no_gundam.w
            || output_size_gundam.h != output_size_no_gundam.h,
        "Gundam should change output dimensions: with_gundam={}x{}, without={}x{}",
        output_size_gundam.w,
        output_size_gundam.h,
        output_size_no_gundam.w,
        output_size_no_gundam.h
    );

    // Gundam output should be larger (composite of tiles + global view)
    let pixels_gundam = output_size_gundam.w * output_size_gundam.h;
    let pixels_no_gundam = output_size_no_gundam.w * output_size_no_gundam.h;
    assert!(
        pixels_gundam > pixels_no_gundam,
        "Gundam composite should have more pixels: {} vs {}",
        pixels_gundam,
        pixels_no_gundam
    );
}

#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_gundam_processor_integration_behavior() {
    // Test the Gundam processor behavior by testing the builder pattern
    // Since we can't easily inspect the internal pipeline, we'll test that
    // the builder correctly adds processors and the session behaves as expected

    let mock_stream = MockStream::new(1920, 1080, 30);
    let mock_source = MockCaptureSource::new(1920, 1080);

    // Build session with Gundam
    let mut session = CaptureSession::builder()
        .with_gundam()
        .with_stream(mock_stream.clone())
        .with_capture_source(mock_source)
        .build()
        .unwrap();

    // Test initialization
    let input_size = hybrid_screen_capture::processing::Size { w: 1920, h: 1080 };
    let output_size = session.get_output_size().await.unwrap();

    // Gundam should produce output different from input
    // The exact size depends on the tiling algorithm, but it should be different
    assert!(
        output_size.w != input_size.w || output_size.h != input_size.h,
        "Gundam should change output dimensions, got input: {}x{}, output: {}x{}",
        input_size.w,
        input_size.h,
        output_size.w,
        output_size.h
    );

    // The output should be larger due to tiling
    let input_pixels = input_size.w * input_size.h;
    let output_pixels = output_size.w * output_size.h;
    assert!(
        output_pixels > input_pixels,
        "Gundam composite should have more pixels than input ({} vs {})",
        output_pixels,
        input_pixels
    );
}

#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_gundam_processor_different_resolutions() {
    let resolutions = vec![
        (640, 480, "small resolution"),
        (1024, 768, "standard resolution"),
        (1920, 1080, "FHD resolution"),
    ];

    for (width, height, description) in resolutions {
        let mock_stream = MockStream::new(width, height, 30);
        let mock_source = MockCaptureSource::new(width, height);

        let mut session = CaptureSession::builder()
            .with_gundam()
            .with_stream(mock_stream.clone())
            .with_capture_source(mock_source)
            .build()
            .unwrap();

        // Test initialization
        let output_size = session.get_output_size().await.unwrap();
        let input_size = hybrid_screen_capture::processing::Size {
            w: width,
            h: height,
        };

        assert!(
            output_size.w > 0,
            "Width should be positive for {}",
            description
        );
        assert!(
            output_size.h > 0,
            "Height should be positive for {}",
            description
        );
        assert!(
            output_size.w >= input_size.w || output_size.h >= input_size.h,
            "Output should be at least as large as input in some dimension for {}",
            description
        );
    }
}
