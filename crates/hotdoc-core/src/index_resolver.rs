//! Index resolution (audit §1.8d — T14).
//!
//! Splits `load_or_build_index`'s former god function into a small
//! resolver: pick the best pack directory, decide whether to reuse the
//! tantivy index, run the build, and mirror the result into SQLite.
//! The Tauri layer (`src-tauri/src/index_state.rs`) becomes a thin
//! shell that calls this.
//!
//! ponytail: not a trait, not async. Single impl, YAGNI. The resolver
//! holds references to the sqlite Connection and the persistent index
//! path; both are app-level singletons. Adding a trait is a
//! distraction the audit specifically called out as out-of-scope.

use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use tracing::{error, info, instrument, warn};

use crate::index::HotdocIndex;
use crate::pack::{self, LoadReport};

/// Dev packs for local iteration: `packs/curate/` at the workspace root.
/// Works in any context (library, test, binary) because CARGO_MANIFEST_DIR
/// is always set by Cargo. The bundled-packs dir lives in $OUT_DIR which
/// only the Tauri app sees; the caller injects that path.
pub fn dev_packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("packs")
        .join("curate")
}

/// Tells the resolver where to look for / write the persistent tantivy
/// index. Returns None if the platform data dir is unavailable.
pub fn persistent_index_dir() -> Option<PathBuf> {
    crate::cli::default_index_dir_option()
}

/// Resolve + build (or reuse) the launcher's tantivy index, and mirror
/// the on-disk pack set into SQLite. Single entry point used by the
/// Tauri launcher; bench binaries build their own indices with
/// `HotdocIndex::build` directly so they don't need the SQLite mirror.
///
/// `db` is the rusqlite Connection that owns the launcher's
/// persistent state. The resolver writes to it during populate_store
/// (T16). Pass the same Connection the Tauri commands use.
///
/// `bundled_packs` is the path to the `include_dir!`'d copy (set by
/// the Tauri build script). The dev packs dir is read from
/// CARGO_MANIFEST_DIR and is always available; the bundled dir is
/// optional — the resolver skips a missing path.
#[instrument(skip_all)]
pub fn resolve_and_build(db: &Connection, bundled_packs: Option<&Path>) -> Result<IndexResolution> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(b) = bundled_packs {
        candidates.push(b.to_path_buf());
    }
    candidates.push(dev_packs_dir());
    for dir in &candidates {
        if !dir.is_dir() {
            continue;
        }
        match resolve_one(dir, db) {
            Ok(Some(res)) => return Ok(res),
            Ok(None) => continue, // dir existed but had no usable packs
            Err(e) => {
                warn!(
                    path = %dir.display(),
                    error = %format!("{e:#}"),
                    "pack dir load failed; trying next candidate"
                );
            }
        }
    }
    Err(anyhow!(
        "no packs found in bundled-packs or dev packs/curate"
    ))
}

/// Resolution outcome: the index + the packs that produced it (for
/// diagnostics / future hot-reload).
pub struct IndexResolution {
    pub index: std::sync::Arc<HotdocIndex>,
    pub packs: Vec<crate::pack::Pack>,
    /// Where the tantivy index lives on disk. None for the runtime tmp
    /// fallback (see index_state::load_or_build_index pre-refactor).
    pub index_dir: Option<PathBuf>,
}

