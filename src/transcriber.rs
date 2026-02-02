use anyhow::Result;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};
use whisper_rs_sys::ggml_log_level;

const MAX_QUEUE: usize = 20;

/// Route all whisper.cpp C-level logging through Rust's `log` crate at trace level,
/// so it's hidden by default but visible with `RUST_LOG=trace`.
unsafe extern "C" fn whisper_log_callback(
    level: ggml_log_level,
    text: *const std::os::raw::c_char,
    _user_data: *mut std::os::raw::c_void,
) {
    if text.is_null() {
        return;
    }
    let msg = unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy();
    let trimmed = msg.trim();
    if trimmed.is_empty() {
        return;
    }
    match level {
        whisper_rs_sys::ggml_log_level_GGML_LOG_LEVEL_ERROR => log::error!(target: "whisper_cpp", "{}", trimmed),
        whisper_rs_sys::ggml_log_level_GGML_LOG_LEVEL_WARN => log::warn!(target: "whisper_cpp", "{}", trimmed),
        _ => log::trace!(target: "whisper_cpp", "{}", trimmed),
    }
}

/// Call once before loading any model to suppress whisper.cpp debug/info spam.
pub fn install_log_callback() {
    unsafe {
        whisper_rs_sys::whisper_log_set(Some(whisper_log_callback), std::ptr::null_mut());
    }
}

pub fn load_context(model_path: &std::path::Path, use_gpu: bool) -> Result<WhisperContext> {
    let mut params = WhisperContextParameters::default();
    params.use_gpu(use_gpu);
    log::info!("Loading whisper model (use_gpu={})", use_gpu);
    let ctx = WhisperContext::new_with_params(
        model_path.to_str().unwrap_or_default(),
        params,
    )
    .map_err(|e| anyhow::anyhow!("Failed to load whisper model: {e}"))?;
    Ok(ctx)
}

pub fn transcribe(state: &mut WhisperState, audio: &[f32], language: &str, beam_size: i32) -> Result<String> {
    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: beam_size,
        patience: -1.0,
    });

    if !language.is_empty() {
        params.set_language(Some(language));
    }
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_debug_mode(false);

    state
        .full(params, audio)
        .map_err(|e| anyhow::anyhow!("whisper inference failed: {e}"))?;

    let n = state.full_n_segments().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut text = String::new();
    for i in 0..n {
        if let Ok(seg) = state.full_get_segment_text(i) {
            text.push_str(&seg);
        }
    }
    Ok(text.trim().to_string())
}

/// Spawns a worker thread that receives audio chunks and sends back transcribed text.
pub fn spawn_worker(
    ctx: WhisperContext,
    language: String,
    beam_size: i32,
    audio_rx: mpsc::Receiver<Vec<f32>>,
    text_tx: mpsc::Sender<String>,
) {
    thread::spawn(move || {
        let mut state = ctx.create_state().expect("failed to create whisper state");
        let mut queue: VecDeque<Vec<f32>> = VecDeque::with_capacity(MAX_QUEUE);
        loop {
            // Block on first item
            let audio = match audio_rx.recv() {
                Ok(a) => a,
                Err(_) => break,
            };
            queue.push_back(audio);

            // Drain any additional pending items
            while let Ok(a) = audio_rx.try_recv() {
                queue.push_back(a);
                if queue.len() > MAX_QUEUE {
                    queue.pop_front();
                }
            }

            // Process all queued audio
            while let Some(audio) = queue.pop_front() {
                match transcribe(&mut state, &audio, &language, beam_size) {
                    Ok(text) if !text.is_empty() => {
                        let _ = text_tx.send(text);
                    }
                    Ok(_) => log::debug!("Empty transcription result"),
                    Err(e) => log::error!("Transcription error: {e}"),
                }
            }
        }
    });
}
