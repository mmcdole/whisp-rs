use anyhow::{Context, Result};
use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, EventType, InputEvent, Key};
use std::thread;
use std::time::Duration;

const INTER_EVENT_DELAY: Duration = Duration::from_millis(2);

pub struct VirtualKeyboard {
    device: evdev::uinput::VirtualDevice,
}

impl VirtualKeyboard {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for code in 0..768u16 {
            keys.insert(Key::new(code));
        }

        let device = VirtualDeviceBuilder::new()
            .context("failed to open /dev/uinput")?
            .name("whisp-virtual-keyboard")
            .with_keys(&keys)
            .context("failed to register key capabilities")?
            .build()
            .context("failed to create virtual keyboard device")?;

        // Give udev time to create the device node and compositors time to recognize it.
        thread::sleep(Duration::from_millis(100));

        Ok(Self { device })
    }

    /// Type text by sending individual key events.
    /// Supports ASCII printable characters. Non-mappable characters are skipped with a warning.
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        for ch in text.chars() {
            if let Some((key, shift)) = char_to_key(ch) {
                if shift {
                    self.device
                        .emit(&[InputEvent::new(EventType::KEY, Key::KEY_LEFTSHIFT.code(), 1)])
                        .context("failed to press shift")?;
                    thread::sleep(INTER_EVENT_DELAY);
                }

                self.device
                    .emit(&[InputEvent::new(EventType::KEY, key.code(), 1)])
                    .context("failed to press key")?;
                thread::sleep(INTER_EVENT_DELAY);
                self.device
                    .emit(&[InputEvent::new(EventType::KEY, key.code(), 0)])
                    .context("failed to release key")?;
                thread::sleep(INTER_EVENT_DELAY);

                if shift {
                    self.device
                        .emit(&[InputEvent::new(
                            EventType::KEY,
                            Key::KEY_LEFTSHIFT.code(),
                            0,
                        )])
                        .context("failed to release shift")?;
                    thread::sleep(INTER_EVENT_DELAY);
                }
            } else {
                log::warn!("uinput: no key mapping for character '{ch}' (U+{:04X}), skipping", ch as u32);
            }
        }
        Ok(())
    }
}

/// Check if /dev/uinput is accessible for writing.
pub fn is_available() -> bool {
    use std::fs::OpenOptions;
    OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .is_ok()
}

