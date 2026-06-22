use std::process::ExitCode;
use std::time::Instant;

use hotdoc_core::index::HotdocIndex;
use hotdoc_core::pack;

const RUNS: usize = 100;
const MAX_P50_MS: u128 = 16;
const LIMIT: usize = 8;

const QUERIES: &[&str] = &[
    "git stash",
    "git stash pop",
    "git stsh",
    "git undo last commit",
    "git discard all changes",
    "git create branch",
    "git delete branch",
    "git cherry pick",
    "git log graph",
    "docker ps",
    "docker stop",
    "docker rm",
    "docker build",
    "docker images",
    "kubectl get pods",
    "kubectl logs",
    "kubectl exec",
    "kubectl rollout restart",
    "gh pr create",
    "gh pr merge",
    "gh issue list",
    "curl basic auth",
    "curl follow redirect",
    "curl save output",
];

fn main() -> ExitCode {
    let packs_dir = hotdoc_core::cli::default_packs_dir();
    let packs = match pack::load_dir(&packs_dir) {
        Ok(r) => {
            for (path, errs) in &r.failed {
                for e in errs {
                    eprintln!("warn: failed to load {}: {e}", path.display());
                }
            }
            if r.all_failed() {
                eprintln!(
                    "all packs failed in {} ({} files)",
                    packs_dir.display(),
                    r.failed.len()
                );
                return ExitCode::from(1);
            }
            if r.loaded.is_empty() {
                eprintln!("no packs found in {}", packs_dir.display());
                return ExitCode::from(1);
            }
            r.loaded
        }
        Err(e) => {
            eprintln!("loading packs: {e:#}");
            return ExitCode::from(1);
        }
    };
    let total: usize = packs.iter().map(|p| p.entries.len()).sum();
    eprintln!("indexing {} entries from {} packs", total, packs.len());

    let tmp = std::env::temp_dir().join(format!(
        "hotdoc-bench-search-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    let idx = match HotdocIndex::build(&packs, &tmp) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("index build failed: {e:#}");
            let _ = std::fs::remove_dir_all(&tmp);
            return ExitCode::from(1);
        }
    };

    let mut durations_ms: Vec<u128> = Vec::with_capacity(RUNS * QUERIES.len());
    for run in 0..RUNS {
        for q in QUERIES {
            let start = Instant::now();
            let res = idx.search(q, LIMIT, &Default::default());
            let elapsed = start.elapsed().as_millis();
            if let Err(e) = res {
                eprintln!("run {run} query {q:?}: search failed: {e:#}");
                let _ = std::fs::remove_dir_all(&tmp);
                return ExitCode::from(1);
            }
            durations_ms.push(elapsed);
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);

    durations_ms.sort_unstable();
    let p50 = durations_ms[durations_ms.len() / 2];
    let p95 = durations_ms[(durations_ms.len() * 19) / 20];
    let p99 = durations_ms[(durations_ms.len() * 99) / 100];
    println!(
        "SEARCH_BENCH: p50={}ms p95={}ms p99={}ms runs={} queries={}",
        p50,
        p95,
        p99,
        RUNS,
        QUERIES.len()
    );
    if p50 > MAX_P50_MS {
        eprintln!(
            "SEARCH_BENCH: p50={}ms exceeds ceiling {}ms (NFR-2)",
            p50, MAX_P50_MS
        );
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
