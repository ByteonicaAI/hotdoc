use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use hotdoc_core::index::HotdocIndex;
use hotdoc_core::pack;

pub struct AppState {
    // ponytail: tantivy's IndexReader wraps an Arc and is cheap to clone.
    // Wrapping the whole HotdocIndex in a Mutex serialised every search
    // against itself, which became a real bottleneck once M2 adds the
    // recents-write hot path. HotdocIndex::search takes &self and is
    // already thread-safe via the inner reader.
    pub index: Arc<HotdocIndex>,
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

pub fn load_or_build_index() -> Result<Arc<HotdocIndex>> {
    let candidates = [bundled_packs_dir(), dev_packs_dir()];
    for dir in &candidates {
        if dir.is_dir() {
            match pack::load_dir(dir) {
                Ok(packs) if !packs.is_empty() => {
                    eprintln!("hotdoc: loaded {} packs from {}", packs.len(), dir.display());
                    if let Some(p) = persistent_index_dir() {
                        if p.is_dir() {
                            if let Ok(idx) = HotdocIndex::open(&p) {
                                eprintln!("hotdoc: reusing index at {}", p.display());
                                return Ok(Arc::new(idx));
                            }
                        }
                        std::fs::create_dir_all(&p).ok();
                        match HotdocIndex::build(&packs, &p) {
                            Ok(idx) => return Ok(Arc::new(idx)),
                            Err(e) => eprintln!(
                                "hotdoc: persistent build failed ({e:#}), falling back to tmp"
                            ),
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
                Ok(_) => continue,
                Err(e) => eprintln!("hotdoc: failed to load {}: {e:#}", dir.display()),
            }
        }
    }
    anyhow::bail!("no packs found in bundled-packs or dev packs/curate")
}
