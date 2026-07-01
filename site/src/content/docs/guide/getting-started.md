---
title: Getting started
description: The core hotdoc loop — hotkey, search, copy, paste.
---

hotdoc has one job: get you the exact syntax for a command, into your clipboard, in under five
seconds. This page walks through that loop end to end.

## First launch

The first time hotdoc starts, it builds its local search index. This happens automatically and
only once — every launch after that is instant. No network access is involved.

## The core loop

1. Press the global hotkey to open the launcher, from anywhere on your desktop:

   ```
   Ctrl+Shift+Space
   ```

   The window centers itself and takes focus.

2. Type a few fuzzy words describing what you want. Results appear as you type (search is
   debounced, so there's no need to pause).

3. Move the selection with the arrow keys:

   ```
   ↑ / ↓
   ```

4. Press `Enter` on the selected result. This copies that command's syntax to your clipboard
   **and closes the window automatically.** hotdoc does not type or paste anything for you — it
   only puts the syntax on the clipboard.

5. Paste it wherever you need it, manually:

   ```
   Ctrl+V
   ```

That's the entire loop: hotkey, type, arrow to select, `Enter`, paste.

## Wayland and other compositors

Some Wayland compositors don't allow apps to register a global hotkey, so `Ctrl+Shift+Space` may
not fire. If that's the case for you, bind a keyboard shortcut in your compositor's settings to
run:

```bash
hotdoc-cli toggle
```

This opens or closes the launcher the same way the hotkey does. See the
[CLI reference](../reference/cli/) for the full command.

## Next

Continue to [Daily use](../daily-use/) for the rest of the search behavior — typo tolerance,
recents, pinning, and the command palette.
