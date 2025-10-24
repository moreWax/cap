//! Integration tests for graceful shutdown functionality
//!
//! These tests verify that the CaptureSession can be shut down gracefully
//! without resource leaks or hanging processes.

use std::time::Duration;
use tokio::time::timeout;

#[cfg(feature = "rtsp-streaming")]
use hybrid_screen_capture::processing::processing::{Stream, StreamConfig, StreamFormat};
use hybrid_screen_capture::session::CaptureSession;

/// Test that graceful shutdown works correctly
#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_graceful_shutdown() {
    // Create a minimal session with a mock capture source and mock stream
    let mock_source = MockCaptureSource::new();
    let mock_stream = MockStream::new();

    let session = CaptureSession::builder()
        .with_stream(mock_stream)
        .with_capture_source(mock_source)
        .build()
        .expect("Failed to build session");

    // Start the session in a background task
    let session_handle = tokio::spawn(async move { session.run().await });

    // Wait for the session to complete (it should shut down gracefully after mock frames)
    let result = timeout(Duration::from_secs(5), session_handle).await;

    // The session should complete normally (not be aborted)
    match result {
        Ok(Ok(_)) => (), // Expected - session completed gracefully
        Ok(Err(_)) => panic!("Session was aborted unexpectedly"),
        Err(_) => panic!("Session did not complete within timeout"),
    }
}

/// Mock stream for testing
struct MockStream;

impl MockStream {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Stream for MockStream {
    fn config(&self) -> &StreamConfig {
        // Create a static config for testing
        static CONFIG: std::sync::OnceLock<StreamConfig> = std::sync::OnceLock::new();
        CONFIG.get_or_init(|| StreamConfig {
            width: 1920,
            height: 1080,
            fps: 30,
            format: StreamFormat::File {
                path: "test.mp4".to_string(),
            },
        })
    }

    async fn initialize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn send_frame(&mut self, _frame: cap_rtsp::BgraFrame) -> anyhow::Result<()> {
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Mock capture source for testing
struct MockCaptureSource {
    frame_count: std::sync::atomic::AtomicUsize,
}

impl MockCaptureSource {
    fn new() -> Self {
        Self {
            frame_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl hybrid_screen_capture::session::CaptureSource for MockCaptureSource {
    async fn capture_frame(&mut self) -> anyhow::Result<cap_rtsp::BgraFrame> {
        use std::sync::atomic::Ordering;

        // Simulate capturing a frame
        let count = self.frame_count.fetch_add(1, Ordering::SeqCst);

        // Create a minimal BGRA frame (1x1 pixel)
        let data = vec![255u8, 0, 0, 255]; // Blue pixel
        let frame = cap_rtsp::BgraFrame {
            width: 1,
            height: 1,
            data: std::sync::Arc::new(data),
            stride: 4,       // 4 bytes per pixel (BGRA)
            pts_ns: Some(0), // Presentation timestamp
        };

        // Stop after a few frames to prevent infinite loop in tests
        if count > 10 {
            // Simulate an error to stop the session
            return Err(anyhow::anyhow!("Test completed"));
        }

        Ok(frame)
    }

    fn input_size(&self) -> hybrid_screen_capture::processing::Size {
        hybrid_screen_capture::processing::Size { w: 1, h: 1 }
    }

    async fn initialize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn shutdown(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
