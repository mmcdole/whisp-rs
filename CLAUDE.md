# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo run                      # run (needs config at ~/.config/whisp/config.toml)
RUST_LOG=debug cargo run       # run with debug logging
```

Run tests with:

```bash
cargo test
```

## Architecture

whisp is a Linux push-to-talk speech-to-text tool. It listens for a hotkey, captures audio, transcribes via sherpa-onnx (Parakeet TDT), and types the result into the active window.

**Main loop (`main.rs`)** orchestrates everything via mpsc channels across ~5 threads:

1. **Hotkey threads** (`hotkey.rs`) — one evdev listener per input device, sends Press/Release events
2. **Audio thread** (`audio.rs`) — cpal callback captures 16kHz mono into a circular buffer (10min max), peak-normalizes on extraction
3. **Transcriber thread** (`transcriber.rs`) — receives audio buffers, runs sherpa-onnx transducer inference, sends text back
4. **Text output thread** (`main.rs`) — receives transcribed text and injects key events through a native uinput virtual keyboard

**Flow:** hotkey press → start recording → hotkey release → stop recording → send audio to transcriber → transcriber returns text → key events injected via uinput

**Supporting modules:**
- `config.rs` — loads TOML config, resolves model paths (HuggingFace Hub preset)
- `uinput.rs` — creates virtual keyboard and maps text characters to evdev key events

## Key Details

- **Linux-only** — evdev for hotkeys and uinput for text injection
- **Runtime access required**: read access to `/dev/input/event*` and write access to `/dev/uinput`
- **Input device access** typically requires user in `input` (or distro-specific `uinput`) group
- Model preset (parakeet-tdt-0.6b-v3) auto-downloads from HuggingFace Hub via `hf-hub`
