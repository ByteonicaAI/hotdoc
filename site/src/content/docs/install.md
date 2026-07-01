---
title: Install
description: Install hotdoc 0.4.0 on Linux from the .deb release bundle, or build it from source.
---

hotdoc ships as a single `.deb` package. There is no AppImage — the `.deb` is the only supported
release artifact.

## Supported platforms

hotdoc targets **Ubuntu 24.04 and later**, which ships WebKitGTK 4.1. Other distributions are not
officially supported — they are, at most, community-untested.

## Install from the release `.deb`

Download the latest `.deb` from the [GitHub releases page](https://github.com/ByteonicaAI/hotdoc/releases/latest),
then install it with `dpkg`:

```bash
sudo dpkg -i hotdoc_0.4.0_amd64.deb
```

If `dpkg` reports missing dependencies, install them with `apt` instead so it resolves the
dependency chain for you:

```bash
sudo apt install ./hotdoc_0.4.0_amd64.deb
```

## Build from source

### System dependencies

hotdoc is built with [Tauri v2](https://v2.tauri.app/), which needs WebKitGTK and a handful of
native libraries. Install the exact package set used in CI:

```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

You'll also need:

- Node.js >= 24
- pnpm >= 10
- A Rust toolchain (stable), as required by Tauri v2

### Build

From the repository root:

```bash
pnpm install
pnpm build:linux
```

`build:linux` runs `tauri build --bundles deb`, which produces a single `.deb` under:

```
src-tauri/target/release/bundle/deb/
```

Install the resulting package the same way as the release bundle:

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/*.deb
```

## First launch

On first launch, hotdoc builds its local search index. This happens automatically and only once —
subsequent launches start instantly. No network access, account, or telemetry is involved.

Once the index is ready, press **Ctrl+Shift+Space** anywhere on your desktop to open hotdoc.

## Data and logs

hotdoc stores its data locally under your home directory:

- Database: `~/.local/share/hotdoc/hotdoc.sqlite`
- Logs: `~/.local/share/hotdoc/logs/` (daily-rotated files named `hotdoc.log.YYYY-MM-DD`)

## Uninstall

```bash
sudo apt remove hotdoc
```

or, equivalently:

```bash
sudo dpkg -r hotdoc
```

## Next steps

Head to [Getting started](../guide/getting-started/) for a walkthrough of your first search, or
[Troubleshooting](../reference/troubleshooting/) if something isn't working as expected.
