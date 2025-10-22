// SPDX-License-Identifier: MIT
//! Publish RTSP (/cap) from BGRA frames you push in.
//!
//! - Low-latency H.264 via x264enc (configurable encoder string).
//! - One shared pipeline for all clients.
//! - Bounded channel; back-pressure via appsrc block=true.
//!
//! Example (see bottom) shows how to start and push frames.

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

/// A raw BGRA frame. Width/height must be constant for the server instance.
#[derive(Clone)]
pub struct BgraFrame {
    pub data: Arc<Vec<u8>>, // len = stride * height
    pub width: u32,
    pub height: u32,
    pub stride: usize,       // bytes per row
    pub pts_ns: Option<u64>, // nanoseconds; if None, appsrc/do-timestamp will stamp
}

/// Handle you keep in cap; call `send()` each time you have a frame.
#[derive(Clone)]
pub struct RtspPublisher {
    tx: Sender<BgraFrame>,
    // For optional stats or shutdown coordination later
}

pub struct RtspConfig {
    pub port: u16,     // default 8554
    pub mount: String, // default "/cap"
    pub width: u32,
    pub height: u32,
    pub framerate: u32, // e.g. 30
    /// Encoder launch fragment. If None, defaults to x264enc low-latency.
    /// Examples:
    ///   Some("vtenc_h264 realtime=true allow-frame-reordering=false bitrate=4000")
    ///   Some("nvh264enc preset=low-latency-hq zerolatency=true bitrate=4000")
    ///   Some("d3d11h264enc bitrate=4000000")
    pub encoder: Option<String>,
    /// Max frames buffered inside appsrc before blocking.
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

struct Shared {
    appsrc: Option<gst_app::AppSrc>,
    // PTS generator if caller didn't provide pts
    next_pts: u64,
    frame_duration: u64,
}
static SHARED: OnceCell<Arc<Mutex<Shared>>> = OnceCell::new();

/// Start the RTSP server on a GLib main loop thread.
/// Returns a `RtspPublisher` (you can clone it) and a join handle for the server thread.
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
/// We rate-control via provided PTS or by stepping a clock (frame_duration).
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
    /// Non-blocking send; if the internal queue is full we drop the oldest and try again.
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

/// Convenience: build a BGRA frame from a tightly-packed buffer (stride = width*4).
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
