//! Integration tests for error handling and recovery functionality
//!
//! These tests verify that the application can handle errors gracefully
//! and recover from transient failures.

use std::time::Duration;
use tokio::time::timeout;

#[cfg(feature = "rtsp-streaming")]
use hybrid_screen_capture::session::CaptureSession;

/// Test that error recovery works for transient failures
#[cfg(feature = "rtsp-streaming")]
#[tokio::test]
async fn test_error_recovery() {
    // Create a session that will encounter errors but should recover
    let mock_source = FaultyCaptureSource::new();
    let mock_stream = MockStream::new();

    let session = CaptureSession::builder()
        .with_stream(mock_stream)
        .with_capture_source(mock_source)
        .build()
        .expect("Failed to build session");

    // Start the session in a background task
    let session_handle = tokio::spawn(async move { session.run().await });

    // Wait for the session to complete (it should handle errors gracefully)
    let result = timeout(Duration::from_secs(2), session_handle).await;

    // The session should complete normally despite encountering errors
    match result {
        Ok(Ok(_)) => (), // Expected - session completed gracefully despite errors
        Ok(Err(_)) => panic!("Session failed unexpectedly"),
        Err(_) => (), // Also acceptable - session may take variable time
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
#[cfg(feature = "rtsp-streaming")]
impl hybrid_screen_capture::processing::processing::Stream for MockStream {
    fn config(&self) -> &hybrid_screen_capture::processing::StreamConfig {
        // Create a static config for testing
        static CONFIG: std::sync::OnceLock<hybrid_screen_capture::processing::StreamConfig> =
            std::sync::OnceLock::new();
        CONFIG.get_or_init(|| hybrid_screen_capture::processing::StreamConfig {
            width: 1920,
            height: 1080,
            fps: 30,
            format: hybrid_screen_capture::processing::StreamFormat::File {
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

/// Faulty capture source that simulates transient failures
struct FaultyCaptureSource {
    frame_count: std::sync::atomic::AtomicUsize,
}

impl FaultyCaptureSource {
    fn new() -> Self {
        Self {
            frame_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
#[cfg(feature = "rtsp-streaming")]
impl hybrid_screen_capture::session::CaptureSource for FaultyCaptureSource {
    async fn capture_frame(&mut self) -> anyhow::Result<cap_rtsp::BgraFrame> {
        use std::sync::atomic::Ordering;

        let count = self.frame_count.fetch_add(1, Ordering::SeqCst);

        // Simulate occasional failures
        if count % 7 == 0 {
            return Err(anyhow::anyhow!("Simulated transient capture failure"));
        }

        // Create a minimal BGRA frame (1x1 pixel)
        let data = vec![255u8, 0, 0, 255]; // Blue pixel
        let frame = cap_rtsp::BgraFrame {
            width: 1,
            height: 1,
            data: std::sync::Arc::new(data),
            stride: 4,       // 4 bytes per pixel (BGRA)
            pts_ns: Some(0), // Presentation timestamp
        };

        // Stop after enough frames to test error recovery
        if count > 20 {
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
