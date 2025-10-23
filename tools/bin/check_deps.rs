use clap::{Arg, Command};
use std::env;
use std::process::{Command as ProcessCommand, Stdio};

#[derive(Debug)]
struct CheckResult {
    name: String,
    passed: bool,
    version: Option<String>,
}

impl CheckResult {
    fn pass(name: String) -> Self {
        Self {
            name,
            passed: true,
            version: None,
        }
    }

    fn pass_with_version(name: String, version: String) -> Self {
        Self {
            name,
            passed: true,
            version: Some(version),
        }
    }

    fn fail(name: String) -> Self {
        Self {
            name,
            passed: false,
            version: None,
        }
    }
}

fn check_command(name: &str, cmd: &mut ProcessCommand) -> CheckResult {
    match cmd.stdout(Stdio::null()).stderr(Stdio::null()).status() {
        Ok(status) if status.success() => CheckResult::pass(name.to_string()),
        _ => CheckResult::fail(name.to_string()),
    }
}

fn check_command_with_version(name: &str, cmd: &mut ProcessCommand, version_cmd: &mut ProcessCommand) -> CheckResult {
    match cmd.stdout(Stdio::null()).stderr(Stdio::null()).status() {
        Ok(status) if status.success() => {
            if let Ok(output) = version_cmd.output() {
                if let Ok(version) = String::from_utf8(output.stdout) {
                    let version = version.trim().to_string();
                    CheckResult::pass_with_version(name.to_string(), version)
                } else {
                    CheckResult::pass(name.to_string())
                }
            } else {
                CheckResult::pass(name.to_string())
            }
        }
        _ => CheckResult::fail(name.to_string()),
    }
}

fn check_pkgconfig(pcname: &str) -> CheckResult {
    let mut cmd = ProcessCommand::new("pkg-config");
    cmd.arg("--exists").arg(pcname);

    if check_command("", &mut cmd).passed {
        let mut version_cmd = ProcessCommand::new("pkg-config");
        version_cmd.arg("--modversion").arg(pcname);
        check_command_with_version(&format!("pkg-config: {}", pcname), &mut cmd, &mut version_cmd)
    } else {
        CheckResult::fail(format!("pkg-config: {}", pcname))
    }
}

fn print_section(title: &str) {
    println!();
    println!("== {} ==", title);
}

fn print_result(result: &CheckResult) {
    match (&result.passed, &result.version) {
        (true, Some(version)) => println!("[OK]   {} ({})", result.name, version),
        (true, None) => println!("[OK]   {}", result.name),
        (false, _) => println!("[FAIL] {}", result.name),
    }
}

fn main() {
    let matches = Command::new("check_deps")
        .about("Checks availability of FFmpeg, pkg-config + GStreamer dev modules, and key GStreamer plugins")
        .arg(
            Arg::new("no-default-features")
                .long("no-default-features")
                .help("Skip GStreamer checks (for X11-only builds)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("features")
                .long("features")
                .help("Explicitly set features to check (e.g., \"wayland-pipe\")")
                .value_name("FEATURES"),
        )
        .get_matches();

    let no_default_features = matches.get_flag("no-default-features");
    let features_arg = matches.get_one::<String>("features");

    let mut wayland_feature = !no_default_features;
    if let Some(features) = features_arg {
        wayland_feature = features.contains("wayland-pipe");
    }

    let os = env::consts::OS;
    println!("Detected OS: {}", os);
    println!("Wayland feature: {}", if wayland_feature { "on" } else { "off" });

    let mut results = Vec::new();

    // Core tools
    print_section("Core tools");

    let mut ffmpeg_cmd = ProcessCommand::new("ffmpeg");
    ffmpeg_cmd.arg("-version");
    results.push(check_command("ffmpeg on PATH", &mut ffmpeg_cmd));

    let mut pkgconfig_cmd = ProcessCommand::new("pkg-config");
    pkgconfig_cmd.arg("--version");
    results.push(check_command("pkg-config on PATH", &mut pkgconfig_cmd));

    // Platform-specific checks
    match os {
        "linux" => {
            if wayland_feature {
                print_section("GStreamer development headers (pkg-config)");
                results.push(check_pkgconfig("gstreamer-1.0"));
                results.push(check_pkgconfig("gstreamer-base-1.0"));
                results.push(check_pkgconfig("gstreamer-video-1.0"));

                print_section("GStreamer runtime plugins");
                let mut gst_inspect_cmd = ProcessCommand::new("gst-inspect-1.0");
                gst_inspect_cmd.arg("--version");
                results.push(check_command("gst-inspect-1.0", &mut gst_inspect_cmd));

                let mut pipewire_cmd = ProcessCommand::new("gst-inspect-1.0");
                pipewire_cmd.arg("pipewiresrc");
                results.push(check_command("pipewiresrc plugin", &mut pipewire_cmd));

                let mut x264_cmd = ProcessCommand::new("gst-inspect-1.0");
                x264_cmd.arg("x264enc");
                results.push(check_command("x264enc plugin", &mut x264_cmd));

                print_section("Optional (PipeWire dev)");
                let pipewire_check = check_pkgconfig("libpipewire-0.3");
                if pipewire_check.passed {
                    results.push(pipewire_check);
                } else {
                    println!("[WARN] libpipewire-0.3 pkg-config not found (runtime may still work)");
                }
            }
        }
        "macos" | "windows" => {
            // Nothing extra beyond ffmpeg for these platforms
        }
        _ => {
            println!("[WARN] Unknown OS; ran basic checks only.");
        }
    }

    // Print results
    for result in &results {
        print_result(result);
    }

    // Summary
    print_section("Summary");
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;
    println!("Passed: {}, Failed: {}", passed, failed);

    if failed > 0 {
        println!();
        println!("Some required dependencies are missing.");
        println!("Hints (Ubuntu/Debian):");
        println!("  sudo apt-get install -y \\");
        println!("    gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \\");
        println!("    gstreamer1.0-plugins-bad gstreamer1.0-pipewire gstreamer1.0-libav \\");
        println!("    gstreamer1.0-plugins-ugly");
        println!("  sudo apt-get install -y \\");
        println!("    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \\");
        println!("    libpipewire-0.3-dev pkg-config");
        std::process::exit(1);
    }
}