---
title: Settings
description: The four settings, the system tray, diagnostics, and where hotdoc keeps its data.
---

## The four settings

hotdoc has a deliberately small settings surface:

- **hotkey** — the global shortcut that opens the launcher. Default: `Ctrl+Shift+Space`.
- **theme** — `light`, `dark`, or `system`. Default: `system`.
- **autostart** — launch hotdoc at login. Default: off. Turning it on creates an XDG autostart
  entry.
- **recents_enabled** — whether hotdoc remembers and shows recent activations. Default: on.

### Reserved hotkey combos

The following combos are reserved by the system and can't be assigned as the hotkey:

```
Ctrl+C
Ctrl+V
Ctrl+X
Ctrl+Z
Ctrl+Y
Ctrl+A
```

## System tray

Right-click the tray icon to open its menu (left-click is currently unsupported on Linux):

- **Preferences** — open the settings window
- **Reload index** — rebuild the search index
- **Open data folder** — open `~/.local/share/hotdoc` in your file manager
- **Copy diagnostics** — copy a redacted diagnostics bundle to the clipboard, useful when filing
  a bug report
- **Quit** — exit hotdoc

## Data and logs

hotdoc keeps everything local, under your home directory:

- Database: `~/.local/share/hotdoc/hotdoc.sqlite`
- Logs: `~/.local/share/hotdoc/logs/`, daily-rotated as `hotdoc.log.YYYY-MM-DD`
