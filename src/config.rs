use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");

/// Named model presets: (repo, file)
fn resolve_preset(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "tiny" => ("ggerganov/whisper.cpp", "ggml-tiny.bin"),
        "tiny.en" => ("ggerganov/whisper.cpp", "ggml-tiny.en.bin"),
        "base" => ("ggerganov/whisper.cpp", "ggml-base.bin"),
        "base.en" => ("ggerganov/whisper.cpp", "ggml-base.en.bin"),
        "small" => ("ggerganov/whisper.cpp", "ggml-small.bin"),
        "small.en" => ("ggerganov/whisper.cpp", "ggml-small.en.bin"),
        "medium" => ("ggerganov/whisper.cpp", "ggml-medium.bin"),
        "medium.en" => ("ggerganov/whisper.cpp", "ggml-medium.en.bin"),
        "large-v1" => ("ggerganov/whisper.cpp", "ggml-large-v1.bin"),
        "large-v2" => ("ggerganov/whisper.cpp", "ggml-large-v2.bin"),
        "large-v3" => ("ggerganov/whisper.cpp", "ggml-large-v3.bin"),
        "large-v3-turbo" => ("ggerganov/whisper.cpp", "ggml-large-v3-turbo.bin"),
        "distil-large-v3" => ("distil-whisper/distil-large-v3-ggml", "ggml-distil-large-v3.bin"),
        "distil-large-v3.5" => ("distil-whisper/distil-large-v3.5-ggml", "ggml-model.bin"),
        _ => return None,
    })
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub language: String,
    pub audio_device: String,
    pub beam_size: i32,
    pub min_recording_seconds: f64,
    pub debounce_ms: u64,
    /// Named preset (e.g. "medium.en", "distil-large-v3"). Overrides model_repo/model_file.
    pub model: Option<String>,
    pub model_repo: String,
    pub model_file: String,
    pub model_path: Option<String>,
    /// Use GPU for inference if available (default: true)
    pub use_gpu: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "insert".into(),
            language: "en".into(),
            audio_device: String::new(),
            beam_size: 5,
            min_recording_seconds: 1.0,
            debounce_ms: 100,
            model: Some("distil-large-v3".into()),
            model_repo: "ggerganov/whisper.cpp".into(),
            model_file: "ggml-medium.en.bin".into(),
            model_path: None,
            use_gpu: true,
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("whisp-rs")
        .join("config.toml")
}

pub fn load_config() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, DEFAULT_CONFIG)?;
        log::info!("Created default config at {}", path.display());
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let config: Config = toml::from_str(&text)
        .with_context(|| format!("parsing config from {}", path.display()))?;
    Ok(config)
}

pub fn resolve_model_path(config: &Config) -> Result<PathBuf> {
    // 1. Explicit local path
    if let Some(ref p) = config.model_path {
        let path = PathBuf::from(p);
        anyhow::ensure!(path.exists(), "model_path does not exist: {}", p);
        return Ok(path);
    }

    // 2. Named preset or manual repo/file
    let (repo, file) = if let Some(ref preset) = config.model {
        let (r, f) = resolve_preset(preset).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown model preset '{}'. Valid presets: tiny, tiny.en, base, base.en, \
                 small, small.en, medium, medium.en, large-v1, large-v2, large-v3, \
                 large-v3-turbo, distil-large-v3, distil-large-v3.5",
                preset
            )
        })?;
        (r.to_string(), f.to_string())
    } else {
        (config.model_repo.clone(), config.model_file.clone())
    };

    log::info!("Downloading model {}/{} from HuggingFace...", repo, file);
    let api = hf_hub::api::sync::Api::new()?;
    let hf_repo = api.model(repo);
    let path = hf_repo.get(&file)?;
    log::info!("Model ready at {}", path.display());
    Ok(path)
}
