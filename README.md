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

## Milestones

| Milestone                      | Status          | Scope                                                                                                                                                                |
| ------------------------------ | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **M0** — Scaffold              | Done            | Tauri 2 + Svelte 5 + CI/lint/test + global hotkey + single-instance                                                                                                  |
| **M1** — Core (alpha)          | Done            | Tantivy index + search/query IPC + security gates (CSP, https-only URL)                                                                                              |
| **M2** — Public readiness      | **Done** (v0.2) | 5 curated packs (git, docker, kubectl, gh, curl), golden query set (24 queries), NFR benches, AppImage + .deb packaging, workspace hygiene, logging infra            |
| **M2.5** — Refactor            | Done            | `eprintln!` → `tracing!` sweep, file rotation logging, `log_error` IPC bridge, `EntryMeta` struct                                                                    |
| **M3** — Recents & nav         | **Done**        | Recents (FR-R*), ↑/↓ keyboard nav (FR-S3), pinned (FR-P*), command palette, tray icon, settings                                                                      |
| **M3.5** — Polish & audit-debt | **Planned**     | Theme apply, persistent-index reuse, SQLite pack/entry population, scorer↔spec reconciliation, SEC-5 lint, FR-I8 — see `docs/internal/plan-m3.5-audit-mitigation.md` |
| **v1.0** — Public release      | **TBD**         | All FR/NFR/SEC gates green, signed distributable                                                                                                                     |
| **v1.1** — Cross-platform      | **TBD**         | Windows + macOS port, pack-update channel, full a11y, personal snippets                                                                                              |

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
- **NFR benches** — `bench-open` (index-build p50 ≤ 50ms) and `bench-search` (search p50 ≤ 16ms)
- **CI-gated golden queries** — 24 search-precision tests in CI
- **5 curated packs** — git (15 cards), docker (18 cards), kubectl (18 cards), gh (12 cards), curl (15 cards) — 13 more skeletal packs awaiting content
- **Security** — strict CSP, https-only URL allowlist, escape-then-highlight rendering, `unwrap_used = deny`, `unsafe_code = forbid`

## Quick start

```bash
pnpm install
pnpm tauri dev
```

Build artifacts:

```bash
pnpm tauri build --target appimage  # AppImage
pnpm tauri build --target deb        # .deb
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
docs/internal/        # PRD, spec, plans, handoffs, methodology
```

## License

Dual **MIT OR Apache-2.0** (code). Bundled tldr-derived content remains CC-BY-4.0, attributed per pack.
