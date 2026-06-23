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
            match HotdocIndex::open(&p) {
                Ok(idx) => {
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
                Err(e) => {
                    // FR-I5: corrupt or incompatible index — log diagnostic and rebuild.
                    warn!(
                        path = %p.display(),
                        error = %format!("{e:#}"),
                        "index open failed (corrupt or incompatible) — will rebuild"
                    );
                }
            }
        }
        // No persistent index yet (or corrupt — see warn above) — build it.
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

    // FR-I5 / NFR-8: a corrupt or truncated tantivy index is silently
    // discarded and rebuilt from the pack set. The rebuild must produce a
    // working, searchable index.
    #[test]
    fn corrupt_index_falls_back_to_rebuild() {
        let dir = fresh_dir();
        std::fs::create_dir_all(&dir).expect("mkdir");
        let pack = make_pack("git");
        let packs = vec![pack];

        // Build a valid index.
        let idx = HotdocIndex::build(&packs, &dir).expect("initial build");
        let hits = idx.search("x", 8, &Default::default()).expect("search ok");
        assert!(!hits.is_empty(), "initial index must be searchable");
        drop(idx);

        // Corrupt the meta.json file (tantivy's index descriptor).
        let meta_path = dir.join("meta.json");
        if meta_path.exists() {
            std::fs::write(&meta_path, b"CORRUPTED").expect("corrupt meta.json");
        } else {
            // Fallback: corrupt any file in the dir.
            let first = std::fs::read_dir(&dir)
                .expect("readdir")
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| p.is_file())
                .expect("at least one file in index dir");
            std::fs::write(&first, b"CORRUPTED").expect("corrupt segment");
        }

        // Open must fail on the corrupt index.
        let open_result = HotdocIndex::open(&dir);
        assert!(
            open_result.is_err(),
            "opening a corrupt index must return an error"
        );

        // Rebuild must succeed (build() removes and recreates the dir).
        let rebuilt = HotdocIndex::build(&packs, &dir).expect("rebuild after corruption");
        let hits = rebuilt
            .search("x", 8, &Default::default())
            .expect("search after rebuild");
        assert!(
            !hits.is_empty(),
            "rebuilt index must be searchable; got zero hits"
        );
        assert_eq!(hits[0].id, "git-1", "rebuilt index returns correct entry");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ponytail: T6.5 atomicity wire-up. `populate_store` is supposed
    // to rewrite both `packs` and `entries` in a single transaction,
    // so a crash between the two can never leave the DB half-
    // populated. This test pre-seeds a fake `packs` row (no matching
    // `entries` row, to avoid the FK from entries.pack_id → packs.id
    // blocking the populate's DELETE FROM packs step — that FK
    // protection is working as intended and not what we're testing
    // here), then asserts the fake pack is gone and the real pack
    // + entry are present after populate. If a future refactor
    // breaks the outer-tx wire-up (e.g. someone moves back to two
    // separate `upsert_all` calls), this test exercises the
    // "both tables actually got rewritten by the same transaction"
    // invariant directly.
    //
    // Failure-injection (making `entries::upsert_all_tx` fail and
    // proving `packs` rolls back) is left for a future test — the
    // happy-path coverage + the source-visible `?` propagation +
    // rusqlite's `Transaction` drop-without-commit semantics are
    // the contract.
    #[test]
    fn populate_store_replaces_existing_data_atomically() {
        let (_dbdir, conn) = open_test_db();
        // Pre-seed a fake pack with no matching entry. The
        // entries.pack_id → packs.id FK would block a DELETE
        // FROM packs if a matching entry existed; we want to
        // exercise the atomicity wire-up, not the FK behavior.
        conn.execute(
            "INSERT INTO packs(id, name, version, source, license, indexed_at) \
             VALUES ('ghost-pack', 'Ghost', '0.0.1', 'curated', 'MIT', 0)",
            [],
        )
        .expect("seed ghost pack");
        // Sanity: pre-populate has the ghost row.
        let n_ghost_before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM packs WHERE id = 'ghost-pack'",
                [],
                |r| r.get(0),
            )
            .expect("count ghost packs before");
        assert_eq!(n_ghost_before, 1);
        // Real pack list with no ghost-pack.
        let packs = vec![make_pack("real")];
        let index_dir = fresh_dir();
        let _idx = HotdocIndex::build(&packs, &index_dir).expect("build");
        HotdocIndex::populate_store(&conn, &packs).expect("populate");
        // The ghost pack must be gone (proving packs::upsert_all_tx
        // ran inside the outer tx).
        let n_ghost_packs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM packs WHERE id = 'ghost-pack'",
                [],
                |r| r.get(0),
            )
            .expect("count ghost packs after");
        assert_eq!(n_ghost_packs, 0, "ghost pack must be replaced");
        // The real pack + entry must be present (proving both
        // upserts landed in the same committed tx — if the
        // entries upsert had been rolled back separately, the
        // real entry would be missing).
        let n_real_packs: i64 = conn
            .query_row("SELECT COUNT(*) FROM packs WHERE id = 'real'", [], |r| {
                r.get(0)
            })
            .expect("count real packs");
        assert_eq!(n_real_packs, 1, "real pack must be present");
        let n_real_entries: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE pack_id = 'real'",
                [],
                |r| r.get(0),
            )
            .expect("count real entries");
        assert_eq!(n_real_entries, 1, "real entry must be present");

        let _ = std::fs::remove_dir_all(&index_dir);
    }

    // ponytail: T6.5-FU failure-injection counterpart to
    // populate_store_replaces_existing_data_atomically. That test
    // proves the happy path (pre-seeded data gets atomically
    // replaced). This test proves the audit's actual contract: if
    // entries::upsert_all_tx fails partway, the packs writes that
    // already succeeded are rolled back. The fix's rollback
    // guarantee is what closes the half-populated-state gap; the
    // happy-path test alone cannot distinguish "atomic" from
    // "sequential and consistent" — both end in the same final
    // state when nothing fails.
    //
    // How the failure is injected: a SQLite trigger on the entries
    // table that fires on INSERT and aborts when pack_id matches
    // the populate's real pack. The trigger fires during the
    // entries::upsert_all_tx inner loop, after packs::upsert_all_tx
    // has already committed its DELETE + INSERT to the outer tx.
    // The ? propagates, the outer Transaction drops without
    // commit, and the rollback path is exercised.
    //
    // If a future refactor moves back to two separate
    // upsert_all calls (or breaks the outer-tx wire-up), this
    // test fails loudly: the packs table will retain the
    // "real" row that packs::upsert_all_tx wrote, proving the
    // rollback didn't happen.
    #[test]
    fn populate_store_rolls_back_packs_when_entries_fail() {
        let (_dbdir, conn) = open_test_db();
        // Install a trigger that aborts the first INSERT into
        // entries with pack_id='real'. Fires inside
        // entries::upsert_all_tx, after packs::upsert_all_tx
        // has fully written its row.
        conn.execute(
            "CREATE TRIGGER fail_on_real_entry \
             INSERT ON entries \
             WHEN NEW.pack_id = 'real' \
             BEGIN \
               SELECT RAISE(ABORT, 'injected failure for atomicity test'); \
             END",
            [],
        )
        .expect("install trigger");
        // Sanity: packs table is empty before populate.
        let n_packs_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))
            .expect("count packs before");
        assert_eq!(n_packs_before, 0, "fresh db must start with 0 packs");
        // populate MUST error out (the trigger aborts the
        // entries INSERT). The test asserts the error rather
        // than swallowing it.
        let packs = vec![make_pack("real")];
        let index_dir = fresh_dir();
        let _idx = HotdocIndex::build(&packs, &index_dir).expect("build");
        let result = HotdocIndex::populate_store(&conn, &packs);
        assert!(
            result.is_err(),
            "populate must fail when entries trigger aborts; got Ok"
        );
        // The audit's contract: packs writes that already
        // happened (DELETE FROM packs + INSERT INTO packs
        // 'real') must be rolled back. If both tables are empty
        // post-failure, the rollback worked. If packs retains
        // the 'real' row, the wire-up is broken.
        let n_packs_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))
            .expect("count packs after");
        assert_eq!(
            n_packs_after, 0,
            "packs writes must be rolled back when entries fails; \
             a non-zero count means the outer-tx wire-up is broken"
        );
        let n_entries_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .expect("count entries after");
        assert_eq!(
            n_entries_after, 0,
            "entries must remain empty (trigger fired before any INSERT committed)"
        );

        let _ = std::fs::remove_dir_all(&index_dir);
    }
}
