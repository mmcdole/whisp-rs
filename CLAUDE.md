# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo run                      # run (needs config at ~/.config/whisp-rs/config.toml)
RUST_LOG=debug cargo run       # run with debug logging
```

No tests or lints are configured yet.

## Architecture

whisp-rs is a Linux push-to-talk speech-to-text tool. It listens for a hotkey, captures audio, transcribes via sherpa-onnx (Parakeet TDT), and pastes the result into the active window.

**Main loop (`main.rs`)** orchestrates everything via mpsc channels across ~5 threads:

1. **Hotkey threads** (`hotkey.rs`) — one evdev listener per input device, sends Press/Release events
2. **Audio thread** (`audio.rs`) — cpal callback captures 16kHz mono into a circular buffer (10min max), peak-normalizes on extraction
3. **Transcriber thread** (`transcriber.rs`) — receives audio buffers, runs sherpa-onnx transducer inference, sends text back
4. **Text output thread** (`main.rs`) — receives transcribed text, writes to clipboard, simulates paste, restores original clipboard

**Flow:** hotkey press → start recording → hotkey release → stop recording → send audio to transcriber → transcriber returns text → clipboard set → paste simulated → clipboard restored

**Supporting modules:**
- `config.rs` — loads TOML config, resolves model paths (HuggingFace Hub preset)
- `clipboard.rs` — X11 (xclip) and Wayland (wl-copy/wl-paste) clipboard operations
- `paste.rs` — detects active window type (terminal vs GUI) and desktop environment (X11/Wayland/KDE), dispatches to xdotool/wtype/ydotool/kdotool

## Key Details

- **Linux-only** — evdev for hotkeys, X11/Wayland for clipboard/paste
- **Runtime tools required**: xclip or wl-copy/wl-paste (clipboard), xdotool or wtype/ydotool (paste)
- **Input device access** requires user in `input` group
- Model preset (parakeet-tdt-0.6b-v3) auto-downloads from HuggingFace Hub via `hf-hub`
