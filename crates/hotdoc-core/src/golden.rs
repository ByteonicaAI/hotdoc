use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::index::{HotdocIndex, SearchHit};

#[derive(serde::Deserialize, serde::Serialize)]
pub struct GoldenQuery {
    pub query: String,
    pub expected_first: Option<String>,
    #[serde(default)]
    pub acceptable_top3: Vec<String>,
    // ponytail: ids that must NOT appear in the top-3. Lets an adversarial
    // query assert a real exclusion ("git-stash must drop out of top-3")
    // instead of only a positive expected_first.
    #[serde(default)]
    pub forbidden_top3: Vec<String>,
    // ponytail: Option<bool> (not bool + #[serde(default)]) preserves the
    // distinction between "not tagged" (None → filtered out) and "explicitly
    // non-adversarial" (Some(false) → filtered out). Both behave identically
    // today, but the type lets future code branch on the intent.
    #[serde(default)]
    pub adversarial: Option<bool>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct GoldenFile {
    pub queries: Vec<GoldenQuery>,
}

// Floors; regressions below these fail CI.
// Measured 2026-07-01 (post alias-curation): p@1=1.000 p@3=1.000 mrr=1.000
// (n=66, full query set).
//
// NFR-3 (strict first-result accuracy ≥0.95) is now MET. `p_at_1` is STRICT
// (only counts `expected_first` at rank 1 — `acceptable_top3` plays no part).
// It rose 0.909→1.000 after curating natural-language aliases onto the six
// cards that previously lost rank 1 to a sibling variant (nginx-test-config,
// psql-connect, psql-list-tables, ssh-tunneling, tmux-new-session,
// curl-follow-redirects).
//
// EVAL_MIN_P1 is deliberately pinned at the NFR-3 CONTRACT (0.95), NOT at
// measured−0.001. A 1.000 measurement on a 66-query set that aliases were
// curated against is not a generalisation claim; the meaningful gate is the
// spec requirement (≥0.95, i.e. ≥63/66), which leaves honest headroom for
// benign corpus churn while still failing CI the moment NFR-3 is violated.
//
// EVAL_MIN_P3 is near-vacuous as a gate: `p_at_3` is lenient (any id in a
// query's `acceptable_top3` counts as a hit), and the golden set's
// permissive alternates over-determine a 1.000 measurement — p@3 basically
// can't drop below its floor without a real regression elsewhere already
// having tripped p@1 or MRR. Strict p@1 and MRR are the load-bearing gates;
// p@3 is a secondary tripwire. MRR floor (0.97) sits below the 1.000
// measurement with headroom — a single rank-1→rank-2 slip costs ~0.008.
const EVAL_MIN_P1: f64 = 0.95;
const EVAL_MIN_P3: f64 = 0.999;
const EVAL_MIN_MRR: f64 = 0.97;

pub struct EvalMetrics {
    /// Strict precision@1: fraction of queries where `expected_first` lands
    /// at rank 1. `acceptable_top3` is NOT consulted here — this is the
    /// literal NFR-3 number.
    pub p_at_1: f64,
    /// Lenient precision@3: counts a hit if `expected_first` OR any
    /// `acceptable_top3` id appears in the top 3. This is the gated metric.
    pub p_at_3: f64,
    /// Strict precision@3: counts a hit only if `expected_first` itself
    /// appears in the top 3 (ignores `acceptable_top3`). Informational only
    /// — no floor is defined for this field.
    pub p_at_3_strict: f64,
    pub mrr: f64,
    pub n: usize,
}

impl EvalMetrics {
    /// Compute precision@1, precision@3, and MRR over the positive-target
    /// population (queries with `expected_first.is_some()`).
    /// `search` is a pure closure: query string → ordered list of hit ids.
    pub fn compute(file: &GoldenFile, search: impl Fn(&str) -> Vec<String>) -> EvalMetrics {
        let population: Vec<&GoldenQuery> = file
            .queries
            .iter()
            .filter(|q| q.expected_first.is_some())
            .collect();
        let n = population.len();
        if n == 0 {
            return EvalMetrics {
                p_at_1: 0.0,
                p_at_3: 0.0,
                p_at_3_strict: 0.0,
                mrr: 0.0,
                n: 0,
            };
        }
        let mut p1_sum = 0.0f64;
        let mut p3_sum = 0.0f64;
        let mut p3_strict_sum = 0.0f64;
        let mut mrr_sum = 0.0f64;
        for q in &population {
            let expected = q.expected_first.as_deref().expect("filtered to Some above");
            let hits = search(&q.query);
            let rank = hits.iter().position(|id| id == expected);
            if rank == Some(0) {
                p1_sum += 1.0;
            }
            let top3_len = hits.len().min(3);
            let top3 = &hits[..top3_len];
            let strict_in_top3 = top3.iter().any(|id| id == expected);
            if strict_in_top3 {
                p3_strict_sum += 1.0;
            }
            let in_top3 = strict_in_top3
                || q.acceptable_top3
                    .iter()
                    .any(|a| top3.iter().any(|id| id == a));
            if in_top3 {
                p3_sum += 1.0;
            }
            if let Some(r) = rank {
                mrr_sum += 1.0 / ((r as f64) + 1.0);
            }
        }
        let n_f = n as f64;
        EvalMetrics {
            p_at_1: p1_sum / n_f,
            p_at_3: p3_sum / n_f,
            p_at_3_strict: p3_strict_sum / n_f,
            mrr: mrr_sum / n_f,
            n,
        }
    }
}

pub fn cmd_eval(
    queries_path: Option<&str>,
    index_path: Option<&str>,
    adversarial_only: bool,
) -> Result<()> {
    let golden_path = queries_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_golden_path);
    let index_dir = index_path
        .map(std::path::PathBuf::from)
        .unwrap_or_else(crate::cli::default_index_dir);
    let raw = fs::read_to_string(&golden_path).context("reading golden file")?;
    let mut gf: GoldenFile = serde_json::from_str(&raw).context("parsing golden JSON")?;
    if adversarial_only {
        gf.queries.retain(|q| q.adversarial.unwrap_or(false));
        if gf.queries.is_empty() {
            println!(
                "EVAL: no adversarial queries in {} (filter matched 0)",
                golden_path.display()
            );
            return Ok(());
        }
    }
    let idx = HotdocIndex::open(&index_dir)?;
    let search = |q: &str| -> Vec<String> {
        idx.search(q, 8)
            .map(|hits| hits.into_iter().map(|h| h.id).collect())
            .unwrap_or_default()
    };
    let m = EvalMetrics::compute(&gf, search);
    // p@1 is always strict (expected_first at rank 1 only — acceptable_top3
    // is never consulted). p@3 is the lenient/gated metric; p@3(strict) is
    // printed alongside for transparency (see doc comment on the floor
    // constants for why p@3's floor is near-vacuous). Labelled explicitly
    // so the real NFR-3 number (~0.909, below the 0.95 target) stays
    // visible instead of hiding behind an ambiguous "p@1".
    println!(
        "EVAL: p@1(strict)={:.3} p@3={:.3} p@3(strict)={:.3} mrr={:.3} (n={})",
        m.p_at_1, m.p_at_3, m.p_at_3_strict, m.mrr, m.n
    );
    if !adversarial_only {
        if m.p_at_1 < EVAL_MIN_P1 {
            anyhow::bail!(
                "p@1(strict) {:.3} dropped below floor {:.3}",
                m.p_at_1,
                EVAL_MIN_P1
            );
        }
        if m.p_at_3 < EVAL_MIN_P3 {
            anyhow::bail!("p@3 {:.3} dropped below floor {:.3}", m.p_at_3, EVAL_MIN_P3);
        }
        if m.mrr < EVAL_MIN_MRR {
            anyhow::bail!("mrr {:.3} dropped below floor {:.3}", m.mrr, EVAL_MIN_MRR);
        }
    }
    Ok(())
}

