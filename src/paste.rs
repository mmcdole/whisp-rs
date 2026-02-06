use anyhow::{bail, Context, Result};
use std::process::Command;

use crate::config::TypeBackend;
use crate::hotkey;

#[derive(Debug, Clone, Copy)]
pub enum InputBackend {
    Xdotool,
    Wtype,
    Ydotool,
}

#[derive(Debug, Clone, Copy)]
enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Super,
}

#[derive(Debug)]
struct ParsedCombo {
    modifiers: Vec<Modifier>,
    key_name: String,
}

use crate::util;

fn is_wayland() -> bool {
    util::is_wayland()
}

fn wayland_desktop() -> String {
    std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn has_command(name: &str) -> bool {
    util::has_command(name)
}

pub fn resolve_input_backend(pref: TypeBackend) -> Result<InputBackend> {
    match pref {
        TypeBackend::Auto => detect_auto_backend(),
        TypeBackend::Xdotool => {
            if !has_command("xdotool") {
                bail!("xdotool is not installed");
            }
            Ok(InputBackend::Xdotool)
        }
        TypeBackend::Wtype => {
            if !has_command("wtype") {
                bail!("wtype is not installed");
            }
            Ok(InputBackend::Wtype)
        }
        TypeBackend::Ydotool => {
            if !has_command("ydotool") {
                bail!("ydotool is not installed");
            }
            Ok(InputBackend::Ydotool)
        }
    }
}

pub fn detect_auto_backend() -> Result<InputBackend> {
    let candidates = auto_backend_candidates()?;
    Ok(candidates[0])
}

pub fn backend_command_name(backend: InputBackend) -> &'static str {
    match backend {
        InputBackend::Xdotool => "xdotool",
        InputBackend::Wtype => "wtype",
        InputBackend::Ydotool => "ydotool",
    }
}

pub fn focused_app_identifiers() -> Vec<String> {
    if is_wayland() {
        get_focused_app_wayland()
            .map(|app| vec![app.to_ascii_lowercase()])
            .unwrap_or_default()
    } else {
        get_active_window_classes_x11()
            .into_iter()
            .map(|class_name| class_name.to_ascii_lowercase())
            .collect()
    }
}

pub fn send_combo_auto(combo: &str) -> Result<InputBackend> {
    let candidates = auto_backend_candidates()?;
    let mut last_err = None;

    for backend in candidates {
        match send_combo_with_backend(backend, combo) {
            Ok(()) => return Ok(backend),
            Err(err) => {
                log::warn!(
                    "Paste backend {} failed: {}. Trying next fallback if available.",
                    backend_command_name(backend),
                    err
                );
                last_err = Some(err);
            }
        }
    }

    Err(last_err.expect("candidates list is non-empty"))
}

pub fn send_combo_with_backend(backend: InputBackend, combo: &str) -> Result<()> {
    match backend {
        InputBackend::Xdotool => run_command(
            Command::new("xdotool").args(["key", "--delay", "0", "--clearmodifiers", combo]),
            "xdotool key",
        ),
        InputBackend::Wtype => {
            let parsed = parse_combo(combo)?;
            run_wtype_combo(&parsed)
        }
        InputBackend::Ydotool => {
            let parsed = parse_combo(combo)?;
            run_ydotool_combo(&parsed)
        }
    }
}

pub fn type_text(backend_pref: TypeBackend, text: &str) -> Result<InputBackend> {
    if matches!(backend_pref, TypeBackend::Auto) {
        let candidates = auto_backend_candidates()?;
        let mut last_err = None;
        for backend in candidates {
            match type_text_with_backend(backend, text) {
                Ok(()) => return Ok(backend),
                Err(err) => {
                    log::warn!(
                        "Type backend {} failed: {}. Trying next fallback if available.",
                        backend_command_name(backend),
                        err
                    );
                    last_err = Some(err);
                }
            }
        }
        return Err(last_err.expect("candidates list is non-empty"));
    }

    let backend = resolve_input_backend(backend_pref)?;
    type_text_with_backend(backend, text)?;
    Ok(backend)
}