// ponytail: the inner return is Result<Option<IndexResolution>> — None
// when the dir exists but has no usable packs, so the caller can try
// the next candidate. The persistent index is tried first (re-use is
// free); only on miss do we re-build.
fn resolve_one(dir: &Path, db: &Connection) -> Result<Option<IndexResolution>> {
    let report = load_with_logging(dir)?;
    for (path, errs) in &report.failed {
        for e in errs {
            warn!(file = %path.display(), error = %e, "failed to load pack");
        }
    }
    if report.all_failed() {
        error!(
            path = %dir.display(),
            failed = report.failed.len(),
            "all packs in dir failed; trying next candidate"
        );
        return Ok(None);
    }
    if report.loaded.is_empty() {
        return Ok(None);
    }
    info!(packs = report.loaded.len(), path = %dir.display(), "loaded packs");
    let packs = report.loaded;

    // 1) Try to reuse the persistent index.
    if let Some(p) = persistent_index_dir() {
        if p.is_dir() {
            if let Ok(idx) = HotdocIndex::open(&p) {
                info!(path = %p.display(), "reusing index");
                // ponytail: even when reusing, the on-disk pack set may
                // have changed since last build (e.g. user edited a pack
                // JSON). Re-populate the SQLite mirror so pinned::list and
                // the palette validator see current content. Full replace
                // is cheap at 5 packs / 79 entries.
                if let Err(e) = HotdocIndex::populate_store(db, &packs) {
                    warn!(error = %format!("{e:#}"), "populate_store on reuse failed");
                }
                return Ok(Some(IndexResolution {
                    index: std::sync::Arc::new(idx),
                    packs,
                    index_dir: Some(p),
                }));
            }
        }
        // No persistent index yet — build it.
        std::fs::create_dir_all(&p).ok();
        match HotdocIndex::build(&packs, &p) {
            Ok(idx) => {
                if let Err(e) = HotdocIndex::populate_store(db, &packs) {
                    warn!(error = %format!("{e:#}"), "populate_store on build failed");
                }
                return Ok(Some(IndexResolution {
                    index: std::sync::Arc::new(idx),
                    packs,
                    index_dir: Some(p),
                }));
            }
            Err(e) => {
                warn!(error = %format!("{e:#}"), "persistent build failed; falling back to tmp");
            }
        }
    }

    // 2) Runtime tmp fallback.
    let tmp = std::env::temp_dir().join(format!(
        "hotdoc-runtime-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let idx = HotdocIndex::build(&packs, &tmp).context("building runtime index")?;
    // Tmp mirror — same code path, just a different DB connection or
    // skipped if the caller's DB rejects the writes. Currently the
    // launcher's `db` is the real DB; tmp fallbacks are bench/test
    // paths and don't reach here.
    if let Err(e) = HotdocIndex::populate_store(db, &packs) {
        warn!(error = %format!("{e:#}"), "populate_store on tmp failed");
    }
    Ok(Some(IndexResolution {
        index: std::sync::Arc::new(idx),
        packs,
        index_dir: None,
    }))
}

/// Load packs with partial-failure semantics + per-file warn log. Used
/// by both the resolver and the bench / CLI paths that want a
/// `LoadReport` shape.
#[instrument]
pub fn load_with_logging(dir: &Path) -> Result<LoadReport> {
    let report = pack::load_dir(dir)?;
    for (path, errs) in &report.failed {
        for e in errs {
            warn!(file = %path.display(), error = %e, "failed to load pack");
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{Entry, EntrySource, Example};
    use crate::store::{migrate, open as open_store};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fresh_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "hotdoc-resolver-test-{}-{}-{}",
            std::process::id(),
            n,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn good_pack_json(id: &str) -> String {
        format!(
            r#"{{
                "id": "{id}",
                "name": "{id}",
                "version": "1.0.0",
                "source": "test",
                "license": "MIT",
                "entries": [
                    {{
                        "id": "{id}-1",
                        "title": "T",
                        "syntax": "x",
                        "description": "d",
                        "source": "curated"
                    }}
                ]
            }}"#
        )
    }

    fn open_test_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = open_store(&dir.path().join("hotdoc.sqlite")).expect("open");
        migrate(&conn).expect("migrate");
        (dir, conn)
    }

    fn make_pack(id: &str) -> crate::pack::Pack {
        crate::pack::Pack {
            id: id.to_string(),
            name: id.to_string(),
            version: "1.0.0".to_string(),
            source: "test".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            entries: vec![Entry {
                id: format!("{id}-1"),
                title: "T".to_string(),
                syntax: "x".to_string(),
                description: "d".to_string(),
                examples: vec![Example {
                    description: "ex".into(),
                    code: format!("code-{id}"),
                }],
                tags: vec![],
                source: EntrySource::Curated,
                source_url: None,
            }],
        }
    }

    #[test]
    fn resolver_returns_loaded_packs() {
        // Build a 1-pack dir; load_with_logging should return the pack
        // and zero failures.
        let dir = fresh_dir();
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("alpha.json"), good_pack_json("alpha")).expect("write");
        let report = load_with_logging(&dir).expect("load");
        assert_eq!(report.loaded.len(), 1);
        assert_eq!(report.loaded[0].id, "alpha");
        assert!(report.failed.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolver_warns_per_failed_pack() {
        let dir = fresh_dir();
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("alpha.json"), good_pack_json("alpha")).expect("write");
        std::fs::write(dir.join("broken.json"), "not json").expect("write broken");
        let report = load_with_logging(&dir).expect("load");
        assert_eq!(report.loaded.len(), 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(
            report.failed[0].0.file_name().expect("path has file_name"),
            "broken.json"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_and_build_populates_store() {
        // Real Tauri-shaped test: packs dir + an in-temp sqlite db.
        // resolve_and_build needs a persistent_index_dir that maps to a
        // tmp path. We can't override it in the production code, so we
        // drive the pieces directly: load → build → populate_store.
        let (_dbdir, conn) = open_test_db();
        let dir = fresh_dir();
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("alpha.json"), good_pack_json("alpha")).expect("write");
        std::fs::write(dir.join("beta.json"), good_pack_json("beta")).expect("write");
        let packs: Vec<crate::pack::Pack> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .map(|p| {
                let raw = std::fs::read_to_string(&p).expect("read");
                let pack: crate::pack::Pack = serde_json::from_str(&raw).expect("parse");
                pack
            })
            .collect();
        let index_dir = fresh_dir();
        let _idx = HotdocIndex::build(&packs, &index_dir).expect("build");
        HotdocIndex::populate_store(&conn, &packs).expect("populate");
        // every pack row + every entry row should be present
        let n_packs: i64 = conn
            .query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))
            .expect("count packs");
        let n_entries: i64 = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .expect("count entries");
        assert_eq!(n_packs, 2);
        assert_eq!(n_entries, 2);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&index_dir);
    }

    #[test]
    fn pinned_list_returns_real_card_after_index_populate() {
        // The "real pin is dead" audit finding (T16 acceptance):
        // pin a real card id, run populate_store, JOIN returns full
        // content. Before T16, entries was empty → JOIN returned [].
        let (_dbdir, conn) = open_test_db();
        let packs = vec![make_pack("git")];
        // First: pin a card id, then populate. Without populate, the
        // JOIN returns nothing.
        crate::store::pinned::add(&conn, "git-1").expect("pin");
        let rows = crate::store::pinned::list(&conn).expect("list before populate");
        assert!(
            rows.is_empty(),
            "without populate, pinned JOIN should be empty (pre-T16 bug)"
        );
        // Now populate and re-list — real card content surfaces.
        HotdocIndex::populate_store(&conn, &packs).expect("populate");
        let rows = crate::store::pinned::list(&conn).expect("list after populate");
        assert_eq!(
            rows.len(),
            1,
            "after populate, pinned JOIN returns the real card"
        );
        assert_eq!(rows[0].id, "git-1");
        assert_eq!(rows[0].pack_id, "git");
        assert_eq!(rows[0].syntax, "x");
        assert_eq!(rows[0].description, "d");
    }
}
