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
    let mut durations_ms: Vec<u128> = Vec::with_capacity(queries.len());
    for q in &queries {
        let start = Instant::now();
        let hits: Vec<SearchHit> = idx.search(&q.query, 8, &Default::default())?;
        let elapsed = start.elapsed().as_millis();
        durations_ms.push(elapsed);
        let first = hits.first().map(|h| h.id.clone());
        let top3: Vec<String> = hits.iter().take(3).map(|h| h.id.clone()).collect();
        let ok = match &q.expected_first {
            None => hits.is_empty(),
            Some(expected) => {
                first.as_deref() == Some(expected.as_str())
                    || (!q.acceptable_top3.is_empty()
                        && q.acceptable_top3
                            .iter()
                            .any(|a| first.as_deref() == Some(a.as_str())))
            }
        };
        if !ok {
            failed += 1;
            eprintln!(
                "FAIL  {:?}: expected_first={:?} got={:?} top3={:?}",
                q.query, q.expected_first, first, top3
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
        std::fs::write(&baseline_only, serde_json::to_string(&gf).unwrap())
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
}
