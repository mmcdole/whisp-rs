# whisp

`whisp` is a Linux desktop background utility for push-to-talk speech-to-text.
Hold a hotkey, speak, release, and `whisp` outputs text to the active window.

## Support status

- Linux only.
- Hotkey capture uses `evdev` (`/dev/input/event*`), so the user typically needs membership in the `input` group.
- X11 is supported with `xdotool` and `xprop` (`xclip` needed for paste mode).
- Wayland support depends on compositor policy for synthetic input; install `wtype` or `ydotool` (`wl-clipboard` needed for paste mode).

## Build and install

```bash
cargo build --release
make install
```

Default install locations:

- Binary: `~/.local/bin/whisp`
- Shared libs: `~/.local/lib/`
- User unit: `~/.config/systemd/user/whisp.service`

Override paths with standard make variables (`PREFIX`, `DESTDIR`, `BINDIR`, `LIBDIR`, `SYSTEMD_USER_UNITDIR`).

## Enable as a user service

```bash
systemctl --user daemon-reload
systemctl --user enable --now whisp.service
journalctl --user -u whisp.service -f
```

Stop/start manually:

```bash
systemctl --user stop whisp.service
systemctl --user start whisp.service
```

## Basic usage and verification

Run interactively:

```bash
whisp
```

Health checks:

```bash
whisp --check
whisp --list-hotkeys
whisp --list-audio-devices
```

Pre-download model files:

```bash
whisp --predownload-model
```

Write a fresh config template:

```bash
whisp --write-default-config --config ~/.config/whisp/config.toml
```

## Configuration

Config path (default): `~/.config/whisp/config.toml`

Example:

```toml
hotkey = "insert"
audio_device = ""
debounce_ms = 100
model = "parakeet-tdt-0.6b-v3"

[output]
mode = "paste"

[output.paste]
default_combo = "ctrl+v"

[output.paste.app_overrides]
alacritty = "ctrl+shift+v"
kitty = "ctrl+shift+v"
"org.wezfurlong.wezterm" = "ctrl+shift+v"
"gnome-terminal-server" = "ctrl+shift+v"
konsole = "ctrl+shift+v"
"xfce4-terminal" = "ctrl+shift+v"
tilix = "ctrl+shift+v"
foot = "ctrl+shift+v"
xterm = "shift+insert"
ghostty = "ctrl+shift+v"

[output.type]
backend = "auto"
```

`hotkey` is a single key (not a chord). Any evdev key name is valid.
Use `whisp --list-hotkeys` to print recognized values.
Aliases supported: `ctrl`, `shift`, `alt`, `super`, `meta`.

Output modes:

- `output.mode = "paste"` uses clipboard + configurable paste key combos.
- `output.mode = "type"` injects text directly.
- App-specific paste overrides are configured under `output.paste.app_overrides`.
- Typing delay is always `0` for all backends (`xdotool`, `wtype`, `ydotool`).
- In `backend = "auto"` on Wayland: KDE/Plasma prefers `ydotool`, other compositors prefer `wtype` (with fallback to the other backend when available).

## Model auto-download

On startup (or with `--predownload-model`), `whisp` fetches the Parakeet 0.6B preset files from Hugging Face if missing.
Cache location is under `~/.cache/huggingface` by default.

## Uninstall

```bash
make uninstall
```

This removes installed binary, shared libs, and user unit. It does not remove config or model cache.

Optional purge:

```bash
make purge PURGE_CONFIG=1 PURGE_CACHE=1
```

`PURGE_CACHE=1` removes only the current `whisp` Parakeet model cache directory, not all Hugging Face caches.
