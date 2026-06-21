# Hotdoc — Agent Guidelines

## Project identity

**Hotdoc** is a local-first, keyboard-driven desktop launcher that answers one question fast: "what's the command for the tool I already know?" Press a global hotkey, type fuzzy words, get the command, copy, paste, back to flow.

Built with Tauri v2 (Rust backend) + Svelte 5 + TypeScript (Vite frontend). Not a web app — a system-level desktop application.

## Stack

| Layer        | Tech                                                                          | Key constraints                                             |
| ------------ | ----------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Frontend     | Svelte 5 (runes), TypeScript strict, Vite 6                                   | `verbatimModuleSyntax`, `noUncheckedIndexedAccess`          |
| Backend      | Rust edition 2021, Tauri v2, tantivy 0.22                                     | `unsafe_code = forbid`, `unwrap_used = deny`                |
| UI testing   | vitest + jsdom + `@testing-library/svelte`                                    | Tauri APIs mocked via `vi.mock`                             |
| Rust testing | cargo test (workspace)                                                        | golden-query harness in `crates/hotdoc-core/src/golden.rs`  |
| Lint         | eslint + typescript-eslint (strict, type-checked)                             | Prettier for formatting (last in eslint config)             |
| Format       | Prettier (semicolons, double quotes, trailing comma, 100 width)               | `pnpm run format` for write, `pnpm run format:check` for CI |
| Commits      | Conventional commits (`@commitlint/config-conventional`)                      | `type(scope): message` format                               |
| Package      | pnpm >=10, Node >=24                                                          | workspace root only                                         |
| CI           | `pnpm run verify:all` (typecheck + lint + format:check + test + cargo checks) | Full Rust workspace as well                                 |

## Workspace layout

```
hotdoc/
├── src/                  # Svelte 5 frontend (TS + .svelte)
│   ├── main.ts           # Entry: mount App
│   ├── App.svelte        # Root component
│   ├── App.test.ts       # Vitest tests
│   └── lib/
│       ├── types.ts      # SearchHit, SOURCE_LABEL
│       ├── tauri.ts      # Typed Tauri IPC wrappers (invoke)
│       ├── useLauncher.svelte.ts   # Svelte 5 runes class (Launcher)
│       └── ResultItem.svelte
├── src-tauri/            # Tauri v2 app (Rust)
│   └── src/
│       ├── main.rs       # Tauri binary entry
│       ├── lib.rs        # Tauri setup, plugins, window mgmt
│       ├── commands.rs   # IPC commands (search, copy_syntax, hide_window)
│       ├── index_state.rs# Index loading (persistent / temp)
│       ├── hotkey.rs     # Global shortcut registration
│       └── toggle.rs     # CLI toggle for window visibility
├── crates/hotdoc-core/   # Core library (no Tauri deps)
│   └── src/
│       ├── lib.rs        # Module exports
│       ├── index.rs      # HotdocIndex: tantivy build/open/search
│       ├── pack.rs       # Pack types, loader, validator
│       ├── cli.rs        # hotdoc-cli subcommands (index, query, copy, bench)
│       ├── golden.rs     # Golden-query benchmark harness
│       └── bin/
│           ├── hotdoc-cli.rs  # CLI binary entry
│           └── bench-open.rs  # Open-time benchmark binary
├── packs/                # Data packs (curated tool cards)
│   └── curate/           # Dev packs (editable, bundled at build time)
├── docs/
│   ├── internal/         # Plans, specs, handoffs, PRD, gap analyses
│   └── seed-data/        # Golden queries, fixture data (excluded from format)
└── graphify-out/         # Knowledge graph (AST, cross-file relationships)
```

## Key npm / pnpm scripts

| Script              | What it does                                                                                  |
| ------------------- | --------------------------------------------------------------------------------------------- |
| `pnpm dev`          | Vite dev server (for Tauri frontend)                                                          |
| `pnpm build`        | Vite production build                                                                         |
| `pnpm test`         | vitest (frontend unit tests)                                                                  |
| `pnpm typecheck`    | `svelte-check --tsconfig ./tsconfig.json`                                                     |
| `pnpm lint`         | eslint                                                                                        |
| `pnpm format`       | Prettier write                                                                                |
| `pnpm format:check` | Prettier check                                                                                |
| `pnpm tauri`        | `tauri` CLI passthrough                                                                       |
| `pnpm test:rust`    | `cargo test`                                                                                  |
| `pnpm verify:all`   | Full CI gate (typecheck + lint + format:check + test + cargo fmt + cargo clippy + cargo test) |
| `pnpm bnr`          | Shorthand for `pnpm build && pnpm dev`                                                        |

## Code conventions

### TypeScript / Svelte

