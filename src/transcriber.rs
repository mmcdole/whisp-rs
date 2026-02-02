use anyhow::Result;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;

const MAX_QUEUE: usize = 20;

// --- Whisper backend ---

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};
use whisper_rs_sys::ggml_log_level;

/// Route all whisper.cpp C-level logging through Rust's `log` crate at trace level.
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

pub fn install_log_callback() {
    unsafe {
        whisper_rs_sys::whisper_log_set(Some(whisper_log_callback), std::ptr::null_mut());
    }
}

struct WhisperTranscriber {
    state: WhisperState,
    language: String,
    beam_size: i32,
}

impl WhisperTranscriber {
    fn new(model_path: &std::path::Path, use_gpu: bool, language: String, beam_size: i32) -> Result<Self> {
        let mut params = WhisperContextParameters::default();
        params.use_gpu(use_gpu);
        log::info!("Loading whisper model (use_gpu={})", use_gpu);
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().unwrap_or_default(),
            params,
        )
        .map_err(|e| anyhow::anyhow!("Failed to load whisper model: {e}"))?;

        let state = ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("Failed to create whisper state: {e}"))?;

        Ok(Self {
            state,
            language,
            beam_size,
        })
    }

    fn transcribe(&mut self, audio: &[f32]) -> Result<String> {
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: self.beam_size,
            patience: -1.0,
        });

        if !self.language.is_empty() {
            params.set_language(Some(&self.language));
        }
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_debug_mode(false);

        self.state
            .full(params, audio)
            .map_err(|e| anyhow::anyhow!("whisper inference failed: {e}"))?;

        let n = self.state.full_n_segments().map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut text = String::new();
        for i in 0..n {
            if let Ok(seg) = self.state.full_get_segment_text(i) {
                text.push_str(&seg);
            }
        }
        Ok(text.trim().to_string())
    }
}

// --- Sherpa backend ---

use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};

struct SherpaTranscriber {
    recognizer: TransducerRecognizer,
}

impl SherpaTranscriber {
    fn new(paths: &crate::config::SherpaModelPaths) -> Result<Self> {
        let config = TransducerConfig {
            encoder: paths.encoder.to_string_lossy().into_owned(),
            decoder: paths.decoder.to_string_lossy().into_owned(),
            joiner: paths.joiner.to_string_lossy().into_owned(),
            tokens: paths.tokens.to_string_lossy().into_owned(),
            sample_rate: 16000,
            feature_dim: 80,
            num_threads: 4,
            decoding_method: "greedy_search".into(),
            model_type: "nemo_transducer".into(),
            ..Default::default()
        };
        log::info!("Loading sherpa transducer model");
        let recognizer = TransducerRecognizer::new(config)
            .map_err(|e| anyhow::anyhow!("Failed to create sherpa recognizer: {e}"))?;
        Ok(Self { recognizer })
    }

    fn transcribe(&mut self, audio: &[f32]) -> Result<String> {
        let text = self.recognizer.transcribe(16000, audio);
        Ok(text.trim().to_string())
    }
}

// --- Backend enum ---

enum Backend {
    Whisper(WhisperTranscriber),
    Sherpa(SherpaTranscriber),
}

impl Backend {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String> {
        match self {
            Backend::Whisper(w) => w.transcribe(audio),
            Backend::Sherpa(s) => s.transcribe(audio),
        }
    }
}

// --- Public API ---

pub enum TranscriberInit {
    Whisper {
        model_path: std::path::PathBuf,
        use_gpu: bool,
        language: String,
        beam_size: i32,
    },
    Sherpa {
        paths: crate::config::SherpaModelPaths,
    },
}

pub fn spawn_worker(
    init: TranscriberInit,
    audio_rx: mpsc::Receiver<Vec<f32>>,
    text_tx: mpsc::Sender<String>,
) {
    thread::spawn(move || {
        let mut backend = match init {
            TranscriberInit::Whisper { model_path, use_gpu, language, beam_size } => {
                Backend::Whisper(
                    WhisperTranscriber::new(&model_path, use_gpu, language, beam_size)
                        .expect("failed to init whisper backend"),
                )
            }
            TranscriberInit::Sherpa { paths } => {
                Backend::Sherpa(
                    SherpaTranscriber::new(&paths).expect("failed to init sherpa backend"),
                )
            }
        };

        log::info!("Transcription worker ready");

        let mut queue: VecDeque<Vec<f32>> = VecDeque::with_capacity(MAX_QUEUE);
        loop {
            let audio = match audio_rx.recv() {
                Ok(a) => a,
                Err(_) => break,
            };
            queue.push_back(audio);

            while let Ok(a) = audio_rx.try_recv() {
                queue.push_back(a);
                if queue.len() > MAX_QUEUE {
                    queue.pop_front();
                }
            }

            while let Some(audio) = queue.pop_front() {
                match backend.transcribe(&audio) {
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
