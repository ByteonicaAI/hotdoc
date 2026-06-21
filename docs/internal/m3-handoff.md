# M3 Handoff — Recents / Nav / Pinned / Palette / Settings / Tray

**Date:** 2026-06-22
**From:** session that committed T1–T4 and started T5
**To:** the next session continuing M3
**Branch:** `main` (clean working tree at last commit)

---

## 0. Where to start

Read these in order:

1. `docs/internal/plan-m3-recents-nav-and-settings.md` — the master plan, has full T1–T10 detail + review log
2. `docs/internal/m2.5-to-m3-handoff.md` — pre-M3 audit findings still relevant
3. `docs/internal/spec-v0.2.md` FR-R1–R4, FR-P1–P3, FR-S3, FR-S6–S9, FR-X1–X4, FR-T1–T3, §4.2.1, §7.5, §9.2 — behavior source of truth
4. `AGENTS.md` — code conventions (Svelte 5 runes, `unsafe_code = forbid`, `unwrap_used = deny`, `verbatimModuleSyntax`, etc.)
5. **This handoff** — covers what's done, what's queued, and the plan-vs-reality gaps

## 1. What's committed (T1–T4)

| Commit | Task | What landed |
|--------|------|-------------|
| `acb6faf` | T1 | `crates/hotdoc-core/src/store/{mod,meta,recents}.rs`; `open` + `migrate`; spec §9.2 schema; WAL + FK; `default_db_path()`; `StoreError { Db, Io, MissingDataDir }` |
| `8f97401` | T2 | `crates/hotdoc-core/src/store/recents.rs`; `record`/`top_n`/`clear`/`is_enabled`; 20-row FIFO cap; `recents_enabled` setting gate; `AppState.db: Arc<Mutex<Connection>>`; 3 IPC commands; `Launcher.activate()` hooks `recents.onActivation` |
| `8ba1d5c` | T3 | `src/lib/EmptyView.svelte`; `Launcher.loadEmptyView()`; `hotdoc://refresh-empty-view` emit on `Focused(true)`; `listen()` in `App.svelte` `onMount`; input `value={launcher.query}` binding |
| `aa80103` | T4 | `Launcher.selectedIndex = $state(-1)`; `ArrowUp`/`ArrowDown` wrap-around; Enter reads `results[selectedIndex]`; `ResultItem` `active={i === launcher.selectedIndex}` |

**Test counts (current):**
- Rust: **30 tests** (17 baseline + 3 store infra + 6 recents + 4 pinned — the 4 pinned pass but T5's frontend tests aren't committed yet)
- Frontend: **21 tests** (9 baseline + 2 recents activation + 4 empty-view + 5 nav + 2 pinned toggle — pending T5 commit)

**Verification at HEAD:** `pnpm run verify:all` exits 0 (typecheck + lint + format:check + vitest + cargo fmt + clippy + cargo test).

## 2. What's pending

| # | Task | State |
|---|------|-------|
| T5 | Pinned (Ctrl+P, IPC, empty-view section, ResultItem 📌) | Backend + frontend code DONE; tests failing due to Svelte 5 reactivity bug in `EmptyView.svelte` (see §3); 4 new tests drafted, not committed |
| T6 | Command palette (`> recents` / `> settings` / `> help` / `> <pack_id>` / `> recents clear`) | Not started |
| T8+T9 | Settings panel (hotkey rebind + OS deny-list + theme + autostart + recents toggle) | Not started |
| T7 | Tray icon (right-click menu; Linux: no left-click per `tauri-2.11.3/src/tray/mod.rs:66`) | Not started |
| T10 | Test guardrail | Embedded in each task; no separate commit |

**Estimated remaining:** T5 = ~1h, T6 = ~3h, T8+T9 = ~5h, T7 = ~2h. Total ≈ 11h of focused work.

## 3. Known issues to fix FIRST in next session

### 3a. T5 Svelte 5 reactivity bug (BLOCKS T5 commit)

`pnpm run test` currently fails:

```
TypeError: Cannot read properties of undefined (reading 'length')
 ❯ src/lib/EmptyView.svelte:52:15
```

