use anyhow::{anyhow, bail, Context, Result};
use hf_hub::{Repo, RepoType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::hotkey;

const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");
const MODEL_DOWNLOAD_ATTEMPTS: usize = 3;

#[derive(Clone, Copy)]
struct ModelPreset {
    repo: &'static str,
    revision: &'static str,
    files: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Paste,
    Type,
}

impl Default for OutputMode {
    fn default() -> Self {
        Self::Paste
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TypeBackend {
    Auto,
    Xdotool,
    Wtype,
    Ydotool,
}

impl Default for TypeBackend {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PasteOutputConfig {
    pub default_combo: String,
    pub app_overrides: BTreeMap<String, String>,
}

impl Default for PasteOutputConfig {
    fn default() -> Self {
        Self {
            default_combo: "ctrl+v".to_string(),
            app_overrides: default_app_overrides(),
        }
    }
}

fn default_app_overrides() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("alacritty".to_string(), "ctrl+shift+v".to_string()),
        ("kitty".to_string(), "ctrl+shift+v".to_string()),
        (
            "org.wezfurlong.wezterm".to_string(),
            "ctrl+shift+v".to_string(),
        ),
        (
            "gnome-terminal-server".to_string(),
            "ctrl+shift+v".to_string(),
        ),
        ("konsole".to_string(), "ctrl+shift+v".to_string()),
        ("xfce4-terminal".to_string(), "ctrl+shift+v".to_string()),
        ("tilix".to_string(), "ctrl+shift+v".to_string()),
        ("foot".to_string(), "ctrl+shift+v".to_string()),
        ("xterm".to_string(), "shift+insert".to_string()),
        ("ghostty".to_string(), "ctrl+shift+v".to_string()),
    ])
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeOutputConfig {
    pub backend: TypeBackend,
}

impl Default for TypeOutputConfig {
    fn default() -> Self {
        Self {
            backend: TypeBackend::Auto,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub paste: PasteOutputConfig,
    #[serde(rename = "type")]
    pub type_mode: TypeOutputConfig,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            mode: OutputMode::Paste,
            paste: PasteOutputConfig::default(),
            type_mode: TypeOutputConfig::default(),
        }
    }
}

pub fn available_presets() -> &'static [&'static str] {
    &["parakeet-tdt-0.6b-v3"]
}

/// Named model presets.
fn resolve_preset(name: &str) -> Option<ModelPreset> {
    Some(match name {
        "parakeet-tdt-0.6b-v3" => ModelPreset {
            repo: "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            revision: "main",
            files: &[
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "joiner.int8.onnx",
                "tokens.txt",
            ],
        },
        _ => return None,
    })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub hotkey: String,
    pub audio_device: String,
    pub debounce_ms: u64,
    /// Named preset (e.g. "parakeet-tdt-0.6b-v3").
    pub model: String,
    pub output: OutputConfig,
}

/// Resolved paths for sherpa transducer model files.
#[derive(Debug)]
pub struct ModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
    pub created: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "insert".into(),
            audio_device: String::new(),
            debounce_ms: 100,
            model: "parakeet-tdt-0.6b-v3".into(),
            output: OutputConfig::default(),
        }
    }
}

impl Config {
    fn normalize(&mut self) {
        self.hotkey = hotkey::normalize_hotkey_name(&self.hotkey);
        self.output.paste.default_combo = self.output.paste.default_combo.trim().to_string();

        let normalized: BTreeMap<String, String> = self
            .output
            .paste
            .app_overrides
            .iter()
            .map(|(app, combo)| (app.trim().to_ascii_lowercase(), combo.trim().to_string()))
            .collect();
        self.output.paste.app_overrides = normalized;
    }

    /// Validate configuration values.
    pub fn validate(&self) -> Result<()> {
        hotkey::parse_hotkey(&self.hotkey).with_context(|| {
            format!(
                "Invalid hotkey '{}'. Any evdev key name is accepted. Run `whisp --list-hotkeys` to see all supported values.",
                self.hotkey
            )
        })?;

        if self.debounce_ms > 5000 {
            bail!(
                "debounce_ms {} exceeds maximum of 5000ms. Use a value between 0-5000.",
                self.debounce_ms
            );
        }

        if resolve_preset(&self.model).is_none() {
            bail!(
                "Unknown model '{}'. Available presets: {}",
                self.model,
                available_presets().join(", ")
            );
        }

        self.output.validate()?;

        Ok(())
    }
}

impl OutputConfig {
    fn validate(&self) -> Result<()> {
        if self.paste.default_combo.trim().is_empty() {
            bail!("output.paste.default_combo must not be empty");
        }

        for (app, combo) in &self.paste.app_overrides {
            if app.trim().is_empty() {
                bail!("output.paste.app_overrides contains an empty app key");
            }
            if combo.trim().is_empty() {
                bail!("output.paste.app_overrides['{app}'] has an empty combo");
            }
        }

        Ok(())
    }
}

pub fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("whisp")
        .join("config.toml")
}

pub fn model_cache_hint() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("huggingface")
}

pub fn write_default_config(path_override: Option<&Path>, force: bool) -> Result<PathBuf> {
    let path = path_override
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);

    if path.exists() && !force {
        bail!(
            "Config already exists at {}. Re-run with --force to overwrite.",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
    }

    fs::write(&path, DEFAULT_CONFIG)
        .with_context(|| format!("writing default config to {}", path.display()))?;

    Ok(path)
}

