use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::{error, info, instrument, warn};

use hotdoc_core::index::HotdocIndex;
use hotdoc_core::pack;

pub struct AppState {
    // ponytail: tantivy's IndexReader wraps an Arc and is cheap to clone.
    // Wrapping the whole HotdocIndex in a Mutex serialised every search
    // against itself, which became a real bottleneck once M2 adds the
    // recents-write hot path. HotdocIndex::search takes &self and is
    // already thread-safe via the inner reader.
    pub index: Arc<HotdocIndex>,
    // rusqlite::Connection is not Sync in 0.31 (RefCell-based internals);
    // the plan's "Send + Sync since 0.27" claim is wrong for 0.31. A Mutex
    // serialises commands, which is fine: each command is a single
    // short-lived transaction and the hot path is one `record()` per
    // activation, not per keystroke.
    pub db: Arc<Mutex<Connection>>,
}

pub fn bundled_packs_dir() -> PathBuf {
    PathBuf::from(env!("OUT_DIR")).join("bundled-packs")
}

pub fn dev_packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("packs").join("curate")
}

// ponytail: spec §11 NFR-1 (open p50 <= 150ms) cannot survive a full
// tantivy rebuild on every launch once packs cross ~50 cards. The plan
// T8 called for dirs::data_local_dir() / "hotdoc" / "index" — the old
// code dropped everything in /tmp with no persistence, which is the
// exact leak P1-2 flagged. This keeps the persistent location; the
// "skip-rebuild-if-version-unchanged" check is a follow-up.
pub fn persistent_index_dir() -> Option<PathBuf> {
    hotdoc_core::cli::default_index_dir_option()
}

#[instrument]
pub fn load_or_build_index() -> Result<Arc<HotdocIndex>> {
    let candidates = [bundled_packs_dir(), dev_packs_dir()];
    for dir in &candidates {
        if dir.is_dir() {
            // ponytail: FR-I8 — log per-failed-pack at warn, error if all
            // fail. A directory that exists but is empty (or all-failed)
            // is not fatal; we move on to the next candidate. The bail at
            // the bottom handles the "no packs at all" case.
            match pack::load_dir(dir) {
                Ok(report) => {
                    for (path, errs) in &report.failed {
                        for e in errs {
                            warn!(
                                file = %path.display(),
                                error = %e,
                                "failed to load pack"
                            );
                        }
                    }
                    if report.all_failed() {
                        error!(
                            path = %dir.display(),
                            failed = report.failed.len(),
                            "all packs in dir failed; trying next candidate"
                        );
                        continue;
                    }
                    if report.loaded.is_empty() {
                        continue;
                    }
                    info!(packs = report.loaded.len(), path = %dir.display(), "loaded packs");
                    let packs = report.loaded;
                    if let Some(p) = persistent_index_dir() {
                        if p.is_dir() {
                            if let Ok(idx) = HotdocIndex::open(&p) {
                                info!(path = %p.display(), "reusing index");
                                return Ok(Arc::new(idx));
                            }
                        }
                        std::fs::create_dir_all(&p).ok();
                        match HotdocIndex::build(&packs, &p) {
                            Ok(idx) => return Ok(Arc::new(idx)),
                            Err(e) => {
                                warn!(error = %format!("{e:#}"), "persistent build failed; falling back to tmp")
                            }
                        }
                    }
                    let tmp = std::env::temp_dir().join(format!(
                        "hotdoc-runtime-{}-{}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    ));
                    return HotdocIndex::build(&packs, &tmp)
                        .map(Arc::new)
                        .context("building runtime index");
                }
                Err(e) => {
                    warn!(path = %dir.display(), error = %format!("{e:#}"), "failed to load pack dir")
                }
            }
        }
    }
    anyhow::bail!("no packs found in bundled-packs or dev packs/curate")
}
