# Installing and using Hotdoc on Linux

Hotdoc is a keyboard-driven desktop launcher that answers one question fast: _what is the exact syntax for the tool I already know?_ Press a global hotkey, type a few words, copy the result, and return to your flow — all in under five seconds.

This guide covers building from source, installing, first launch, daily use, and configuration on Ubuntu 24.04 and later (including 26.04). Other systemd-based Debian-family distributions work the same way. Fedora and Arch users need to adapt the package names in the system dependencies step.

---

## Contents

1. [Prerequisites](#1-prerequisites)
2. [Build from source](#2-build-from-source)
3. [Install](#3-install)
4. [First launch and initial setup](#4-first-launch-and-initial-setup)
5. [Daily use](#5-daily-use)
6. [Global hotkey](#6-global-hotkey)
7. [Wayland and the CLI toggle](#7-wayland-and-the-cli-toggle)
8. [System tray](#8-system-tray)
9. [Settings](#9-settings)
10. [Autostart](#10-autostart)
11. [Logs and data locations](#11-logs-and-data-locations)
12. [Uninstalling](#12-uninstalling)
13. [Troubleshooting](#13-troubleshooting)

---

## 1. Prerequisites

### System packages

Install the required development libraries. All of these are available in the default Ubuntu 24.04 / 26.04 repositories.

```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

For `.deb` bundling, `dpkg` and `binutils` are required — both are present by default on Ubuntu.

For the AppImage bundle, the `wget` above is sufficient. At _runtime_ the AppImage also needs FUSE 2:

```bash
# Ubuntu 24.04 and later
sudo apt install -y libfuse2t64

# Ubuntu 22.04 and earlier
sudo apt install -y libfuse2
```

If you cannot install FUSE, run the AppImage with `--appimage-extract-and-run` instead of executing it directly.

### Rust toolchain

Hotdoc's backend is written in Rust. Install `rustup` and the stable toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the on-screen prompts (the defaults are correct). Then reload your shell environment:

```bash
source "$HOME/.cargo/env"
```

Verify:

```bash
rustc --version   # must be 1.77 or later
cargo --version
```

### Node.js and pnpm

Hotdoc's frontend requires Node.js 24 and pnpm 10. The recommended way to manage Node versions is `fnm`:

```bash
curl -fsSL https://fnm.vercel.app/install | bash
source "$HOME/.local/share/fnm/env"   # adjust if fnm chose a different path
fnm install 24
fnm use 24
node --version   # must print v24.x.x
```

Install pnpm:

```bash
npm install -g pnpm@10
pnpm --version   # must print 10.x.x
```

---

## 2. Build from source

Clone the repository and move into the project root:

```bash
git clone https://github.com/ByteonicaAI/hotdoc.git
cd hotdoc
```

Install JavaScript dependencies:

```bash
pnpm install
```

Build the application. This compiles the Svelte frontend, compiles the Rust backend, and produces both a `.deb` package and an AppImage in one step:

```bash
pnpm tauri build --bundles deb,appimage
```

The first build downloads and compiles all Rust crates, which can take several minutes. Subsequent builds are incremental and much faster.

Output artifacts:

```
src-tauri/target/release/bundle/deb/hotdoc_0.4.0_amd64.deb
src-tauri/target/release/bundle/appimage/hotdoc_0.4.0_amd64.AppImage
```

> **Note:** The version string in the filename reflects the current `version` field in `package.json` and `src-tauri/tauri.conf.json`.

### Optional: run the full verification suite

Before shipping, you can run the complete CI gate locally. This requires `xvfb` for the headless GUI benchmarks:

```bash
sudo apt install -y xvfb
pnpm verify:all
```

This runs TypeScript checks, ESLint, Prettier formatting checks, Vitest unit tests, pnpm and Cargo security audits, string extraction checks, Clippy, `cargo fmt`, Rust unit tests, a no-bundle Tauri build, headless open-time and memory benchmarks, and the golden search-precision query set.

---

## 3. Install

Choose one install method. The `.deb` package is recommended on Ubuntu and other Debian-family systems.

### Option A — .deb package (recommended)

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/hotdoc_0.4.0_amd64.deb
```

What the package installs:

| Path                                     | Contents                                   |
| ---------------------------------------- | ------------------------------------------ |
| `/usr/bin/hotdoc`                        | CLI binary (used for the `toggle` command) |
| `/usr/lib/hotdoc/hotdoc`                 | GUI application binary                     |
| `/usr/share/applications/hotdoc.desktop` | Desktop entry (app launcher integration)   |
| `/usr/share/icons/…`                     | Application icon                           |

No files are written outside these standard paths. Uninstalling removes all of them cleanly.

### Option B — AppImage (portable, no root)

```bash
chmod +x src-tauri/target/release/bundle/appimage/hotdoc_0.4.0_amd64.AppImage
./src-tauri/target/release/bundle/appimage/hotdoc_0.4.0_amd64.AppImage
```

The AppImage is self-contained. No install step is required. Move it anywhere on your `$PATH` if you want to launch it by name:

```bash
mv hotdoc_0.4.0_amd64.AppImage ~/.local/bin/hotdoc
chmod +x ~/.local/bin/hotdoc
hotdoc   # starts the GUI
```

The AppImage does not install a `/usr/bin/hotdoc` CLI binary. To use the `toggle` command with an AppImage install, create a wrapper script manually:

```bash
cat > ~/.local/bin/hotdoc-toggle <<'EOF'
#!/bin/sh
# Sends a UDP toggle signal to the running Hotdoc instance.
exec hotdoc-cli toggle
EOF
chmod +x ~/.local/bin/hotdoc-toggle
```

Or bind your desktop environment shortcut directly to the AppImage path instead of relying on the CLI.

---

## 4. First launch and initial setup

### Starting the app

After a `.deb` install, launch Hotdoc from your application menu (search for "Hotdoc") or from a terminal:

```bash
hotdoc
```

After an AppImage install:

```bash
./hotdoc_0.4.0_amd64.AppImage
# or, if you moved it to $PATH:
hotdoc
```

### What happens on first launch

1. Hotdoc builds its full-text search index from the bundled command packs. This takes under 50 ms.
2. A desktop notification appears: **"Press Ctrl+Shift+Space to open Hotdoc"**. This notification fires once and is never shown again.
3. A tray icon appears in your system notification area.
4. The launcher window is hidden. It only appears when you press the global hotkey or run `hotdoc toggle`.

### Single-instance enforcement

Hotdoc enforces a single-instance policy. If you launch a second copy while one is already running, the running instance's window comes to the foreground instead and the second process exits immediately.

---

## 5. Daily use

### Opening the launcher

Press `Ctrl+Shift+Space` from any application. The launcher window appears centered near the top of your primary monitor, 60 pixels from the top edge.

### Searching

Type any words related to the command you are looking for. Search is fuzzy, case-insensitive, and prefix-aware. Tokens of four or more characters tolerate one edit distance (one substitution, insertion, or deletion).

Examples:

| What you type      | What you find                            |
| ------------------ | ---------------------------------------- |
| `git rebase`       | `git rebase --onto <newbase> <upstream>` |
| `docker ps`        | `docker ps --all --format "table …"`     |
| `kubectl get pods` | `kubectl get pods -n <namespace>`        |
| `curl head`        | `curl -I <url>`                          |
| `gh pr create`     | `gh pr create --title "…" --body "…"`    |

Results are ranked by source priority (`official` > `cheat-sheet` > `curated`) with an exact-match boost applied when the query matches the command title or syntax directly.

### Keyboard actions

| Key           | Action                                                                   |
| ------------- | ------------------------------------------------------------------------ |
| `↑` / `↓`     | Move selection up or down through results                                |
| `Enter`       | Copy the selected command **syntax** to clipboard                        |
| `Shift+Enter` | Copy the selected command **example** to clipboard                       |
| `Ctrl+Enter`  | Open the source URL for the selected result in your browser (https-only) |
| `Escape`      | Hide the launcher window                                                 |

### After copying

The launcher hides automatically when it loses focus. Once you press `Enter` or `Shift+Enter`, the syntax or example is in your clipboard. Switch to your terminal and paste with `Ctrl+Shift+V` (or `Ctrl+V` in most GUI terminals).

### Zero-result state

If no results match your query, the launcher shows a "No matches" message with a suggestion to try broader terms. The search index covers git, docker, kubectl, gh, and curl by default.

---

## 6. Global hotkey

### Default hotkey

The default global hotkey is `Ctrl+Shift+Space`. It works on X11 and on XWayland sessions. On native Wayland compositors where global hotkeys are not supported by the underlying plugin, use the CLI toggle instead (see [section 7](#7-wayland-and-the-cli-toggle)).

The hotkey is registered system-wide via `tauri-plugin-global-shortcut` on startup. It intercepts the key combination regardless of which application has focus.

### Changing the hotkey

Open the launcher (`Ctrl+Shift+Space`) and navigate to **Settings** via the tray icon's **Preferences** item. In the hotkey field, type your preferred combination and confirm.

Valid modifiers: `Ctrl`, `Shift`, `Alt`, `Super` (Windows key).

The following combinations are blocked at the application layer because intercepting them would break clipboard and edit operations across every other application:

- `Ctrl+C`
- `Ctrl+V`
- `Ctrl+X`
- `Ctrl+Z`
- `Ctrl+Y`
- `Ctrl+A`

If the OS refuses to register your chosen combination (because another application or the desktop environment has already claimed it), Hotdoc shows an error message and retains the previous hotkey. In that case, release the combination in your DE's keyboard settings first.

### Hotkey not working

On **GNOME**, if `Ctrl+Shift+Space` conflicts with an input method shortcut, go to **Settings → Keyboard → Special Character Entry** and disable or change the conflicting binding.

On **KDE Plasma**, check **System Settings → Shortcuts → Global Shortcuts** for conflicts.

On **i3/Sway and other tiling WMs**, ensure the compositor is not consuming the combination before Hotdoc can see it.

---

## 7. Wayland and the CLI toggle

On native Wayland sessions (no XWayland), `tauri-plugin-global-shortcut` cannot register a system-wide hotkey. The recommended workaround is to bind a shortcut in your compositor to run `hotdoc toggle`, which sends a UDP signal to the running Hotdoc instance via loopback port `47474`.

### Setting up a compositor shortcut

**GNOME (Wayland):**

1. Open **Settings → Keyboard → View and Customize Shortcuts → Custom Shortcuts**.
2. Click **+** to add a shortcut.
3. Set the command to `hotdoc toggle` (or the full path `/usr/bin/hotdoc toggle` for `.deb` installs).
4. Assign `Ctrl+Shift+Space` (or any combination not already claimed).
5. Click **Add**.

**KDE Plasma (Wayland):**

1. Open **System Settings → Shortcuts → Custom Shortcuts**.
2. Click **Edit → New → Global Shortcut → Command/URL**.
3. Set the trigger key combination and the command to `/usr/bin/hotdoc toggle`.
4. Apply.

**Sway:**

Add to `~/.config/sway/config`:

```
bindsym Ctrl+Shift+Space exec hotdoc toggle
```

Reload sway config: `swaymsg reload`.

**Hyprland:**

Add to `~/.config/hypr/hyprland.conf`:

```
bind = CTRL SHIFT, Space, exec, hotdoc toggle
```

### How the toggle command works

`hotdoc toggle` sends a single UDP byte to `127.0.0.1:47474`. The running Hotdoc instance listens on that port and responds by showing and focusing the launcher window. If no instance is running, the command times out after 500 ms and exits with an error:

```
hotdoc server did not respond (is it running?)
```

Port `47474` is loopback-only. It is never exposed to any network interface.

---

## 8. System tray

Hotdoc places an icon in the system tray (notification area) when running. The icon shows the tooltip **"Hotdoc — Ctrl+Shift+Space"** on hover.

> **Linux note:** Left-click on the tray icon does not open the launcher. This is a known limitation of `tauri-plugin-tray` on Linux — the `TrayIconEvent::Click` event is not emitted by the underlying library. Use the global hotkey or `hotdoc toggle` to open the window.

Right-click the tray icon to access the context menu:

| Menu item            | Action                                                                                |
| -------------------- | ------------------------------------------------------------------------------------- |
| **Preferences**      | Opens the Settings panel inside the launcher window                                   |
| **Reload index**     | Rebuilds the search index from the bundled packs and shows a count of indexed entries |
| **Open data folder** | Opens `~/.local/share/hotdoc/` in your file manager                                   |
| **Copy diagnostics** | Copies a redacted diagnostic report to your clipboard (useful for bug reports)        |
| **Quit**             | Exits Hotdoc completely and removes the tray icon                                     |

---

## 9. Settings

Access settings via **Tray icon → Preferences** or via the command palette inside the launcher.

| Setting       | Description                                                                            |
| ------------- | -------------------------------------------------------------------------------------- |
| **Hotkey**    | The global key combination that opens the launcher. See [section 6](#6-global-hotkey). |
| **Autostart** | Launch Hotdoc automatically when you log in. See [section 10](#10-autostart).          |
| **Theme**     | Light or dark. Follows system default if not set explicitly.                           |

Settings are persisted to the SQLite database at `~/.local/share/hotdoc/hotdoc.db`. No plain-text config files are written.

---

## 10. Autostart

Enable autostart from **Tray icon → Preferences → Autostart**, or from the Settings panel. When enabled, Hotdoc registers itself with `tauri-plugin-autostart` so it starts when you log in.

To check whether autostart is active outside the app:

```bash
# systemd user autostart via .desktop file
ls ~/.config/autostart/ | grep hotdoc
```

To disable autostart without launching the app, remove the autostart entry:

```bash
rm ~/.config/autostart/hotdoc.desktop
```

---

## 11. Logs and data locations

All runtime data is stored under `~/.local/share/hotdoc/`:

| Path                                               | Contents                                                                |
| -------------------------------------------------- | ----------------------------------------------------------------------- |
| `~/.local/share/hotdoc/hotdoc.db`                  | SQLite database — packs, entries, recents, pinned, settings, search log |
| `~/.local/share/hotdoc/index/`                     | Tantivy full-text search index                                          |
| `~/.local/share/hotdoc/logs/hotdoc.log.YYYY-MM-DD` | Structured logs, daily rotation                                         |

### Reading logs

```bash
# Today's log
tail -f ~/.local/share/hotdoc/logs/hotdoc.log.$(date +%Y-%m-%d)

# Last 100 lines across all log files
tail -n 100 ~/.local/share/hotdoc/logs/hotdoc.log.*
```

In release builds, log verbosity is controlled by the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug hotdoc   # verbose output
RUST_LOG=warn hotdoc    # warnings and errors only (default is no var = info)
```

### Rebuilding the index

If search results seem stale or incorrect after a pack update, rebuild the index via **Tray icon → Reload index**. The launcher shows a toast confirming the number of entries after the rebuild completes.

---

## 12. Uninstalling

### .deb install

```bash
sudo dpkg -P hotdoc
```

This removes all files installed by the package (`/usr/bin/hotdoc`, `/usr/lib/hotdoc/`, `.desktop` entry, icons). It does not remove user data.

To also remove user data and logs:

```bash
rm -rf ~/.local/share/hotdoc/
rm -f ~/.config/autostart/hotdoc.desktop
```

### AppImage

Delete the AppImage file. No system files were written.

To also remove user data:

```bash
rm -rf ~/.local/share/hotdoc/
rm -f ~/.config/autostart/hotdoc.desktop
```

---

## 13. Troubleshooting

### Launcher window does not appear when pressing the hotkey

1. Confirm Hotdoc is running: `pgrep -a hotdoc` should show a process.
2. Check whether another application has claimed the same key combination. Open your DE's keyboard shortcut settings and search for `Ctrl+Shift+Space`.
3. On native Wayland, global hotkeys are not supported. Use `hotdoc toggle` bound to a compositor shortcut instead (see [section 7](#7-wayland-and-the-cli-toggle)).
4. Check the log for registration errors: `grep "hotkey" ~/.local/share/hotdoc/logs/hotdoc.log.*`

### `hotdoc toggle` says "hotdoc server did not respond"

The running Hotdoc instance listens on UDP `127.0.0.1:47474`. Possible causes:

- Hotdoc is not running. Start it first.
- A firewall rule is blocking loopback UDP. Check: `sudo ufw status` — loopback traffic should never be blocked, but some configurations are overly restrictive.
- Another process has bound port `47474`. Check: `ss -ulnp | grep 47474`.

### AppImage fails to start — "fuse: device not found"

Install FUSE 2:

```bash
sudo apt install -y libfuse2t64   # Ubuntu 24.04+
sudo apt install -y libfuse2      # Ubuntu 22.04
```

Alternatively, extract and run without FUSE:

```bash
./hotdoc_0.4.0_amd64.AppImage --appimage-extract-and-run
```

### Tray icon is not visible

Some desktop environments (notably GNOME with no AppIndicator extension) do not show application tray icons by default. Install the GNOME Shell extension **AppIndicator and KStatusNotifierItem Support**:

```bash
sudo apt install -y gnome-shell-extension-appindicator
```

Then enable it in **GNOME Extensions** or with:

```bash
gnome-extensions enable ubuntu-appindicators@ubuntu.com
```

Log out and back in for the change to take effect.

### Search returns no results

The index is built from bundled packs at startup. If the index is missing or corrupt:

1. Use **Tray icon → Reload index** to rebuild from scratch.
2. If the launcher will not open, delete the index directory and restart:
   ```bash
   rm -rf ~/.local/share/hotdoc/index/
   hotdoc
   ```

### Database migration failed

On startup, if Hotdoc detects a corrupt or unmigrateable SQLite database, it automatically deletes the database file and rebuilds from scratch. You will lose recents, pinned entries, and settings, but the search index is rebuilt from the bundled packs immediately. The event is logged at `ERROR` level in the log file.

To force a clean reset manually:

```bash
rm -rf ~/.local/share/hotdoc/
hotdoc
```
