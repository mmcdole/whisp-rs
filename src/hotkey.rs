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

/// Parse a hotkey name (e.g. "insert", "f4", "leftctrl") to an evdev Key.
/// Matches against `KEY_{NAME}` debug representation for all key codes 0..768.
pub fn parse_hotkey(name: &str) -> Result<Key> {
    let target = format!("KEY_{}", name.to_uppercase());
    for code in 0..768u16 {
        let key = Key::new(code);
        if format!("{:?}", key) == target {
            return Ok(key);
        }
    }
    bail!("Unknown hotkey: {name}")
}

/// Convert an evdev Key to a lowercase name without the `KEY_` prefix.
pub fn key_to_name(key: Key) -> String {
    let dbg = format!("{:?}", key);
    dbg.strip_prefix("KEY_")
        .unwrap_or(&dbg)
        .to_lowercase()
}

/// Block until a key is pressed on any evdev device, return that key.
pub fn wait_for_keypress() -> Result<Key> {
    let mut devices: Vec<evdev::Device> = Vec::new();
    for (path, device) in evdev::enumerate() {
        if device.supported_keys().is_some() {
            match evdev::Device::open(&path) {
                Ok(dev) => devices.push(dev),
                Err(_) => continue,
            }
        }
    }
    if devices.is_empty() {
        bail!("No input devices found. Ensure you are in the 'input' group.");
    }

    // Poll all devices for a key-down event
    loop {
        for dev in &mut devices {
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if ev.event_type() == evdev::EventType::KEY && ev.value() == 1 {
                            return Ok(Key::new(ev.code()));
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    }
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
                            if ev.event_type() == evdev::EventType::KEY
                                && ev.code() == key.code()
                            {
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
