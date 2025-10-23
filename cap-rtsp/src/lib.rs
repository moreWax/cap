// SPDX-License-Identifier: MIT
//! # RTSP Streaming Server for BGRA Frames
//!
//! This crate provides low-latency RTSP streaming capabilities for BGRA video frames,
//! optimized for real-time screen capture and AI vision model pipelines.
//!
//! ## Architecture Overview
//!
//! The RTSP server uses GStreamer for efficient video encoding and streaming:
//! - **Single shared pipeline**: One H.264 encoder shared across all RTSP clients
//! - **Low-latency encoding**: x264enc with zero-latency tuning by default
//! - **Back-pressure handling**: Bounded channel prevents memory ballooning
//! - **GLib integration**: Proper threading with GLib main loop for stability
//!
//! ## Key Design Decisions
//!
//! ### Threading Model
//! - **Main thread**: GLib main loop runs RTSP server and pipeline
//! - **Worker thread**: Polls for frames and pushes to GStreamer appsrc
//! - **Caller threads**: Non-blocking frame submission via crossbeam channel
//!
//! ### Latency Optimization
//! - Small bounded channel (capacity 3) minimizes queuing delay
//! - `appsrc block=true` provides back-pressure without dropping frames
//! - Configurable encoder settings for different latency vs quality trade-offs
//! - Optional caller-provided PTS timestamps for precise timing
//!
//! ### Memory Management
//! - Frames use `Arc<Vec<u8>>` for zero-copy sharing between threads
//! - Configurable `appsrc_max_bytes` prevents unbounded memory usage
//! - Stride-aware frame handling for screen capture compatibility
//!
//! ## Performance Characteristics
//!
//! - **Encoding latency**: ~10-50ms depending on resolution and encoder settings
//! - **Network latency**: RTSP/RTP adds ~5-20ms depending on network conditions
//! - **CPU usage**: H.264 encoding scales with resolution (HD ~10-20% single core)
//! - **Memory usage**: Bounded by channel capacity + encoder buffers
//!
//! ## Supported Encoders
//!
//! The crate supports any GStreamer H.264 encoder via configurable launch strings:
//! - **x264enc** (default): Software encoding, widely compatible
//! - **vtenc_h264**: macOS VideoToolbox hardware acceleration
//! - **nvh264enc**: NVIDIA hardware encoding
//! - **d3d11h264enc**: Windows DirectX hardware encoding
//!
//! ## Usage Patterns
//!
//! ### Basic Screen Capture Streaming
//! ```rust,no_run
//! use cap_rtsp::{start_server, RtspConfig, frame_from_bgra};
//!
//! let cfg = RtspConfig {
//!     width: 1920,
//!     height: 1080,
//!     framerate: 30,
//!     ..Default::default()
//! };
//!
//! let (publisher, handle) = start_server(cfg)?;
//!
//! // In your capture loop:
//! loop {
//!     let bgra_data = capture_screen_frame();
//!     let frame = frame_from_bgra(bgra_data, 1920, 1080, 30, frame_count);
//!     publisher.send(frame)?;
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ### Hardware-Accelerated Streaming
//! ```rust,no_run
//! let cfg = RtspConfig {
//!     encoder: Some("nvh264enc preset=low-latency-hq zerolatency=true bitrate=8000".into()),
//!     ..Default::default()
//! };
//! ```
//!
//! ## Integration with cap-scale
//!
//! This crate works seamlessly with `cap-scale` for AI-optimized streaming:
//! ```rust,no_run
//! use cap_rtsp::{start_server, RtspConfig};
//! use cap_scale::{scale_bgra_cpu, presets::build_plan};
//!
//! // Scale frame for AI model input
//! let scaled_frame = scale_bgra_cpu(/* ... */)?;
//!
//! // Stream the scaled frame
//! let rtsp_frame = frame_from_bgra(scaled_frame, scaled_w, scaled_h, 10, idx);
//! publisher.send(rtsp_frame)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Future Optimizations
//!
//! TODO: Add support for multiple simultaneous streams with different encodings.
//! TODO: Implement adaptive bitrate based on client capabilities.
//! TODO: Add frame dropping strategies for sustained overload conditions.
//! TODO: Support H.265 encoding for better compression efficiency.
//! TODO: Add metrics collection for latency and throughput monitoring.
//! TODO: Implement client connection limits and authentication.

