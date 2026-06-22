use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::time::Instant;

use crate::cli::default_index_dir;
use crate::index::{HotdocIndex, SearchHit};

#[derive(serde::Deserialize)]
pub struct GoldenQuery {
    pub query: String,
    pub expected_first: Option<String>,
    #[serde(default)]
    pub acceptable_top3: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct GoldenFile {
    pub queries: Vec<GoldenQuery>,
}

pub fn cmd_bench(golden_path: &Path, index_dir: &Path) -> Result<()> {
    let raw = fs::read_to_string(golden_path).context("reading golden file")?;
    let gf: GoldenFile = serde_json::from_str(&raw).context("parsing golden JSON")?;
    let idx = HotdocIndex::open(index_dir)?;
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut durations_ms: Vec<u128> = Vec::with_capacity(gf.queries.len());
    for q in &gf.queries {
        let start = Instant::now();
        let hits: Vec<SearchHit> = idx.search(&q.query, 8)?;
        let elapsed = start.elapsed().as_millis();
        durations_ms.push(elapsed);
        let first = hits.first().map(|h| h.id.clone());
        let top3: Vec<String> = hits.iter().take(3).map(|h| h.id.clone()).collect();
        let ok = match &q.expected_first {
            None => hits.is_empty(),
            Some(expected) => first.as_deref() == Some(expected.as_str()),
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

pub fn re_export_default_index_dir() -> std::path::PathBuf {
    default_index_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_index_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hotdoc-golden-test-{}", std::process::id()))
    }

    #[test]
    fn golden_returns_expected_hits() {
        let packs_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("packs")
            .join("curate");
        let report = crate::pack::load_dir(&packs_dir).expect("load real packs");
        let packs = report.loaded;
        let index_dir = fresh_index_dir();
        let _ = std::fs::remove_dir_all(&index_dir);
        crate::index::HotdocIndex::build(&packs, &index_dir).expect("build index");
        cmd_bench(&default_golden_path(), &index_dir).expect("all golden queries must pass");
        let _ = std::fs::remove_dir_all(&index_dir);
    }
}
