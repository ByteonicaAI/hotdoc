# Hotdoc — M4.5 Pre-Release Hardening Handoff

**Date:** 2026-06-23
**From:** M4.5 execution session (CI-breaking → reconciliation)
**To:** the agent(s) continuing M4.5 / starting M5
**Status:** **IN PROGRESS — T1 through T6 done (7 commits). T7 through T15 still pending (T6.5 added as separate follow-up).** Each remaining task follows the default workflow (plan → approval gate → execute). Rust backend, schema, CI, dependency additions, and broad refactors need owner approval before implementation.

---

## 0. What this session already did (do NOT redo)

4 commits on `main`, all attributed to `Rohit Kumar Bindal <rohit.bindal@byteonica.com>` (existing gitconfig — no Hermes-branded or `@users.noreply.*` overrides, ever):

| Commit | Task | Files | Δ |
|---|---|---|---|
| `979e995` | M4.5-T1 — FR-R4 empty-view recents gate + NFR-10 aria-label extraction | 3 | +13/-4 |
| `00d6099` | M4.5-T2 — NFR-10 string extraction to STRINGS map (all `.ts`/`.svelte` literals + format() helper) | 5 | +61/-27 |
| `f1f95bd` | M4.5-T3 (part 2) — audit-regression false-positive note in gap-analysis | 1 | +393 |
| `eb58176` | M4.5-T3 (part 1) — `gh-repo-view [<repo>]` → `<repo>` placeholder normalization | 1 | +1/-1 |
| `0c19f0d` | M4.5-T4 — SEC-3: AboutPanel anchor → IPC button + DetailsPane `isHttpsUrl` guard + shared `src/lib/url.ts` | 6 | +161/-15 |
| `2b143fa` | M4.5-T5 (part 1) — `prune_old` + `unchecked_transaction` wrap on `record` + 4 tests; `open()` calls `prune_old` with 30d cutoff (graceful `no such table` skip before `migrate()`) | 2 | +254/-1 |
| `84d8af5` | M4.5-T5 (part 2) — gap-analysis P2-11 row reattribution (false closure → M4.5-T5) + "no undispositioned P1/P2" → M4.5 close-out | 1 | +3/-2 |
| `051cf52` | M4.5-T6 — `apply_source_priority` drops unused `_score` param + `recents.rs:221` drops `let _ = ` binding; **populate_store outer-tx NOT closed** (became T6.5) | 2 | +6/-4 |

**Net effect:** CI grep gate clean, NFR-10 extraction complete, SEC-3 closed, search_log retention now real (was false closure), search_log::record has the same atomicity primitive as recents::record, `apply_source_priority` no longer carries a vestigial parameter, the recents atomicity test has a cleaner thread join. **T6.5 (populate_store outer-tx) remains undispositioned** — see §5.9 for paths. Test count: 95 Rust + 71 frontend (unchanged in T6).

---

## 1. Pre-flight findings the next session needs to know

These changed the scope from `plan-m4.5-pre-release-hardening.md`. The next session must read them before executing any plan step.

### 1.1 The "84→79 content regression" is a false positive (T3)

The audit (2026-06-23, HIGH) and the plan both say `git` regressed 16→15 and `gh` regressed 13→12. Verified false:

- `git log --follow packs/curate/git.json` — only commit is `f2d34c9` (M1, 2026-06-20), created with 15 entries. No later modification.
- `git log --follow packs/curate/gh.json` — only commit is `1abbea3` (M2 T3 partial, 2026-06-21), created with 12 entries. No later modification.
- The 2026-06-22 audit miscounted docker/kubectl/git/gh by one each; the 2026-06-23 audit trusted those numbers.
- Total on disk: 79 (correct). `jq '.entries | length'` confirms.

**Implication for T3:** owner pre-approved "drop M4.5-T3 entry-restoration scope." Only the `gh-repo-view` notation fix + a gap-analysis false-positive note were implemented. Skip the "restore 5 entries" branch if the plan is re-read.