pub fn load_config(path_override: Option<&Path>) -> Result<LoadedConfig> {
    let path = path_override
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);

    if !path.exists() {
        write_default_config(Some(&path), false)?;
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading config from {}", path.display()))?;
        let mut config = parse_config_text(&path, &text)?;
        config.normalize();
        config.validate()?;
        return Ok(LoadedConfig {
            config,
            path,
            created: true,
        });
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let mut config = parse_config_text(&path, &text)?;
    config.normalize();
    config.validate()?;

    Ok(LoadedConfig {
        config,
        path,
        created: false,
    })
}

fn parse_config_text(path: &Path, text: &str) -> Result<Config> {
    let raw: toml::Value =
        toml::from_str(text).with_context(|| format!("parsing TOML from {}", path.display()))?;
    if raw.get("language").is_some() {
        bail!(
            "Config key 'language' was removed. Delete 'language' from {}",
            path.display()
        );
    }

    let config: Config =
        toml::from_str(text).with_context(|| format!("parsing config from {}", path.display()))?;
    Ok(config)
}

pub fn resolve_model_paths(config: &Config) -> Result<ModelPaths> {
    let preset = resolve_preset(&config.model).ok_or_else(|| {
        anyhow!(
            "Unknown model preset '{}'. Valid presets: {}",
            config.model,
            available_presets().join(", ")
        )
    })?;

    log::info!(
        "Ensuring model files for '{}' are available (repo={}, revision={})",
        config.model,
        preset.repo,
        preset.revision
    );
    log::info!("Model cache root: {}", model_cache_hint().display());

    let api = hf_hub::api::sync::Api::new().context("initializing Hugging Face API")?;
    let hf_repo = api.repo(Repo::with_revision(
        preset.repo.to_string(),
        RepoType::Model,
        preset.revision.to_string(),
    ));

    let mut paths = Vec::with_capacity(preset.files.len());
    for file in preset.files {
        let path = download_with_retries(&hf_repo, file)?;
        log::info!("Model file ready: {} -> {}", file, path.display());
        paths.push(path);
    }

    Ok(ModelPaths {
        encoder: paths[0].clone(),
        decoder: paths[1].clone(),
        joiner: paths[2].clone(),
        tokens: paths[3].clone(),
    })
}

fn download_with_retries(hf_repo: &hf_hub::api::sync::ApiRepo, file: &str) -> Result<PathBuf> {
    let mut last_err = None;
    for attempt in 1..=MODEL_DOWNLOAD_ATTEMPTS {
        match hf_repo.get(file) {
            Ok(path) => return Ok(path),
            Err(err) => {
                last_err = Some(err);
                if attempt < MODEL_DOWNLOAD_ATTEMPTS {
                    let backoff_ms = 500u64 * (1u64 << ((attempt - 1) as u32));
                    let backoff = Duration::from_millis(backoff_ms);
                    log::warn!(
                        "Model download failed for '{}' (attempt {}/{}). Retrying in {}ms...",
                        file,
                        attempt,
                        MODEL_DOWNLOAD_ATTEMPTS,
                        backoff.as_millis()
                    );
                    thread::sleep(backoff);
                }
            }
        }
    }

    let err = last_err.expect("download loop guarantees at least one attempt");
    Err(anyhow!(
        "Failed to fetch model file '{}' after {} attempts: {}",
        file,
        MODEL_DOWNLOAD_ATTEMPTS,
        err
    ))
}

#[cfg(test)]
mod tests {
    use super::{Config, OutputMode, TypeBackend};
    use std::path::Path;

    #[test]
    fn defaults_keep_insert_hotkey() {
        let cfg = Config::default();
        assert_eq!(cfg.hotkey, "insert");
        assert_eq!(cfg.output.mode, OutputMode::Paste);
        assert_eq!(cfg.output.type_mode.backend, TypeBackend::Auto);
        assert_eq!(
            cfg.output.paste.app_overrides.get("alacritty"),
            Some(&"ctrl+shift+v".to_string())
        );
    }

    #[test]
    fn rejects_unknown_config_fields() {
        let text = r#"
hotkey = "insert"
audio_device = ""
debounce_ms = 100
model = "parakeet-tdt-0.6b-v3"
unexpected = true
"#;
        let result = toml::from_str::<Config>(text);
        assert!(result.is_err(), "unknown fields must be rejected");
    }

    #[test]
    fn rejects_removed_language_key() {
        let text = r#"
hotkey = "insert"
language = "en"
audio_device = ""
debounce_ms = 100
model = "parakeet-tdt-0.6b-v3"
"#;
        let err = super::parse_config_text(Path::new("/tmp/test.toml"), text).unwrap_err();
        assert!(err.to_string().contains("language"));
    }

    #[test]
    fn normalizes_app_override_keys() {
        let text = r#"
hotkey = "insert"
audio_device = ""
debounce_ms = 100
model = "parakeet-tdt-0.6b-v3"
[output]
mode = "paste"
[output.paste]
default_combo = "ctrl+v"
[output.paste.app_overrides]
" Alacritty " = " ctrl+shift+v "
"#;
        let mut cfg = super::parse_config_text(Path::new("/tmp/test.toml"), text).unwrap();
        cfg.normalize();
        assert!(cfg.output.paste.app_overrides.contains_key("alacritty"));
        assert_eq!(
            cfg.output.paste.app_overrides.get("alacritty").unwrap(),
            "ctrl+shift+v"
        );
    }
}
