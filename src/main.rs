mod audio;
mod clipboard;
mod config;
mod hotkey;
mod paste;
mod transcriber;

use anyhow::Result;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cfg = config::load_config()?;
    log::info!("Config loaded: hotkey={}, language={}, backend={}", cfg.hotkey, cfg.language, cfg.backend);

    let transcriber_init = match cfg.backend.as_str() {
        "sherpa" => {
            let paths = config::resolve_sherpa_model_paths(&cfg)?;
            transcriber::TranscriberInit::Sherpa { paths }
        }
        "whisper" | _ => {
            transcriber::install_log_callback();
            let model_path = config::resolve_model_path(&cfg)?;
            transcriber::TranscriberInit::Whisper {
                model_path,
                use_gpu: cfg.use_gpu,
                language: cfg.language.clone(),
                beam_size: cfg.beam_size,
            }
        }
    };
    log::info!("Model resolved");

    let audio_capture = audio::AudioCapture::new(&cfg.audio_device)?;

    // Channels
    let (hotkey_tx, hotkey_rx) = mpsc::channel();
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();
    let (text_tx, text_rx) = mpsc::channel::<String>();

    // Hotkey listener
    hotkey::spawn_listener(&cfg.hotkey, hotkey_tx)?;

    // Transcription worker
    transcriber::spawn_worker(transcriber_init, audio_rx, text_tx);

    // Text output thread
    std::thread::spawn(move || {
        for text in text_rx {
            log::info!("Transcribed: {text}");
            let original = clipboard::backup();
            if clipboard::set(&text).is_ok() {
                std::thread::sleep(Duration::from_millis(10));
                paste::paste_to_active_window();
                std::thread::sleep(Duration::from_millis(50));
            } else {
                log::error!("Failed to set clipboard");
            }
            clipboard::restore(original);
        }
    });

    println!("whisp-rs ready. Hold {} to record.", cfg.hotkey);

    let debounce = Duration::from_millis(cfg.debounce_ms);
    let mut recording = false;
    let mut record_start = Instant::now();
    let mut last_stop = Instant::now() - debounce; // allow immediate first use

    for event in hotkey_rx {
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

    Ok(())
}
