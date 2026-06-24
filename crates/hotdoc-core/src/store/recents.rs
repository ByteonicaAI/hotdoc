//! Recents table CRUD (spec FR-R1–R4).
//!
//! Records the queries a user activated. Capped at 20 stored rows; older
//! rows are evicted FIFO. Reads respect the `settings.recents_enabled`
//! toggle (FR-R4) — recording becomes a no-op when disabled.

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::instrument;

use crate::store::Result;

/// Spec FR-R1 storage cap. After `record()`, evict the oldest rows beyond
/// this count.
const MAX_RECENTS: usize = 20;

/// One row from the `recents` table.
#[derive(Debug, Clone, Serialize)]
pub struct Recent {
    pub query: String,
    pub copied_syntax: Option<String>,
    pub last_used_at: i64,
    pub use_count: i64,
}

/// Insert or bump `query` in the recents table. No-op when recents are
/// disabled in settings. Evicts the oldest rows beyond [`MAX_RECENTS`].
/// Insert + evict run in a single transaction (NFR-8 atomicity) — a crash
/// between the two would otherwise leave the table over-cap by one row.
#[instrument(skip(conn))]
pub fn record(conn: &Connection, query: &str, syntax: Option<&str>) -> Result<()> {
    if !is_enabled(conn)? {
        return Ok(());
    }
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let now = crate::store::time::unix_now_ms();
    tx.execute(
        "INSERT INTO recents(query, copied_syntax, last_used_at, use_count) VALUES (?1, ?2, ?3, 1) \
         ON CONFLICT(query) DO UPDATE SET \
           copied_syntax = excluded.copied_syntax, \
           last_used_at = excluded.last_used_at, \
           use_count = recents.use_count + 1",
        params![trimmed, syntax, now],
    )?;
    tx.execute(
        "DELETE FROM recents WHERE query NOT IN ( \
           SELECT query FROM recents ORDER BY last_used_at DESC, use_count DESC LIMIT ?1 \
         )",
        params![MAX_RECENTS as i64],
    )?;
    tx.commit()?;
    Ok(())
}

