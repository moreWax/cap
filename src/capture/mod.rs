//! # Capture Module
//!
//! This module contains platform-specific screen capture implementations.

pub mod scrap;
#[cfg(feature = "wayland-pipe")]
pub mod wayland;