fn auto_backend_candidates() -> Result<Vec<InputBackend>> {
    let candidates = if is_wayland() {
        let desktop = wayland_desktop();
        if desktop.contains("kde") || desktop.contains("plasma") {
            vec![InputBackend::Ydotool, InputBackend::Wtype]
        } else {
            vec![InputBackend::Wtype, InputBackend::Ydotool]
        }
    } else {
        vec![InputBackend::Xdotool]
    };

    let available: Vec<InputBackend> = candidates
        .into_iter()
        .filter(|backend| has_command(backend_command_name(*backend)))
        .collect();

    if !available.is_empty() {
        return Ok(available);
    }

    if is_wayland() {
        bail!("No usable Wayland input backend found. Install ydotool and/or wtype.");
    }
    bail!("No usable X11 input backend found. Install xdotool.");
}

fn type_text_with_backend(backend: InputBackend, text: &str) -> Result<()> {
    const ZERO_DELAY_MS: &str = "0";
    match backend {
        InputBackend::Xdotool => run_command(
            Command::new("xdotool").args([
                "type",
                "--delay",
                ZERO_DELAY_MS,
                "--clearmodifiers",
                "--",
                text,
            ]),
            "xdotool type",
        ),
        InputBackend::Wtype => run_command(
            Command::new("wtype").args(["-d", ZERO_DELAY_MS, "--", text]),
            "wtype",
        ),
        InputBackend::Ydotool => run_command(
            Command::new("ydotool").args(["type", "--key-delay", ZERO_DELAY_MS, "--", text]),
            "ydotool type",
        ),
    }
}

fn run_command(cmd: &mut Command, context: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {context}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("{context} exited with {status}");
    }
}

fn parse_combo(combo: &str) -> Result<ParsedCombo> {
    let parts: Vec<String> = combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if parts.is_empty() {
        bail!("Invalid combo '{}': empty key combination", combo);
    }

    let mut modifiers = Vec::new();
    for token in &parts[..parts.len() - 1] {
        modifiers.push(parse_modifier(token)?);
    }

    let key_name = parts
        .last()
        .expect("parts has at least one element")
        .to_string();
    hotkey::parse_hotkey(&key_name)
        .with_context(|| format!("Invalid key '{}' in combo '{}'", key_name, combo))?;

    Ok(ParsedCombo {
        modifiers,
        key_name,
    })
}

fn parse_modifier(token: &str) -> Result<Modifier> {
    let normalized = hotkey::normalize_hotkey_name(token);
    match normalized.as_str() {
        "leftctrl" | "rightctrl" => Ok(Modifier::Ctrl),
        "leftshift" | "rightshift" => Ok(Modifier::Shift),
        "leftalt" | "rightalt" => Ok(Modifier::Alt),
        "leftmeta" | "rightmeta" => Ok(Modifier::Super),
        _ => bail!(
            "Invalid modifier '{}'. Supported modifiers: ctrl, shift, alt, super/meta",
            token
        ),
    }
}

fn modifier_hotkey_name(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Ctrl => "leftctrl",
        Modifier::Shift => "leftshift",
        Modifier::Alt => "leftalt",
        Modifier::Super => "leftmeta",
    }
}

fn modifier_wtype_name(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Ctrl => "ctrl",
        Modifier::Shift => "shift",
        Modifier::Alt => "alt",
        Modifier::Super => "logo",
    }
}

fn wtype_key_name(key_name: &str) -> String {
    let normalized = hotkey::normalize_hotkey_name(key_name);
    match normalized.as_str() {
        "escape" => "Escape".to_string(),
        "enter" => "Return".to_string(),
        "backspace" => "BackSpace".to_string(),
        "pagedown" => "Page_Down".to_string(),
        "pageup" => "Page_Up".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        _ => {
            if let Some(num) = normalized.strip_prefix('f') {
                if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                    return format!("F{num}");
                }
            }
            normalized
        }
    }
}

