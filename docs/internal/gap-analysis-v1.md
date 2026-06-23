# Hotdoc v1 — Gap Analysis on `spec-v0.2.md`

## M4 disposition (2026-06-23) — all P1/P2 gaps resolved or deferred to v1.1

| Gap | Disposition | Closed by |
|-----|-------------|-----------|
| P1-1 Ctrl+C close/copy split | **Closed** — FR-L3 implemented (empty→close, text→copy input) | M3 |
| P1-2 Tab details-pane geometry | **Closed** — FR-C6 Tab details pane, 720×420→620 | M4-T13 |
| P1-3 Auto-close 300ms rule | **Closed** — FR-C1/C2 close-on-keystroke implemented | M4-T14 |
| P1-4 Recents write timing | **Closed** — written on Enter/Shift+Enter/Ctrl+Enter activation | M3 |
| P1-5 search_log clicked_result_id | **Closed** — `clicked_result_id` = activated row at keypress | M3.5 |
| P1-6 8 vs 18 packs disagreement | **Accepted (content gap)** — 5 packs shipped; 13 remaining at owner cadence (FR-D1 v1.1) | Deliberate |
| P1-7 Hotkey validation on rebind | **Closed** — deny-list validated in SettingsPanel | M3 |
| P1-8 Command palette spec | **Closed** — `> settings`, `> about`, `> help`, `> <pack>`, `> recents [clear]` | M3+M4-T12 |
| P1-9 `personal` source enum v1 absence | **Closed** — SEC-5 build-time lint rejects `source: "personal"` | M3.5 |
| P1-10 search_count ambiguity | **Closed** — §7.2 popularity uses `search_log` aggregated by recency decay | M4-T1 |
| P1-11 NFR-7 CI test harness | **Closed** — `scripts/nfr-network.sh` (tcpdump non-loopback) CI-gated | M4-T8 |
| P1-12 Single-instance gate | **Closed** — `tauri-plugin-single-instance` in DEPENDENCIES + FR-N in spec | M0 |
| P2-1 FR-I4 indexing timeout/UX | **Accepted deviation** — index loads synchronously; steady-count footer; streaming N/N → v1.1 | M4-T14 |
| P2-2 FR-I5 rebuild UX | **Closed** — migration failure triggers rebuild; recents/pinned/settings preserved | M4-T2 |
| P2-3 NFR-4 idle definition | **Closed** — CI soak test (30s default, SOAK_SECONDS override); RSS < 250MB | M4-T5 |
| P2-4 NFR-5 WebView exclusion | **N/A (Linux)** — AppImage/deb bundles, no WebView2 installer; WebView2 budget clarification is Windows v1.1 | Platform override |
| P2-5 NFR-6 budget | **Closed** — CI gate: ≤3MB/pack; 5 packs × 3MB = 15MB ceiling enforced | M4-T7 |
| P2-6 Malformed pack at runtime | **Closed** — FR-I8: loader skips bad packs, logs warn, surfaces diagnostic | M3.5 |
| P2-7 source_url rot | **Accepted (v1)** — fire-and-forget URL hand-off; freshness check → v1.1 | Deliberate |
| P2-8 Settings UI surface | **Closed** — SettingsPanel: hotkey, theme, launch-at-login, recents toggle, diagnostics, About | M3+M4-T12 |
| P2-9 Tray balloon Windows | **N/A (Linux)** — Linux-only v1.0 | Platform override |
| P2-10 Pin eviction at 12 | **Closed** — evict oldest by `pinned_at` in single transaction | M3 |
| P2-11 search_log retention | **Closed** — rows older than 30d pruned on launch | M3 |
| P2-12 Chaos test write granularity | **Closed** — NFR-8 WAL chaos test; every mutation in a single SQLite transaction | M4-T3 |
| P2-13 `> recents` palette | **Closed** — `> recents`, `> recents clear`, `> settings`, `> help`, `> about` | M3+M4-T12 |
| P2-14 `> docker compose` parsing | **Closed** — first token = pack-id (exact match); remainder = fuzzy query on that pack | M3 |

**P3 items:** all deferred to v1.1 (visual tokens, popular time window, cold-index budget, uninstall behaviour, golden versioning, glossary, WAL-on-network-share, in-flight debounce race, R10 details-pane Ctrl+C, non-ASCII, font enumeration, meta/settings split, ranking magnitude, R7 env-var redirect).

**No P1 or P2 gap remains undispositioned as of M4 close-out.**

## M4.5 disposition (2026-06-23) — pre-release audit reconciliation

| Gap | Disposition | Closed by |
|-----|-------------|-----------|
| Content regression 84→79 (audit 2026-06-23, HIGH) | **Closed — false positive.** `git log --follow packs/curate/git.json` and `git log --follow packs/curate/gh.json` show neither file was modified between authoring and the audit. `git` has been 15 entries since `f2d34c9` (M1, 2026-06-20); `gh` has been 12 entries since `1abbea3` (M2 T3 partial, 2026-06-21). The 2026-06-22 audit miscounted `docker/kubectl/git/gh` by one each; the 2026-06-23 audit trusted those numbers. Verified by `jq '.entries \| length'` on each pack and `git log --follow` history. Total real entries on disk: 79. No restoration needed. | M4.5-T3 |
| `gh-repo-view` `[<repo>]` mixed-bracket notation | **Closed** — changed to `<repo>` to match the `<placeholder>` convention used in every other entry | M4.5-T3 |

---

> **Source reviewed:** `docs/internal/spec-v0.2.md` (full read, 724 lines).
> **Companion docs referenced (for cross-check of FR/ID alignment):** `docs/internal/PRD.md`, `docs/internal/DEPENDENCIES.md`.
> **Scope of this report:** gaps *inside the spec* that risk v1 being implemented "by invented convention" rather than by specified intent. The spec is in good shape overall — the strengths are listed at the end so they aren't lost in the gap noise.
>
> **2026-06-20 platform override:** v1.0 ships on **Linux only**; Windows and macOS → v1.1. Several P1 items below reference Windows-specific surfaces (Win11 tray overflow, `%APPDATA%`, Windows IME, etc.). The product-side implications are resolved by `PRD.md §1, NFR-11` (now Linux-anchored). The *implementation* P1s remain — e.g. "what is the Linux equivalent of G1's first-run dialog?" and "what replaces G2's Windows-tray-overflow failure mode?" are still open and need an answer before M0/M1 code lands. Cross-check each P1 against the new Linux target during your reconciliation pass.

The spec uses milestone labels **Phase 0–5** in §14; the parent prompt asks about **M0–M5**. I treat those as the same thing (Phase 0/1/2/3/4/5 ↔ M0–M5).

Severity legend:
- **P1 — Critical gaps:** fix before Phase 2 (search+UI) is committed, or behavior will be invented at code time and is hard to back out of.
- **P2 — Should fix before v1:** needed for correctness, security, or "no surprises" by the time M5 (Distribution) signs off.
- **P3 — Nice to have:** polish, post-launch ergonomics; safe to ship as-is and iterate.