pub fn cmd_bench(golden_path: &Path, index_dir: &Path, filter_adversarial: bool) -> Result<()> {
    let raw = fs::read_to_string(golden_path).context("reading golden file")?;
    let gf: GoldenFile = serde_json::from_str(&raw).context("parsing golden JSON")?;
    let mut queries = gf.queries;
    if filter_adversarial {
        queries.retain(|q| q.adversarial.unwrap_or(false));
        // ponytail: empty adversarial set exits 0 (not bail) — a fixture in
        // transition (no adversarial queries yet) or a CI gate during the
        // pre-5.3 phase is a valid state, not an error.
        if queries.is_empty() {
            println!(
                "BENCH: no adversarial queries in {} (filter matched 0)",
                golden_path.display()
            );
            return Ok(());
        }
    }
    let idx = HotdocIndex::open(index_dir)?;
    let mut passed = 0usize;
    let mut failed = 0usize;
    // Finding B: `positive` above is lenient (accepts any acceptable_top3 id
    // at rank 1), which can mask the real NFR-3 (strict) number. Track the
    // strict count in parallel — expected_first at rank 1, full stop — and
    // report it alongside the existing lenient pass/fail line. This is
    // informational only; it does not change what makes the bench pass/fail.
    let mut strict_p1_hits = 0usize;
    let mut strict_p1_total = 0usize;
    let mut durations_ms: Vec<u128> = Vec::with_capacity(queries.len());
    for q in &queries {
        let start = Instant::now();
        let hits: Vec<SearchHit> = idx.search(&q.query, 8)?;
        let elapsed = start.elapsed().as_millis();
        durations_ms.push(elapsed);
        let first = hits.first().map(|h| h.id.clone());
        let top3: Vec<String> = hits.iter().take(3).map(|h| h.id.clone()).collect();
        let positive = match &q.expected_first {
            None => hits.is_empty(),
            Some(expected) => {
                first.as_deref() == Some(expected.as_str())
                    || (!q.acceptable_top3.is_empty()
                        && q.acceptable_top3
                            .iter()
                            .any(|a| first.as_deref() == Some(a.as_str())))
            }
        };
        if let Some(expected) = &q.expected_first {
            strict_p1_total += 1;
            if first.as_deref() == Some(expected.as_str()) {
                strict_p1_hits += 1;
            }
        }
        let forbidden_hit = q.forbidden_top3.iter().any(|f| top3.iter().any(|t| t == f));
        let ok = positive && !forbidden_hit;
        if !ok {
            failed += 1;
            eprintln!(
                "FAIL  {:?}: expected_first={:?} got={:?} top3={:?} forbidden_hit={}",
                q.query, q.expected_first, first, top3, forbidden_hit
            );
        } else {
            passed += 1;
        }
    }
    durations_ms.sort_unstable();
    let p50 = durations_ms
        .get(durations_ms.len() / 2)
        .copied()
        .unwrap_or(0);
    println!(
        "BENCH: {}/{} passed, p50={}ms",
        passed,
        passed + failed,
        p50
    );
    let strict_p1_rate = if strict_p1_total == 0 {
        0.0
    } else {
        strict_p1_hits as f64 / strict_p1_total as f64
    };
    println!(
        "BENCH: strict p@1(expected_first only)={}/{} ({:.3}) — informational, not gated here; see `eval` for the floor",
        strict_p1_hits, strict_p1_total, strict_p1_rate
    );
    if failed > 0 {
        anyhow::bail!("{} golden queries failed", failed);
    }
    Ok(())
}

