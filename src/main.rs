mod audio;
mod clipboard;
mod config;
mod hotkey;
mod paste;
mod transcriber;

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_help() {
    println!(
        r#"whisp {VERSION} - Push-to-talk speech-to-text

USAGE:
    whisp [OPTIONS]

OPTIONS:
    --help      Show this help message
    --version   Show version information
    --config    Run the configuration wizard

EXAMPLES:
    whisp              Start whisp with existing config
    whisp --config     Configure hotkey, audio device, and model

CONFIGURATION:
    Config file: ~/.config/whisp/config.toml

REQUIREMENTS:
    - User must be in the 'input' group for hotkey detection
    - Clipboard: xclip (X11) or wl-copy/wl-paste (Wayland)
    - Paste: xdotool (X11) or wtype/ydotool (Wayland)
    - Audio: PulseAudio or PipeWire with pactl"#
    );
}

fn check_runtime_deps() -> Result<()> {
    let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let mut missing = Vec::new();

    // Check clipboard tools
    let has_xclip = has_command("xclip");
    let has_wl_copy = has_command("wl-copy") && has_command("wl-paste");

    if is_wayland && !has_wl_copy {
        missing.push("wl-clipboard (wl-copy, wl-paste) for Wayland clipboard");
    } else if !is_wayland && !has_xclip {
        missing.push("xclip for X11 clipboard");
    }

    // Check paste tools
    let has_xdotool = has_command("xdotool");
    let has_wtype = has_command("wtype");
    let has_ydotool = has_command("ydotool");

    if is_wayland {
        if !has_wtype && !has_ydotool {
            missing.push("wtype or ydotool for Wayland text paste");
        }
    } else if !has_xdotool {
        missing.push("xdotool for X11 text paste");
    }

    // Check pactl for audio device selection
    if !has_command("pactl") {
        missing.push("pactl (pulseaudio-utils or pipewire-pulse) for audio device selection");
    }

    if !missing.is_empty() {
        anyhow::bail!(
            "Missing required tools:\n  - {}\n\nInstall them and try again.",
            missing.join("\n  - ")
        );
    }

    Ok(())
}

fn has_command(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map_or(false, |o| o.status.success())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Parse command line arguments
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--version" | "-V" => {
                println!("whisp {VERSION}");
                return Ok(());
            }
            "--config" => {
                return config::run_wizard();
            }
            other => {
                eprintln!("Unknown option: {other}");
                eprintln!("Run 'whisp --help' for usage.");
                std::process::exit(1);
            }
        }
    }

    // Check runtime dependencies before proceeding
    check_runtime_deps()?;

    let cfg = config::load_config()?;
    log::info!(
        "Config loaded: hotkey={}, language={}, model={}",
        cfg.hotkey,
        cfg.language,
        cfg.model
    );

    let paths = config::resolve_model_paths(&cfg)?;
    log::info!("Model resolved");

    let audio_capture = audio::AudioCapture::new(&cfg.audio_device)?;

    // Set up graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_handler = shutdown.clone();
    ctrlc::set_handler(move || {
        log::info!("Shutting down...");
        shutdown_handler.store(true, Ordering::SeqCst);
    })?;

    // Channels
    let (hotkey_tx, hotkey_rx) = mpsc::channel();
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
    let (text_tx, text_rx) = mpsc::channel::<String>();

    // Hotkey listener
    hotkey::spawn_listener(&cfg.hotkey, hotkey_tx)?;

    // Transcription worker
    transcriber::spawn_worker(paths, audio_rx, text_tx)?;

    // Text output thread
    std::thread::spawn(move || {
        for text in text_rx {
            log::info!("Transcribed: {text}");
            let original = clipboard::backup();
            if clipboard::set(&text).is_ok() {
                std::thread::sleep(Duration::from_millis(10));
                paste::paste_to_active_window();
                std::thread::sleep(Duration::from_millis(500));
            } else {
                log::error!("Failed to set clipboard");
            }
            clipboard::restore(original);
        }
    });

    println!("whisp ready. Hold {} to record. Press Ctrl+C to exit.", cfg.hotkey);

    let debounce = Duration::from_millis(cfg.debounce_ms);
    let mut recording = false;
    let mut record_start = Instant::now();
    let mut last_stop = Instant::now() - debounce; // allow immediate first use

    loop {
        // Check for shutdown
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        // Use recv_timeout to allow periodic shutdown checks
        let event = match hotkey_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log::warn!("Hotkey channel disconnected");
                break;
            }
        };

        match event {
            hotkey::HotkeyEvent::Pressed => {
                if recording {
                    continue;
                }
                if last_stop.elapsed() < debounce {
                    continue;
                }
                audio_capture.start_recording();
                record_start = Instant::now();
                recording = true;
                log::info!("Recording...");
            }
            hotkey::HotkeyEvent::Released => {
                if !recording {
                    continue;
                }
                recording = false;
                let audio = audio_capture.stop_recording();
                last_stop = Instant::now();
                let duration = record_start.elapsed();
                if audio.is_empty() {
                    log::info!("No audio captured");
                    continue;
                }
                log::info!("Captured {:.2}s of audio", duration.as_secs_f64());
                let _ = audio_tx.send(audio);
            }
        }
    }

    // Clean shutdown - drop audio_tx to signal transcriber to exit
    drop(audio_tx);
    log::info!("Goodbye!");

    Ok(())
}
