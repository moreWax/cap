//! # Processing Module
//!
//! This module contains the frame processing pipeline for screen capture operations.

pub mod processing;

// Re-export commonly used types for convenience
#[cfg(feature = "rtsp-streaming")]
pub use processing::{
    FileStream, FrameProcessor, GundamProcessor, ProcessingPipeline, RtspStream, ScalingProcessor,
    Stream, StreamMultiplexer,
};
pub use processing::{Size, StreamConfig, StreamFormat};
