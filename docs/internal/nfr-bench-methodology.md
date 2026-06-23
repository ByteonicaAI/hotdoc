# Hotdoc — NFR Bench Methodology

Two bench binaries, two different NFRs, two different methodologies.

## `bench-open` (NFR-1, index-build slice)

**What it measures:** time to build the Tantivy index from `packs/curate/`.
This is the slice of cold-open that's pure CPU work — pack parsing,
schema construction, document indexing, segment commit.

**What it does NOT measure:** Tauri webview startup, GTK init, webview
prewarm, OS file cache warm-up. Those vary wildly by environment and
are not part of NFR-1's index-budget claim.

**Runs:** 30 fresh builds in `std::env::temp_dir()`. Reports p50 and p95.
Fails CI if p50 > 150ms (the spec NFR-1 ceiling — see spec-v0.2.md).

**Where it runs:** both CI and the user's local box. The bench is
deterministic and machine-independent (fresh tempdir, no FS cache
assumption). The 150ms figure is the spec gate; Tantivy's measured
reality at M2's 79-entry content scale is well under 50ms, but the
gate tracks the spec so a future content-scale regression doesn't
fail loudly. Expect ~3× growth as content moves from 79 → 200+
entries (the bench ceiling is set with that headroom in mind).

## `bench-search` (NFR-2, search p50)

**What it measures:** wall-clock time of `HotdocIndex::search()` for
each query in `crates/hotdoc-core/src/bin/bench-search.rs::QUERIES`
(the same 24 queries the golden set exercises, picked for query-mode
coverage: exact, prefix, fuzzy, multi-token).

**What it does NOT measure:** IPC marshaling, Tauri command dispatch,
Svelte render time, debounce timing. NFR-2's "≤16ms p50 hotkey→paste"
is the end-to-end number; this bench measures the search slice only.

**Runs:** 100 × 24 = 2400 searches. The index is built once at the
start of the run and reused. Reports p50, p95, p99. Fails CI if p50
> 16ms.

**Where it runs:** both CI and the user's local box. The query set is
fixed (no randomization), so the bench is comparable across machines.

**Why this is the right shape:** the index is held in memory after
`build()`. Reusing it across runs measures the search code path in
isolation, not the index-load cost. The first call after `build()` is
slightly slower (lazy field readers); warm-cache timings start at
run 2.

## Why two benches, not one

NFR-1 (open time) is dominated by index build at M2 scale. NFR-2
(search time) is dominated by query parsing + BM25 scoring. They
have different cost centers, different scaling behavior (index build
grows with `entries × fields`, search grows with `entries × tokens ×
clause_count`), and different failure modes (build fails on schema
mismatch, search fails on OOM). Conflating them in one binary hides
which one regressed.

## Local vs CI numbers

CI runs on `ubuntu-latest` GitHub runners. The user's local box may
be faster or slower. The ceilings (50ms for index-build, 16ms for
search) are set against the spec's NFR budget, not against CI's
measured baseline. CI gate is "are the ceilings met," not "is the
delta small."

If a future CI environment changes and the bench fails the ceiling,
the fix is (a) bump the ceiling with a one-line spec change
justifying it, or (b) profile and optimize. Not "ignore the bench."
