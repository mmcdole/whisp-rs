use std::process::Command;
use std::sync::OnceLock;

/// Known terminal application identifiers (class names or app_ids).
/// Uses exact matching to avoid false positives like "determine" matching "term".
const TERMINAL_IDENTIFIERS: &[&str] = &[
    // Generic
    "terminal",
    "gnome-terminal",
    "gnome-terminal-server",
    "mate-terminal",
    "xfce4-terminal",
    "lxterminal",
    "qterminal",
    "deepin-terminal",
    "elementary-terminal",
    "pantheon-terminal",
    "tilix",
    "guake",
    "tilda",
    "yakuake",
    "terminology",
    "terminator",
    "termite",
    "termit",
    // Popular modern terminals
    "kitty",
    "alacritty",
    "wezterm",
    "ghostty",
    "foot",
    "rio",
    "warp",
    "hyper",
    "tabby",
    "contour",
    "cool-retro-term",
    // Classic terminals
    "xterm",
    "xterm-256color",
    "uxterm",
    "rxvt",
    "urxvt",
    "mrxvt",
    "aterm",
    "eterm",
    "st",
    "st-256color",
    "sakura",
    // KDE
    "konsole",
    "konsolepart",
    "yakuake",
    // Multiplexers (when detected as window class)
    "tmux",
    "screen",
];

#[derive(Debug, Clone, Copy)]
enum PasteBackend {
    Wtype,
    Ydotool,
    Xdotool,
}

impl std::fmt::Display for PasteBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteBackend::Wtype => write!(f, "wtype"),
            PasteBackend::Ydotool => write!(f, "ydotool"),
            PasteBackend::Xdotool => write!(f, "xdotool"),
        }
    }
}

fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

fn has_command(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map_or(false, |o| o.status.success())
}

fn detect_paste_backend() -> PasteBackend {
    if is_wayland() {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
            .unwrap_or_default()
            .to_lowercase();

        let reason;
        let backend;

        if desktop.contains("kde") || desktop.contains("plasma") {
            reason = "KDE Wayland";
            backend = PasteBackend::Ydotool;
        } else if has_command("wtype") {
            reason = "Wayland (wtype available)";
            backend = PasteBackend::Wtype;
        } else {
            reason = "Wayland (wtype not found, falling back)";
            backend = PasteBackend::Ydotool;
        }

        if !has_command(&backend.to_string()) {
            log::warn!(
                "Paste backend {} selected ({}) but not found in PATH! Paste will fail.",
                backend, reason
            );
        } else {
            log::info!("Paste backend: {} ({})", backend, reason);
        }

        backend
    } else {
        if !has_command("xdotool") {
            log::warn!("Paste backend xdotool (X11) not found in PATH! Paste will fail.");
        } else {
            log::info!("Paste backend: xdotool (X11)");
        }
        PasteBackend::Xdotool
    }
}

static PASTE_BACKEND: OnceLock<PasteBackend> = OnceLock::new();

fn get_backend() -> PasteBackend {
    *PASTE_BACKEND.get_or_init(detect_paste_backend)
}

fn is_terminal_class(name: &str) -> bool {
    let lower = name.to_lowercase();
    TERMINAL_IDENTIFIERS.iter().any(|t| lower == *t)
}

// --- X11 ---

fn get_active_window_classes_x11() -> Vec<String> {
    let win_id = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
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
        .map(|o| {
            let text = String::from_utf8_lossy(&o.stdout);
            text.split('"')
                .enumerate()
                .filter(|(i, _)| i % 2 == 1)
                .map(|(_, s)| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn is_terminal_x11() -> bool {
    get_active_window_classes_x11()
        .iter()
        .any(|c| is_terminal_class(c))
}

// --- Wayland ---

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
    while let Some(pos) = json_text[search_from..].find("\"focused\":true") {
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

fn is_terminal_wayland() -> bool {
    get_focused_app_wayland()
        .map(|name| is_terminal_class(&name))
        .unwrap_or(false)
}

fn detect_terminal() -> bool {
    if is_wayland() {
        is_terminal_wayland()
    } else {
        is_terminal_x11()
    }
}

fn do_paste(backend: PasteBackend, is_terminal: bool) {
    let ok = match backend {
        PasteBackend::Xdotool => {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let combo = if is_terminal { "ctrl+shift+v" } else { "ctrl+v" };
            Command::new("xdotool").args(["key", combo]).status()
        }
        PasteBackend::Wtype => {
            let mut cmd = Command::new("wtype");
            cmd.args(["-M", "ctrl"]);
            if is_terminal {
                cmd.args(["-M", "shift"]);
            }
            cmd.args(["-k", "v"]);
            cmd.status()
        }
        PasteBackend::Ydotool => {
            let keys = if is_terminal {
                "29:1 42:1 47:1 47:0 42:0 29:0"
            } else {
                "29:1 47:1 47:0 29:0"
            };
            let args: Vec<&str> = keys.split_whitespace().collect();
            let mut ydotool_args = vec!["key"];
            ydotool_args.extend(args);
            Command::new("ydotool").args(&ydotool_args).status()
        }
    };
    match ok {
        Ok(s) if s.success() => log::debug!("Pasted via {}", backend),
        Ok(s) => log::warn!("{} exited with {}", backend, s),
        Err(e) => log::warn!("{} failed to run: {}", backend, e),
    }
}

// --- Public API ---

pub fn paste_to_active_window() {
    let backend = get_backend();
    let is_term = detect_terminal();
    do_paste(backend, is_term);
}
