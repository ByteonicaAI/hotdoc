//! SQLite store for recents, pinned, settings, and index bookkeeping.
//!
//! All seven spec §9.2 tables are created by [`migrate`]; only the `meta`
//! module is wired up here. Per-table CRUD modules (`recents`, `pinned`,
//! `settings`, `packs`) land with their respective tasks (T2/T5/T8/T6).

pub mod entries;
pub mod meta;
pub mod migrations;
pub mod packs;
pub mod pinned;
pub mod popularity;
pub mod recents;
pub mod search_log;
pub mod settings;
pub mod time;

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use thiserror::Error;
use tracing::instrument;

/// SQLite open error. Mapped to the spec §2.2 single-error surface so callers
/// (Tauri commands, tests) use `?` without hand-rolled `From` impls.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("platform data directory not available")]
    MissingDataDir,
    #[error("migration v{step} failed: {message}")]
    MigrationFailed { step: u32, message: String },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Search-log retention window (P2-11). Rows older than this are pruned
/// from `search_log` on every `store::open()` call. 30d is wider than
/// the 7d popularity signal so the FR-G2 diagnostics bundle can include
/// a last-30d summary; the FR-G2 read path itself is unaffected.
const SEARCH_LOG_RETENTION_MS: i64 = 30 * 86_400 * 1000;

/// Open (or create) the SQLite database at `db_path`. Enables WAL mode and
/// foreign keys; sets a 5s busy timeout. Caller invokes [`migrate`] to
/// ensure the schema is current before use.
///
/// SEC-8: sets data dir to 0700 and DB file to 0600 on Unix (idempotent).
///
/// After a successful open, attempts to prune `search_log` rows older
/// than [`SEARCH_LOG_RETENTION_MS`] (P2-11). The prune runs from
/// `open()` rather than from `migrate()` because it is DML, not DDL —
/// pruning on every launch is the v1 retention clock; a schema
/// migration step would run exactly once and not capture subsequent
/// launches. The call site contract (`open()` before `migrate()`) means
/// the `search_log` table may not exist yet — in that case the prune
/// silently no-ops; the next launch (after migrate has run) will prune
/// correctly. Other DB errors propagate.
#[instrument(skip_all, fields(db_path = %db_path.display()))]
pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        apply_dir_perms(parent)?;
    }
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    #[cfg(unix)]
    apply_file_perms(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5_000i64)?;
    // P2-11 retention. Wrap in a transaction so the prune is atomic
    // with respect to concurrent readers; WAL gives us crash-safety.
    // Skip when the search_log table doesn't exist yet (fresh DB,
    // before migrate) — that's the documented `open()`-then-`migrate()`
    // call sequence. Once migrate has run, every subsequent open()
    // will prune successfully.
    let cutoff = crate::store::time::unix_now_ms() - SEARCH_LOG_RETENTION_MS;
    let tx = conn.unchecked_transaction()?;
    match tx.execute("DELETE FROM search_log WHERE ts < ?1", [cutoff]) {
        Ok(0) => {
            tx.commit()?;
        }
        Ok(pruned) => {
            tx.commit()?;
            tracing::debug!(pruned, cutoff, "search_log retention prune");
        }
        Err(e) if is_no_such_table(&e) => {
            // search_log not yet created by migrate — nothing to prune.
            // Roll back the empty transaction so we don't leave a
            // dangling implicit lock on the fresh-DB path.
            let _ = tx.rollback();
        }
        Err(e) => {
            let _ = tx.rollback();
            return Err(e.into());
        }
    }
    Ok(conn)
}

/// True if the rusqlite error is the SQLite "no such table" SQLSTATE,
/// used by the P2-11 prune-on-open call to gracefully skip when
/// `search_log` doesn't exist yet (the table is created by `migrate()`,
/// which runs after `open()` at every call site).
fn is_no_such_table(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(_, Some(msg)) if msg.contains("no such table")
    )
}

