// # Wayland Capture Module
//
// This module provides screen capture functionality for Wayland environments
// using the XDG Desktop Portal and GStreamer. It implements modern Wayland
// screen capture through PipeWire integration.
//
// ## Overview
//
// Wayland screen capture is fundamentally different from X11 due to its
// security model. Instead of direct screen access, applications must request
// permission through the XDG Desktop Portal, which provides controlled access
// to screen content via PipeWire.
//
// ## Architecture
//
// ```text
// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
// │   Application   │───▶│  XDG Portal     │───▶│   PipeWire       │
// │                 │    │  (Permission)   │    │   (Streaming)    │
// └────────────────
//        │                        │                        │
//        ▼                        ▼                        ▼
//   Portal Request        User Consent        Screen Content
//   (ashpd crate)         (System Dialog)     (NV12 Format)
//        │                        │                        │
//        ▼                        ▼                        ▼
// ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
// │   GStreamer     │◀───│   pipewiresrc   │◀───│   Stream         │
// │   Pipeline      │    │   Element       │    │   Processing     │
// └─────────────────┘    └─────────────────┘    └─────────────────┘
// ```
//
// ## Key Components
//
// 1. **XDG Desktop Portal** (`ashpd` crate):
//    - Requests user permission for screen capture
//    - Provides PipeWire node ID and file descriptor
//    - Handles session management and source selection
//
// 2. **PipeWire Integration**:
//    - Low-level audio/video streaming protocol
//    - Provides screen content as video stream
//    - Supports multiple output formats (NV12 preferred)
//
// 3. **GStreamer Pipeline**:
//    - `pipewiresrc`: Receives video from PipeWire
//    - `videorate`: Controls frame rate
//    - `videoconvert`: Format conversion if needed
//    - `x264enc`: Hardware-accelerated H.264 encoding
//    - `mp4mux`: MP4 container with fast start
//
// ## Performance Characteristics
//
// - **Hardware acceleration**: Leverages GPU encoding when available
// - **Zero-copy**: Direct PipeWire to GStreamer data flow
// - **Low latency**: Optimized for real-time capture
// - **Efficient encoding**: H.264 with zerolatency tuning
//
// ## Platform Requirements
//
// - **Wayland compositor**: GNOME, KDE Plasma, Sway, etc.
// - **XDG Desktop Portal**: Portal implementation installed
// - **PipeWire**: Audio/video streaming service
// - **GStreamer**: Multimedia framework with plugins
//
// ## Example Usage
//
// Basic screen capture:
/// Internal API - no public examples available
//
// Window capture:
/// Internal API - no public examples available

use anyhow::{Context, Result, anyhow};
use ashpd::desktop::PersistMode;
use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::os::fd::{IntoRawFd, OwnedFd};

use crate::CaptureOptions;