Line 52: `{#if pinned.length > 0}`. `pinned` is `undefined` even though `App.svelte` passes `pinned={launcher.pinnedList}`. Root cause (suspected): the `invoke` mock returns `undefined` for unhandled commands (per `beforeEach`), so `getPinned()` resolves to `undefined` and Svelte 5's `$state` proxy briefly propagates `undefined` into the prop. Even though `pinnedList = $state<SearchHit[]>([])` is initialised, a `Promise<undefined>` from the Tauri wrapper overrides it.

**Fix:** defensive defaults in `EmptyView.svelte`:

```svelte
const {
  recents = [],
  pinned = [],
  onSelectRecent,
  onSelectPinned,
  selectedIndex,
  pinnedOffset,
}: Props = $props();
```

Also make `tauri.ts`'s `getPinned()` return type stay `Promise<SearchHit[]>` but document that mock impls must return `[]` not `undefined`. Then the T5 frontend tests (which DO mock `get_pinned`) will pass.

**Tests already drafted** (at `src/App.test.ts:350-386`, not yet committed):
- `Ctrl+P pins the highlighted row, fires pin_entry IPC`
- `Ctrl+P again unpins (calls unpin_entry)`

These need the EmptyView fix to land first.

### 3b. Plan gaps discovered and fixed (do not re-fix)

These were real plan errors caught during execution:

1. **Plan claimed `rusqlite::Connection` is `Send + Sync` since 0.27.** False for 0.31 — `RefCell<InnerConnection>` is `!Sync`. **Fix landed:** `AppState.db: Arc<Mutex<Connection>>`. Plan was wrong on this point.

2. **Plan omitted `rusqlite` from `src-tauri/Cargo.toml`.** The `Arc<Mutex<Connection>>` type signature requires it as a direct dep, not just transitively. **Fix landed:** added `rusqlite = { version = "0.31", features = ["bundled"] }` to `src-tauri/Cargo.toml`.

3. **Plan called for `open()` in `src-tauri/src/lib.rs` to inline the `dirs::data_local_dir()` resolution.** Added `hotdoc_core::store::default_db_path()` to keep the path-resolver in core (where `dirs` already lives) and avoid adding `dirs` to src-tauri.

4. **`StoreError::InvalidPath` takes a `PathBuf` arg.** Switched to a custom `MissingDataDir` variant.

5. **`migrate()` stored `schema_version` as integer, but `meta.value` column is TEXT.** Stored as `"1"` string; comparison is `Option<&str> != Some("1")`.

6. **Last-used-at timestamps in seconds (as_secs)** cause test `evicts_beyond_20` to fail (all 25 records share the same second). **Fix landed:** `as_millis()` for millisecond resolution. This matches spec §9.2 wording "INTEGER NOT NULL" — no constraint on units.

### 3c. Conventions established (follow)

- **Tests use `InvokeArgs` import** for the typed signature: `import type { InvokeArgs } from "@tauri-apps/api/core"`. Required to silence TS errors on the `invoke` mock.
- **`(cmd: string, args?: InvokeArgs)`** signature; cast args as needed: `(args as { query?: string } | undefined)?.query`.
- **`vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }))`** — must be added to `App.test.ts` top mocks before any test that renders `App` (T3 added this).
- **`beforeEach`** mock impl returns `[]` for `get_recents` and `get_pinned`; everything else `undefined`. T5 tests override per-case.
- **`launcher.value={launcher.query}`** is required on `<input>` for programmatic query updates (e.g. recent click) to flow back to the DOM. Don't remove this.
- **`ResultItem` props shape** is `{ hit: SearchHit; active: boolean; pinned?: boolean }`. The `pinned?` prop added in T5 renders `📌`.
- **Launcher constructor is sync** (not `async create()` as plan suggested). `validPackIds` for T6 palette filters lands in T6; `Launcher.create()` was a plan-level complexity that didn't pan out for the simple synchronous use case.

### 3d. Tauri `Emitter` import

`src-tauri/src/lib.rs` already imports `tauri::{Emitter, Manager}`. T7 (tray) and T8 (autostart plugin init) both need additional imports — check before adding.

## 4. Task-specific notes for remaining work

### T6 — Command palette

