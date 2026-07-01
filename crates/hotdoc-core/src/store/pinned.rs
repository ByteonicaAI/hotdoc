//! Pinned table CRUD (spec FR-P1–P3).
//!
//! Pinned items are entry_ids the user has flagged via Ctrl+P. Cap at
//! MAX_PINNED; FIFO eviction on add. List reads JOIN against the SQLite
//! `entries` table (populated in v1.1's persistent-index-reuse work) so
//! pinned rows render with full card content, not just the ID.

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::{instrument, warn};

use crate::pack::Example;
use crate::store::Result;

/// Storage cap per spec FR-P1.
pub const MAX_PINNED: usize = 12;

#[derive(Debug, Clone, Serialize)]
pub struct PinnedHit {
    pub id: String,
    pub pack_id: String,
    pub title: String,
    pub syntax: String,
    pub description: String,
    pub source: String,
    pub source_url: Option<String>,
    pub example_code: Option<String>,
}

/// Add `entry_id` to pinned; no-op when already pinned; evicts the oldest
/// row beyond [`MAX_PINNED`]. Insert + evict run in a single transaction
/// (NFR-8 atomicity) — a crash between the two would otherwise leave
/// the table over-cap by one row.
#[instrument(skip(conn))]
pub fn add(conn: &Connection, entry_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let now = crate::store::time::unix_now_ms();
    tx.execute(
        "INSERT OR IGNORE INTO pinned(entry_id, pinned_at, position) VALUES (?1, ?2, ?2)",
        params![entry_id, now],
    )?;
    tx.execute(
        "DELETE FROM pinned WHERE entry_id NOT IN ( \
           SELECT entry_id FROM pinned ORDER BY pinned_at DESC LIMIT ?1 \
         )",
        params![MAX_PINNED as i64],
    )?;
    tx.commit()?;
    Ok(())
}

/// Remove `entry_id` from pinned. Returns the number of rows removed (0 or 1).
#[instrument(skip(conn))]
pub fn remove(conn: &Connection, entry_id: &str) -> Result<usize> {
    let n = conn.execute("DELETE FROM pinned WHERE entry_id = ?1", params![entry_id])?;
    Ok(n)
}