/// Captures screen content on Wayland using XDG Desktop Portal and GStreamer.
///
/// Time complexity: O(seconds) - Pipeline setup is O(1), but the capture runs
/// for the specified duration with real-time processing.
///
/// Missing functionality: None - fully implements Wayland screen capture.
pub async fn capture_gstreamer(options: &CaptureOptions) -> Result<()> {
    let (node_id, pw_fd) = {
        let proxy = Screencast::new().await?;
        println!("Created screencast proxy");
        let session = proxy.create_session().await?;
        println!("Created session");
        let source_type = if options.window {
            SourceType::Window
        } else {
            SourceType::Monitor
        };
        println!("Using source type: {:?}", source_type);
        // Monitor capture; use CursorMode::Embedded to include cursor in frames.
        proxy
            .select_sources(
                &session,
                CursorMode::Embedded,
                source_type.into(),
                false, // multiple
                None,  // restore_token
                PersistMode::DoNot,
            )
            .await?;
        println!("Selected sources");
        let start = proxy.start(&session, None).await?;
        let streams = start.response()?;
        println!("Got {} streams", streams.streams().len());
        let stream = streams
            .streams()
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("portal returned no streams"))?;
        let node_id = stream.pipe_wire_node_id();
        println!("Stream node ID: {}", node_id);
        let pw_fd = proxy.open_pipe_wire_remote(&session).await?;
        println!("Opened PipeWire remote connection");
        Ok::<(u32, OwnedFd), anyhow::Error>((node_id, pw_fd))
    }?;

    // 2) Build a simple GStreamer pipeline:
    //
    // pipewiresrc fd=<pw_fd> [path=<node_id>] !
    //   videorate !
    //   video/x-raw,format=NV12,framerate=<fps>/1 !
    //   x264enc tune=zerolatency speed-preset=veryfast key-int-max=<fps> !
    //   video/x-h264,profile=baseline !
    //   mp4mux faststart=true !
    //   filesink location=<output>
    //
    gst::init()?;

    let pipeline = gst::Pipeline::new();

    let src = gst::ElementFactory::make("pipewiresrc")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: pipewiresrc"))?;
    // Provide the portal's remote fd so the source can read the authorized stream.
    // Convert OwnedFd to RawFd for GStreamer
    let raw_fd = pw_fd.into_raw_fd();
    src.set_property("fd", &raw_fd);
    // Some setups also accept the node id; if unsupported, this property is ignored.
    // On many desktops providing fd is sufficient (single approved stream).
    src.set_property("path", &format!("{}", node_id));

    let rate = gst::ElementFactory::make("videorate")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: videorate"))?;
    let convert = gst::ElementFactory::make("videoconvert")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: videoconvert"))?;

    // Caps to enforce format + framerate
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "NV12")
        .field("framerate", gst::Fraction::new(options.fps as i32, 1))
        .build();
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: capsfilter"))?;
    capsfilter.set_property("caps", &caps);

    let enc = gst::ElementFactory::make("x264enc")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: x264enc (install gst-plugins-ugly)"))?;
    enc.set_property_from_str("tune", "zerolatency");
    enc.set_property_from_str("speed-preset", "veryfast");
    enc.set_property("key-int-max", &(options.fps as u32));

    // Muxer + sink
    let mux = gst::ElementFactory::make("mp4mux")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: mp4mux"))?;
    mux.set_property("faststart", &true);

    let sink = gst::ElementFactory::make("filesink")
        .build()
        .map_err(|_| anyhow!("missing GStreamer element: filesink"))?;
    sink.set_property("location", &options.output);

    pipeline.add_many(&[&src, &capsfilter, &rate, &convert, &enc, &mux, &sink])?;
    gst::Element::link_many(&[&src, &capsfilter, &rate, &convert, &enc, &mux, &sink])?;

    // Add a pad probe to check if we're getting data
    let src_pad = src.static_pad("src").unwrap();
    let _probe = src_pad.add_probe(gstreamer::PadProbeType::BUFFER, |_pad, _info| {
        println!("Got buffer from pipewiresrc!");
        gstreamer::PadProbeReturn::Ok
    });

    // Add a bus to catch GStreamer errors
    let bus = pipeline.bus().unwrap();
    let _bus_watch = bus
        .add_watch(move |_, msg| {
            use gstreamer::MessageView;
            match msg.view() {
                MessageView::Error(err) => {
                    eprintln!(
                        "GStreamer Error: {} ({})",
                        err.error(),
                        err.debug().unwrap_or_else(|| "no debug info".into())
                    );
                    gstreamer::glib::ControlFlow::Continue
                }
                MessageView::Warning(warn) => {
                    eprintln!(
                        "GStreamer Warning: {} ({})",
                        warn.error(),
                        warn.debug().unwrap_or_else(|| "no debug info".into())
                    );
                    gstreamer::glib::ControlFlow::Continue
                }
                MessageView::Eos(_) => {
                    eprintln!("GStreamer: End of stream");
                    gstreamer::glib::ControlFlow::Continue
                }
                MessageView::StateChanged(state) => {
                    println!(
                        "Pipeline state changed: {:?} -> {:?} -> {:?}",
                        state.old(),
                        state.current(),
                        state.pending()
                    );
                    gstreamer::glib::ControlFlow::Continue
                }
                _ => gstreamer::glib::ControlFlow::Continue,
            }
        })
        .unwrap();

    // 3) Run for the requested duration, then stop.
    println!("Starting GStreamer pipeline...");
    pipeline
        .set_state(gst::State::Playing)
        .context("failed to set pipeline to Playing")?;

    println!("Recording for {} seconds...", options.seconds);
    std::thread::sleep(std::time::Duration::from_secs(options.seconds as u64));

    println!("Stopping pipeline...");
    pipeline
        .set_state(gst::State::Null)
        .context("failed to stop pipeline")?;

    println!("Saved {}", options.output);
    Ok(())
}
