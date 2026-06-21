//! SQLite store for recents, pinned, settings, and index bookkeeping.
//!
//! All seven spec §9.2 tables are created by [`migrate`]; only the `meta`
//! module is wired up here. Per-table CRUD modules (`recents`, `pinned`,
//! `settings`, `packs`) land with their respective tasks (T2/T5/T8/T6).

pub mod meta;
pub mod packs;
pub mod pinned;
pub mod recents;

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
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Open (or create) the SQLite database at `db_path`. Enables WAL mode and
/// foreign keys; sets a 5s busy timeout. Caller invokes [`migrate`] to
/// ensure the schema is current before use.
#[instrument(skip_all, fields(db_path = %db_path.display()))]
pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5_000i64)?;
    Ok(conn)
}

/// Idempotent schema migration. Creates the seven spec §9.2 tables and their
/// indices if missing; bumps `meta.schema_version` on a successful migration
/// (no-op when the row already records the current version).
#[instrument(skip_all)]
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_SQL)?;

    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();

    if current.as_deref() != Some(SCHEMA_VERSION_STR) {
        conn.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![SCHEMA_VERSION_STR],
        )?;
    }
    Ok(())
}

/// Pre-rendered `SCHEMA_VERSION` so `migrate()` doesn't format on every call.
const SCHEMA_VERSION_STR: &str = "1";

/// Default on-disk database path: `dirs::data_local_dir() / "hotdoc" / "hotdoc.sqlite"`.
/// Same directory the persistent Tantivy index lives in.
#[instrument]
pub fn default_db_path() -> Result<std::path::PathBuf> {
    let dir = dirs::data_local_dir().ok_or(StoreError::MissingDataDir)?;
    Ok(dir.join("hotdoc").join("hotdoc.sqlite"))
}

/// The full schema as one batch. Mirrors spec §9.2 verbatim.
const SCHEMA_SQL: &str = "
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
}
