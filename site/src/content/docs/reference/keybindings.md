---
title: Keybindings
description: Every hotdoc keyboard shortcut — the global hotkey, navigation, result actions, and window controls.
---

This page lists every keyboard shortcut hotdoc responds to, grouped by where it applies.

## Global (OS-level)

This shortcut works anywhere on your desktop, even when hotdoc isn't focused:

| Shortcut           | Action                     |
| ------------------ | -------------------------- |
| `Ctrl+Shift+Space` | Open or focus the launcher |

If this doesn't fire for you (common on Wayland), see [Troubleshooting](../troubleshooting/) for
the `hotdoc-cli toggle` workaround.

## Navigation

These work while the launcher is open and results are showing:

| Shortcut  | Action                                                        |
| --------- | ------------------------------------------------------------- |
| `↑` / `↓` | Move the selection up or down the results list (wraps around) |

## Actions on the selected result

| Shortcut                            | Action                                                                                        |
| ----------------------------------- | --------------------------------------------------------------------------------------------- |
| `Enter`                             | Copy the command's **syntax** to the clipboard, then close the window automatically           |
| `Shift+Enter`                       | Copy the **example code** instead of the syntax                                               |
| `Ctrl+Enter` (`Cmd+Enter` on macOS) | Open the command's source URL in your browser (`https://` sources only), then hide the window |
| `Tab`                               | Toggle the **Details** pane for the selected result                                           |
| `Ctrl+P` (`Cmd+P` on macOS)         | Pin or unpin the selected result                                                              |

A few things worth being explicit about:

- `Enter` does **not** paste anything for you. It only puts the syntax on the clipboard — you
  paste it manually wherever you need it.
- `Ctrl+Enter` / `Cmd+Enter` does **not** copy anything. It opens the source URL and hides the
  window.

## Window

| Shortcut                                      | Action                                                         |
| --------------------------------------------- | -------------------------------------------------------------- |
| `Esc`                                         | Close the Details pane if it's open, otherwise hide the window |
| `Ctrl+C` (`Cmd+C` on macOS) on an empty query | Hide the window                                                |

## Reserved combos

`Ctrl+C`, `Ctrl+V`, `Ctrl+X`, `Ctrl+Z`, `Ctrl+Y`, and `Ctrl+A` are reserved by the system and can't
be assigned as the global hotkey. See [Settings](../../guide/settings/) for how the hotkey is
configured.
