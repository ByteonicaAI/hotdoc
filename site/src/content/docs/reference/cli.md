---
title: CLI
description: hotdoc-cli subcommands and flags — index, query, copy, toggle, bench, and eval.
---

hotdoc ships a companion command-line tool, `hotdoc-cli`, for scripting, indexing, and binding
keyboard shortcuts on desktops that don't cooperate with global hotkeys. It has a small,
hand-written argument parser — invoke it as:

```bash
hotdoc-cli <subcommand> [args] [--flags]
```

## Everyday commands

These are the subcommands you're most likely to use day to day.

### `index`

Build the search index from packs.

```bash
hotdoc-cli index [--packs <dir>] [--out <dir>]
```

| Flag            | Description                          | Default                                                       |
| --------------- | ------------------------------------ | ------------------------------------------------------------- |
| `--packs <dir>` | Directory of packs to index          | the bundled curated packs                                     |
| `--out <dir>`   | Output directory for the built index | the app's index directory under `~/.local/share/hotdoc/index` |

### `query <text>`

Search the index and print tab-separated hits.

```bash
hotdoc-cli query "<text>" [--limit <n>] [--index <dir>]
```

| Flag            | Description                     | Default                   |
| --------------- | ------------------------------- | ------------------------- |
| `--limit <n>`   | Maximum number of hits to print | `8`                       |
| `--index <dir>` | Index directory to search       | the app's index directory |

### `copy <text>`

Search and copy the top hit's syntax to the clipboard — useful for scripts or shortcut bindings
that want the same "copy the answer" behavior as the app itself.

```bash
hotdoc-cli copy "<text>" [--index <dir>]
```

| Flag            | Description               | Default                   |
| --------------- | ------------------------- | ------------------------- |
| `--index <dir>` | Index directory to search | the app's index directory |

### `toggle`

Show or hide the running hotdoc window, by sending a UDP datagram to `127.0.0.1:47474`. No flags.

```bash
hotdoc-cli toggle
```

This is the recommended way to open hotdoc on Wayland compositors that won't deliver a global
hotkey to the app: bind a desktop or compositor keyboard shortcut to run `hotdoc-cli toggle`
instead of relying on `Ctrl+Shift+Space`. See
[Getting started](../../guide/getting-started/#wayland-and-other-compositors) and
[Troubleshooting](../troubleshooting/) for more.

## Developer / maintenance commands

These are used for benchmarking and evaluating ranking quality against a set of golden queries —
most users won't need them.

### `bench`

Run the golden-query benchmark.

```bash
hotdoc-cli bench [--queries <path>] [--index <dir>] [--adversarial]
```

| Flag               | Description                            |
| ------------------ | -------------------------------------- |
| `--queries <path>` | Path to the golden-query file          |
| `--index <dir>`    | Index directory to benchmark against   |
| `--adversarial`    | Include adversarial queries in the run |

### `eval`

Evaluate ranker precision and MRR (mean reciprocal rank) against golden queries.

```bash
hotdoc-cli eval [--queries <path>] [--index <dir>] [--adversarial]
```

| Flag               | Description                            |
| ------------------ | -------------------------------------- |
| `--queries <path>` | Path to the golden-query file          |
| `--index <dir>`    | Index directory to evaluate against    |
| `--adversarial`    | Include adversarial queries in the run |
