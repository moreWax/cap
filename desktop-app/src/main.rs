#[cfg(not(target_arch = "wasm32"))]
use eframe::egui;
#[cfg(not(target_arch = "wasm32"))]
use hybrid_screen_capture::config::CaptureConfig;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
#[cfg(not(target_arch = "wasm32"))]
use async_channel::{unbounded, Receiver, Sender};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
struct ScreenCaptureApp {
    config: CaptureConfig,
    recording: bool,
    status: &'static str,
    runtime: Option<Runtime>,
    status_tx: Option<Sender<&'static str>>,
    status_rx: Option<Receiver<&'static str>>,
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

            if ui.button(if self.recording { "Stop Recording" } else { "Start Recording" }).clicked() {
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

#[cfg(not(target_arch = "wasm32"))]
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

// wasm32 entrypoint: use Dioxus web
#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use dioxus_web::launch::launch_cfg;

#[cfg(target_arch = "wasm32")]
fn main() {
    launch_cfg(wasm_app, dioxus_web::Config::default());
}

#[cfg(target_arch = "wasm32")]
#[component]
fn wasm_app() -> Element {
    let mut output = use_signal(|| "capture.mp4".to_string());
    let mut fps = use_signal(|| 30u32);
    let mut seconds = use_signal(|| 10u32);
    let mut crf = use_signal(|| 23u8);
    let mut window = use_signal(|| false);
    let mut command = use_signal(|| String::new());

    let mut update_command = move || {
        let cmd = format!(
            "cargo run --release -- --output {} --fps {} --seconds {} --crf {}{}",
            output(),
            fps(),
            seconds(),
            crf(),
            if window() { " --window" } else { "" }
        );
        command.set(cmd);
    };

    // Update command when any setting changes
    use_effect(move || {
        update_command();
    });

    rsx! {
        div { class: "container",
            h1 { "🎥 Screen Capture Web Configurator" }

            p { class: "info",
                "⚠️ Screen capture cannot run in web browsers for security reasons. "
                "Use this tool to configure your settings and get the command to run on your local machine."
            }

            div { class: "form-group",
                label { "Output file:" }
                input {
                    r#type: "text",
                    value: "{output}",
                    oninput: move |e| output.set(e.value())
                }
            }

            div { class: "form-group",
                label { "FPS: {fps}" }
                input {
                    r#type: "range",
                    min: "10",
                    max: "60",
                    value: "{fps}",
                    oninput: move |e| fps.set(e.value().parse().unwrap_or(30))
                }
            }

            div { class: "form-group",
                label { "Duration (seconds):" }
                input {
                    r#type: "number",
                    value: "{seconds}",
                    oninput: move |e| seconds.set(e.value().parse().unwrap_or(10))
                }
            }

            div { class: "form-group",
                label { "CRF (quality): {crf}" }
                input {
                    r#type: "range",
                    min: "18",
                    max: "28",
                    value: "{crf}",
                    oninput: move |e| crf.set(e.value().parse().unwrap_or(23))
                }
            }

            div { class: "form-group",
                label {
                    input {
                        r#type: "checkbox",
                        checked: "{window}",
                        oninput: move |e| window.set(e.value() == "true")
                    }
                    " Capture specific window"
                }
                if window() {
                    p { class: "warning",
                        "⚠️ Window capture only works on Windows and macOS. On Linux, this will show an error."
                    }
                }
            }

            div { class: "command-section",
                h3 { "Command to run:" }
                div { class: "command-box",
                    pre { "{command}" }
                }
                button {
                    onclick: move |_| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.navigator().clipboard().write_text(&command());
                        }
                    },
                    "Copy Command"
                }
            }

            div { class: "instructions",
                h3 { "How to use:" }
                ol {
                    li { "Configure your capture settings above" }
                    li { "Copy the generated command" }
                    li { "Open a terminal on your local machine" }
                    li { "Navigate to the project directory" }
                    li { "Paste and run the command" }
                }
            }

            div { class: "platform-notes",
                h3 { "Platform Notes:" }
                ul {
                    li { "🪟 Windows: Full support for screen and window capture" }
                    li { "🍎 macOS: Full support for screen and window capture" }
                    li { "🐧 Linux X11: Screen capture only (window capture not supported)" }
                    li { "🐧 Linux Wayland: Screen capture via portal (requires GStreamer)" }
                }
            }
        }
    }
}