### 1.2 Plan paths reference `content/packs/...` — real path is `packs/curate/...`

`jq '[.entries[]] | length' content/packs/git.json` won't work. Real path: `packs/curate/git.json`. Update any CI scripts / acceptance commands accordingly.

### 1.3 `crates/hotdoc-core/src/recents.rs` lives at `crates/hotdoc-core/src/store/recents.rs`

Plan T6 line reference `recents.rs:221` is off-by-tree. Real path: `crates/hotdoc-core/src/store/recents.rs:221`. Real change: `let _ = reader.join().expect("reader join")` → `reader.join().expect("reader join")` (drops the unused binding).

### 1.4 `packs::upsert_all` and `entries::upsert_all` already use `unchecked_transaction()` internally

T6's outer-transaction wrap is still valuable (atomicity *between* packs-then-entries), but it's a wrap, not a rewrite. Current code commits each one separately at `crates/hotdoc-core/src/index.rs:115-122` — wrap both in one outer `conn.transaction(|tx| ...)` there.

### 1.5 `apply_source_priority(_score: f32, source: &str)` at `index.rs:505`

`_score` parameter is genuinely unused — BM25 score is already passed in via the `adjusted_score = score + exact + src + pop` line at `index.rs:186`. Cleanest fix: remove the parameter entirely (it's `pub(crate)` — call sites in `index.rs:184` are in the same crate). No `let _ = score;` shim needed.

### 1.6 `docs/internal/**` is gitignored, but `gap-analysis-v1.md` was force-added back in T3

Commit `68b0086` originally tracked it. At session start it was untracked + gitignored. The T3 commit (`f1f95bd`) used `git add -f docs/internal/gap-analysis-v1.md` to restore tracking. **If subsequent tasks (e.g. T15 reconciliation) need to update gap-analysis**, they should `git add -f docs/internal/gap-analysis-v1.md` and not worry about the `.gitignore` rule. Other docs in `docs/internal/` remain untracked — that's intentional per repo convention.

---

## 2. Repository state at session end

```
$ git status
On branch main
Changes not staged for commit:
  modified:   AGENTS.md       ← pre-existing, NOT mine
  modified:   README.md       ← pre-existing, NOT mine

$ git log --oneline -5
0c19f0d fix(security): aboutpanel + detailspane route urls through rust https gate (M4.5-T4)
eb58176 fix(content): normalize gh-repo-view placeholder to <repo> (M4.5-T3)
f1f95bd fix(content): gh-repo-view notation + audit-regression false-positive note (M4.5-T3)
00d6099 feat(i18n): nfr-10 string extraction to STRINGS map (M4.5-T2)
979e995 feat(launcher): recents-empty-view gate + aria-label extraction (M4.5-T1)
```

**Pre-existing uncommitted changes** (AGENTS.md, README.md — milestone table updates from a prior session): leave them alone unless the next session is explicitly asked to commit them.

**Git author identity:** `Rohit Kumar Bindal <rohit.bindal@byteonica.com>` (set in repo gitconfig). Never override with `--author`, `--config user.*`, or `GIT_AUTHOR_*` env vars. Same rule as the existing memory note (PR #113/#114/#116, byteonica-portal launch) — owner identity is a hard rule.

---

## 3. Files touched so far (full inventory)

```
src/lib/strings.ts                (T1: +3 aria-label keys; T2: +23 toast/about/details/footer keys + format() helper)
src/lib/EmptyView.svelte          (T1: 3 aria-label swaps)
src/lib/useLauncher.svelte.ts     (T1: loadEmptyView recents gate; T2: STRINGS/format import + 14 toast call sites + 1 help toast; T4: drop local isHttpsUrl, import from ./url, add public showToast())
src/lib/AboutPanel.svelte         (T2: 7 label swaps; T4: <a>→<button> + handleHomepage with isHttpsUrl guard + onToast prop)
src/lib/DetailsPane.svelte        (T2: 2 label swaps; T4: handleSourceLink adds isHttpsUrl guard + onToast prop)
src/App.svelte                    (T2: footer status format; T4: onToast props on AboutPanel + DetailsPane)
src/App.test.ts                   (T4: +4 SEC-3 routing tests)
src/lib/url.ts                    (T4: NEW — shared isHttpsUrl)
packs/curate/gh.json              (T3: [<repo>] → <repo>)
docs/internal/gap-analysis-v1.md  (T3 + T5: audit false-positive note; P2-11 reattribution + implementation narrative)
crates/hotdoc-core/src/store/search_log.rs  (T5: prune_old fn + record tx wrap + module retention doc + 2 unit tests)
crates/hotdoc-core/src/store/mod.rs         (T5: SEARCH_LOG_RETENTION_MS const + open() prune call + is_no_such_table helper + 2 integration tests)
crates/hotdoc-core/src/index.rs             (T6: apply_source_priority drops _score param + call site update; populate_store NOT touched — see T6.5)
crates/hotdoc-core/src/store/recents.rs     (T6: 1-line let _ = reader.join cleanup in record_is_atomic_under_concurrent_reader test)
```

---

## 4. What's left in M4.5 (11 tasks, 0 done this session)

Sequencing from the plan, accounting for the T1→T2 ordering and the false-positive T3:

| Group | Task | Status | Dependencies |
|---|---|---|---|
| B | T5 — `search_log::prune_old` + outer-tx wrap + P2-11 reclose | **Done** | none |
| B | T6 — `apply_source_priority` `_score` removal + `recents.rs:221` `let _ = ` cleanup | **Done** (cleanup only — populate_store outer-tx split to T6.5) | none |
| B | T6.5 — `populate_store` outer-transaction for crash atomicity between `packs::upsert_all` and `entries::upsert_all` | **Pending — T6 follow-up** | requires 4-file refactor (refactor `upsert_all` to take `&Transaction` or add tx-abstraction trait) |
| B | T7 — version 0.1.0 → 0.4.0 in `package.json` + `tauri.conf.json` + `src-tauri/Cargo.toml`; `resizable: true` → `false` | **Next** | independent of T6.5; XS |
| B | T8 — FR-T3 first-launch notification (adds `tauri-plugin-notification`, fires once via `first_launch_shown` setting) | after T7 | version should be correct before first-launch fires |
| C | T9 — bench-open: 30 iterations in CI, `MAX_P50_MS = 150` | independent | needs spec-vs-CI p50 discussion (CI is 50ms, spec is 150ms — loosening to spec) |
| C | T10 — nfr-memory.sh: aggregate parent + descendant VmRSS | independent | trivial shell edit |
| C | T11 — NFR-7 soak disposition | independent | Option A (recommended, owner-approved): accept 60s CI soak as deviation, document 600s pre-release manual smoke |
| D | T12 — boot settings mock + FR-C6 tests (8 new) + TI-3 `_meta.draft_count` | independent | mostly test additions |
| D | T13 — `--mark-bg` CSS token + `SearchHit.source` union type | independent | type union change touches multiple `.svelte` files that pattern-match `source` |
| E | T14 — `verify:all` completeness + `cargo-audit` cache + commitlint `scope-enum` + `string:check` | after T9/T10/T11 | verify:all adds the bench scripts |
| F | T15 — gap-analysis + audit reconciliation | last | depends on all above closing |

**Recommended next task:** T7 (version bump to 0.4.0 + `resizable: false`). It's independent of the rest of M4.5, XS scope, and unlocks T8 (which depends on the version being correct). T6.5 (populate_store outer-tx refactor) should land in its own task with a fresh design proposal — see §5.9.

---

## 5. Known traps the next session will hit (and how to dodge)

### 5.1 commitlint `subject-case` blocks uppercase letters in commit subjects

The commitlint default rule rejects subject-case like `feat(launcher): FR-R4 …` (it reads "FR" as sentence-case start). T1 hit this. **Workaround:** rephrase to avoid the uppercase letter sequence — e.g. `feat(launcher): recents-empty-view gate + …`. Or add a `subject-case: false` rule to `commitlint.config.js` (T14 may do this anyway if a `scope-enum` lands).

### 5.2 lint-staged's "staging from tasks" can swallow working-tree changes

Twice this session, lint-staged's pre-commit phase caused a stash-dance that dropped a file from the commit (e.g. T3 doc commit dropped the JSON change, forcing a follow-up commit). **Workaround:** after `git commit`, always check `git show --stat HEAD` and `git status`. If a file is unexpectedly uncommitted, commit it as a follow-up with a follow-up message — don't try to amend a previously-clean commit.

### 5.3 husky pre-commit re-runs svelte-check / eslint / prettier on staged files

The hook runs even on doc-only / data-only commits. For commits that change only `.md` or `.json` files, this is wasted work but harmless. **No workaround needed** — just expect the `*.{ts,js,svelte,mjs,cjs} → 0 files` skip messages.

### 5.4 `prettier --write` rewrites JSON keys in alphabetical order on `packs/curate/*.json`

This is the formatter's default. **Don't run `prettier --write` on pack files** — the canonical pack order (entries appear in authoring order, which is the order the user learns them) is meaningful for the `q . entries` queries in golden queries. If prettier reformats a pack, run `git checkout -- packs/curate/<file>.json` to revert.

### 5.5 The `docs/internal/**` gitignore + the previously-tracked `gap-analysis-v1.md`

See §1.6. If a follow-up task (likely T15) updates gap-analysis, force-add it. Other internal docs stay untracked — that's the repo's working-doc convention.

### 5.6 `pnpm test` console output looks alarming but is fine

Every App.test.ts run prints a dozen `load empty view failed` / `applyTheme rejected unknown value` log lines. These come from the launcher's defensive `log.warn` calls when test mocks return `undefined` for IPC commands. They are NOT test failures — all 71 tests pass. The console noise is cosmetic.

### 5.7 T5 hit: prune-on-`open()` breaks the `open()`-then-`migrate()` call sequence

The plan said "call `prune_old` from `store::open()` after DB init" — sounds clean, but the call-site contract is `open()` first, `migrate()` second (migrate is called by the Tauri `try_open` closure and every test fixture, not by `open()` itself). A naive wire-up runs `DELETE FROM search_log` on a fresh DB where `search_log` doesn't exist yet → 51 tests fail with `no such table: search_log`.

**Fix that landed:** match the `no such table` rusqlite error and rollback the empty tx. Same pattern as `migrations.rs::is_no_such_table`. The first launch on a fresh DB skips the prune; once `migrate()` runs, every subsequent `open()` prunes successfully. Don't try to "fix" this by moving the prune into `migrate()` — that's a schema migration step which only runs on version bumps, so retention wouldn't actually advance per-launch.

### 5.8 T5: the integration test (`open_prunes_search_log_rows_older_than_retention`) had to land in T5-T3, not T5-T2

The test asserts the wire-up works, so it cannot pass before the wire-up exists. T5-T2 only tests the in-module primitives (`prune_old` + tx wrap); T5-T3 tests the integration. Don't try to put both in one task — the failure surface gets confusing.

### 5.9 T6: `populate_store` outer-tx requires a 4-file refactor, not a wire-up

The plan said "wrap `packs::upsert_all` + `entries::upsert_all` in one outer `conn.transaction()`." Sounds simple, but `upsert_all` *already* opens its own `unchecked_transaction()` internally (each is a self-contained all-or-nothing table replace). A naive outer wrap fails with `cannot start a transaction within a transaction` — rusqlite's `unchecked_transaction` does NOT auto-savepoint (the Savepoint API is separate: `Transaction::savepoint()`). T6 caught this on the first test run (2 tests failed with the SQLite error).

**Three paths to the real fix (none shipped in T6):**
1. Refactor `upsert_all` to take `&Transaction` (or a tx-trait) so the outer call passes its transaction in. Touches `packs.rs`, `entries.rs`, their tests, `index_resolver.rs` test fixtures. The right way, but ~4 files and a new abstraction.
2. Inline the upsert SQL into `populate_store` inside `conn.transaction(|tx| { ... })`. Smaller diff, but duplicates the SQL and increases the blast radius of any future upsert change.
3. Leave the gap open. The audit flagged it as P1 (LOW but real) — crash recovery has to be the path that catches the half-populated state, not the write path. Acceptable but undispositioned.

**What's in T6 as shipped:** cleanup only. `_score` param dropped on `apply_source_priority` (vestigial; score is already in the additive) and the `let _ = ` binding on `recents.rs:221` dropped (`.expect()` already consumes the `thread::Result`). 2 trivial edits, 1 commit. The audit half-populated-state gap is **NOT** closed by T6.

**T6.5 follow-up:** the populate_store atomicity fix as a dedicated task. Owner picks one of the three paths above. Should be a separate plan/spec/approval cycle because path 1 is a 4-file refactor with a trait abstraction — wider than the M4.5 "smallest correct change" budget.

---

## 6. Verification matrix (cumulative, post-T4)

| Check | Status | Notes |
|---|---|---|
| `pnpm typecheck` | ✅ 0 errors, 0 warnings | |
| `pnpm lint` | ✅ clean | |
| `pnpm format:check` | ✅ clean | |
| `pnpm test --run` | ✅ 71/71 passing | was 67 before T4 |
| `pnpm audit --audit-level=high` | not run this session | (T14 will add it to verify:all) |
| `cargo test --workspace` | ✅ 95 unit + 1 chaos, all pass | verified after T5 (was 91; T6 unchanged) |
| `cargo fmt --all --check` | ✅ clean | |
| `cargo clippy --all-targets -- -D warnings` | ✅ clean | |
| NFR-10 aria-label grep | ✅ PASS | zero hits |
| NFR-10 .ts literal scan | ✅ PASS | only matches inside strings.ts |
| `cargo run -p hotdoc-core --bin bench-open` | not run this session | T9 work |
| `bash scripts/nfr-memory.sh` | not run this session | T10 work |
| `bash scripts/nfr-open-bench.sh` | not run this session | T9 work |

`pnpm verify:all` has not been re-run end-to-end this session (would require the bench scripts to exist on disk — they do — and the audit to be cached). T14 will add the missing pieces (`pnpm audit`, `cargo audit`, `bash scripts/nfr-open-bench.sh 30`, `bash scripts/nfr-memory.sh`, `pnpm run string:check`).

---

## 7. Communication protocol for the next session

The next session inherits the existing workflow: default workflow = propose plan → wait for approval → write spec → wait for approval → break into tasks → wait for approval → execute. Per AGENTS.md and the orchestrator persona, that means:

1. **Read this handoff first** (it explains the false-positive T3 + the AGENTS.md/README.md pre-existing working-tree changes).
2. **Propose the next task** (recommended: T5) with: goal, files affected, risks, acceptance. **Wait** for approval.
3. **Apply changes, run local checks** (`pnpm typecheck && pnpm lint && pnpm format:check && pnpm test --run && cd src-tauri && cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace`).
4. **Commit** with conventional-commit message + M4.5-Tx label in the subject or body. Author = the existing gitconfig identity; never override.
5. **Update this handoff** at session end with the same template (§0 table, §3 inventory, §6 verification matrix).
6. **Stop** after each task unless explicitly given autonomy for the full milestone.

The "**after it's done, update the doc and stop**" pattern from T1 → T2 → T3 → T4 should continue.