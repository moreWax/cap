use async_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use hybrid_screen_capture::config::CaptureConfig;
use tokio::runtime::Runtime;

#[derive(Default)]
struct ScreenCaptureApp {
    config: CaptureConfig,
    recording: bool,
    status: &'static str,
    runtime: Option<Runtime>,
    status_tx: Option<Sender<&'static str>>,
    status_rx: Option<Receiver<&'static str>>,
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

            if ui
                .button(if self.recording {
                    "Stop Recording"
                } else {
                    "Start Recording"
                })
                .clicked()
            {
                if self.recording {
                    self.recording = false;
                    self.status = "Ready";
                } else {
                    self.recording = true;
                    self.status = "Recording...";

                    // Validate config before starting
                    if let Err(e) = self.config.validate() {
                        self.status = "Error";
                        eprintln!("Configuration error: {}", e);
                        self.recording = false;
                        return;
                    }

                    let options = self.config.to_capture_options();

                    let status_tx = self.status_tx.as_ref().unwrap().clone();
                    let runtime = self.runtime.as_ref().unwrap();

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