- **Svelte 5 runes**: use `$state`, `$derived`, `$effect` — no legacy `$:` reactive statements. `.svelte.ts` files for runes-based classes.
- **Type imports**: `@typescript-eslint/consistent-type-imports` with `prefer: "type-imports"` and `fixStyle: "inline-type-imports"`. Use `import type { Foo }` or inline `import { type Foo }`.
- **Strict null checks**: `noUncheckedIndexedAccess` is on — array/record access returns `T | undefined`.
- **`noImplicitOverride`**: required for subclass method overrides.
- **`verbatimModuleSyntax`**: don't elide imports based on usage — import what you use, use what you import.
- **No `console.log`**: use `console.warn`, `console.error`, or `console.info` (eslint-enforced).
- **Unused vars**: prefix with `_` (e.g. `_unused`, `_event`).
- **Tauri IPC**: thin typed wrappers in `src/lib/tauri.ts` — invoke Tauri commands by string name.
- **Testing**: Vitest + jsdom + `@testing-library/svelte`. Tauri plugins (clipboard, opener, core invoke) are mocked via `vi.mock` at module level. Prefer `fireEvent` over `userEvent` for keyboard events.

### Rust

- **Workspace lints** (enforced):
  - `unsafe_code = forbid` — zero unsafe allowed
  - `unwrap_used = deny` — use `?`, `.context()`, `.ok()`, or match instead
  - `expect_used = allow` — `expect()` is permitted
  - clippy: correctness deny, warn on suspicious/style/complexity/perf
- **Error handling**: `anyhow::Result` with `.context()` for contextual errors. Tauri commands return `Result<T, String>` (string error for JS surface).
- **Concurrency**: `HotdocIndex` wrapped in `Arc` (not `Mutex`) — tantivy's `IndexReader` is already thread-safe internally.
- **Crate split**: `hotdoc-core` has zero Tauri dependencies — pure library. `src-tauri` is the Tauri shell that depends on `hotdoc-core`.
- **CLI**: `hotdoc-cli` binary in `crates/hotdoc-core/src/bin/hotdoc-cli.rs` with clap subcommands (index, query, copy, bench).
- **Tests**: inline `#[cfg(test)] mod tests` in source files. Test-specific packs path resolves via `CARGO_MANIFEST_DIR`.

### Data / packs

- **Packs**: JSON files in `packs/curate/` loaded via `pack::load_dir()`. Each pack has entries with id, title, syntax, description, source, source_url, tags, examples.
- **Index**: tantivy BM25 search over packs. Schema fields include id, title, syntax, description, tags, example_codes, source, pack_id.
- **Persistent index**: stored under `dirs::data_local_dir() / "hotdoc" / "index"` for fast re-launch.
- **Golden queries**: in `docs/seed-data/tests/search/golden_queries.json` for NFR-3 performance gate.

### Comments

- **`// ponytail:`** — design rationale comments. These explain _why_ something is the way it is, especially when it's the simplest possible approach. Keep them concise and factual.

## Architecture patterns

- **IPC**: Tauri `#[tauri::command]` functions in `commands.rs` are invoked from the frontend via `invoke("cmd_name", { args })`. Commands return `Result<T, String>`. Auto-rejected on Err → caught in `useLauncher.svelte.ts`.
- **Window lifecycle**: single webview window ("main"), hidden on focus loss (`Focused(false)`), shown via global hotkey or CLI toggle. Positioned at top-center of primary monitor.
- **Global hotkey**: registered in `hotkey.rs` via `tauri-plugin-global-shortcut`. Toggles via CLI fallback (Wayland path in `toggle.rs`).
- **Security**: `source_url` open gate — only `https:` URLs allowed (`isHttpsUrl` check in `useLauncher.svelte.ts`).

## Testing expectations

- **Frontend tests**: `src/**/*.{test,spec}.{ts,js,svelte}` via vitest + jsdom. All Tauri plugin calls mocked. Test launcher interactions (input, keyboard, clipboard, hide, open URL, zero-result state).
- **Rust tests**: inline `#[test]` in source files. Integration tests use fresh index built from dev packs. Test search precision, fuzzy matching, prefix matching, empty queries, and edge cases (short tokens, gibberish).
- **Performance**: golden-query harness in `golden.rs` — run via `cargo test` or the `hotdoc-cli bench` subcommand.
- **Verification gate**: `pnpm run verify:all` checks everything.

## Key rules for agents

- **Approval gates**: Rust backend changes, architecture changes, dependency additions, schema changes, CI/CD changes, and broad refactors require user approval before implementation. Propose first, implement after approval.
- **Smallest correct change**: no drive-by refactors or reformatting. Touch only what the task needs.
- **Follow existing conventions**: match the project's code style, module structure, comment style, and testing patterns. `AGENTS.md` conventions take priority over training defaults.
- **Don't rewrite history**: `git` history is meaningful — don't rebase, squash, or force-push unless explicitly asked.
- **Check open files**: some tools/skills produce relevant output; read project context files before starting.
- **Packs data**: seed data (`docs/seed-data/**`) is exempt from formatting — don't run Prettier on it.
- **Security-sensitive**: CSP, URL gates, secret handling — always flag changes here for approval.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

When the user types `/graphify`, invoke the `skill` tool with `skill: "graphify"` before doing anything else.

Rules:

- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- Dirty graphify-out/ files are expected after hooks or incremental updates; dirty graph files are not a reason to skip graphify. Only skip graphify if the task is about stale or incorrect graph output, or the user explicitly says not to use it.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