pub fn default_golden_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("search")
        .join("golden_queries.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_index_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hotdoc-golden-test-{}", std::process::id()))
    }

    #[test]
    fn golden_returns_expected_hits() {
        // ponytail: this test asserts the 50 BASELINE queries pass. The 18
        // adversarial queries (added in WS-A.1, expected_first = target,
        // current ranker fails them) are filtered out here — they have
        // their own gate via `bench --adversarial` and will be re-merged
        // with baseline once 5.3 lands.
        let packs_dir = crate::cli::default_packs_dir();
        let report = crate::pack::load_dir(&packs_dir).expect("load real packs");
        let packs = report.loaded;
        let index_dir = fresh_index_dir();
        let _ = std::fs::remove_dir_all(&index_dir);
        crate::index::HotdocIndex::build(&packs, &index_dir).expect("build index");

        let raw = std::fs::read_to_string(default_golden_path()).expect("read golden file");
        let mut gf: GoldenFile = serde_json::from_str(&raw).expect("parse golden JSON");
        let adv_count = gf
            .queries
            .iter()
            .filter(|q| q.adversarial.unwrap_or(false))
            .count();
        gf.queries.retain(|q| !q.adversarial.unwrap_or(false));
        let dir = tempfile::tempdir().expect("tempdir");
        let baseline_only = dir.path().join("baseline.json");
        std::fs::write(
            &baseline_only,
            serde_json::to_string(&gf).expect("serialize GoldenFile"),
        )
        .expect("write baseline-only fixture");
        cmd_bench(&baseline_only, &index_dir, false).expect("baseline golden queries must pass");
        assert!(
            adv_count > 0,
            "fixture should have adversarial queries to justify this filter"
        );
        let _ = std::fs::remove_dir_all(&index_dir);
    }

    // ponytail: M4.5-T12 / TI-3. The golden fixture's _meta.draft_count
    // is meant to mirror the queries array length. If a future edit
    // drops a query but forgets to bump draft_count (or vice-versa),
    // this test fails loudly. The GoldenFile struct used by cmd_bench
    // intentionally ignores _meta, so we parse a side struct here.
    #[derive(serde::Deserialize)]
    struct GoldenMeta {
        #[serde(default)]
        draft_count: Option<usize>,
    }
    #[derive(serde::Deserialize)]
    struct GoldenFileWithMeta {
        #[serde(default)]
        queries: Vec<GoldenQuery>,
        _meta: GoldenMeta,
    }

    #[test]
    fn golden_meta_draft_count_matches_queries_len() {
        let path = default_golden_path();
        let raw = fs::read_to_string(&path).expect("read golden file");
        let gf: GoldenFileWithMeta =
            serde_json::from_str(&raw).expect("parse golden JSON (with _meta)");
        let declared = gf
            ._meta
            .draft_count
            .expect("_meta.draft_count is required for the TI-3 invariant");
        assert_eq!(
            declared,
            gf.queries.len(),
            "_meta.draft_count ({}) must equal queries.len() ({}) in {}",
            declared,
            gf.queries.len(),
            path.display(),
        );
    }

    #[test]
    fn eval_metrics_compute_basic() {
        let file = GoldenFile {
            queries: vec![
                GoldenQuery {
                    query: "a".into(),
                    expected_first: Some("x".into()),
                    acceptable_top3: vec![],
                    forbidden_top3: vec![],
                    adversarial: None,
                },
                GoldenQuery {
                    query: "b".into(),
                    expected_first: Some("y".into()),
                    acceptable_top3: vec![],
                    forbidden_top3: vec![],
                    adversarial: None,
                },
                GoldenQuery {
                    query: "gib".into(),
                    expected_first: None,
                    acceptable_top3: vec![],
                    forbidden_top3: vec![],
                    adversarial: None,
                },
            ],
        };
        // "a" → x at rank 0; "b" → y at rank 2; "gib" excluded.
        let m = EvalMetrics::compute(&file, |q| match q {
            "a" => vec!["x".into(), "z".into()],
            "b" => vec!["p".into(), "q".into(), "y".into()],
            _ => vec!["anything".into()],
        });
        assert_eq!(m.n, 2);
        assert!((m.p_at_1 - 0.5).abs() < 1e-9); // only "a"
        assert!((m.p_at_3 - 1.0).abs() < 1e-9); // both within top3
        assert!((m.mrr - ((1.0 + 1.0 / 3.0) / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn adversarial_filter_keeps_only_adversarial_queries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("g.json");
        std::fs::write(
            &path,
            r#"{"queries":[
              {"query":"a","expected_first":null},
              {"query":"b","expected_first":null,"adversarial":true},
              {"query":"c","expected_first":null,"adversarial":false}
            ]}"#,
        )
        .expect("write fixture");
        let raw = std::fs::read_to_string(&path).expect("read fixture");
        let gf: GoldenFile = serde_json::from_str(&raw).expect("parse fixture");
        let mut qs = gf.queries;
        qs.retain(|q| q.adversarial.unwrap_or(false));
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].query, "b");
    }

    #[test]
    fn forbidden_top3_fails_when_excluded_id_present() {
        // A query whose expected_first matches but a forbidden id sits in
        // top-3 must FAIL — proving forbidden_top3 is enforced.
        let packs_dir = crate::cli::default_packs_dir();
        let packs = crate::pack::load_dir(&packs_dir).expect("load").loaded;
        // ponytail: use a distinct dir suffix so this test does not race the
        // index lock with golden_returns_expected_hits when run in parallel.
        let index_dir =
            std::env::temp_dir().join(format!("hotdoc-golden-forbidden-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&index_dir);
        crate::index::HotdocIndex::build(&packs, &index_dir).expect("build");
        let dir = tempfile::tempdir().expect("tempdir");
        let f = dir.path().join("g.json");
        // 'git stash' returns git-stash first; forbid it → must fail.
        std::fs::write(
            &f,
            r#"{"_meta":{"draft_count":1},"queries":[
              {"query":"git stash","expected_first":"git-stash","forbidden_top3":["git-stash"]}
            ]}"#,
        )
        .expect("write");
        let res = cmd_bench(&f, &index_dir, false);
        let _ = std::fs::remove_dir_all(&index_dir);
        assert!(res.is_err(), "forbidden id in top-3 must fail the bench");
    }

    // FINDING A (post-v1 eval-floor audit): EVAL_MIN_P1 / EVAL_MIN_P3 /
    // EVAL_MIN_MRR were previously enforced ONLY by the `eval` step in
    // .github/workflows/ci.yml. Renaming or dropping that YAML step would
    // silently disable the NFR-3 gate while `cargo test` stayed green. This
    // test builds the REAL index from the bundled/dev packs, runs the exact
    // same `EvalMetrics::compute` path `cmd_eval` uses (no duplicated metric
    // logic), and asserts the SAME floor constants — so the gate now lives
    // in version-controlled code, not YAML.
    //
    // Uses a real in-index built into a scratch temp dir (never the
    // persistent OS index dir from `default_index_dir`), following the
    // pattern already used by `golden_returns_expected_hits` and
    // `forbidden_top3_fails_when_excluded_id_present` above. Uses its own
    // dir-name suffix so it doesn't race those tests' index dirs when
    // `cargo test` runs suites in parallel within the same process.
    #[test]
    fn eval_floors_hold_on_real_corpus() {
        let packs_dir = crate::cli::default_packs_dir();
        let packs = crate::pack::load_dir(&packs_dir)
            .expect("load real packs")
            .loaded;
        let index_dir =
            std::env::temp_dir().join(format!("hotdoc-golden-eval-floor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&index_dir);
        crate::index::HotdocIndex::build(&packs, &index_dir).expect("build index");

        let idx = crate::index::HotdocIndex::open(&index_dir).expect("open index");
        let raw = std::fs::read_to_string(default_golden_path()).expect("read golden file");
        let gf: GoldenFile = serde_json::from_str(&raw).expect("parse golden JSON");
        // Full query set, unfiltered — matches cmd_eval's default (non
        // --adversarial) behavior and the n=66 the floors were measured on.
        let search = |q: &str| -> Vec<String> {
            idx.search(q, 8)
                .map(|hits| hits.into_iter().map(|h| h.id).collect())
                .unwrap_or_default()
        };
        let m = EvalMetrics::compute(&gf, search);
        let _ = std::fs::remove_dir_all(&index_dir);

        assert!(
            m.p_at_1 >= EVAL_MIN_P1,
            "p@1(strict) {:.3} dropped below floor {:.3} — this is the version-controlled \
             NFR-3 gate (previously YAML-only, see FINDING A)",
            m.p_at_1,
            EVAL_MIN_P1
        );
        assert!(
            m.p_at_3 >= EVAL_MIN_P3,
            "p@3 {:.3} dropped below floor {:.3}",
            m.p_at_3,
            EVAL_MIN_P3
        );
        assert!(
            m.mrr >= EVAL_MIN_MRR,
            "mrr {:.3} dropped below floor {:.3}",
            m.mrr,
            EVAL_MIN_MRR
        );
    }
}
