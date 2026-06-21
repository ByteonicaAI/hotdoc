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
    pub last_used_at: i64,
    pub use_count: i64,
}

/// Insert or bump `query` in the recents table. No-op when recents are
/// disabled in settings. Evicts the oldest rows beyond [`MAX_RECENTS`].
#[instrument(skip(conn))]
pub fn record(conn: &Connection, query: &str) -> Result<()> {
    if !is_enabled(conn)? {
        return Ok(());
    }
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let now = unix_now();

    conn.execute(
        "INSERT INTO recents(query, last_used_at, use_count) VALUES (?1, ?2, 1) \
         ON CONFLICT(query) DO UPDATE SET \
           last_used_at = excluded.last_used_at, \
           use_count = recents.use_count + 1",
        params![trimmed, now],
    )?;

    evict_beyond_cap(conn)?;
    Ok(())
}

/// Return the most recently used `n` queries (MRU by `last_used_at`, ties
/// broken by `use_count`).
#[instrument(skip(conn))]
pub fn top_n(conn: &Connection, n: usize) -> Result<Vec<Recent>> {
    let mut stmt = conn.prepare(
        "SELECT query, last_used_at, use_count \
         FROM recents \
         ORDER BY last_used_at DESC, use_count DESC \
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |row| {
        Ok(Recent {
            query: row.get(0)?,
            last_used_at: row.get(1)?,
            use_count: row.get(2)?,
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

/// True when the user has not disabled recents in settings.
#[instrument(skip(conn))]
pub fn is_enabled(conn: &Connection) -> Result<bool> {
    let v: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'recents_enabled'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(v.as_deref() != Some("false"))
}

fn evict_beyond_cap(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM recents WHERE query NOT IN ( \
           SELECT query FROM recents ORDER BY last_used_at DESC, use_count DESC LIMIT ?1 \
         )",
        params![MAX_RECENTS as i64],
    )?;
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
        record(&conn, "git status").expect("record");
        let rows = top_n(&conn, 5).expect("top_n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].query, "git status");
        assert_eq!(rows[0].use_count, 1);
    }

    #[test]
    fn record_bumps_use_count_on_repeat() {
        let (_dir, conn) = open();
        record(&conn, "docker ps").expect("record");
        record(&conn, "docker ps").expect("record");
        record(&conn, "docker ps").expect("record");
        let rows = top_n(&conn, 5).expect("top_n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].use_count, 3);
    }

    #[test]
    fn evicts_beyond_20() {
        let (_dir, conn) = open();
        for i in 0..25 {
            let q = format!("query-{i:02}");
            record(&conn, &q).expect("record");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let rows = top_n(&conn, 100).expect("top_n");
        assert_eq!(rows.len(), 20, "should cap at 20 rows");
        assert_eq!(rows[0].query, "query-24", "newest first");
    }

    #[test]
    fn clear_drops_all_rows() {
        let (_dir, conn) = open();
        record(&conn, "a").expect("record");
        record(&conn, "b").expect("record");
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
        record(&conn, "ignored").expect("record");
        assert!(top_n(&conn, 5).expect("top_n").is_empty());
    }

    #[test]
    fn empty_query_is_noop() {
        let (_dir, conn) = open();
        record(&conn, "   ").expect("record");
        assert!(top_n(&conn, 5).expect("top_n").is_empty());
    }
}