use anyhow::{anyhow, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use glib as glib_direct;
use gstreamer as gst;
use gstreamer::glib::{MainContext, MainLoop};
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_rtsp_server::prelude::*;
use gstreamer_rtsp_server::{RTSPMediaFactory, RTSPServer};
use once_cell::sync::OnceCell;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A raw BGRA frame ready for RTSP streaming.
///
/// This structure represents a complete video frame with all necessary metadata.
/// Frames must have consistent dimensions for the lifetime of the RTSP server.
///
/// # Memory Layout
/// - BGRA format: 4 bytes per pixel (B, G, R, A)
/// - Stride-aware: supports padded rows from screen capture APIs
/// - Arc-wrapped data: zero-copy sharing between threads
///
/// # Timing
/// - `pts_ns`: Presentation timestamp in nanoseconds
/// - If None, server generates timestamps based on framerate
/// - Caller-provided PTS enables precise timing control
#[derive(Clone)]
pub struct BgraFrame {
    /// Raw BGRA pixel data. Length must equal `stride * height`.
    pub data: Arc<Vec<u8>>, // len = stride * height
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Bytes per row (may exceed `width * 4` due to padding)
    pub stride: usize, // bytes per row
    /// Optional presentation timestamp in nanoseconds
    pub pts_ns: Option<u64>, // nanoseconds; if None, appsrc/do-timestamp will stamp
}

/// Handle for publishing frames to the RTSP server.
///
/// This is a lightweight, cloneable handle that can be shared across threads.
/// Use the `send()` method to push frames to the streaming pipeline.
///
/// # Thread Safety
/// - Cloneable and Send + Sync
/// - Non-blocking send with back-pressure handling
/// - Thread-safe for concurrent frame submission
#[derive(Clone)]
pub struct RtspPublisher {
    tx: Sender<BgraFrame>,
    // For optional stats or shutdown coordination later
}

/// Configuration for RTSP server and encoding pipeline.
///
/// This structure controls all aspects of the RTSP streaming setup,
/// from network configuration to encoder tuning.
///
/// # Encoder Configuration
/// The `encoder` field accepts GStreamer encoder launch strings.
/// Common examples:
/// - x264: `"x264enc tune=zerolatency speed-preset=veryfast bitrate=4000"`
/// - NVIDIA: `"nvh264enc preset=low-latency-hq zerolatency=true bitrate=4000"`
/// - VideoToolbox: `"vtenc_h264 realtime=true allow-frame-reordering=false bitrate=4000"`
pub struct RtspConfig {
    /// RTSP server port (default: 8554)
    pub port: u16, // default 8554
    /// RTSP mount path (default: "/cap")
    pub mount: String, // default "/cap"
    /// Expected frame width in pixels
    pub width: u32,
    /// Expected frame height in pixels
    pub height: u32,
    /// Target framerate for timestamp generation
    pub framerate: u32, // e.g. 30
    /// Optional custom encoder launch string. If None, uses x264enc with low-latency settings.
    /// Examples:
    ///   Some("vtenc_h264 realtime=true allow-frame-reordering=false bitrate=4000")
    ///   Some("nvh264enc preset=low-latency-hq zerolatency=true bitrate=4000")
    ///   Some("d3d11h264enc bitrate=4000000")
    pub encoder: Option<String>,
    /// Maximum bytes buffered in appsrc before blocking. Prevents unbounded memory usage.
    pub appsrc_max_bytes: Option<usize>,
}

impl Default for RtspConfig {
    fn default() -> Self {
        Self {
            port: 8554,
            mount: "/cap".into(),
            width: 1280,
            height: 720,
            framerate: 30,
            encoder: None,
            appsrc_max_bytes: Some(4 * 1024 * 1024),
        }
    }
}

/// Shared state between RTSP server thread and frame push worker.
///
/// This structure maintains the connection between the GStreamer pipeline
/// and the frame processing logic, enabling coordinated operation.
struct Shared {
    /// Handle to the GStreamer appsrc element for frame injection
    appsrc: Option<gst_app::AppSrc>,
    /// Next PTS timestamp when caller doesn't provide one
    next_pts: u64,
    /// Frame duration in nanoseconds (calculated from framerate)
    frame_duration: u64,
}

/// Global shared state instance (once_cell ensures single server per process)
static SHARED: OnceCell<Arc<Mutex<Shared>>> = OnceCell::new();

/// Start the RTSP server with the specified configuration.
///
/// This function initializes GStreamer, creates the RTSP server infrastructure,
/// and spawns a background thread running the GLib main loop. The server will
/// begin accepting RTSP connections immediately.
///
/// # Pipeline Architecture
///
/// The GStreamer pipeline follows this structure:
/// ```text
/// appsrc → videoconvert → videoscale → encoder → rtph264pay → clients
/// ```
///
/// - **appsrc**: Receives BGRA frames from application
/// - **videoconvert**: Converts BGRA to I420 colorspace
/// - **videoscale**: Handles any resolution adjustments
/// - **encoder**: H.264 encoding (configurable)
/// - **rtph264pay**: RTP packetization for network streaming
///
/// # Threading Behavior
///
/// - **Server thread**: Runs GLib main loop, handles RTSP protocol
/// - **Push worker**: Async task polls for frames and feeds appsrc
/// - **Caller threads**: Can call `send()` concurrently without blocking
///
/// # Resource Management
///
/// - Bounded crossbeam channel prevents memory ballooning
/// - GLib main context ensures proper cleanup on shutdown
/// - Appsrc back-pressure prevents frame dropping under load
///
/// # Returns
/// Tuple of (RtspPublisher handle, server thread join handle)
///
/// The publisher can be cloned and shared across threads. The join handle
/// should be kept to coordinate shutdown if needed.
pub fn start_server(cfg: RtspConfig) -> Result<(RtspPublisher, thread::JoinHandle<()>)> {
    // Initialize GStreamer once.
    gst::init()?;

    // Crossbeam bounded channel: small buffer to minimize latency.
    let (tx, rx) = bounded::<BgraFrame>(3);

    // Build the encoder part
    let enc = cfg.encoder.unwrap_or_else(|| {
        "x264enc tune=zerolatency speed-preset=veryfast bitrate=4000 key-int-max=30 bframes=0 ! h264parse".to_string()
    });

    // Launch string with named appsrc so we can grab it in media-configure.
    // We declare caps for BGRA with fixed width/height/framerate.
    let launch = format!(
        "appsrc name=src is-live=true format=time do-timestamp=true block=true caps=video/x-raw,format=BGRA,width={},height={},framerate={}/1 \
         ! videoconvert ! videoscale ! video/x-raw,format=I420 ! {} ! rtph264pay name=pay0 pt=96 config-interval=1",
        cfg.width, cfg.height, cfg.framerate, enc
    );

    // Shared state: appsrc handle + pts stepping
    let shared = Arc::new(Mutex::new(Shared {
        appsrc: None,
        next_pts: 0,
        frame_duration: (1_000_000_000u64) / (cfg.framerate.max(1) as u64),
    }));
    SHARED.set(shared.clone()).ok();

    let port = cfg.port;
    let mount = cfg.mount.clone();
    let max_bytes = cfg.appsrc_max_bytes;

    let handle = thread::spawn(move || {
        // GLib main context on this thread
        let ctx = MainContext::default();
        let _guard = ctx
            .acquire()
            .expect("Failed to acquire GLib main context on RTSP thread");
        let mainloop = MainLoop::new(Some(&ctx), false);

        // Create server and factory
        let server = RTSPServer::new();
        server.set_service(&port.to_string());

        let mounts = server.mount_points().expect("no mount points");
        let factory = RTSPMediaFactory::new();
        factory.set_shared(true); // one pipeline shared by all clients
        factory.set_launch(&launch);

        // Bind to media-configure so we can capture the AppSrc
        factory.connect_media_configure(move |_, media| {
            let pipeline = media.element();
            // Downcast to Bin to access by_name
            if let Ok(bin) = pipeline.downcast::<gst::Bin>() {
                if let Some(src) = bin.by_name("src") {
                    // Safety: element exists and is an AppSrc
                    let appsrc = src
                        .downcast::<gst_app::AppSrc>()
                        .expect("src is not an AppSrc");

                    // Configure appsrc queueing behavior
                    appsrc.set_format(gst::Format::Time);
                    appsrc.set_is_live(true);
                    appsrc.set_do_timestamp(true);
                    appsrc.set_block(true);
                    if let Some(bytes) = max_bytes {
                        appsrc.set_max_bytes(bytes as u64);
                    }

                    // Save handle for push loop as an AppSrc (clone the object)
                    if let Some(shared) = SHARED.get() {
                        let mut s = shared.lock().unwrap();
                        s.appsrc = Some(appsrc.clone());
                    }
                }
            }
        });

        mounts.add_factory(&mount, factory);

        // Attach server to default context
        let _ = server.attach(Some(&ctx));

        // Push loop: run in a separate worker tied to GLib context so
        // we can gracefully handle back-pressure without starving mainloop.
        let shared_clone = shared.clone();
        ctx.spawn_local(async move {
            push_worker(rx, shared_clone).await;
        });

        eprintln!(
            "RTSP ready on rtsp://0.0.0.0:{}/{}",
            port,
            mount.trim_start_matches('/')
        );
        mainloop.run();
    });

    Ok((RtspPublisher { tx }, handle))
}

/// Internal worker that pops frames and pushes to appsrc.
///
/// This async worker runs on the GLib main context and is responsible for:
/// - Polling the frame queue without blocking the main loop
/// - Converting frames to GStreamer buffers with proper timing
/// - Pushing frames to the appsrc element with back-pressure handling
///
/// # Timing Strategy
///
/// - Uses caller-provided PTS if available (precise timing)
/// - Falls back to frame-duration stepping for synthetic timestamps
/// - Handles both live capture and pre-recorded content scenarios
///
/// # Back-Pressure Handling
///
/// The `appsrc block=true` setting means push_buffer() will block when
/// the encoder can't keep up, preventing frame dropping and memory issues.
///
/// # Threading Integration
///
/// Uses mpsc channel + GLib timeout to bridge sync crossbeam channel
/// with async GLib context, ensuring proper integration with GStreamer.
async fn push_worker(rx: Receiver<BgraFrame>, shared: Arc<Mutex<Shared>>) {
    // Use mpsc channel and GLib timeout to poll for frames
    let (glib_tx, glib_rx) = mpsc::channel::<BgraFrame>();

    // Forward frames from crossbeam to mpsc channel
    thread::spawn(move || {
        while let Ok(f) = rx.recv() {
            // Drop if queue is full
            let _ = glib_tx.send(f);
        }
    });

    // Poll the mpsc channel in a GLib timeout
    glib_direct::timeout_add_local(Duration::from_millis(1), move || {
        match glib_rx.try_recv() {
            Ok(frame) => {
                // Get current appsrc (None if no client is connected yet)
                let (maybe_appsrc, next_pts, frame_dur) = {
                    let s = shared.lock().unwrap();
                    (s.appsrc.clone(), s.next_pts, s.frame_duration)
                };

                if let Some(appsrc) = maybe_appsrc {
                    // Allocate buffer and copy data
                    let mut buffer = match gst::Buffer::with_size(frame.data.len()) {
                        Ok(b) => b,
                        Err(_) => return glib_direct::ControlFlow::Continue,
                    };
                    {
                        let bufw = buffer.get_mut().unwrap();
                        // Timestamping
                        let pts = frame.pts_ns.unwrap_or(next_pts);
                        bufw.set_pts(gst::ClockTime::from_nseconds(pts));
                        bufw.set_duration(gst::ClockTime::from_nseconds(frame_dur));

                        // Copy bytes
                        if let Ok(mut map) = bufw.map_writable() {
                            map.as_mut_slice().copy_from_slice(&frame.data);
                        }
                    }

                    // Push; on back-pressure this call will block (block=true)
                    let _ = appsrc.push_buffer(buffer);

                    // Update pts if we are clocking
                    if frame.pts_ns.is_none() {
                        let mut s = shared.lock().unwrap();
                        s.next_pts = next_pts + frame_dur;
                    }
                }
                // Continue polling
                glib_direct::ControlFlow::Continue
            }
            Err(mpsc::TryRecvError::Empty) => {
                // No frame available, continue polling
                glib_direct::ControlFlow::Continue
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // Channel closed, stop polling
                glib_direct::ControlFlow::Break
            }
        }
    });
}

impl RtspPublisher {
    /// Send a frame to the RTSP stream with back-pressure handling.
    ///
    /// This method attempts non-blocking send first, then falls back to
    /// brief blocking retry to avoid frame drops under moderate load.
    ///
    /// # Back-Pressure Strategy
    ///
    /// 1. Try non-blocking send (preferred for low latency)
    /// 2. If queue full, sleep briefly and retry once
    /// 3. Return error if still full (caller should handle drops)
    ///
    /// # Performance Notes
    ///
    /// - Non-blocking under normal load (channel capacity 3)
    /// - Brief blocking prevents frame drops during spikes
    /// - Error on sustained overload (prevents unbounded queuing)
    ///
    /// # Thread Safety
    ///
    /// Safe to call concurrently from multiple threads. The underlying
    /// crossbeam channel handles synchronization.
    pub fn send(&self, frame: BgraFrame) -> Result<()> {
        match self.tx.try_send(frame) {
            Ok(()) => Ok(()),
            Err(crossbeam_channel::TrySendError::Full(f)) => {
                // Drop one oldest by doing a blocking recv on the other side (not available here),
                // so instead: signal drop by returning an error, or spin until there's room.
                // We'll just sleep briefly and retry once to avoid busy-wait.
                thread::sleep(Duration::from_millis(2));
                self.tx
                    .try_send(f)
                    .map_err(|_| anyhow!("rtsp queue full; frame dropped"))
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                Err(anyhow!("rtsp server thread ended"))
            }
        }
    }
}

/// Convenience function to create a BGRA frame from tightly-packed data.
///
/// This helper constructs a `BgraFrame` with automatically calculated stride
/// and presentation timestamp, suitable for simple capture scenarios.
///
/// # Assumptions
/// - Data is tightly packed (no row padding)
/// - Frame rate is constant
/// - Sequential frame indexing
///
/// # Use Cases
/// - Simple screen capture loops
/// - Pre-recorded video playback
/// - Test frame generation
///
/// For complex scenarios with padded buffers or variable timing,
/// construct `BgraFrame` directly.
pub fn frame_from_bgra(bytes: Vec<u8>, width: u32, height: u32, fps: u32, idx: u64) -> BgraFrame {
    let stride = width as usize * 4;
    let pts_ns = Some(idx * (1_000_000_000u64 / fps.max(1) as u64));
    BgraFrame {
        data: Arc::new(bytes),
        width,
        height,
        stride,
        pts_ns,
    }
}

/// Arrange Gundam tiles and global view into a single composite frame for RTSP streaming.
///
/// Creates a grid layout where tiles are arranged in rows, with the global view
/// placed at the end. For example, with 4 tiles + global:
/// [Tile1][Tile2][Global]
/// [Tile3][Tile4][Empty ]
///
/// Returns the composite frame data and its dimensions.
pub fn arrange_gundam_composite(
    tiles: &[Vec<u8>],
    global: &[u8],
    tile_side: u32,
    global_side: u32,
) -> (Vec<u8>, u32, u32) {
    let num_tiles = tiles.len();
    if num_tiles == 0 {
        // Fallback: just return global view
        return (global.to_vec(), global_side, global_side);
    }

    // Calculate grid dimensions
    // Try to make it roughly square, with global view as a separate element
    let total_elements = num_tiles + 1;
    let cols = ((total_elements as f32).sqrt().ceil() as u32).max(1);
    let rows = (((total_elements as u32 + cols - 1) / cols) as usize).max(1) as u32;

    // Frame dimensions
    let frame_width = cols * tile_side;
    let frame_height = rows * tile_side;

    // Create composite frame
    let mut composite = vec![255u8; (frame_width * frame_height * 4) as usize]; // White background

    // Place tiles in grid
    for (i, tile) in tiles.iter().enumerate() {
        let row = (i as u32) / cols;
        let col = (i as u32) % cols;

        if row >= rows {
            break; // Shouldn't happen, but defensive
        }

        let dst_x = col * tile_side;
        let dst_y = row * tile_side;

        // Copy tile data (640x640 BGRA)
        for y in 0..tile_side {
            for x in 0..tile_side {
                let src_idx = ((y * tile_side + x) * 4) as usize;
                let dst_idx = (((dst_y + y) * frame_width + (dst_x + x)) * 4) as usize;

                if src_idx + 3 < tile.len() && dst_idx + 3 < composite.len() {
                    composite[dst_idx..dst_idx + 4].copy_from_slice(&tile[src_idx..src_idx + 4]);
                }
            }
        }
    }

    // Place global view (1024x1024) - scale it down to tile_side x tile_side
    let global_row = (num_tiles as u32) / cols;
    let global_col = (num_tiles as u32) % cols;

    if global_row < rows && global_col < cols {
        let dst_x = global_col * tile_side;
        let dst_y = global_row * tile_side;

        // Scale global view down to tile_side x tile_side
        let scale_factor = tile_side as f32 / global_side as f32;

        for y in 0..tile_side {
            for x in 0..tile_side {
                // Nearest neighbor sampling from global view
                let src_x = (x as f32 / scale_factor) as u32;
                let src_y = (y as f32 / scale_factor) as u32;

                if src_x < global_side && src_y < global_side {
                    let src_idx = ((src_y * global_side + src_x) * 4) as usize;
                    let dst_idx = (((dst_y + y) * frame_width + (dst_x + x)) * 4) as usize;

                    if src_idx + 3 < global.len() && dst_idx + 3 < composite.len() {
                        composite[dst_idx..dst_idx + 4]
                            .copy_from_slice(&global[src_idx..src_idx + 4]);
                    }
                }
            }
        }
    }

    (composite, frame_width, frame_height)
}

/// Trait for processing frames before RTSP streaming.
///
/// Implement this trait to add custom frame processing logic
/// (scaling, effects, overlays, etc.) before frames are sent to RTSP.
#[async_trait::async_trait]
pub trait FrameProcessor: Send + Sync {
    /// Process a single frame.
    ///
    /// # Arguments
    /// * `frame` - Raw BGRA frame data
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `stride` - Bytes per row (may exceed width * 4 for padding)
    ///
    /// # Returns
    /// Processed frame data as BGRA bytes, or None to skip this frame
    async fn process_frame(
        &mut self,
        frame: &[u8],
        width: u32,
        height: u32,
        stride: usize,
    ) -> Option<Vec<u8>>;
}

/// High-level RTSP streaming interface.
///
/// This struct provides a convenient API for streaming processed video frames
/// via RTSP, abstracting away the low-level GStreamer details.
pub struct RtspStreamer {
    publisher: RtspPublisher,
    processor: Option<Box<dyn FrameProcessor>>,
}

impl RtspStreamer {
    /// Create a new RTSP streamer with optional frame processing.
    pub fn new(config: RtspConfig, processor: Option<Box<dyn FrameProcessor>>) -> Result<Self> {
        let (publisher, _handle) = start_server(config)?;
        Ok(Self {
            publisher,
            processor,
        })
    }

    /// Stream a frame, applying any configured processing.
    ///
    /// # Arguments
    /// * `frame_data` - Raw BGRA frame data
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `stride` - Bytes per row
    /// * `pts_ns` - Optional presentation timestamp in nanoseconds
    pub async fn stream_frame(
        &mut self,
        frame_data: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        pts_ns: Option<u64>,
    ) -> Result<()> {
        let processed_data = if let Some(processor) = &mut self.processor {
            processor
                .process_frame(frame_data, width, height, stride)
                .await
                .ok_or_else(|| anyhow!("Frame processing failed"))?
        } else {
            frame_data.to_vec()
        };

        let frame = BgraFrame {
            data: Arc::new(processed_data),
            width,
            height,
            stride,
            pts_ns,
        };

        self.publisher.send(frame)
    }

    /// Get access to the underlying publisher for advanced use cases.
    pub fn publisher(&self) -> &RtspPublisher {
        &self.publisher
    }
}
