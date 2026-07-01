# hotdoc

**Local-first, keyboard-driven desktop launcher** that answers one question fast: _"what's the exact syntax for the tool I already know?"_

Press `Ctrl+Shift+Space`, type a few fuzzy words, get the command, copy it, paste it, back to flow. Target ≤ 5 seconds from hotkey to paste. No AI, no account, no subscription, no browser, no network.

## Stack

| Layer         | Tech                               |
| ------------- | ---------------------------------- |
| App framework | Tauri v2                           |
| Frontend      | Svelte 5 + TypeScript (Vite SPA)   |
| Search        | Tantivy (BM25 + fuzzy, in-process) |
| Storage       | SQLite via rusqlite (WAL)          |
| CLI           | hotdoc-cli (clap-based)            |

## Roadmap

### v1.0 — Public release (Linux)

- [x] Core launcher — global hotkey, fuzzy search, keyboard-only navigation
- [x] 18 curated packs (729 command cards)
- [x] Recents, pinned, command palette, tray, settings
- [x] Deterministic ranker + CI-gated golden queries (strict first-result ≥ 95%)
- [x] Security gates — strict CSP, https-only opens, least-privilege IPC, content lint
- [x] NFR benches — open-time, search latency, idle memory, install & index size
- [x] Diagnostics bundle (redacted) + structured file-rotated logging
- [ ] Signed distributable (GPG + SHA256SUMS)
- [ ] Clean-box packaged-install verification

### v1.1 — Cross-platform & beyond

- [ ] Windows + macOS port
- [ ] Pack-update channel
- [ ] Full accessibility (screen-reader / AT support)
- [ ] Personal snippets
- [ ] Streaming index progress

## Current features

- **Global hotkey** (`Ctrl+Shift+Space`) — opens the launcher from any app; also works via CLI toggle (Wayland fallback)
- **Fuzzy search** — case-insensitive, prefix-aware, edit-distance 1 for 4+ char tokens
- **Source-priority ranking** — `official` > `cheat-sheet` > `curated`
- **Exact-match boost** — cards whose syntax/title matches the query get extra weight
- **Copy syntax** (`Enter`) or **copy example** (`Shift+Enter`)
- **Open source URL** (`Ctrl+Enter`) — https-only allowlist gate
- **Focus-loss hides** — window auto-hides on blur
- **Zero-result state** — shows "No matches" when query finds nothing
- **Single-instance** — only one process runs; second launch focuses the existing window
- **Structured logging** — `tracing` with daily file rotation under `~/.local/share/hotdoc/logs/`
- **NFR benches** — `bench-index-build` (index build p50 ≤ 150ms) and `bench-search` (search p50 ≤ 16ms)
- **CI-gated golden queries** — 68 search-precision queries with committed precision/MRR floors
- **18 curated packs** — 729 command cards spanning common CLI tools
- **Security** — strict CSP, https-only URL allowlist, escape-then-highlight rendering, `unwrap_used = deny`, `unsafe_code = forbid`

## Quick start

```bash
pnpm install
pnpm tauri dev
```

Build artifacts:

```bash
pnpm build:linux   # .deb
pnpm build:mac     # .app + .dmg (on macOS)
pnpm build:clean   # clean rebuild, all bundles
```

Verify everything:

```bash
pnpm verify:all
```

## Project layout

```
src/                  # Svelte 5 frontend (TypeScript)
src-tauri/            # Tauri v2 shell (Rust)
crates/hotdoc-core/   # Core library (Tantivy index, search, pack loader)
packs/curate/         # Curated tool card JSON manifests
tests/search/         # Golden query set (CI-gated)
```

## License

Dual **MIT OR Apache-2.0** (code). Bundled tldr-derived content remains CC-BY-4.0, attributed per pack.
