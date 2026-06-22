use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::Connection;
use tracing::{info, instrument};

use hotdoc_core::index::HotdocIndex;
use hotdoc_core::index_resolver;

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

/// ponytail: T14. The body used to be ~80 LOC of mixed "where to look
/// for packs / how to load them / when to reuse the tantivy index /
/// what to do on failure" logic. T14 splits that into
/// `hotdoc_core::index_resolver::resolve_and_build` and leaves this
/// function as a thin shell. Spec §11 NFR-1 (open p50 <= 150ms) is met
/// by the tantivy-open fast path inside the resolver — a typical
/// second launch is a single SQLite migrate + an `Index::open_in_dir`,
/// well under 50ms (see `bench-open`).
#[instrument]
pub fn load_or_build_index(db: &Connection) -> Result<Arc<HotdocIndex>> {
    let conn = db;
    let bundled = bundled_packs_dir();
    let res = index_resolver::resolve_and_build(conn, Some(&bundled))?;
    info!(packs = res.packs.len(), "index resolved");
    Ok(res.index)
}

/// T13 + tray integration: force-rebuild the tantivy index and the
/// SQLite mirror from the on-disk packs, ignoring any persistent
/// index. Used by the tray "Reload index" menu item.
#[instrument]
pub fn reload_index(db: &Connection) -> Result<Arc<HotdocIndex>> {
    use hotdoc_core::cli::default_index_dir_option;
    if let Some(p) = default_index_dir_option() {
        if p.is_dir() {
            // ponytail: index.rs's build() now propagates non-NotFound
            // errors; here we just blow away the dir and let build()
            // recreate it. If remove fails, build() will surface the
            // error with full context.
            if let Err(e) = std::fs::remove_dir_all(&p) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(anyhow::anyhow!(
                        "removing persistent index dir {}: {e}",
                        p.display()
                    ));
                }
            }
        }
    }
    load_or_build_index(db)
}

pub fn bundled_packs_dir() -> PathBuf {
    PathBuf::from(env!("OUT_DIR")).join("bundled-packs")
}

#[allow(dead_code)] // re-export for tests in lib.rs + future use
pub fn dev_packs_dir() -> PathBuf {
    index_resolver::dev_packs_dir()
}

#[allow(dead_code)] // re-export for tests + future use
pub fn persistent_index_dir() -> Option<PathBuf> {
    index_resolver::persistent_index_dir()
}
