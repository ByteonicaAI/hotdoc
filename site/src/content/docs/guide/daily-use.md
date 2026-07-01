---
title: Daily use
description: Search behavior, result actions, recents, pinning, and the command palette.
---

Once you know the core loop from [Getting started](../getting-started/), this page covers
everything else you'll use day to day.

## Search

Search is typo-tolerant: it combines fuzzy matching with prefix matching (BM25 ranking with a
fuzzy rescue pass), so near-misses and partial words still surface the right result.

## Result actions

With a result selected, each key does something different:

- `Enter` — copy the command's syntax to the clipboard and close the window
- `Shift+Enter` — copy the result's example code instead of the syntax
- `Ctrl+Enter` (`Cmd+Enter` on macOS) — open the command's source URL in your browser
  (`https://` sources only)
- `Tab` — toggle the Details pane for the selected result
- `Esc` — close the Details pane if it's open, otherwise hide the window

## Recents

hotdoc remembers what you've activated recently and shows it in the empty view (before you've
typed anything). Recents can be turned off entirely with the `recents_enabled` setting — see
[Settings](../settings/).

## Pinned

Pin the selected result so it always shows up first in the empty view:

```
Ctrl+P
```

(`Cmd+P` on macOS.) Press the same shortcut again on a pinned item to unpin it.

## Command palette

Type a leading `>` in the search box to switch into the command palette. This is a query prefix,
not a keyboard shortcut. Available commands:

```
> recents
> recents clear
> settings
> about
> help
```

You can also filter results down to a single pack by typing `>` followed by the pack's ID, for
example:

```
> git
```

See [Packs](../packs/) for the full list of pack IDs.