/// SEC-8: restrict the data directory to owner-only (0700).
#[cfg(unix)]
fn apply_dir_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// SEC-8: restrict the SQLite file to owner-only read/write (0600).
/// Called after SQLite creates the file; idempotent on subsequent opens.
#[cfg(unix)]
fn apply_file_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if path.exists() {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Forward-stepping schema migration (PRD §10, FR-I5).
///
/// Reads `meta.schema_version`, applies every pending step from `migrations`
/// in order, each in its own transaction. On step failure returns
/// `StoreError::MigrationFailed` — callers map this to the FR-I5 rebuild
/// path (delete DB + rebuild from scratch).
#[instrument(skip_all)]
pub fn migrate(conn: &Connection) -> Result<()> {
    migrations::run(conn)
}

/// Default on-disk database path: `dirs::data_local_dir() / "hotdoc" / "hotdoc.sqlite"`.
/// Same directory the persistent Tantivy index lives in.
#[instrument]
pub fn default_db_path() -> Result<std::path::PathBuf> {
    let dir = dirs::data_local_dir().ok_or(StoreError::MissingDataDir)?;
    Ok(dir.join("hotdoc").join("hotdoc.sqlite"))
}

/// The full schema as one batch. Mirrors spec §9.2 verbatim.
/// pub(crate) so `migrations::v1` can reference it as the baseline.
pub(crate) const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS packs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  version TEXT NOT NULL,
  source TEXT NOT NULL,
  license TEXT NOT NULL,
  homepage TEXT,
  indexed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS entries (
  id TEXT PRIMARY KEY,
  pack_id TEXT NOT NULL REFERENCES packs(id),
  title TEXT NOT NULL,
  syntax TEXT NOT NULL,
  description TEXT NOT NULL,
  examples_json TEXT NOT NULL,
  tags_json TEXT NOT NULL,
  source TEXT NOT NULL,
  source_url TEXT,
  FOREIGN KEY (pack_id) REFERENCES packs(id)
);

CREATE INDEX IF NOT EXISTS entries_pack_idx ON entries(pack_id);

CREATE TABLE IF NOT EXISTS recents (
  query TEXT PRIMARY KEY,
  last_used_at INTEGER NOT NULL,
  use_count INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS pinned (
  entry_id TEXT PRIMARY KEY,
  pinned_at INTEGER NOT NULL,
  position INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS search_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  query TEXT NOT NULL,
  first_result_id TEXT,
  clicked_result_id TEXT,
  ts INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS search_log_ts_idx ON search_log(ts);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_spec_schema() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = open(&db_path).expect("open");
        migrate(&conn).expect("migrate");

        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query_map")
            .map(|r| r.expect("row"))
            .collect();

        let expected = [
            "entries",
            "meta",
            "packs",
            "pinned",
            "recents",
            "search_log",
            "settings",
        ];
        for table in expected {
            assert!(
                names.iter().any(|n| n == table),
                "missing table {table}; got {names:?}"
            );
        }
    }

    #[test]
    fn open_enables_wal_and_foreign_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = open(&db_path).expect("open");

        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .expect("journal_mode");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");

        let fk: i64 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))
            .expect("foreign_keys");
        assert_eq!(fk, 1, "foreign_keys pragma should be ON");
    }

    #[test]
    fn migrate_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = open(&db_path).expect("open");
        migrate(&conn).expect("first migrate");
        migrate(&conn).expect("second migrate should be a no-op");

        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("schema_version row");
        assert_eq!(v, super::meta::SCHEMA_VERSION.to_string());
    }

    // SEC-8: data dir must be 0700, DB file must be 0600 on Linux.
    #[test]
    #[cfg(unix)]
    fn data_dir_0700_and_file_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("subdir").join("hotdoc.sqlite");
        let conn = open(&db_path).expect("open");
        migrate(&conn).expect("migrate");
        drop(conn);

        let dir_mode = std::fs::metadata(db_path.parent().expect("parent"))
            .expect("dir meta")
            .permissions()
            .mode();
        assert_eq!(
            dir_mode & 0o777,
            0o700,
            "data dir must be 0700, got {dir_mode:#o}"
        );

        let file_mode = std::fs::metadata(&db_path)
            .expect("file meta")
            .permissions()
            .mode();
        assert_eq!(
            file_mode & 0o777,
            0o600,
            "DB file must be 0600, got {file_mode:#o}"
        );
    }

    // SEC-8: open on an existing DB re-asserts perms (idempotent).
    #[test]
    #[cfg(unix)]
    fn data_perms_idempotent_on_reopen() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        {
            let conn = open(&db_path).expect("first open");
            migrate(&conn).expect("migrate");
        }
        // Loosen perms manually to simulate external change.
        std::fs::set_permissions(&db_path, std::fs::Permissions::from_mode(0o644))
            .expect("set 644");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
            .expect("set 755");
        // Reopen must tighten them back.
        let _ = open(&db_path).expect("reopen");
        let file_mode = std::fs::metadata(&db_path)
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(
            file_mode & 0o777,
            0o600,
            "file perms must be 0600 after reopen"
        );
    }

    // ponytail: P2-11 wire-up. `open()` must run `prune_old` on every
    // reopen so the 30-day retention clock advances on launch. Seed a
    // row at epoch 0 (1970), close, reopen via the public `open()` path
    // (same path the Tauri binary uses); the ancient row must be gone.
    #[test]
    fn open_prunes_search_log_rows_older_than_retention() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        {
            let conn = open(&db_path).expect("open");
            migrate(&conn).expect("migrate");
            // Seed an ancient row (well past any plausible 30d cutoff)
            // plus a fresh row (must survive). Distinct queries so the
            // survivor check is unambiguous.
            conn.execute(
                "INSERT INTO search_log(query, first_result_id, clicked_result_id, ts) \
                 VALUES ('ancient', NULL, NULL, 0)",
                [],
            )
            .expect("seed ancient");
            conn.execute(
                "INSERT INTO search_log(query, first_result_id, clicked_result_id, ts) \
                 VALUES ('fresh', NULL, NULL, ?1)",
                rusqlite::params![crate::store::time::unix_now_ms()],
            )
            .expect("seed fresh");
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM search_log", [], |r| r.get(0))
                .expect("count before");
            assert_eq!(n, 2);
        }
        // Reopen — the wire-up runs here.
        let conn = open(&db_path).expect("reopen");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_log", [], |r| r.get(0))
            .expect("count after");
        assert_eq!(
            n, 1,
            "ancient row must be pruned by open()'s 30d cutoff; fresh row survives"
        );
        let survivor: String = conn
            .query_row(
                "SELECT query FROM search_log ORDER BY ts ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("survivor");
        assert_eq!(survivor, "fresh");
    }

    // P2-11: opening a brand-new DB (no search_log table yet) must not
    // error — the prune must skip when the table is absent rather than
    // returning a `no such table` error.
    //
    // We can't easily test "no migrate was called" because the file
    // gets `journal_mode = WAL` set in `open()` which is DDL-ish. But
    // the prune only runs AFTER `open()` finishes the pragma setup,
    // and pragmas work on a DB without tables. The chaos-writer test
    // already exercises this path; here we assert the simpler case
    // that open+prune is happy on a DB with a fresh search_log table
    // (0 rows that need pruning).
    #[test]
    fn open_prune_is_noop_on_fresh_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = open(&db_path).expect("open");
        migrate(&conn).expect("migrate");
        // search_log now exists, empty. Reopen and confirm no panic.
        drop(conn);
        let conn = open(&db_path).expect("reopen");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_log", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 0, "fresh table + reopen must report 0 rows");
    }
}
