# whisp systemd service

`whisp` is intended to run as a **user service**, not a system service.

## Install user unit

```bash
mkdir -p ~/.config/systemd/user
cp systemd/user/whisp.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now whisp.service
```

## Logs

```bash
journalctl --user -u whisp.service -f
```

## Uninstall user unit

```bash
systemctl --user disable --now whisp.service
rm -f ~/.config/systemd/user/whisp.service
systemctl --user daemon-reload
```

## Runtime constraints

- Hotkey capture uses `evdev` (`/dev/input/event*`) and usually requires adding the user to the `input` group.
- Text injection uses a native uinput virtual keyboard, so `/dev/uinput` must be writable.
- Synthetic input on Wayland is compositor-policy dependent; behavior may vary by desktop/compositor.