- **Pure-function helper** lives at `src/lib/launcher/commandMode.ts` — easy to unit-test in isolation.
- **IPC dep:** `list_packs` (returns `Vec<String>` of pack IDs). Currently **does not exist** — needs to be added to `commands.rs` and `tauri.ts`. Plan §5 Edit 0 specifies a `pub mod packs` in `store/` with `list_ids` reading from the (empty in M3) `packs` table.
- **Fallthrough behavior:** unknown `>` commands strip the `>` and run as normal search.
- **Post-filter choice:** the plan picks frontend-side filter (no Rust change to `commands::search`). Document the trade-off: `> docker` returns top-8 results filtered to docker — if docker has more than 8 hits, refine.
- **Test count budget:** 1 unit (commandMode) + 6 frontend integration.

### T8+T9 — Settings panel

- **3 new deps:** `tauri-plugin-autostart = "2"` in `src-tauri/Cargo.toml`. (Plan also lists `tauri-plugin-autostart` needing Linux/macOS cfg-guards — verify `LinuxLauncher::Rfd` variant name when the crate lands in `~/.cargo/registry/src/`.)
- **HOTKEY REBIND SIGNATURE CHANGE** (plan §5 T8 Edit 8): `hotkey::register(app)` becomes `hotkey::register(app, combo)`. Update `src-tauri/src/lib.rs:54` call site in the same commit — without it, `cargo build` fails with E0061.
- **Deny-list:** `Ctrl+C`, `Ctrl+V`, `Ctrl+X`, `Ctrl+Z`, `Ctrl+Y`, `Ctrl+A` (per plan's verification against `global-hotkey-0.8.0/src/hotkey.rs:168-232`).
- **Two distinct error strings:** app-level deny-list rejection vs OS-level refusal at registration time. Both retained per spec P1-7, surfaced in UI as different badges.
- **Autostart file path:** `~/.config/autostart/com.byteonica.hotdoc.desktop` (derived from `tauri.conf.json:5` identifier `"com.byteonica.hotdoc"`, NOT `hotdoc.desktop` as pass-1 of the plan had it wrong).
- **Settings surface:** modal or new route — `src/lib/SettingsPanel.svelte`. Triggered from tray "Preferences" (T7) AND from `> settings` palette command (T6).
- **Test count budget:** 2 backend (`settings::rebind_rejects_reserved`, `settings::rebind_accepts_valid_combo`) + 3 frontend.

### T7 — Tray icon

- **Use `linux-desktop-app-verify` skill** before starting — load with `skill_view(name='linux-desktop-app-verify')`. The skill documents the compositor matrix.
- **Linux-specific limitation:** `TrayIconEvent::Click` is unsupported on Linux per `tauri-2.11.3/src/tray/mod.rs:66`. **Left-click as show is not implementable on Linux in M3.** Right-click menu only. The plan acknowledges this — do not implement `on_tray_icon_event`.
- **Menu builder:** `MenuBuilder::new(handle).items(&[&prefs, &reload, &open_folder, &sep, &quit]).build()?`. Each item via `MenuItemBuilder::with_id(id, label).build(handle)?`. NOT `Menu::with_items` — that API under-specified.
- **Open folder:** use `tauri_plugin_opener::OpenerExt::open_path(dirs::data_local_dir().unwrap().join("hotdoc"), None::<&str>)`. NOT `opener::open` (separate crate, not in `Cargo.toml`).
- **T7 follows T8** in the wave table (not parallel) — Preferences menu item needs `launcher.openSettings()` method added in T8.
- **Test count budget:** 1 (tray menu id string for "Quit" to prevent accidental rename).
- **Verify step:** `ls src-tauri/icons/32x32.png` before `pnpm tauri build` — the bundle needs the icon file present.

### Definition of Done (from plan §7)

M3 ships when **all**:
- `pnpm verify:all` exits 0
- `cargo clippy --workspace --all-targets -- -D warnings` exits 0
- 25 existing golden queries still pass (`cargo run -p hotdoc-core -- bench`)
- New tests pass (current 30 + ~12 more = ~42 by end of T8)
- Manual verify via `linux-desktop-app-verify` skill: hotkey opens, ↓↓ navigates, Enter copies highlighted, Ctrl+P pins, `> recents clear` clears, settings panel saves, autostart toggle creates/removes file.

## 5. Frontend state shape (current)

```ts
class Launcher {
  query: string = $state("");
  results: SearchHit[] = $state([]);
  toast: string | null = $state(null);
  recentList: Recent[] = $state([]);
  pinnedList: SearchHit[] = $state([]);
  pinnedIds: Set<string> = $state(new Set());
  selectedIndex: number = $state(-1);
  // derived:
  zeroResult: boolean  // query non-empty && results empty
  emptyQuery: boolean  // query.trim() === ""

  // methods:
  runSearch() / activate(shift, ctrl) / onKey(e) / onInput(value) / doHide() / reset()
  loadEmptyView() / selectRecent(query) / selectPinned(hit) / togglePin()
}
```

T6 will add `validPackIds: Set<string>` and `parseCommand()` dispatch. T8 will add `openSettings()` method (a placeholder until SettingsPanel lands).

## 6. File-by-file map (current)

```
crates/hotdoc-core/
  Cargo.toml                            [rusqlite + thiserror + tempfile(dev)]
  src/
    lib.rs                              [pub mod store]
    store/
      mod.rs                            [open, migrate, default_db_path, StoreError, StoreError::MissingDataDir]
      meta.rs                           [SCHEMA_VERSION = 1]
      recents.rs                        [record, top_n, clear, is_enabled + 6 tests]
      pinned.rs                         [add, remove, list, is_pinned + 4 tests]

src-tauri/
  Cargo.toml                            [+rusqlite (transitive)]
  src/
    lib.rs                              [+ store open at startup, Emitter import, 7 IPC commands registered, Focused(true) emits hotdoc://refresh-empty-view]
    index_state.rs                      [AppState { index: Arc<HotdocIndex>, db: Arc<Mutex<Connection>> }]
    commands.rs                         [search, copy_syntax, hide_window, log_error, record_recent, get_recents, clear_recents, pin_entry, unpin_entry, get_pinned]

src/
  App.svelte                            [+ EmptyView branch when emptyQuery, value={launcher.query} on input, listen() in onMount]
  App.test.ts                           [21 tests]
  lib/
    tauri.ts                            [+ recordRecent/getRecents/clearRecents, pinEntry/unpinEntry/getPinned, Recent type re-exported from types.ts]
    types.ts                            [+ Recent type]
    ResultItem.svelte                   [+ pinned? prop, 📌 icon]
    EmptyView.svelte                    [recent buttons + pinned cards] ← BUG: needs default [] for pinned (see §3a)
    useLauncher.svelte.ts               [+ recentList, pinnedList, pinnedIds, selectedIndex, loadEmptyView, selectRecent, selectPinned, togglePin, Ctrl+P handler]
    launcher/
      recents.ts                        [onActivation helper]
      pinned.ts                         [toggle, fetchPinned helpers]
```

## 7. Risky / surprising bits

- **Default mocks in `beforeEach` matter.** If you add a new IPC command without mocking it, tests fail with `Cannot read properties of undefined` cascading from `App.svelte` into `EmptyView.svelte`. Add the cmd name to `beforeEach` first.
- **EmptyView prop defaults are load-bearing.** Without `= []` defaults on `recents`/`pinned` in the destructure, Svelte 5's `$props()` proxy can briefly read undefined.
- **`packFilter` field on Launcher** does not exist yet. T6 adds it. The plan's T6 Edit 6 references `this.packFilter = cmd.pack_id` — needs to be added with a `$state<string | null>(null)` field at that point.
- **`OpenFlags` import path:** `rusqlite::OpenFlags` (verified during T1; works with `SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_CREATE`).
- **Mutating `Arc<Mutex<Connection>>`** every IPC command takes the lock briefly. Long-held locks would block other commands; current usage is fire-and-forget so fine.

## 8. Quick commands

```bash
# Verify gate (full CI)
pnpm run verify:all

# Frontend only
pnpm run test
pnpm run typecheck
pnpm run lint
pnpm run format

# Rust only
cargo test -p hotdoc-core
cargo test -p hotdoc-core store::pinned    # filtered
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## 9. Once M3 ships

- Update `docs/internal/done/` with M3 handoff (mirror this one, list final commit list)
- Update `docs/internal/spec-v0.2.md` only if any spec gap was closed (not expected — M3 features were spec-locked)
- Run `pnpm tauri build --debug` to confirm Linux AppImage bundles
- Manual verify on a Linux desktop via `linux-desktop-app-verify` skill before tagging the M3 release

Good luck. Start by fixing the `EmptyView.svelte` prop-defaults bug (§3a) so T5 can commit, then proceed T6 → T8 → T7 per the wave table.