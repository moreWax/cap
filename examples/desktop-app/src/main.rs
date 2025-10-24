use async_channel::{Receiver, Sender, unbounded};
use cap_scale::presets::TokenPreset;
use eframe::egui;
use hybrid_screen_capture::config::config::CaptureConfig;
#[cfg(feature = "session")]
use hybrid_screen_capture::session::{CaptureSession, CaptureSource};
use tokio::runtime::Runtime;
use tokio::sync::watch;

struct ScreenCaptureApp {
    config: CaptureConfig,
    recording: bool,
    status: &'static str,
    runtime: Option<Runtime>,
    status_tx: Option<Sender<&'static str>>,
    status_rx: Option<Receiver<&'static str>>,

    // UI state
    preset_index: usize, // 0 = None, 1.. = presets
    rtsp_enabled: bool,
    rtsp_port: u16,

    // Session control
    session_shutdown: Option<watch::Sender<bool>>,
    session_running: bool,
}

impl ScreenCaptureApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (status_tx, status_rx) = unbounded();
        Self {
            config: CaptureConfig::default(),
            recording: false,
            status: "Ready",
            runtime: Some(Runtime::new().unwrap()),
            status_tx: Some(status_tx),
            status_rx: Some(status_rx),

            preset_index: 0,
            rtsp_enabled: false,
            rtsp_port: 8554,

            session_shutdown: None,
            session_running: false,
        }
    }
}

