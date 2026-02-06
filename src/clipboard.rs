use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::util;

fn is_wayland() -> bool {
    util::is_wayland()
}

pub fn backup() -> Option<String> {
    if is_wayland() {
        Command::new("wl-paste")
            .arg("--no-newline")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    }
}

pub fn set(text: &str) -> Result<()> {
    if is_wayland() {
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("wl-copy failed to start: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("wl-copy exited with {status}");
        }
    } else {
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("xclip failed to start: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            anyhow::bail!("xclip exited with {status}");
        }
    }
    Ok(())
}

pub fn restore(original: Option<String>) {
    if let Some(text) = original {
        if let Err(e) = set(&text) {
            log::warn!("Failed to restore clipboard: {e}");
        }
    }
}
