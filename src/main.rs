mod audio;
mod config;
mod hotkey;
mod output;
mod transcriber;
mod uinput;
mod util;

use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Default, Debug)]
struct CliOptions {
    show_help: bool,
    show_version: bool,
    list_hotkeys: bool,
    list_audio_devices: bool,
    write_default_config: bool,
    force: bool,
    config_path: Option<PathBuf>,
    check_only: bool,
    predownload_model: bool,
}

fn print_help() {
    println!(
        r#"whisp {VERSION} - Push-to-talk speech-to-text

USAGE:
    whisp [OPTIONS]

OPTIONS:
    --help, -h                   Show this help message
    --version, -V                Show version information
    --list-hotkeys               List all recognized evdev key names
    --list-audio-devices         List available input source names for config
    --write-default-config       Write default config to --config path (or default path)
    --force                      Overwrite file when used with --write-default-config
    --config <path>              Override config file path
    --check                      Validate dependencies, config, and model availability
    --predownload-model          Download model files and exit

EXAMPLES:
    whisp
    whisp --list-hotkeys
    whisp --list-audio-devices
    whisp --write-default-config --config ~/.config/whisp/config.toml
    whisp --config ~/.config/whisp/config.toml
    whisp --check
    whisp --predownload-model

CONFIGURATION:
    Default config: ~/.config/whisp/config.toml
    Default hotkey: insert

REQUIREMENTS:
    - User must be in the 'input' group for hotkey detection and typing
    - /dev/uinput must be accessible (used for virtual keyboard input)
    - Audio device override by name: pactl (PulseAudio/PipeWire)"#
    );
}

fn parse_args() -> Result<CliOptions> {
    let mut opts = CliOptions::default();
    let mut args = std::env::args().skip(1).peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => opts.show_help = true,
            "--version" | "-V" => opts.show_version = true,
            "--list-hotkeys" => opts.list_hotkeys = true,
            "--list-audio-devices" => opts.list_audio_devices = true,
            "--write-default-config" => opts.write_default_config = true,
            "--force" => opts.force = true,
            "--check" => opts.check_only = true,
            "--predownload-model" => opts.predownload_model = true,
            "--config" => {
                let Some(path) = args.next() else {
                    bail!(
                        "The --config flag now requires a file path. Interactive setup was removed.\n\
                         Example: whisp --config ~/.config/whisp/config.toml"
                    );
                };
                if path.starts_with('-') {
                    bail!("Expected path after --config, got flag '{path}'");
                }
                opts.config_path = Some(PathBuf::from(path));
            }
            other if other.starts_with("--config=") => {
                let path = other.trim_start_matches("--config=");
                if path.is_empty() {
                    bail!("--config= requires a non-empty path");
                }
                opts.config_path = Some(PathBuf::from(path));
            }
            other => {
                bail!("Unknown option: {other}. Run 'whisp --help' for usage.");
            }
        }
    }

    if opts.force && !opts.write_default_config {
        bail!("--force is only valid with --write-default-config");
    }

    Ok(opts)
}

fn check_runtime_deps(config: &config::Config) -> Result<()> {
    let mut missing: Vec<String> = Vec::new();

    if !uinput::is_available() {
        missing.push(
            "/dev/uinput is not accessible. Ensure user is in the 'input' group (or 'uinput' group on some distros)".to_string(),
        );
    }

    if !config.audio_device.is_empty() && !util::has_command("pactl") {
        missing.push(
            "pactl (pulseaudio-utils or pipewire-pulse) is required when audio_device is set"
                .to_string(),
        );
    }

    if !missing.is_empty() {
        anyhow::bail!(
            "Missing requirements:\n  - {}\n\nFix and try again.",
            missing.join("\n  - ")
        );
    }

    Ok(())
}

fn run_check(config: &config::Config) -> Result<()> {
    check_runtime_deps(config)?;
    let paths = config::resolve_model_paths(config)?;
    transcriber::validate_model(&paths)?;
    println!("whisp check OK");
    Ok(())
}

fn print_audio_devices() -> Result<()> {
    let devices = audio::list_input_sources()?;
    println!("Available input sources (use `audio_device = \"<name>\"`):");
    for source in devices {
        println!("  {}  ({})", source.name, source.description);
    }
    Ok(())
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = parse_args()?;
    if cli.show_help {
        print_help();
        return Ok(());
    }
    if cli.show_version {
        println!("whisp {VERSION}");
        return Ok(());
    }
    if cli.list_hotkeys {
        for key in hotkey::list_supported_hotkeys() {
            println!("{key}");
        }
        return Ok(());
    }
    if cli.list_audio_devices {
        print_audio_devices()?;
        return Ok(());
    }
    if cli.write_default_config {
        let path = config::write_default_config(cli.config_path.as_deref(), cli.force)?;
        println!("Wrote default config to {}", path.display());
        return Ok(());
    }

    let loaded = config::load_config(cli.config_path.as_deref())?;
    if loaded.created {
        log::info!(
            "Created default config at {}",
            loaded.path.to_string_lossy()
        );
    } else {
        log::info!("Using config {}", loaded.path.to_string_lossy());
    }

    if cli.predownload_model {
        let _ = config::resolve_model_paths(&loaded.config)?;
        println!(
            "Model '{}' is available in cache: {}",
            loaded.config.model,
            config::model_cache_hint().display()
        );
        return Ok(());
    }

    if cli.check_only {
        run_check(&loaded.config)?;
        return Ok(());
    }

    check_runtime_deps(&loaded.config)?;

    log::info!(
        "Config loaded: hotkey={}, model={}",
        loaded.config.hotkey,
        loaded.config.model
    );

    let paths = config::resolve_model_paths(&loaded.config)?;
    log::info!("Model resolved");

    let audio_capture = audio::AudioCapture::new(&loaded.config.audio_device)?;
    let mut vkbd = uinput::VirtualKeyboard::new()
        .context("failed to initialize virtual keyboard (/dev/uinput)")?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_handler = shutdown.clone();
    ctrlc::set_handler(move || {
        log::info!("Shutting down...");
        shutdown_handler.store(true, Ordering::SeqCst);
    })?;

    let (hotkey_tx, hotkey_rx) = mpsc::channel();
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
    let (text_tx, text_rx) = mpsc::channel::<String>();

    hotkey::spawn_listener(&loaded.config.hotkey, hotkey_tx)?;
    transcriber::spawn_worker(paths, audio_rx, text_tx)?;

    std::thread::spawn(move || {
        for text in text_rx {
            log::info!("Transcribed: {text}");
            if let Err(err) = output::emit_text(&text, &mut vkbd) {
                log::error!("Failed to emit output text: {err}");
            }
        }
    });

    println!(
        "whisp ready. Hold {} to record. Press Ctrl+C to exit.",
        loaded.config.hotkey
    );

    let debounce = Duration::from_millis(loaded.config.debounce_ms);
    let mut recording = false;
    let mut record_start = Instant::now();
    let mut last_stop = Instant::now() - debounce;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

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

    drop(audio_tx);
    log::info!("Goodbye!");

    Ok(())
}
