//! # Core Infrastructure Module
//!
//! This module contains the fundamental infrastructure components for zero-copy
//! screen capture operations, including buffer management, inter-thread communication,
//! and performance analysis utilities.

pub mod buffer_pool;
pub mod performance_analysis;
pub mod ring_buffer;
