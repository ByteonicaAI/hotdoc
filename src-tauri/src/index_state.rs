use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use rusqlite::Connection;
use tauri::Manager;
use tracing::{info, instrument, warn};

use hotdoc_core::index::HotdocIndex;
use hotdoc_core::index_resolver;

pub struct AppState {
    // M5-T2: RwLock<Arc<HotdocIndex>> lets rebuild_index hot-swap the
    // pointer without restarting. HotdocIndex::search takes &self and is
    // thread-safe; readers hold a read lock only for the duration of the
    // Arc::clone (cheap), then release before calling search.
    pub index: RwLock<Arc<HotdocIndex>>,
    // rusqlite::Connection is not Sync in 0.31; Mutex serialises commands.
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
///
/// `bundled_packs` is the caller-resolved "shipped packs" directory —
/// in production this is the Tauri resource dir (see
/// `resource_packs_dir`), in dev/test callers may pass `None` or the
/// build-time `bundled_packs_dir()`. `resolve_and_build` tries this
/// path first, then always falls back to
/// `index_resolver::dev_packs_dir()` (source tree `packs/curate/`),
/// so passing `None` here is safe — dev/test behavior is unaffected.
#[instrument(skip(db))]
pub fn load_or_build_index(
    db: &Connection,
    bundled_packs: Option<&Path>,
) -> Result<Arc<HotdocIndex>> {
    let res = index_resolver::resolve_and_build(db, bundled_packs)?;
    info!(packs = res.packs.len(), "index resolved");
    Ok(res.index)
}

/// T13 + tray integration: force-rebuild the tantivy index and the
/// SQLite mirror from the on-disk packs, ignoring any persistent
/// index. Used by the tray "Reload index" menu item.
#[instrument(skip(db))]
pub fn reload_index(db: &Connection, bundled_packs: Option<&Path>) -> Result<Arc<HotdocIndex>> {
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
    load_or_build_index(db, bundled_packs)
}

/// Where the shipped app finds its packs at runtime: the Tauri v2
/// resource dir (populated at package time from `bundle.resources` in
/// `tauri.conf.json`, which maps `../packs/curate/` → `packs/curate/`
/// under the resource root — see `src-tauri/tauri.conf.json`), joined
/// with `packs/curate`.
///
/// Returns `None` (after logging a warning) if the resource dir can't
/// be resolved — e.g. a dev/test context where `.setup()` never ran
/// against a packaged bundle. Callers pass the `None` straight through
/// to `resolve_and_build`, which then falls back to
/// `index_resolver::dev_packs_dir()`; a missing resource dir is a
/// recoverable condition, never a panic.
pub fn resource_packs_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    match app.path().resource_dir() {
        Ok(dir) => Some(dir.join("packs").join("curate")),
        Err(e) => {
            warn!(
                error = %format!("{e:#}"),
                "resource_dir unavailable; falling back to dev packs"
            );
            None
        }
    }
}

/// Build-time staging copy of `packs/curate/` under `$OUT_DIR`,
/// written by `src-tauri/build.rs` (plain `fs::copy`, not an
/// `include_dir!`). This is a dev/test convenience path — it lives on
/// the *build machine* only, so it is never a valid production pack
/// source; production resolves packs via `resource_packs_dir` instead.
/// Kept around for the build/test-time sanity check in `lib.rs`
/// (`bundled_packs_or_dev_packs_present`).
#[allow(dead_code)] // only referenced from lib.rs's #[cfg(test)] module
pub fn bundled_packs_dir() -> PathBuf {
    PathBuf::from(env!("OUT_DIR")).join("bundled-packs")
}

#[allow(dead_code)] // re-export for tests in lib.rs + future use
pub fn dev_packs_dir() -> PathBuf {
    index_resolver::dev_packs_dir()
}
