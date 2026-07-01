---
title: Troubleshooting
description: Fixes for common issues — hotkey not firing on Wayland, tray clicks, stale results, logs, and a corrupt database.
---

## The global hotkey doesn't open the launcher

This is common on Wayland, where the compositor may not deliver a global shortcut to the app at
all. If `Ctrl+Shift+Space` doesn't do anything, bind a desktop or compositor keyboard shortcut to
run this instead:

```bash
hotdoc-cli toggle
```

It shows or hides the launcher the same way the hotkey does. See the [CLI reference](../cli/) for
details.

## Left-clicking the tray icon does nothing (Linux)

Left-click on the tray icon is unsupported on Linux. Use **right-click** instead to open the tray
menu:

- **Preferences** — open the settings window
- **Reload index** — rebuild the search index
- **Open data folder** — open `~/.local/share/hotdoc` in your file manager
- **Copy diagnostics** — copy a redacted diagnostics bundle to the clipboard
- **Quit** — exit hotdoc

## Search results look stale, or a pack changed

Rebuild the index from the tray menu: right-click the tray icon and choose **Reload index**.

## Collecting logs for a bug report

Logs live at `~/.local/share/hotdoc/logs/`, daily-rotated as `hotdoc.log.YYYY-MM-DD`. Attach the
relevant file when reporting a bug, or use tray → **Copy diagnostics** to copy a redacted
diagnostics bundle to your clipboard instead.

## The app won't start after a bad shutdown

hotdoc's SQLite database (`~/.local/share/hotdoc/hotdoc.sqlite`) automatically rebuilds itself if
it's detected as corrupt on launch. If problems persist, close hotdoc and remove that file
yourself — this forces a clean rebuild the next time hotdoc starts.