fn run_wtype_combo(parsed: &ParsedCombo) -> Result<()> {
    let mut cmd = Command::new("wtype");
    cmd.args(["-d", "0"]);
    for modifier in &parsed.modifiers {
        cmd.args(["-M", modifier_wtype_name(*modifier)]);
    }
    let key_name = wtype_key_name(&parsed.key_name);
    cmd.args(["-k", &key_name]);
    for modifier in parsed.modifiers.iter().rev() {
        cmd.args(["-m", modifier_wtype_name(*modifier)]);
    }
    run_command(&mut cmd, "wtype combo")
}

fn run_ydotool_combo(parsed: &ParsedCombo) -> Result<()> {
    let mut events = Vec::new();

    for modifier in &parsed.modifiers {
        let code = hotkey::parse_hotkey(modifier_hotkey_name(*modifier))
            .with_context(|| format!("Invalid modifier {:?}", modifier))?
            .code();
        events.push(format!("{code}:1"));
    }

    let key_code = hotkey::parse_hotkey(&parsed.key_name)
        .with_context(|| format!("Invalid key '{}' for ydotool combo", parsed.key_name))?
        .code();
    events.push(format!("{key_code}:1"));
    events.push(format!("{key_code}:0"));

    for modifier in parsed.modifiers.iter().rev() {
        let code = hotkey::parse_hotkey(modifier_hotkey_name(*modifier))
            .with_context(|| format!("Invalid modifier {:?}", modifier))?
            .code();
        events.push(format!("{code}:0"));
    }

    let mut cmd = Command::new("ydotool");
    cmd.arg("key");
    cmd.args(["--key-delay", "0"]);
    for event in &events {
        cmd.arg(event);
    }
    run_command(&mut cmd, "ydotool key")
}

fn get_active_window_classes_x11() -> Vec<String> {
    let win_id = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        });

    let Some(win_id) = win_id else {
        return Vec::new();
    };

    Command::new("xprop")
        .args(["-id", &win_id, "WM_CLASS"])
        .output()
        .ok()
        .map(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.split('"')
                .enumerate()
                .filter(|(idx, _)| idx % 2 == 1)
                .map(|(_, value)| value.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn get_focused_app_wayland() -> Option<String> {
    if let Ok(output) = Command::new("swaymsg").args(["-t", "get_tree"]).output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(app_id) = find_focused_app_id(&text) {
                return Some(app_id);
            }
        }
    }

    if let Ok(output) = Command::new("kdotool").arg("getactivewindow").output() {
        if output.status.success() {
            let win_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(output) = Command::new("kdotool")
                .args(["getwindowclassname", &win_id])
                .output()
            {
                if output.status.success() {
                    return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
                }
            }
        }
    }

    None
}

fn find_focused_app_id(json_text: &str) -> Option<String> {
    let focused_marker = "\"focused\":true";
    let mut search_from = 0;
    while let Some(pos) = json_text[search_from..].find(focused_marker) {
        let abs_pos = search_from + pos;
        let region = &json_text[abs_pos.saturating_sub(500)..abs_pos];
        if let Some(aid_pos) = region.rfind("\"app_id\":\"") {
            let start = aid_pos + "\"app_id\":\"".len();
            if let Some(end) = region[start..].find('"') {
                let app_id = &region[start..start + end];
                if !app_id.is_empty() {
                    return Some(app_id.to_string());
                }
            }
        }
        search_from = abs_pos + focused_marker.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_combo, Modifier};

    #[test]
    fn combo_parsing_supports_modifier_and_key() {
        let parsed = parse_combo("ctrl+shift+v").expect("combo should parse");
        assert_eq!(parsed.modifiers.len(), 2);
        assert!(matches!(parsed.modifiers[0], Modifier::Ctrl));
        assert!(matches!(parsed.modifiers[1], Modifier::Shift));
        assert_eq!(parsed.key_name, "v");
    }

    #[test]
    fn combo_parsing_rejects_invalid_modifier() {
        let err = parse_combo("capslock+v").expect_err("invalid modifier should fail");
        assert!(err.to_string().contains("Invalid modifier"));
    }
}
