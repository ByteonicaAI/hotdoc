# Hotdoc — Linux install (M2)

Both bundles build from the repo root:

```bash
pnpm tauri build --bundles deb,appimage
```

That produces:

- `src-tauri/target/release/bundle/deb/hotdoc_0.1.0_amd64.deb`
- `src-tauri/target/release/bundle/appimage/hotdoc_0.1.0_amd64.AppImage`

## .deb (Ubuntu / Debian)

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/hotdoc_0.1.0_amd64.deb
hotdoc toggle   # shows + focuses the launcher window
```

Uninstall:

```bash
sudo dpkg -P hotdoc
```

`.deb` is the recommended install path on Debian-family systems. The CLI
binary lands at `/usr/bin/hotdoc`; the GUI binary at
`/usr/lib/hotdoc/hotdoc`; `.desktop` file + icon at the standard
system paths. No orphan files.

## AppImage (any distro)

```bash
chmod +x src-tauri/target/release/bundle/appimage/hotdoc_0.1.0_amd64.AppImage
./src-tauri/target/release/bundle/appimage/hotdoc_0.1.0_amd64.AppImage
```

AppImage is portable: no install, no sudo. The downside is no CLI
binary — the toggle command is reached by symlinking the bundled
binary or using a wrapper:

```bash
sudo ln -s /opt/hotdoc.AppImage /usr/local/bin/hotdoc-cli
```

(Or skip the CLI entirely and bind your DE shortcut to the AppImage
directly.)

### Runtime deps for AppImage

- FUSE 2 (libfuse2 on older distros, libfuse2t64 on Ubuntu 24.04+).
- On modern Ubuntu without FUSE 2, run with `--appimage-extract-and-run`
  or extract the AppImage manually.

## Verifying the install

After install (deb) or first run (AppImage), press the global hotkey
binding (Ctrl+Shift+Space default), type a query, hit Enter. The
syntax should land on your clipboard. If `hotdoc toggle` from a
terminal does not focus the window, check that no other instance is
running (single-instance plugin) and that the UDP port 47474 is not
firewalled (loopback only — see NFR-7).

## Build prerequisites

Required system packages (Ubuntu 24.04+):

```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl wget file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

For `.deb` bundling: `dpkg`, `binutils` (always present).
For AppImage bundling: `wget` (already in the apt list above) and
FUSE 2 (libfuse2 or libfuse2t64).
