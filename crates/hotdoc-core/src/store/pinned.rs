//! Pinned table CRUD (spec FR-P1–P3).
//!
//! Pinned items are entry_ids the user has flagged via Ctrl+P. Cap at
//! MAX_PINNED; FIFO eviction on add. List reads JOIN against the SQLite
//! `entries` table (populated in v1.1's persistent-index-reuse work) so
//! pinned rows render with full card content, not just the ID.

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::instrument;

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
/// row beyond [`MAX_PINNED`].
#[instrument(skip(conn))]
pub fn add(conn: &Connection, entry_id: &str) -> Result<()> {
    let now = unix_now();
    conn.execute(
        "INSERT OR IGNORE INTO pinned(entry_id, pinned_at, position) VALUES (?1, ?2, ?2)",
        params![entry_id, now],
    )?;
    evict_beyond_cap(conn)?;
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
        let examples_json: String = row.get(7)?;
        let examples: Vec<Example> = serde_json::from_str(&examples_json).unwrap_or_default();
        Ok(PinnedHit {
            id: row.get(0)?,
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

/// `true` when `entry_id` is currently pinned.
#[instrument(skip(conn))]
pub fn is_pinned(conn: &Connection, entry_id: &str) -> Result<bool> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pinned WHERE entry_id = ?1",
            params![entry_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(n > 0)
}

fn evict_beyond_cap(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM pinned WHERE entry_id NOT IN ( \
           SELECT entry_id FROM pinned ORDER BY pinned_at DESC LIMIT ?1 \
         )",
        params![MAX_PINNED as i64],
    )?;
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
}