/// Map a character to an evdev Key and whether Shift is required.
/// Returns None for unmappable characters (non-ASCII, special Unicode).
fn char_to_key(ch: char) -> Option<(Key, bool)> {
    Some(match ch {
        'a' => (Key::KEY_A, false),
        'b' => (Key::KEY_B, false),
        'c' => (Key::KEY_C, false),
        'd' => (Key::KEY_D, false),
        'e' => (Key::KEY_E, false),
        'f' => (Key::KEY_F, false),
        'g' => (Key::KEY_G, false),
        'h' => (Key::KEY_H, false),
        'i' => (Key::KEY_I, false),
        'j' => (Key::KEY_J, false),
        'k' => (Key::KEY_K, false),
        'l' => (Key::KEY_L, false),
        'm' => (Key::KEY_M, false),
        'n' => (Key::KEY_N, false),
        'o' => (Key::KEY_O, false),
        'p' => (Key::KEY_P, false),
        'q' => (Key::KEY_Q, false),
        'r' => (Key::KEY_R, false),
        's' => (Key::KEY_S, false),
        't' => (Key::KEY_T, false),
        'u' => (Key::KEY_U, false),
        'v' => (Key::KEY_V, false),
        'w' => (Key::KEY_W, false),
        'x' => (Key::KEY_X, false),
        'y' => (Key::KEY_Y, false),
        'z' => (Key::KEY_Z, false),
        'A' => (Key::KEY_A, true),
        'B' => (Key::KEY_B, true),
        'C' => (Key::KEY_C, true),
        'D' => (Key::KEY_D, true),
        'E' => (Key::KEY_E, true),
        'F' => (Key::KEY_F, true),
        'G' => (Key::KEY_G, true),
        'H' => (Key::KEY_H, true),
        'I' => (Key::KEY_I, true),
        'J' => (Key::KEY_J, true),
        'K' => (Key::KEY_K, true),
        'L' => (Key::KEY_L, true),
        'M' => (Key::KEY_M, true),
        'N' => (Key::KEY_N, true),
        'O' => (Key::KEY_O, true),
        'P' => (Key::KEY_P, true),
        'Q' => (Key::KEY_Q, true),
        'R' => (Key::KEY_R, true),
        'S' => (Key::KEY_S, true),
        'T' => (Key::KEY_T, true),
        'U' => (Key::KEY_U, true),
        'V' => (Key::KEY_V, true),
        'W' => (Key::KEY_W, true),
        'X' => (Key::KEY_X, true),
        'Y' => (Key::KEY_Y, true),
        'Z' => (Key::KEY_Z, true),
        '1' => (Key::KEY_1, false),
        '2' => (Key::KEY_2, false),
        '3' => (Key::KEY_3, false),
        '4' => (Key::KEY_4, false),
        '5' => (Key::KEY_5, false),
        '6' => (Key::KEY_6, false),
        '7' => (Key::KEY_7, false),
        '8' => (Key::KEY_8, false),
        '9' => (Key::KEY_9, false),
        '0' => (Key::KEY_0, false),
        '!' => (Key::KEY_1, true),
        '@' => (Key::KEY_2, true),
        '#' => (Key::KEY_3, true),
        '$' => (Key::KEY_4, true),
        '%' => (Key::KEY_5, true),
        '^' => (Key::KEY_6, true),
        '&' => (Key::KEY_7, true),
        '*' => (Key::KEY_8, true),
        '(' => (Key::KEY_9, true),
        ')' => (Key::KEY_0, true),
        ' ' => (Key::KEY_SPACE, false),
        '\n' => (Key::KEY_ENTER, false),
        '\t' => (Key::KEY_TAB, false),
        '-' => (Key::KEY_MINUS, false),
        '_' => (Key::KEY_MINUS, true),
        '=' => (Key::KEY_EQUAL, false),
        '+' => (Key::KEY_EQUAL, true),
        '[' => (Key::KEY_LEFTBRACE, false),
        '{' => (Key::KEY_LEFTBRACE, true),
        ']' => (Key::KEY_RIGHTBRACE, false),
        '}' => (Key::KEY_RIGHTBRACE, true),
        '\\' => (Key::KEY_BACKSLASH, false),
        '|' => (Key::KEY_BACKSLASH, true),
        ';' => (Key::KEY_SEMICOLON, false),
        ':' => (Key::KEY_SEMICOLON, true),
        '\'' => (Key::KEY_APOSTROPHE, false),
        '"' => (Key::KEY_APOSTROPHE, true),
        '`' => (Key::KEY_GRAVE, false),
        '~' => (Key::KEY_GRAVE, true),
        ',' => (Key::KEY_COMMA, false),
        '<' => (Key::KEY_COMMA, true),
        '.' => (Key::KEY_DOT, false),
        '>' => (Key::KEY_DOT, true),
        '/' => (Key::KEY_SLASH, false),
        '?' => (Key::KEY_SLASH, true),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::char_to_key;
    use evdev::Key;

    #[test]
    fn maps_ascii_shifted_and_unshifted_chars() {
        assert_eq!(char_to_key('a'), Some((Key::KEY_A, false)));
        assert_eq!(char_to_key('A'), Some((Key::KEY_A, true)));
        assert_eq!(char_to_key('!'), Some((Key::KEY_1, true)));
    }

    #[test]
    fn returns_none_for_unmappable_unicode() {
        assert_eq!(char_to_key('é'), None);
        assert_eq!(char_to_key('你'), None);
    }
}
