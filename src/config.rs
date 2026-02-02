use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

use crate::audio;

const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");

pub fn available_presets() -> &'static [&'static str] {
    &["parakeet-tdt-0.6b-v3"]
}

/// Named model presets: (repo, &[files])
/// Sherpa transducer models need encoder, decoder, joiner, and tokens files.
fn resolve_preset(name: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match name {
        "parakeet-tdt-0.6b-v3" => (
            "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            &["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"],
        ),
        _ => return None,
    })
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub language: String,
    pub audio_device: String,
    pub debounce_ms: u64,
    /// Named preset (e.g. "parakeet-tdt-0.6b-v3").
    pub model: String,
}

/// Resolved paths for sherpa transducer model files.
#[derive(Debug)]
pub struct ModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "insert".into(),
            language: "en".into(),
            audio_device: String::new(),
            debounce_ms: 100,
            model: "parakeet-tdt-0.6b-v3".into(),
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

pub fn resolve_model_paths(config: &Config) -> Result<ModelPaths> {
    let (repo, files) = resolve_preset(&config.model).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown model preset '{}'. Valid presets: parakeet-tdt-0.6b-v3",
            config.model
        )
    })?;

    log::info!("Downloading model files from {repo}...");
    let api = hf_hub::api::sync::Api::new()?;
    let hf_repo = api.model(repo.to_string());

    let mut paths = Vec::new();
    for file in files {
        let path = hf_repo.get(file)?;
        log::info!("  {} ready at {}", file, path.display());
        paths.push(path);
    }

    Ok(ModelPaths {
        encoder: paths[0].clone(),
        decoder: paths[1].clone(),
        joiner: paths[2].clone(),
        tokens: paths[3].clone(),
    })
}

pub fn run_wizard() -> Result<()> {
    use dialoguer::Select;

    println!("whisp-rs configuration wizard\n");

    // 1. Model selection
    let presets = available_presets();
    let model_idx = Select::new()
        .with_prompt("Select model")
        .items(presets)
        .default(0)
        .interact()?;
    let model = presets[model_idx].to_string();

    // 2. Audio device selection
    let sources = audio::list_pulse_sources()?;
    let mut choices: Vec<String> = vec!["(default)".into()];
    choices.extend(sources.iter().map(|(_, desc)| desc.clone()));
    let dev_idx = Select::new()
        .with_prompt("Select audio input device")
        .items(&choices)
        .default(0)
        .interact()?;
    let audio_device = if dev_idx == 0 {
        String::new()
    } else {
        sources[dev_idx - 1].0.clone()
    };

    // 3. Hotkey selection
    let hotkey_options = &["insert", "pause", "scrolllock", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];
    let hotkey_idx = Select::new()
        .with_prompt("Select push-to-talk hotkey")
        .items(hotkey_options)
        .default(0)
        .interact()?;
    let key_name = hotkey_options[hotkey_idx];

    // Write config
    let toml_content = format!(
        r#"hotkey = "{key_name}"
language = "en"
audio_device = "{audio_device}"
debounce_ms = 100
model = "{model}"
"#
    );

    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &toml_content)?;
    println!("\nConfig written to {}", path.display());

    Ok(())
}