---

## Critical gaps (P1)

### P1-1. `Ctrl+C` close-copy split has no testable contract beyond the spec line
- **Where:** spec §4.1 FR-L3 (line 118); PRD US/L3 (line 83).
- **Gap:** the spec says "Ctrl+C closes only when search input is empty; with text present, Ctrl+C copies any selected input text." It does not define:
  - What happens if the user has typed text but **no selection** (does Ctrl+C still copy the whole input, or no-op?).
  - What "selected" means when the search input is *not* focused (e.g. a card row is highlighted — does Ctrl+C copy the card's syntax? It currently has no FR saying so).
  - Whether the close-on-empty behavior triggers a toast, an animation pause, or just an instant drop.
- **Risk:** "design-by-impl." The 1s toast on `Enter` is well-defined; the `Ctrl+C` branch is not. Two devs will implement it differently. The risk is silently inconsistent behavior that we never go back to fix.
- **Fix:** add to FR-L3 (or a new sub-bullet): "If text is present and no selection, Ctrl+C copies the entire input; with a selection, copies the selection; in both cases, the launcher remains open and shows the standard 1s `Copied:` toast. With empty input, Ctrl+C closes without toast."

### P1-2. "Top 8 results" is the only number fixed for results-list sizing — but `Tab` and details-pane geometry aren't tied to it
- **Where:** spec §4.2 FR-S3 (line 132) and §8.2 (line 437).
- **Gap:** spec says the results list is "top 8." §8.2 says the window is 720×420 and grows to 720×620 when details opens. But:
  - The spec never specifies card height, so we don't know if 8 cards fit in 420 minus input minus footer.
  - The spec never says whether the list scrolls, paginates, or truncates at 8.
  - When the details pane opens via `Tab` (FR-C6), the spec doesn't say whether the *results list* collapses, scrolls behind the pane, or stays visible.
- **Risk:** visual designer guesses at 8; implementation overflows the 420px box; either we get a scrollbar (breaking the "one frame" search render feel) or we shrink rows (hurting readability). Phase 2 will discover this.
- **Fix:** add to FR-S3: "Top 8 results render in a non-scrolling list. If 8+ match, render 8; row height = 56px on default 8px grid; if a 9th match exists, the 8th row is not visually distinguished as 'last' — overflow is silent." Add to FR-C6: "Details pane replaces the rightmost 4 columns of the result list, leaving the first 4 cards still navigable; the window grows to 620px tall."

### P1-3. The "auto-close 300ms or on next keystroke" rule (Flow 2 step 5) is not in any FR
- **Where:** spec §8.1 Flow 2 step 5 (line 403). Same behavior implied for FR-C1 and FR-C2 in the "auto-close" line (line 144–145).
- **Gap:** FR-C1 / FR-C2 say "auto-close" without a timing rule. Flow 2 introduces a 300ms-or-first-keystroke rule, but it's only in the UX walkthrough, not the normative FR. The spec's own §0 says "If you can't write a CI test for it, the requirement is too soft." This is a soft requirement, written in a non-normative section, and the normative section disagrees-by-omission.
- **Risk:** implementation picks 0ms (instant), 100ms (laggy-feeling on a fast machine), or 500ms (blocks next paste). UX is a 5-second flow (G1) — a 500ms auto-close adds 10% to the budget for no reason.
- **Fix:** move the rule into FR-C1/C2: "After toast appears, launcher auto-closes after **300ms** or on the next user keystroke, whichever comes first."

### P1-4. Recents are written *when*? Spec is silent
- **Where:** FR-R1 (line 167) says "last 20 unique queries are stored" and §7.5 (line 366) says "Recents (top 5, MRU) — per FR-R1." FR-S7 mentions `> recents clear` but not `> recents`.
- **Gap:** the spec never says *when* a query is written to `recents`:
  - On every keystroke (after debounce)? That captures typo queries the user never intended.
  - On `Enter` (success path)? Misses the "I looked, didn't copy" path.
  - On any of `Enter` / `Shift+Enter` / `Ctrl+Enter` / `Tab` (first interaction with a result)? Plausible but unstated.
  - On launcher close with non-empty input? Also plausible.
- **Risk:** the recents/popularity signal is the input to the "Top 8 globally most-popular" list in §7.5 (line 368). If we write on every keystroke, the signal is dominated by typo-junk. If we only write on `Enter`, the popularity list is biased toward users who actually paste. Both are defensible — but the choice is not made.
- **Risk to NFR-3:** the "recency-weighted (queries in the last 7d count more)" popularity boost in §7.2 (line 330) depends on this. Wrong write-timing = wrong ranking.
- **Fix:** add a sentence to FR-R1: "A query is written to `recents` the first time a result is activated via `Enter`/`Shift+Enter`/`Ctrl+Enter` (not on keystroke, not on launcher close)." If that's wrong, change it — but lock it.

### P1-5. Search log: `clicked_result_id` is collected but never defined what "click" means
- **Where:** §9.2 schema line 543. The column exists; the spec never says when it gets set.
- **Gap:** the spec collects `query`, `first_result_id`, `clicked_result_id`, `ts`. There's no event named "click" in the keyboard-only launcher. The closest mapping is `Enter` / `Shift+Enter` / `Ctrl+Enter` / `Tab` / `↑↓` navigation. If `clicked_result_id` means "result the user finally activated," it's identical to a first result 80%+ of the time — useless data. If it means "the result the user *landed* on after navigating" (e.g. the highlighted row at the moment of `Enter`), it could be very different and much more useful.
- **Risk:** no measurable behavior; the column is dead weight in the schema. Affects NFR-3 tuning (we have no signal for "what did the user actually pick vs. what we served first").
- **Fix:** add to §9.2 (or a new FR): "`search_log` rows are inserted on `Enter`/`Shift+Enter`/`Ctrl+Enter`; `first_result_id` = the highlighted top result at the moment of the keystroke; `clicked_result_id` = the row highlighted *by the user* at the moment of the keystroke (i.e. the row that will activate). For a query with no `↑↓` navigation, the two are equal."

### P1-6. The "8 packs vs 18 packs" disagreement between spec and PRD is unresolved
- **Where:** spec §4.4 FR-D1 (line 155) says **8 packs**; PRD §6.4 FR-D1 (line 112) says **18 packs**, and the PRD §3.1 G4 says "8 packs."
- **Gap:** the same product has two different pack counts in the two source-of-truth docs. The PRD §0 even says "Where they disagree, this PRD wins." So the official v1 count is likely 18 — but the spec, the *engineering* source-of-truth, still says 8. The spec's §13 D3 (line 653) labels this as an open decision: "My proposal: 8."
- **Risk:** all the spec numbers downstream of this are wrong:
  - FR-I4's "Indexing 8/8…" footer (line 187).
  - NFR-6's "<50MB for 8 packs" budget (line 226; PRD reframes to ~3MB/pack × 18 = ~55MB, line 179).
  - §6.4 content-pipeline "top 10–20 entries per pack" (line 280) — 18 packs × 15 = 270 entries; 8 × 15 = 120. Curation is **2.25×** the load, exactly as the PRD notes.
- **Fix:** resolve D3 *before* Phase 1 starts. If 18, update FR-D1, FR-I4, NFR-6, and §6.4 in the spec. If 8, update the PRD. Either way, the spec is the engineering reference and the PRD is the product reference — the spec is currently out of sync.

### P1-7. Hotkey validation on rebind is in the PRD but not the spec
- **Where:** spec §4.8 FR-X1 (line 194) just says "configurable"; PRD §6.8 FR-X1 (line 144) adds "validated as not-already-registered."
- **Gap:** on Windows, `Ctrl+Shift+Space` is mostly free, but `Ctrl+Space` is taken by IME (R5, line 618). On macOS, `Cmd+Space` is Spotlight; on Linux, `Super` is the system key. The spec never says what happens if a user tries to bind to a known-conflicting combo. Worst case: the OS never delivers the keystroke, the launcher silently doesn't open, and the user blames us.
- **Risk:** R5 (line 618) is "high likelihood, high impact" but the mitigation is "default to Ctrl+Shift+Space; document the choice; make it configurable." A user who rebinds to `Ctrl+Space` will hit the IME conflict with no warning. FR-N/A: nothing tests this.
- **Fix:** add to FR-X1: "On rebind, the new hotkey is validated against a deny-list of known OS-reserved combos (`Ctrl+Space` on Windows IME, `Cmd+Space`/`Cmd+Tab` on macOS, `Super+*` on most Linux WMs). A conflicting binding is rejected with an inline error in the settings UI. The `tauri-plugin-global-shortcut` registration result is treated as authoritative — if the OS refuses the binding, it is rejected and the previous binding is retained."

### P1-8. No spec for the "command mode" command palette
- **Where:** FR-S7 (line 136) says `>` prefix enters command mode and gives two examples: `> docker` (filter by pack) and `> recents` (command palette). The spec also mentions `> recents clear` (FR-R3) and `> settings` (FR-X4) as palette commands. But:
  - The full list of `>` commands is never enumerated.
  - Behavior of unknown commands (`> foo`) is not defined.
  - Help/listing for the palette (e.g. `>?` or `> help`) is not defined.
  - The spec never says whether the palette commands themselves are fuzzy-matched, exact-matched, or use a separate index.
- **Risk:** the command palette is the only way to reach `> recents clear`, `> settings`, and presumably `> open data folder` (which is in the tray menu per FR-T1 but not in `>` mode). Inventory drift between the tray menu and the palette is a near-certainty.
- **Fix:** add §4.2.1 "Command palette" or extend FR-S7: "(a) `> <pack-id>` filters by pack; (b) `> recents`, `> recents clear`, `> settings`, `> help` enter a command palette whose commands are exact-matched; (c) `> help` lists all palette commands; (d) unknown `> <text>` is treated as a pack-id filter (and yields the zero-result state if no pack matches)."

### P1-9. Source-chip mapping is defined for 3 of 4 enum values; `personal` is mentioned but its v1 absence isn't asserted
- **Where:** §7.3 (line 343) maps `official`→Official, `cheat-sheet`→Cheat Sheet, `curated`→Curated, and says `personal`→Personal "in v1.1." §8.3 (line 451) defines a Personal chip color (orange) for v1.1.
- **Gap:** because the builder enforces the closed enum including `personal` (FR-D6), a contributor could write `source: "personal"` in a pack manifest and the builder would accept it. There is no FR that says "no entry in a v1-shipped pack may have `source: \"personal\"`." Without that, the v1.1 design might be retroactively broken.
- **Risk:** low likelihood (we control all 8/18 packs), but it's a content-authoring guardrail that the spec does not name. Affects the integrity of the FR-D6 enum and SEC-5 (build-time content lint).
- **Fix:** add to §6.4 (Content pipeline) or to SEC-5's content-lint rule: "The pack builder rejects any v1 (v1.0) pack manifest containing an entry with `source: \"personal\"` — that enum value is reserved for v1.1 user-authored snippets."

### P1-10. `search_log` and `recents` are SQLite but `search_count` (used in §7.2 popularity) lives in only one of them
- **Where:** §7.2 (line 330) defines `Popularity: log(1 + search_count) * 0.1` with "recency-weighted (queries in the last 7d count more)." §9.2 schema has `recents.use_count` and `search_log` with a `ts` index.
- **Gap:** is `search_count` a count from `recents.use_count` (per-query, MRU-style) or from `search_log` (per-row event)? The two give different numbers. `recents.use_count` is updated only when the query string already exists in `recents` (it's MRU on a unique query); `search_log` writes a row per query. The "recency-weighted" rule says "queries in the last 7d count more" — but `recents` is not time-bounded, it just stores MRU.
- **Risk:** NFR-3 (≥95% first-result accuracy) is partly driven by this. If we conflate the two sources we get noisy popularity, and a regression goes undetected because no one knows which query stream feeds the score.
- **Fix:** add to §7.2: "`search_count` = `COUNT(*) FROM search_log WHERE query = ? AND ts > now - 7d`; the 7-day window is per-query." Remove the ambiguity from the prose.

### P1-11. The NFR-7 "10-minute CI test" has no fixed harness
- **Where:** NFR-7 (line 227). The test must assert "zero outbound connections across 10 minutes of normal use."
- **Gap:** the test design is not specified. Options:
  - A sandboxed Windows VM with Wireshark/mitmproxy capturing all sockets for 10 minutes while a script drives search/copy/pin/settings.
  - A Linux/macOS CI runner with `unshare -n` or an in-process socket-counting proxy.
  - A Rust unit test that statically enumerates the dependency graph and asserts no crate pulls in a network client.
  None of these are committed to. The PRD §11.1 (line 597) calls this a CI gate, so it has to run on every PR.
- **Risk:** Phase 0 (per §14) "Includes the NFR-7 network-monitor test harness" — if the harness isn't specified, the engineer picks the lightest one, and we discover in Phase 4 that a transitive dep (e.g. `reqwest` via `update-notifier`) opens a socket on idle.
- **Fix:** add an appendix or note to NFR-7: "Harness: a Linux CI job (and a Windows VM in Phase 4) running the bundled binary under `mitmproxy`/Windows `netsh trace` for 10 minutes while a scripted input (e.g. `tauri-driver` or a custom Rust test driver) exercises 200 searches, 50 copies, 20 pins, 5 settings opens. Fail on any TCP/TLS handshake to a non-loopback address. The OS-browser handoff in FR-C3 is *not* exercised in this test (it would require mocking the OS browser)."

### P1-12. Spec implies a single-instance gate (R9-resolved / NFR-12) but the spec body does not state it
- **Where:** R9 (line 623) is the *naming* rename — not the single-instance. The spec body never says "only one Hotdoc runs at a time." The PRD §6.10 (line 157) and NFR-12 (line 185) do.
- **Gap:** the parent's task context lists "single-instance gate" as locked (tauri-plugin-single-instance in DEPENDENCIES.md, line 36 of context). The spec text does not contain an FR for this. The NFR-12 measurement is "concurrent-launch test," also not in the spec.
- **Risk:** M0/M1 implementer reads the spec, doesn't see a single-instance rule, and the SQLite WAL file (FR-I3 location) gets two writers racing. Risk is data loss in the chaos test (NFR-8).
- **Fix:** add a one-line FR to §4 (probably its own section) and an NFR-12 row: "FR-N1: only one Hotdoc process holds the index + SQLite. A second launch forwards to the existing instance's launcher and exits. NFR-12: concurrent-launch test asserts that a second invocation within 100ms of the first does not open a second DB handle." This is also the right place to assert the same-instance gate is the OS-level mechanism (named pipe / lock file / Tauri plugin).

---

## Should fix before v1 (P2)

### P2-1. "Indexing 8/8…" footer has no time-out or error path
- **Where:** FR-I4 (line 187) says the footer shows during cold indexing. Phase 4 has no rule for "what if cold indexing never finishes" (e.g. disk full, perms denied, Tantivy write error).
- **Gap:** is the footer also the *only* signal? Does the tray icon's spinner (Flow 1, line 392) also stay? What is the recovery path — kill the app, relaunch (re-trigger FR-I5 corruption path? No, that's for *corrupt* index, not failed write)? Does the user see a blocking error or a silent "index incomplete" badge?
- **Risk:** R1 (line 614) acknowledges 150ms is tight. If indexing is allowed to block the first launch silently, the "first impression" is a dead launcher.
- **Fix:** add to FR-I4: "If cold indexing does not complete within **30s**, the launcher remains open with the current (partial) index and a non-blocking footer `Index incomplete — see logs`; the user can still search, and `Tray → Reload index` retries."

### P2-2. "Rebuild from scratch" on corruption has no UX
- **Where:** FR-I5 (line 188) says "If the index is corrupt, it is rebuilt from scratch and a diagnostic is logged locally." PRD adds a "partial-failure tolerance" via FR-I8 (line 140).
- **Gap:** "rebuilt from scratch" — the user loses recents, pinned, settings? Or only the Tantivy index is rebuilt and SQLite is left alone? The spec says Tantivy holds the entry index; SQLite holds the metadata. So an index rebuild is fast (~2s for 8 packs). But:
  - Are recents/pinned/settings backed up before the rebuild? (Trivially yes, but the spec should say so.)
  - Is the user told "your index was corrupt, I rebuilt it" in plain language, or is it only in the local log (FR-G1)?
  - What if SQLite is the corrupt thing (not Tantivy)? The spec implies both stores are checked. There's no test for "SQLite corrupt."
- **Risk:** silent data loss across what the user perceives as a routine app restart. SEC-8 promises user-only perms but not corruption-survivability beyond what FR-I5 / NFR-8 say.
- **Fix:** add to FR-I5: "Corruption detection runs at launch: Tantivy index → schema sanity check + `IndexReader` open; SQLite → `PRAGMA integrity_check`. On failure, the failing store is rebuilt; **recents, pinned, and settings are preserved** (they are read first, then re-attached after rebuild); a one-time in-launcher banner informs the user `Index was rebuilt; your settings and recents are intact. See logs for details.`"

### P2-3. NFR-4 ("<250MB idle") is unmeasurable in the spec as written
- **Where:** NFR-4 (line 224). "Idle memory footprint < 250MB (webview + Rust core). Measured in CI on idle for 5 minutes."
- **Gap:** "idle" is not defined. Idle after what? After launch with 0 queries? After 1 query? After 8 packs indexed and a couple of searches? The WebView2 process can hold onto a lot more after a search. NFR-7's "10 minutes of normal use" (line 227) is a separate measurement; idle-after-use is different from idle-after-launch.
- **Risk:** CI passes at idle-after-launch (small number) but real users see 600MB+ after 5 minutes of use, and we have no signal.
- **Fix:** add to NFR-4: "Idle is defined as: 60s after a scripted workload of 100 searches + 20 copies + 5 settings opens; no foreground window; tray resident. Measured RSS of the `hotdoc.exe` (Tauri host) + the WebView2 child process."

### P2-4. NFR-5 ("<30MB install") excludes "OS WebView" — but how is that verified?
- **Where:** NFR-5 (line 225).
- **Gap:** on Windows, WebView2 is shipped as `Microsoft.WebView2.*.cab` / `MicrosoftEdgeWebview2Setup.exe`. The bundled installer can either depend on the system-installed WebView2 (typical Tauri 2) or bundle the bootstrapper. The PRD DEPENDENCIES lists `tauri 2` without saying which. The "<30MB" budget is meaningful only if we know which mode we're in.
- **Risk:** installer balloons to 90MB+ if we bundle the bootstrapper silently, and we miss NFR-5.
- **Fix:** add a line to NFR-5: "v1 uses the system-installed WebView2 (no bootstrapper bundle); the installer asserts a system-Windows 10 21H2+ baseline and prints a one-time WebView2 install prompt if absent."

### P2-5. NFR-6 budget isn't enforceable as written
- **Where:** NFR-6 (line 226): "<50MB for 8 packs." PRD §7 (line 179) reframes to "~3MB/pack, ≤~55MB at 18 packs." Spec still says 8/50MB.
- **Gap:** same root cause as P1-6 (pack-count disagreement). Even setting that aside: Tantivy index size is sensitive to which fields are stored vs. fast vs. indexed. The spec names `id` as "STORED + indexed string field (and as a fast field for retrieval)" (§9.2 line 554) but is silent on the others (`title`, `syntax`, `description`, `tags`, `examples`, `source`, `source_url`). If `syntax` is stored and not just indexed, the index balloons.
- **Fix:** in addition to fixing P1-6, add to §6.3 or NFR-6: "`syntax`, `description`, `examples[]` are stored; `tags` and `source` are indexed-only; `title` is both. The 50MB target assumes ~1KB of stored text per entry."

### P2-6. Spec doesn't say what a malformed pack manifest does
- **Where:** spec §6.4 (line 285) says the builder "validates the JSON, runs schema checks, … and emits the bundled `.json` files." That's *build*-time. At *run*-time, FR-D4 (line 158) says content is loaded from a bundled `packs/` directory; PRD FR-I8 (line 140) adds "one malformed pack does not block others; the failed pack is skipped, logged, and surfaced in a non-blocking diagnostic."
- **Gap:** PRD's FR-I8 is a 1.0 commitment; the spec doesn't include it. The build-time check is fine for shipped packs, but a *user* who later hand-edits (e.g. debug session) could trip the runtime loader. There's no test for this in the spec.
- **Risk:** a single bad pack blocks all 8 from indexing; the user sees a dead launcher and an opaque error.
- **Fix:** port PRD FR-I8 into the spec as FR-I8 in §4.7, with the same acceptance criterion. Cite PRD line 140 in the change log.

### P2-7. `source_url` content-changed-on-origin behavior is unspecified
- **Where:** FR-C3 (line 146) opens `source_url` in the system browser. The spec treats this as a fire-and-forget URL hand-off.
- **Gap:** the spec implies a hard-coded `source_url` per card (line 157, "optional `source_url`"). But the upstream doc page can move or 404. The user copies the syntax, pastes, runs, then `Ctrl+Enter`s the source — and the source has rotted. No spec behavior, no test, no fallback.
- **Risk:** the "DevOps/SRE persona" (line 80) explicitly values "current/accurate content." A silently-stale link is worse than no link.
- **Fix:** add to FR-C3: "If `source_url` is not present or is unreachable at *build time* (when the pack was bundled), the card is built without it; at *runtime*, the URL is passed to the OS browser as-is. The build-time `> recents`-style freshness check is out of v1; v1 ships with whatever URL is in the manifest." Document the decision so it isn't silent.

### P2-8. Settings UI is mentioned but never scoped
- **Where:** §4.8 (lines 192–197) lists four settings (hotkey, theme, launch-at-login, accessibility of the settings UI itself). FR-X4 (line 197) says it's reachable from the tray and from `> settings`.
- **Gap:** the *contents* of the settings window are not enumerated:
  - Recents toggle (FR-R4 implies it exists) — what's its label? Where does it live?
  - "Reset to defaults" — not in any FR.
  - "Open data folder" — it's a tray-menu item (FR-T1) but also belongs in settings? Conflict.
  - Diagnostics ("Copy diagnostics" per FR-G2) — where?
  - About / version.
  - The spec does not say whether the settings window is *another* Tauri window or a *panel inside* the launcher. Different process model implications (Tauri windows cost ~80MB each; we already have NFR-4 at 250MB).
- **Risk:** settings drift across the tray menu and the `> settings` palette. The data folder location is in two places; they may disagree.
- **Fix:** add §4.8.1 "Settings window surface" enumerating: recents on/off, hotkey, theme, launch-at-login, "Open data folder," "Copy diagnostics," "Reset to defaults" (with confirmation), version + license info. Specify: the settings window is a *second* Tauri window (not a launcher panel), 480×640, also frameless, with its own always-on-top on focus.

### P2-9. Tray-icon left-click vs right-click on Windows: behavior on first launch
- **Where:** FR-T2 (line 204): "Left-click on tray icon = same as hotkey (open launcher)." FR-T3 (line 205): "first launch, a one-time balloon/tooltip appears explaining the hotkey."
- **Gap:** Windows tray notifications (balloons) were deprecated in Win11 in favor of toast notifications via the Action Center. `tauri-plugin-notification` (not in DEPENDENCIES.md per the parent's lock) would be the modern way. The spec hand-waves "balloon/tooltip" without specifying the API. If we ship on Win10 21H2+, balloons still work; on Win11 they may not show.
- **Risk:** NFR-11 (line 231) is Win10 21H2+ and Win11. The "first launch hotkey hint" is part of the onboarding (Flow 1, line 393). If it doesn't show on Win11, the user doesn't know the hotkey.
- **Fix:** add to FR-T3: "Uses the `tauri-plugin-notification` toast API (with fallback to a Win32 balloon on Win10 21H2). The notification text is `Press Ctrl+Shift+Space to open Hotdoc` and is shown exactly once per user account; a `dont_show_again` flag in `settings` suppresses it on subsequent installs (for the case where the user uninstalls and reinstalls within the suppression window — defaults to never show again once dismissed)."

### P2-10. Pinning 12 items: how is the eviction chosen?
- **Where:** FR-P2 (line 177): "Up to 12 pinned items; older pinning attempts replace the oldest."
- **Gap:** "replace the oldest" — by what clock? `pinned_at` (insertion order) or some other order? The schema (§9.2, line 533) has `pinned_at` and `position`. The spec says "Pin order is the order they were pinned (newest first)" in FR-P3 (line 178). So `position` = rank with newest=0, oldest=11. When a 13th is pinned, the oldest is evicted and the others' positions shift.
- **Risk:** position-shift on every pin is O(n) per insert. Trivial at n=12, but the spec doesn't say whether the `position` column is authoritative or computed. If it's authoritative, the eviction has to renumber 11 rows.
- **Fix:** add to FR-P2: "On pinning a 13th item, the existing row with the largest `pinned_at` is deleted and the remaining 12 rows' `position` column is updated in a single transaction."

### P2-11. `search_log` retention: when (if ever) is it pruned?
- **Where:** §9.2 (line 539) defines the table with `ts` and an index on `ts`. No retention policy.
- **Risk:** the "recency-weighted (queries in the last 7d)" popularity boost (§7.2, line 330) means rows older than 7d are noise. But if we never prune, the table grows unboundedly (a heavy user could write 10k+ rows/year). NFR-6 (≤50MB) and NFR-4 (≤250MB idle) are sensitive to this.
- **Fix:** add to §9.2 (or a new FR-I6): "On app launch, rows in `search_log` with `ts < now - 30d` are deleted (the 7-day popularity window is unaffected; the extra retention is for the FR-G2 diagnostics bundle, which can include last-30d summary stats)."

### P2-12. Crash-safety chaos test: which writes are protected?
- **Where:** NFR-8 (line 228): "chaos test that kills the process mid-write and verifies the DB is consistent."
- **Gap:** the spec lists *tables* (recents, pinned, settings, packs, entries, search_log) but never says which *operations* are part of a write transaction. Is a `pin` a transaction? Is a `set_hotkey` a transaction? Is a `record_recent` a transaction? Without knowing the unit of write, the chaos test is hand-wavy.
- **Risk:** a chaos test that happens to kill the process *between* two `INSERT`s in a `pin` operation could pass with half a pin written. WAL saves us at the SQLite level — but only if the writes were grouped.
- **Fix:** add a sentence to NFR-8: "Every user-visible mutation (pin, unpin, set setting, record recent, log search) is performed in a single SQLite transaction; WAL guarantees that a process kill between transactions loses at most one transaction's worth of work, and the DB is `PRAGMA integrity_check`-clean."

### P2-13. The "command mode" `> recents` palette command is not in the spec
- **Where:** FR-R3 (line 169) says `> recents clear` exists. The spec also mentions `> settings` (FR-X4) and `> recents` itself (FR-S7).
- **Gap:** the spec lists three `>` commands in passing across three different FRs without a consolidated reference. The parent's gap #8 above flagged the command palette; this is the *specific* sub-finding that no list of palette commands exists in the spec body.
- **Fix:** add a §4.2.2 "Command palette reference" subsection with: `> recents` (show recents list), `> recents clear`, `> settings`, `> help` (lists all), `> <pack-id>` (filter by pack), and a note that `> quit` is *not* in v1 (Quit is tray-only).

### P2-14. `> <pack-id>` vs `> docker compose logs`: the spec's own example contradicts itself
- **Where:** FR-S7 (line 136) says `> docker compose logs` "narrows further" — i.e. two-token `> docker compose logs` is treated as a sub-filter on the docker pack, *not* as `> docker` + fuzzy on `compose logs`. Flow 5 (line 421) confirms.
- **Gap:** how does the parser know the *first* token is a pack-id and the *rest* is a query? `compose` is not a pack-id; `logs` is not a pack-id; the user types `> compose logs` (forgetting the `docker` prefix) and gets either: (a) the `compose` pack (which doesn't exist), yielding a zero-result; (b) treated as `>` with no pack and `compose logs` as a fuzzy query. The spec doesn't say.
- **Risk:** a 2-second mental model becomes a 10-second "what is this app doing" once the user mistypes the pack.
- **Fix:** add to FR-S7: "In `>` mode, if the first token matches an installed pack-id exactly, subsequent tokens are a fuzzy query scoped to that pack. If the first token does *not* match any pack-id, the entire query is treated as a fuzzy search (the `>` is ignored). `> comp logs` therefore searches 'comp logs' globally, not 'logs in the comp pack'."

---

## Nice to have (P3)

### P3-1. Visual-design tokens (§8.3) are not bound to a CSS-variable contract
- **Where:** §8.3 (lines 441–451).
- **Gap:** the colors are listed as raw hex (`#0B0B0E`, etc.) but not declared as design tokens / CSS variables. A theme switch in `light`/`dark`/`system` (FR-X2) needs a *single* source of truth; raw hex everywhere is exactly what produces "the dark mode uses the wrong shade of blue" bugs.
- **Fix:** add a note to §8.3: "Colors are defined as CSS custom properties in `tokens.css` (e.g. `--bg`, `--fg`, `--border`, `--chip-official`, `--chip-cheat-sheet`, `--chip-curated`); light and dark themes set the same names; the Svelte components reference the variables, not the raw hex." This is also a v1.1 i18n / a11y / forced-colors prerequisite.

### P3-2. The "Top 8 globally most-popular" list (§7.5) has no time window
- **Where:** §7.5 (line 368). "Top 8 globally most-popular (from the local `search_log` table, not synced)."
- **Gap:** "globally" is undefined. Is it all-time? Last 30 days? The popularity score in §7.2 (line 330) is "queries in the last 7d." The empty-state list should use the same window.
- **Fix:** add to §7.5: "Top 8 globally most-popular = the 8 entries with the highest 7-day `search_log` count, ties broken by recency."

### P3-3. Flow 1's "1–2s" cold indexing is a guess, not a budget
- **Where:** Flow 1 step 3 (line 392). FR-I4 says "typically <2s for v1 content."
- **Gap:** the spec never asserts a *hard* budget for cold indexing time. If 18 packs push it to 8s, is that a regression? NFR-1 (150ms open) is about *interactive* open, not cold indexing. The tray spinner (Flow 1 step 3) hides the latency from the user — but is the *first* hotkey press blocked until indexing is done?
- **Fix:** add to FR-I4: "The first hotkey press is *not* blocked by indexing; the launcher opens immediately, the search input is enabled, and a partial result set is served (with a non-blocking footer `Indexing N/18…`). A search fired while indexing is in progress returns the *currently indexed* subset; the result set is replaced transparently as indexing completes."

### P3-4. Spec doesn't address what happens if the user uninstalls and reinstalls
- **Where:** nowhere in the spec.
- **Gap:** the data folder is at `%APPDATA%\Hotdoc\index\` (FR-I3). On uninstall, does the installer remove it? Windows installers typically prompt "remove settings?" but the spec is silent. The user's recents and pinned may be wiped; the user may not want that.
- **Fix:** add to §10 (Out of scope) or a new §4.11 "Uninstall": "The Windows installer preserves `%APPDATA%\Hotdoc\` on uninstall by default (so recents/pinned/settings survive a reinstall). A `Remove all data` checkbox in the uninstall wizard (or a `--purge` MSI property) deletes it."

### P3-5. The `golden_queries.json` shape is sketched but never versioned
- **Where:** §7.4 (lines 346–361). The shape (`query`, `expected_first`, `acceptable_top3`) is shown; no schema version, no migration story for adding fields.
- **Risk:** low (it's a test fixture), but the spec says the set "grows to 200 in v1.1" (line 361). When fields are added, what happens to old entries? If `acceptable_top3` becomes a list of N items and the assertion hardcodes "3," the test will silently break.
- **Fix:** add a top-level `schema_version: 1` to the fixture and assert it in the CI test.

### P3-6. The spec has no glossary entries for "always-on-top" and "frameless"
- **Where:** §15 (lines 695–706).
- **Gap:** minor; the glossary covers pack/card/snippet/launcher/empty/zero/command/golden/source/hotkey. The two terms most likely to be mis-implemented (`always-on-top` has a Tauri-specific gotcha on Windows when the foreground app is fullscreen) are unmentioned.
- **Fix:** add: "**Always-on-top** — window is created with `WS_EX_TOPMOST` on Windows (via Tauri 2's `WindowBuilder.always_on_top(true)`); the spec does *not* promise behavior when the foreground app is in exclusive fullscreen (a known Windows limitation). **Frameless** — `decorations(false)`; the OS title bar is suppressed; the user moves the window with `Alt+drag` (not specified in v1; deferred — see §10)."

### P3-7. R7 (SQLite WAL on a network share) is mitigated by "validate the path is local" but that path isn't specified
- **Where:** R7 (line 620).
- **Gap:** "Settings validate the path is local" — settings has *no* path field. The data folder is determined by `%APPDATA%`, not a user setting. The user can't move it (no "settings import/export" per §10, line 582). So the validation is hypothetical. If the user has `%APPDATA%` redirected to a network share (common in domain-joined enterprise Windows), SQLite WAL on a UNC path will eventually corrupt. R7's likelihood is "Low" but the *enterprise* use case is exactly when it's likely.
- **Fix:** amend R7 to: "Document: data folder is always at `%APPDATA%\Hotdoc\` (system-resolved, not user-configurable in v1). Domain users with `%APPDATA%` on a network share should expect corruption; the README links to a KB article. v1.1 supports a `HOTDOC_DATA_DIR` env var for redirect to a local path."

### P3-8. Spec is silent on what happens when the user types during the 30ms debounce
- **Where:** FR-S1 (line 130). "Typing in the input schedules a search after a 30ms debounce (resets on each keystroke)."
- **Gap:** well-specified, but: does the *previous* in-flight search (if any) get cancelled, or do two searches race? Tantivy search is fast, so a race is plausible but unlikely to matter. Worth saying.
- **Fix:** add: "Each new keystroke cancels any in-flight IPC `search()` call from the previous keystroke; the previous result is discarded (the UI never displays it)."

### P3-9. R10 is documented as "Low likelihood, Low impact" but it lives in the spec without a fix
- **Where:** R10 (line 622).
- **Gap:** the FR-L3 fix is good. But the spec doesn't address: what if the user has a selection in the *card list* (e.g. they dragged-select a card title in the details pane) and presses Ctrl+C? Should it copy the card's `syntax` (mimicking Enter)? Or the literal text selection (mimicking the input field)? The spec is silent.
- **Fix:** add to FR-L3 or FR-C: "When focus is in the details pane and a non-input text selection exists, Ctrl+C copies the selection (not the card's `syntax`); the launcher remains open."

### P3-10. The spec is silent on multi-byte / non-ASCII queries
- **Where:** §7.1 (line 301) — query processing. `lowercase` on a Unicode string; tokenizer is `[whitespace, "-", "_", "/", "."]`.
- **Gap:** NFR-10 (line 230) says "English only in v1." But the *user* can type non-ASCII at any time (e.g. a CJK IME for `kubectl` documentation, or accented characters in command names). What does the spec promise for `toLowerCase()` of a string with non-ASCII? Rust's `to_lowercase()` is Unicode-correct, but the spec doesn't say so explicitly. The fuzzy edit-distance (line 310) is "edit distance 1, 2" — Tantivy's `Damerau-Levenshtein` operates on bytes, not codepoints. A query like `kubectl` (Latin) vs `кubectl` (Cyrillic к) is *byte-edit-distance-1* but visually a different word.
- **Risk:** Low (English-only content), but a 1-character-from-pickup suggestion like "Did you mean: `кubectl`?" is ugly.
- **Fix:** add to §7.1: "Edit distance is computed over Unicode codepoints (not bytes); queries are lowercased using Unicode case folding (`char::to_lowercase` in Rust)."

### P3-11. The spec promises "all in-pack fonts" but doesn't enumerate them
- **Where:** parent's context: "all in-pack fonts." Spec §8.3 (line 445) names Inter and JetBrains Mono.
- **Gap:** the §6.2 "Tech stack (locked)" doesn't list `@fontsource/inter` and `@fontsource/jetbrains-mono` (or whatever the bundling choice is). Without sub-setting, Inter is ~300KB and JetBrains Mono is ~500KB; both are below the NFR-5 budget but unmentioned.
- **Fix:** add to §6.2: "Fonts: `@fontsource/inter` (variable, 1 weight subset to Latin) + `@fontsource/jetbrains-mono` (variable, 1 weight subset to Latin); bundled as webview assets, no font fetch at runtime."

### P3-12. The `meta` vs `settings` table split (§9.2 lines 495–499) is reasoned but never asserted
- **Where:** spec §9.2 (line 495). The comment says meta is "app/index bookkeeping" and settings is "user-facing preferences," kept separate so a "settings reset never touches index bookkeeping."
- **Gap:** "settings reset" is not in any FR. There's no "Reset to defaults" button in §4.8 (per P2-8, this needs to be added). Without that, the *reason* for the split is invisible. The two-table split is fine; the spec just doesn't surface the user-visible behavior that justifies it.
- **Fix:** tie this to P2-8: "Reset to defaults" deletes all rows in `settings` and re-inserts defaults; `meta` is left untouched; recents and pinned are not part of the reset (user data).

### P3-13. "Source priority" ranking weights (§7.2 line 332) are stated as `>`-chains, not numbers
- **Where:** §7.2 (line 332–333).
- **Gap:** `official > cheat-sheet > curated > personal`. The spec doesn't say what the *magnitude* of the boost is. If the boost is "10×" between `official` and `personal`, the popularity signal is drowned out for a curated card with 50 searches. If the boost is "1.05×," the source ordering is a tiebreaker, not a real boost.
- **Risk:** the golden set is built against the *current* scorer. Without numbers, future tuning becomes ad-hoc.
- **Fix:** add: "Source priority applies a multiplicative weight: `official = 1.0`, `cheat-sheet = 0.95`, `curated = 0.9`, `personal = 0.85` (v1.1). Tunable per spec §7.2's 'configurable' note."

---

## Spec inventory: what the spec calls out as TBD / open

These are the items the spec itself flags as unresolved. They are listed here verbatim so the v1 plan can address them all.

From §13 Open decisions (line 627):

- **D2 — License for our code** (line 643): MIT vs Apache-2.0 vs dual. *Author's recommendation: dual MIT/Apache-2.0.* The PRD §0 (line 6) says it's been decided as dual — but the spec body still has D2 as open. **Inconsistency.**
- **D3 — v1 pack list (8 packs)** (line 653): the 8-pack list vs up-to-2 swaps. *Author's proposal: `git`, `docker`, `kubectl`, `terraform`, `bash`, `aws-cli`, `python`, `pnpm`.* The PRD §6.4 (line 112) has already moved to 18 packs. **Stale.**
- **D4 — First-50 golden queries** (line 660): 50 hand-curated queries needed; the author offers to write a v0 draft; user to add 10–20 personal. **Open.**
- **D5 — Brand color and logo** (line 664): default proposal indigo `#6366F1`; no logo. **Open.**
- **D6 — Where the source code lives** (line 668): the path is given; the question is whether it's personal or Byteonica. **Open.**
- **D7 — Tray icon style on Windows** (line 672): monochrome template (recommended) vs colored app icon. **Open.**

Already resolved (for completeness):

- **D1 — Project name** (line 631): *resolved* to Hotdoc on 2026-06-05.
- **D8 — Copy default** (line 676): *resolved* in v0.2 review (`Enter` = syntax, `Shift+Enter` = example).

---

## Test-infrastructure gaps (post-v1, observed during M2 T3-T4)

These are gaps in **how the tests pin invariants**, not in product behavior. They were surfaced when M2 T3 began (4 of 17 skeleton packs promoted to real content) and T4 expanded the golden query set. Listed here so they get cleaned up after the M2 dev work completes, not as a blocker for ongoing content work.

### TI-1. `load_dir_reads_skeletons` asserted pack *count* — wrong invariant

- **Where:** `crates/hotdoc-core/src/pack.rs::tests::load_dir_reads_skeletons` (was line 165 before 2026-06-21).
- **Problem:** the test asserted `packs.len() >= 17` against `packs/curate/_skeletons/`. Every time a pack moves from skeleton to real content (or back), the test fails. Content churn drives a test red, which is the wrong direction for a content-development project where packs are expected to be added, removed, and revised at any time.
- **Fix already applied (M2 T4, commit pending):** rename to `load_dir_reads_real_packs` and assert by-id membership (`git`, `docker`, `kubectl` are present) against `packs/curate/` (the real-packs dir, not `_skeletons/`). The skeleton subdirectory's contents are no longer test-pinned; they're just "whatever is in flight."
- **Remaining cleanup:** once all 17 packs are authored and `_skeletons/` is deleted, this test should additionally assert on the final 17 pack ids by name. Until then, the by-id membership form is the right shape: it lets the dev work proceed without flipping the test every commit.
- **Why this isn't a blocker:** the test verifies the *loader* behavior, not the *content*. The loader is unchanged; only the data it sees shifts. A count-based assertion on data is the kind of test that ages worst.

### TI-2. Golden query assertions should be allowed to *miss* without flipping CI red mid-content-work

- **Where:** `tests/search/golden_queries.json` + `crates/hotdoc-core/src/golden.rs::cmd_bench`.
- **Problem:** during content churn, an indexer-shape change (T5) or a content edit (T3) can break previously-passing golden queries. Today, `cmd_bench` exits non-zero on any miss, so the bench fails CI. That's correct as a final gate, but it's the wrong shape during a content-development cycle: a query that needs the T5 exact-match bonus to disambiguate should fail *informatively* (with a clear "expected X got Y, T5 would resolve this") rather than as a hard red.
- **Deferred until:** T5 ships (the indexer changes are the natural fix). If a query fails on the *current* indexer but would pass on the T5 indexer, the bench should print a clear "T5-required" tag rather than fail CI.
- **Why this isn't a blocker now:** all 25 current golden queries pass at the current indexer shape (verified empirically 2026-06-21). The deferred cleanup is for queries we *add* during M2 T4-T6 that intentionally exercise T5.

### TI-3. Pack-count metadata in `_meta` of `golden_queries.json` is decorative, not enforced

- **Where:** `tests/search/golden_queries.json::_meta.draft_count`.
- **Problem:** `_meta` documents "draft_count: 25" but nothing reads it. If a future contributor adds queries without bumping the count, the doc lies silently. If they bump the count without adding queries, the count is wrong.
- **Deferred until:** any time. One-liner: a `#[test]` that asserts `gf.queries.len() == _meta.draft_count`. Not worth a separate test infrastructure pass.
- **Why this isn't a blocker now:** the gap is honesty-of-documentation, not a functional bug.

Decisions implicit in the spec but not in §13:

- The pack count (D3) is a §13 item, but the pack list is *also* implicit in §6.4 (line 277) "For each of the 8 packs." The 8-pack assumption propagates to FR-I4 and NFR-6 (P1-6).
- The hotkey default (`Ctrl+Shift+Space`) is asserted in FR-L1 and R5, but *no* D-item in §13 says "if a user rebinds to a known-bad combo, do we reject?" That's P1-7.
- The `personal` source-enum value's v1 absence is asserted in §6.4, FR-D6, and §7.3, but *no* D-item says "v1 build rejects `source: personal` in a pack manifest." That's P1-9.
- The settings UI surface is implicit in §4.8, but *no* D-item enumerates the settings. That's P2-8.

---

## What the spec does well

So the strengths are visible alongside the gaps. The spec is in notably good shape for a v0.2 draft:

- **The product's spine is sharp.** §1's framing (5–15s lookup is the problem; in under 5s is the goal) is concrete and falsifiable, and §1.3's "p50 ≤ 5s hotkey→paste" gives the whole project a single number to orient against. Most v0 specs bury this.
- **The search behavior spec (§7) is unusually rigorous for v0.2.** Per-field BM25 boosts, exact-match bonuses, edit-distance budgets by token length, tie-breakers, and a 50-query golden set with `expected_first` *and* `acceptable_top3` — this is the kind of detail that usually gets invented in code review. Tying it to a CI gate (NFR-3) is the right structural move.
- **The change log (§16) is honest.** The v0.2 review explicitly reconciled 8 contradictions, including the `Ctrl+C` ambiguity (now FR-L3 + R10), the `Enter` vs `Shift+Enter` split (D8), the debounce-measurement clarification (FR-S2 vs NFR-2), and the source-enum definition (FR-D6). A v0.2 that lists its own bugs in the change log is a v0.2 that has been read carefully.
- **The out-of-scope list (§10) is real, not aspirational.** AI, browser extension, cloud sync, plugin system, localization, mobile, pin-influenced ranking, macOS/Linux, settings import/export — all explicitly *named* so they can be caught at PR-review time. The line "Auto-update / background pack polling (only a manual, user-clicked pack-update check in v1 — FR-U1)" pairs the scope fence with the normative rule.
- **Threat framing is named in the PRD if not the spec.** "Clipboard → terminal is the trust boundary" (PRD §8, line 191) is a one-sentence threat model that justifies the SEC-* set (escape-then-highlight, URL allowlist, build-time content lint). Most projects arrive at this realization after a security incident.
- **The risk register (§12) is graded honestly.** R1 (150ms is tight) is *high likelihood, high impact*; R3 (tldr coverage varies) is *medium, medium*; R7 (SQLite WAL on a network share) is *low, high*. The mitigations are concrete (prewarming a hidden WebView, hand-curating top 5 per pack, documenting the local-disk requirement) — not "we'll figure it out."
- **The data model is forward-compatible without being speculative.** §9.3's future-compat fields (`entries.writable`, `entries.updated_at`, `packs.checksum`, `search_log.session_id`) are all named, defaulted false, and v1.1-mapped. A reader can see exactly what v1.1 will need to add — and the v1 code path doesn't have to carry any of it.
- **The ranking "scored query vs composed list" distinction in §7.5 is correct and easy to miss.** Pinning affects the empty-state list (composed, unscored) but not the BM25 search (scored). v0.1 had the bug; v0.2 fixed it in the change log (item 5). This is the kind of bug that ships if the spec doesn't catch it.
- **The pack-count / NFR-budget / index-size triple has a clear, named disagreement (P1-6).** The disagreement is between *the PRD and the spec*, not within the spec — which means it can be resolved in one meeting. The change log's promise of a v0.2 review process is, in fact, surfacing real issues.

The P1/P2/P3 list above is roughly 30 findings; the largest cluster is in **P1** (12 items) and concerns *contracts that affect user-visible behavior at code time* (Ctrl+C, auto-close timing, recents write-timing, search-log click semantics, hotkey validation, command palette, the pack-count disagreement, the single-instance gate). These are the items most likely to cause "we shipped and now the implementation choices are baked in" pain. The P2 cluster is mostly about *measurability and edge cases* (NFR-4, NFR-5, NFR-6 budgets; index corruption; settings surface; the multi-token `>` parser). P3 is polish.
