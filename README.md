# whisp

`whisp` is a Linux desktop background utility for push-to-talk speech-to-text.
Hold a hotkey, speak, release, and `whisp` outputs text to the active window.

## Support status

- Linux only.
- Hotkey capture uses `evdev` (`/dev/input/event*`), so the user typically needs membership in the `input` group.
- Text injection uses a native uinput virtual keyboard (`/dev/uinput` must be writable).

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
```

`hotkey` is a single key (not a chord). Any evdev key name is valid.
Use `whisp --list-hotkeys` to print recognized values.
Aliases supported: `ctrl`, `shift`, `alt`, `super`, `meta`.

Text output:

- Output is always typed through the native uinput virtual keyboard.
- No external clipboard or key-injection helper tools are used.
- Character mapping currently covers ASCII printable characters plus newline (`\n`) and tab (`\t`).
- Unmappable characters are skipped and logged as warnings.

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
