# systemd user service

This repository includes a user-level systemd unit for starting whisp when your graphical session is ready.

## Install

1. Copy the unit file to your user systemd directory:

```bash
mkdir -p ~/.config/systemd/user
cp systemd/user/whisp.service ~/.config/systemd/user/
```

2. Enable and start the service:

```bash
systemctl --user enable --now whisp.service
```

3. Verify logs:

```bash
journalctl --user -u whisp.service -f
```

### Makefile install (optional)

You can install the binary and unit together using:

```bash
make install
```

This builds a release binary with an RPATH pointing to `../lib`, installs it to `~/.local/bin/whisp`, installs the sherpa-onnx and onnxruntime shared libraries to `~/.local/lib`, installs the unit to `~/.config/systemd/user/whisp.service`, and reloads the user systemd daemon.

## Notes

- The service expects a config file at `~/.config/whisp/config.toml`.
- Required runtime tools: xclip or wl-copy/wl-paste (clipboard), xdotool or wtype/ydotool (paste), pactl (audio device selection).
- The unit defaults to `ExecStart=%h/.local/bin/whisp`. The binary is built with an RPATH that points to `../lib` so it can find `~/.local/lib/libsherpa-onnx-c-api.so` and `~/.local/lib/libonnxruntime.so` without `LD_LIBRARY_PATH`. If your binary or libraries live elsewhere, edit `ExecStart` or adjust how you build/install.
- After editing, reload and restart:

```bash
systemctl --user daemon-reload
systemctl --user restart whisp.service
```
