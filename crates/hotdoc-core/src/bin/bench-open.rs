use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use hotdoc_core::index::HotdocIndex;
use hotdoc_core::pack;

const RUNS: usize = 10;
const MAX_P50_MS: u128 = 50;

fn main() -> ExitCode {
    let packs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("packs")
        .join("curate");
    let packs = match pack::load_dir(&packs_dir) {
        Ok(p) if !p.is_empty() => p,
        Ok(_) => {
            eprintln!("no packs found in {}", packs_dir.display());
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("loading packs: {e:#}");
            return ExitCode::from(1);
        }
    };
    let total: usize = packs.iter().map(|p| p.entries.len()).sum();
    eprintln!("packing {} entries from {} packs", total, packs.len());

    let mut durations_ms: Vec<u128> = Vec::with_capacity(RUNS);
    for i in 0..RUNS {
        let tmp = std::env::temp_dir().join(format!(
            "hotdoc-bench-open-{}-{}-{}",
            std::process::id(),
            i,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let start = Instant::now();
        let res = HotdocIndex::build(&packs, &tmp);
        let elapsed = start.elapsed().as_millis();
        let _ = std::fs::remove_dir_all(&tmp);
        if let Err(e) = res {
            eprintln!("run {i}: build failed: {e:#}");
            return ExitCode::from(1);
        }
        durations_ms.push(elapsed);
    }
    durations_ms.sort_unstable();
    let p50 = durations_ms
        .get(durations_ms.len() / 2)
        .copied()
        .unwrap_or(0);
    let p95 = durations_ms
        .get((durations_ms.len() * 19) / 20)
        .copied()
        .unwrap_or(0);
    println!("OPEN_BENCH: p50={}ms p95={}ms runs={}", p50, p95, RUNS);
    if p50 > MAX_P50_MS {
        eprintln!("OPEN_BENCH: p50={}ms exceeds ceiling {}ms", p50, MAX_P50_MS);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
