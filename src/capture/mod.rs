// # Capture Module
//
// This module contains platform-specific screen capture implementations.

pub mod scrap;
pub mod session_sources;
#[cfg(feature = "wayland-pipe")]
pub mod wayland;
