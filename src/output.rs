use anyhow::Result;
use std::time::Duration;

use crate::clipboard;
use crate::config::{OutputConfig, OutputMode, PasteOutputConfig, TypeOutputConfig};
use crate::paste;

pub fn emit_text(config: &OutputConfig, text: &str) -> Result<()> {
    match config.mode {
        OutputMode::Paste => emit_paste(&config.paste, text),
        OutputMode::Type => emit_type(&config.type_mode, text),
    }
}

fn emit_paste(config: &PasteOutputConfig, text: &str) -> Result<()> {
    let original_clipboard = clipboard::backup();

    let result = (|| {
        clipboard::set(text)?;
        std::thread::sleep(Duration::from_millis(10));

        let focused_apps = paste::focused_app_identifiers();
        let (combo, matched_app) = resolve_combo(config, &focused_apps);
        let backend = paste::send_combo_auto(&combo)?;

        if let Some(app) = matched_app {
            log::info!(
                "Output mode=paste backend={} combo='{}' matched_app='{}'",
                paste::backend_command_name(backend),
                combo,
                app
            );
        } else {
            log::info!(
                "Output mode=paste backend={} combo='{}'",
                paste::backend_command_name(backend),
                combo
            );
            if !config.app_overrides.is_empty() {
                log::debug!(
                    "No app override matched. Focused app identifiers: {}",
                    focused_apps.join(", ")
                );
            }
        }

        std::thread::sleep(Duration::from_millis(500));
        Ok(())
    })();

    clipboard::restore(original_clipboard);
    result
}

fn emit_type(config: &TypeOutputConfig, text: &str) -> Result<()> {
    let backend = paste::type_text(config.backend, text)?;
    log::info!(
        "Output mode=type backend={} delay_ms=0",
        paste::backend_command_name(backend)
    );
    Ok(())
}

fn resolve_combo(config: &PasteOutputConfig, focused_apps: &[String]) -> (String, Option<String>) {
    for app in focused_apps {
        if let Some(combo) = config.app_overrides.get(app) {
            return (combo.clone(), Some(app.clone()));
        }
    }
    (config.default_combo.clone(), None)
}
