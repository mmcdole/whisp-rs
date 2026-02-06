use anyhow::{bail, Result};
use evdev::Key;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

const HOTKEY_EXAMPLES: &[&str] = &["a", "f13", "insert", "leftctrl", "leftmeta", "micmute"];

pub fn hotkey_examples() -> &'static [&'static str] {
    HOTKEY_EXAMPLES
}

pub fn list_supported_hotkeys() -> Vec<String> {
    let mut keys: Vec<String> = (0..768u16)
        .map(Key::new)
        .map(|key| format!("{:?}", key))
        .filter_map(|name| name.strip_prefix("KEY_").map(|n| n.to_ascii_lowercase()))
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

pub fn normalize_hotkey_name(name: &str) -> String {
    let mut normalized = name
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "");

    // Accept optional KEY_ prefix from evdev debug-style names.
    if normalized.starts_with("key") && normalized.len() > 3 {
        normalized = normalized[3..].to_string();
    }

    match normalized.as_str() {
        "ctrl" | "control" => "leftctrl".to_string(),
        "shift" => "leftshift".to_string(),
        "alt" | "option" => "leftalt".to_string(),
        "super" | "meta" | "win" | "windows" | "command" | "cmd" => "leftmeta".to_string(),
        "esc" => "escape".to_string(),
        _ => normalized,
    }
}

/// Parse a hotkey name (e.g. "insert", "f4", "leftctrl") to an evdev Key.
/// Matches against `KEY_{NAME}` debug representation for all key codes 0..768.
pub fn parse_hotkey(name: &str) -> Result<Key> {
    let canonical = normalize_hotkey_name(name);
    let target = format!("KEY_{}", canonical.to_uppercase());
    for code in 0..768u16 {
        let key = Key::new(code);
        if format!("{:?}", key) == target {
            return Ok(key);
        }
    }
    bail!(
        "Unknown hotkey '{}'. Any evdev key is valid (examples: {}). Run `whisp --list-hotkeys` to list all recognized key names.",
        name,
        hotkey_examples().join(", ")
    )
}

fn find_devices_with_key(target: Key) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (path, device) in evdev::enumerate() {
        if let Some(keys) = device.supported_keys() {
            if keys.contains(target) {
                paths.push(path);
            }
        }
    }
    paths
}

pub fn spawn_listener(hotkey_name: &str, tx: mpsc::Sender<HotkeyEvent>) -> Result<()> {
    let key = parse_hotkey(hotkey_name)?;
    let devices = find_devices_with_key(key);
    if devices.is_empty() {
        bail!(
            "No input devices found with key {key:?}.\n\nFix: run 'sudo usermod -aG input $USER' then log out and back in."
        );
    }

    for path in devices {
        let tx = tx.clone();
        thread::spawn(move || {
            let Ok(mut dev) = evdev::Device::open(&path) else {
                log::warn!("Could not open {}", path.display());
                return;
            };
            log::debug!("Listening on {}", path.display());
            loop {
                match dev.fetch_events() {
                    Ok(events) => {
                        for ev in events {
                            if ev.event_type() == evdev::EventType::KEY && ev.code() == key.code() {
                                let msg = match ev.value() {
                                    1 => Some(HotkeyEvent::Pressed),
                                    0 => Some(HotkeyEvent::Released),
                                    _ => None, // repeat
                                };
                                if let Some(msg) = msg {
                                    let _ = tx.send(msg);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("evdev read error on {}: {e}", path.display());
                        break;
                    }
                }
            }
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_hotkey;

    #[test]
    fn parses_super_aliases() {
        assert_eq!(
            parse_hotkey("super").expect("super should parse"),
            parse_hotkey("leftmeta").expect("leftmeta should parse")
        );
        assert_eq!(
            parse_hotkey("meta").expect("meta should parse"),
            parse_hotkey("leftmeta").expect("leftmeta should parse")
        );
    }

    #[test]
    fn parses_ctrl_alt_shift_aliases() {
        assert_eq!(
            parse_hotkey("ctrl").expect("ctrl should parse"),
            parse_hotkey("leftctrl").expect("leftctrl should parse")
        );
        assert_eq!(
            parse_hotkey("alt").expect("alt should parse"),
            parse_hotkey("leftalt").expect("leftalt should parse")
        );
        assert_eq!(
            parse_hotkey("shift").expect("shift should parse"),
            parse_hotkey("leftshift").expect("leftshift should parse")
        );
    }
}