/// Return the most recently used `n` queries (MRU by `last_used_at`, ties
/// broken by `use_count`).
#[instrument(skip(conn))]
pub fn top_n(conn: &Connection, n: usize) -> Result<Vec<Recent>> {
    let mut stmt = conn.prepare(
        "SELECT query, copied_syntax, last_used_at, use_count \
         FROM recents \
         ORDER BY last_used_at DESC, use_count DESC \
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |row| {
        Ok(Recent {
            query: row.get(0)?,
            copied_syntax: row.get(1)?,
            last_used_at: row.get(2)?,
            use_count: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Delete every row from `recents`. Returns the number of rows removed.
#[instrument(skip(conn))]
pub fn clear(conn: &Connection) -> Result<usize> {
    let n = conn.execute("DELETE FROM recents", [])?;
    Ok(n)
}

/// True when the user has not disabled recents in settings. Propagates
/// DB errors (the prior `.ok()` swallowed anything beyond
/// `QueryReturnedNoRows`, hiding real failures as "disabled" — audit §3.4).
#[instrument(skip(conn))]
pub fn is_enabled(conn: &Connection) -> Result<bool> {
    let v: Option<String> = match conn.query_row(
        "SELECT value FROM settings WHERE key = 'recents_enabled'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.into()),
    };
    Ok(v.as_deref() != Some("false"))
}

// ponytail: StoreError already covers `Db` and `Io`; rusqlite errors map via
// `?` through the `#[from]` derive on `StoreError::Db`. No bespoke From impls.

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = crate::store::open(&dir.path().join("hotdoc.sqlite")).expect("open");
        crate::store::migrate(&conn).expect("migrate");
        (dir, conn)
    }

    #[test]
    fn record_writes_on_first_call() {
        let (_dir, conn) = open();
        record(&conn, "git status", None).expect("record");
        let rows = top_n(&conn, 5).expect("top_n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].query, "git status");
        assert_eq!(rows[0].use_count, 1);
    }

    #[test]
    fn record_bumps_use_count_on_repeat() {
        let (_dir, conn) = open();
        record(&conn, "docker ps", None).expect("record");
        record(&conn, "docker ps", None).expect("record");
        record(&conn, "docker ps", None).expect("record");
        let rows = top_n(&conn, 5).expect("top_n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].use_count, 3);
    }

    #[test]
    fn evicts_beyond_20() {
        let (_dir, conn) = open();
        for i in 0..25 {
            let q = format!("query-{i:02}");
            record(&conn, &q, None).expect("record");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let rows = top_n(&conn, 100).expect("top_n");
        assert_eq!(rows.len(), 20, "should cap at 20 rows");
        assert_eq!(rows[0].query, "query-24", "newest first");
    }

    #[test]
    fn clear_drops_all_rows() {
        let (_dir, conn) = open();
        record(&conn, "a", None).expect("record");
        record(&conn, "b", None).expect("record");
        let n = clear(&conn).expect("clear");
        assert_eq!(n, 2);
        assert!(top_n(&conn, 5).expect("top_n").is_empty());
    }

    #[test]
    fn disabled_when_recents_enabled_is_false() {
        let (_dir, conn) = open();
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('recents_enabled', 'false') \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )
        .expect("insert setting");
        record(&conn, "ignored", None).expect("record");
        assert!(top_n(&conn, 5).expect("top_n").is_empty());
    }

    #[test]
    fn empty_query_is_noop() {
        let (_dir, conn) = open();
        record(&conn, "   ", None).expect("record");
        assert!(top_n(&conn, 5).expect("top_n").is_empty());
    }

    // ponytail: T19 (NFR-8 atomicity). Same shape as the pinned test:
    // a writer loops `record()` past MAX_RECENTS while a reader polls
    // `SELECT COUNT(*)`. The reader MUST never see more than
    // MAX_RECENTS rows — the single `unchecked_transaction` in
    // `record` is the only thing standing between us and an
    // observable over-cap state under concurrent reads.
    #[test]
    fn record_is_atomic_under_concurrent_reader() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = crate::store::open(&db_path).expect("open");
        crate::store::migrate(&conn).expect("migrate");
        // Default setting (recents_enabled absent → is_enabled true).
        let stop = Arc::new(AtomicBool::new(false));
        let reader_path = db_path.clone();
        let reader_stop = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let r = crate::store::open(&reader_path).expect("reader open");
            crate::store::migrate(&r).expect("reader migrate");
            while !reader_stop.load(Ordering::Relaxed) {
                let n: i64 = r
                    .query_row("SELECT COUNT(*) FROM recents", [], |row| row.get(0))
                    .expect("count");
                assert!(
                    n <= MAX_RECENTS as i64,
                    "reader observed {n} rows in recents; cap = {MAX_RECENTS}"
                );
            }
        });

        for i in 0..(MAX_RECENTS + 4) {
            record(&conn, &format!("q-{i:02}"), None).expect("record");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        stop.store(true, Ordering::Relaxed);
        reader.join().expect("reader join");
    }

    // ponytail: T19 (audit §3.4 error visibility). Pre-T19 the
    // `.ok()` swallowed any DB error and returned `false` for
    // `is_enabled` — the recents-disable toggle became a no-op
    // instead of an error. Drop the `settings` table to force a
    // non-`QueryReturnedNoRows` error.
    #[test]
    fn is_enabled_propagates_db_errors() {
        let (_d, conn) = open();
        // Happy path: no settings row → is_enabled == true.
        assert!(is_enabled(&conn).expect("happy path is_enabled"));
        conn.execute("DROP TABLE settings", []).expect("drop table");
        let err = is_enabled(&conn).expect_err("must propagate a non-QueryReturnedNoRows error");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("no such table")
                || msg.to_lowercase().contains("settings")
                || msg.to_lowercase().contains("query"),
            "expected a schema/query error, got: {msg}"
        );
    }
}