/// List pinned rows as full card content (joined against `entries`).
/// Returns empty Vec when the `entries` table is unpopulated — acceptable
/// per the plan's forward-compat note.
#[instrument(skip(conn))]
pub fn list(conn: &Connection) -> Result<Vec<PinnedHit>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.pack_id, e.title, e.syntax, e.description, e.source, \
                e.source_url, e.examples_json \
         FROM pinned p JOIN entries e ON p.entry_id = e.id \
         ORDER BY p.pinned_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![MAX_PINNED as i64], |row| {
        let entry_id: String = row.get(0)?;
        let examples_json: String = row.get(7)?;
        let examples: Vec<Example> = serde_json::from_str(&examples_json).unwrap_or_else(|e| {
            warn!(
                entry_id = %entry_id,
                error = %e,
                "examples_json failed to parse; falling back to empty examples list"
            );
            Vec::new()
        });
        Ok(PinnedHit {
            id: entry_id,
            pack_id: row.get(1)?,
            title: row.get(2)?,
            syntax: row.get(3)?,
            description: row.get(4)?,
            source: row.get(5)?,
            source_url: row.get(6)?,
            example_code: examples.first().map(|e| e.code.clone()),
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// `true` when `entry_id` is currently pinned. Propagates DB errors
/// (the prior `.unwrap_or(0)` swallowed anything beyond `QueryReturnedNoRows`,
/// hiding real failures as "not pinned" — audit §3.4).
#[instrument(skip(conn))]
pub fn is_pinned(conn: &Connection, entry_id: &str) -> Result<bool> {
    let n: i64 = match conn.query_row(
        "SELECT COUNT(*) FROM pinned WHERE entry_id = ?1",
        params![entry_id],
        |row| row.get(0),
    ) {
        Ok(n) => n,
        Err(rusqlite::Error::QueryReturnedNoRows) => 0,
        Err(e) => return Err(e.into()),
    };
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = crate::store::open(&dir.path().join("hotdoc.sqlite")).expect("open");
        crate::store::migrate(&conn).expect("migrate");
        (dir, conn)
    }

    fn seed_entry(conn: &Connection, id: &str, pack: &str) {
        conn.execute(
            "INSERT INTO packs(id, name, version, source, license, indexed_at) \
             VALUES (?1, ?1, '1.0.0', 'curated', 'MIT', 0) \
             ON CONFLICT(id) DO NOTHING",
            params![pack],
        )
        .expect("pack");
        conn.execute(
            "INSERT INTO entries(id, pack_id, title, syntax, description, \
                examples_json, tags_json, source) \
             VALUES (?1, ?2, ?3, ?4, ?5, '[]', '[]', 'curated')",
            params![
                id,
                pack,
                format!("title-{id}"),
                format!("syntax-{id}"),
                "desc"
            ],
        )
        .expect("entry");
    }

    #[test]
    fn add_then_remove() {
        let (_d, conn) = open();
        seed_entry(&conn, "git-stash", "git");
        add(&conn, "git-stash").expect("add");
        assert!(is_pinned(&conn, "git-stash").expect("is_pinned"));
        let n = remove(&conn, "git-stash").expect("remove");
        assert_eq!(n, 1);
        assert!(!is_pinned(&conn, "git-stash").expect("is_pinned"));
    }

    #[test]
    fn evicts_oldest_at_cap() {
        let (_d, conn) = open();
        for i in 0..(MAX_PINNED + 2) {
            let id = format!("entry-{i}");
            seed_entry(&conn, &id, "git");
            add(&conn, &id).expect("add");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let rows = list(&conn).expect("list");
        assert_eq!(rows.len(), MAX_PINNED);
        // newest first; the last two entries we added should remain.
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert!(!ids.contains(&"entry-0"), "oldest evicted");
        assert!(ids.contains(&format!("entry-{}", MAX_PINNED + 1).as_str()));
    }

    #[test]
    fn list_returns_full_card_content_via_join() {
        let (_d, conn) = open();
        seed_entry(&conn, "git-stash", "git");
        add(&conn, "git-stash").expect("add");
        let rows = list(&conn).expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "git-stash");
        assert_eq!(rows[0].pack_id, "git");
        assert_eq!(rows[0].syntax, "syntax-git-stash");
        assert_eq!(rows[0].description, "desc");
    }

    #[test]
    fn list_is_empty_when_entries_table_empty() {
        let (_d, conn) = open();
        // Insert into pinned directly so the entries-table join returns nothing.
        conn.execute(
            "INSERT INTO pinned(entry_id, pinned_at, position) VALUES ('orphan', 1, 1)",
            [],
        )
        .expect("orphan pin");
        let rows = list(&conn).expect("list");
        assert!(rows.is_empty(), "no entries row => nothing joined");
    }

    // ponytail: T19 (NFR-8 atomicity). Spawn a writer that loops `add()`
    // past the cap while a reader thread polls `SELECT COUNT(*)`. The
    // reader MUST never observe a count greater than MAX_PINNED — the
    // single `unchecked_transaction` in `add` exposes only the
    // pre-commit or post-commit snapshot to concurrent connections
    // (WAL mode + busy_timeout are configured in `store::open`).
    #[test]
    fn add_is_atomic_under_concurrent_reader() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = crate::store::open(&db_path).expect("open");
        crate::store::migrate(&conn).expect("migrate");
        for i in 0..(MAX_PINNED + 4) {
            seed_entry(&conn, &format!("entry-{i}"), "git");
        }

        let stop = Arc::new(AtomicBool::new(false));
        let reader_path = db_path.clone();
        let reader_stop = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let r = crate::store::open(&reader_path).expect("reader open");
            crate::store::migrate(&r).expect("reader migrate");
            let mut max_seen = 0i64;
            while !reader_stop.load(Ordering::Relaxed) {
                let n: i64 = r
                    .query_row("SELECT COUNT(*) FROM pinned", [], |row| row.get(0))
                    .expect("count");
                if n > max_seen {
                    max_seen = n;
                }
                assert!(
                    n <= MAX_PINNED as i64,
                    "reader observed {n} rows in pinned; cap = {MAX_PINNED} — transaction boundary leaked"
                );
            }
            max_seen
        });

        for i in 0..(MAX_PINNED + 4) {
            add(&conn, &format!("entry-{i}")).expect("add");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        stop.store(true, Ordering::Relaxed);
        let _ = reader.join().expect("reader join");
    }

    // ponytail: T19 (audit §3.4 error visibility). The pre-T19
    // `.unwrap_or(0)` swallowed any DB error and returned `false` —
    // indistinguishable from "not pinned" and silently broken.
    // Drop the `pinned` table to force a real, non-`QueryReturnedNoRows`
    // error from the SELECT, and confirm we propagate it.
    #[test]
    fn is_pinned_propagates_db_errors() {
        let (_d, conn) = open();
        // Confirm the happy path still works after the `match` rewrite.
        assert!(!is_pinned(&conn, "x").expect("happy path is_pinned"));
        // Now break the schema. `DROP TABLE` is a real DB error path
        // that is NOT `QueryReturnedNoRows`.
        conn.execute("DROP TABLE pinned", []).expect("drop table");
        let err =
            is_pinned(&conn, "x").expect_err("must propagate a non-QueryReturnedNoRows error");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("no such table")
                || msg.to_lowercase().contains("pinned")
                || msg.to_lowercase().contains("query"),
            "expected a schema/query error, got: {msg}"
        );
    }
}