impl eframe::App for ScreenCaptureApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for status updates from async tasks
        if let Some(rx) = &self.status_rx {
            while let Ok(status) = rx.try_recv() {
                self.status = status;
                if status == "Ready" {
                    self.recording = false;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("🎥 Screen Capture");

            ui.horizontal(|ui| {
                ui.label("Output file:");
                ui.text_edit_singleline(&mut self.config.output);
            });

            ui.horizontal(|ui| {
                ui.label("FPS:");
                ui.add(egui::DragValue::new(&mut self.config.fps).clamp_range(1..=60));
            });

            ui.horizontal(|ui| {
                ui.label("Duration (seconds):");
                ui.add(egui::DragValue::new(&mut self.config.seconds).clamp_range(1..=300));
            });

            ui.horizontal(|ui| {
                ui.label("CRF (quality):");
                ui.add(egui::DragValue::new(&mut self.config.crf).clamp_range(18..=28));
            });

            ui.checkbox(&mut self.config.window, "Capture specific window");

            // Gundam mode toggle
            ui.checkbox(&mut self.config.gundam_mode, "Gundam mode (tiling)");

            // Scaling preset combo
            ui.horizontal(|ui| {
                ui.label("Scaling preset:");
                egui::ComboBox::from_label("")
                    .selected_text(match self.preset_index {
                        0 => "None".to_string(),
                        1 => "P2_56".to_string(),
                        2 => "P4".to_string(),
                        3 => "P6_9".to_string(),
                        4 => "P9".to_string(),
                        5 => "P10_24".to_string(),
                        _ => "None".to_string(),
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.preset_index == 0, "None").clicked() {
                            self.preset_index = 0;
                            self.config.scale_preset = None;
                        }
                        if ui.selectable_label(self.preset_index == 1, "P2_56").clicked() {
                            self.preset_index = 1;
                            self.config.scale_preset = Some(TokenPreset::P2_56_Long640);
                        }
                        if ui.selectable_label(self.preset_index == 2, "P4").clicked() {
                            self.preset_index = 2;
                            self.config.scale_preset = Some(TokenPreset::P4_Long640);
                        }
                        if ui.selectable_label(self.preset_index == 3, "P6_9").clicked() {
                            self.preset_index = 3;
                            self.config.scale_preset = Some(TokenPreset::P6_9_Long512);
                        }
                        if ui.selectable_label(self.preset_index == 4, "P9").clicked() {
                            self.preset_index = 4;
                            self.config.scale_preset = Some(TokenPreset::P9_Long640);
                        }
                        if ui.selectable_label(self.preset_index == 5, "P10_24").clicked() {
                            self.preset_index = 5;
                            self.config.scale_preset = Some(TokenPreset::P10_24_Long640);
                        }
                    });
            });

            // RTSP toggle and port
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.rtsp_enabled, "Enable RTSP stream");
                ui.label("Port:");
                ui.add(egui::DragValue::new(&mut self.rtsp_port).clamp_range(1024..=65535));
            });

            if ui
                .button(if self.session_running {
                    "Stop Recording"
                } else {
                    "Start Recording"
                })
                .clicked()
            {
                if self.session_running {
                    // Request graceful shutdown via the stored sender
                    if let Some(tx) = self.session_shutdown.take() {
                        let _ = tx.send(true);
                        self.status = "Stopping...";
                    } else {
                        // Fallback: just flip state
                        self.session_running = false;
                        self.recording = false;
                        self.status = "Ready";
                    }
                } else {
                    // Start a new session (or fallback capture)
                    // Validate config before starting
                    if let Err(e) = self.config.validate() {
                        self.status = "Error";
                        eprintln!("Configuration error: {}", e);
                        return;
                    }

                    let options = self.config.to_capture_options();
                    let status_tx = self.status_tx.as_ref().unwrap().clone();
                    let runtime = self.runtime.as_ref().unwrap();

                    // Map preset_index to TokenPreset (if any)
                    match self.preset_index {
                        0 => self.config.scale_preset = None,
                        1 => self.config.scale_preset = Some(TokenPreset::P2_56_Long640),
                        2 => self.config.scale_preset = Some(TokenPreset::P4_Long640),
                        3 => self.config.scale_preset = Some(TokenPreset::P6_9_Long512),
                        4 => self.config.scale_preset = Some(TokenPreset::P9_Long640),
                        5 => self.config.scale_preset = Some(TokenPreset::P10_24_Long640),
                        _ => self.config.scale_preset = None,
                    }

                    // If the crate is compiled with the `session` feature, use
                    // the CaptureSession builder and keep a shutdown sender; otherwise
                    // fall back to the legacy helper.
                    #[cfg(feature = "session")]
                    {
                        // Create platform-specific capture source synchronously
                        #[cfg(all(target_os = "linux"))]
                        let capture_source_res = hybrid_screen_capture::capture::session_sources::FFmpegCaptureSource::new(":0.0");

                        #[cfg(any(target_os = "windows", target_os = "macos"))]
                        let capture_source_res = hybrid_screen_capture::capture::session_sources::ScrapCaptureSource::new();

                        let capture_source = match capture_source_res {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("Failed to create capture source: {}", e);
                                self.status = "Error";
                                return;
                            }
                        };

                        // Build session with builders
                        let mut builder = CaptureSession::builder();
                        if let Some(p) = self.config.scale_preset {
                            builder = builder.with_scaling(p);
                        }
                        if self.config.gundam_mode {
                            builder = builder.with_gundam();
                        }

                        let input_size = capture_source.input_size();

                        // Optionally add RTSP stream
                        if self.rtsp_enabled {
                            builder = builder.with_rtsp_stream(self.rtsp_port, input_size.w, input_size.h, options.fps);
                        }

                        // Always add file output for now
                        builder = builder.with_file_output(options.output.clone(), input_size.w, input_size.h, options.fps);
                        builder = builder.with_capture_source(capture_source);

                        let session = match builder.build() {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("Failed to build capture session: {}", e);
                                self.status = "Error";
                                return;
                            }
                        };

                        // Keep a shutdown sender in the app state so we can request
                        // graceful shutdown from the GUI later.
                        let shutdown_sender = session.shutdown_sender();
                        self.session_shutdown = Some(shutdown_sender.clone());
                        self.session_running = true;
                        self.recording = true;

                        // Run session in background
                        runtime.spawn(async move {
                            match session.run().await {
                                Ok(_) => {
                                    println!("Capture session completed successfully");
                                    let _ = status_tx.send("Ready").await;
                                }
                                Err(e) => {
                                    eprintln!("Capture session failed: {}", e);
                                    let _ = status_tx.send("Error").await;
                                }
                            }
                        });
                    }

                    #[cfg(not(feature = "session"))]
                    {
                        // Legacy fallback
                        runtime.spawn(async move {
                            match hybrid_screen_capture::capture_screen(options).await {
                                Ok(_) => {
                                    println!("Capture completed successfully");
                                    let _ = status_tx.send("Ready").await;
                                }
                                Err(e) => {
                                    eprintln!("Capture failed: {}", e);
                                    let _ = status_tx.send("Error").await;
                                }
                            }
                        });

                        self.recording = true;
                    }
                }
            }

            ui.label(self.status);
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Screen Capture",
        options,
        Box::new(|cc| Box::new(ScreenCaptureApp::new(cc))),
    )
}
