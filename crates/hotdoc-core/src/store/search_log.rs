//! Search-log table CRUD (spec §9.2, §7.5 Popular, §7.2 popularity).
//!
//! Each result activation appends one row: the query, the top-ranked
//! result id (`first_result_id`), and the actually-activated card
//! (`clicked_result_id`, = the highlighted row at the activating
//! keystroke). The empty-query view reads [`popular`] for its "Popular"
//! section; v1 does not yet feed the §7.2 popularity boost (that stays a
//! v1.1 ranking change). Rows are local-only and never synced (FR-R2).

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::instrument;

use crate::pack::Example;
use crate::store::Result;

/// One row of the Popular section: full card content, same shape as a
/// pinned hit so the frontend renders it with the existing card view.
#[derive(Debug, Clone, Serialize)]
pub struct PopularHit {
    pub id: String,
    pub pack_id: String,
    pub title: String,
    pub syntax: String,
    pub description: String,
    pub source: String,
    pub source_url: Option<String>,
    pub example_code: Option<String>,
}

/// Append one activation to the search log. `first_id` is the top-ranked
/// result for the query; `clicked_id` is the card the user actually
/// activated (equals `first_id` when no arrow navigation occurred, per
/// §9.2). Both are optional so a zero-result activation can still log the
/// query without a result id.
#[instrument(skip(conn))]
pub fn record(
    conn: &Connection,
    query: &str,
    first_id: Option<&str>,
    clicked_id: Option<&str>,
) -> Result<()> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let now = crate::store::time::unix_now_ms();
    conn.execute(
        "INSERT INTO search_log(query, first_result_id, clicked_result_id, ts) \
         VALUES (?1, ?2, ?3, ?4)",
        params![trimmed, first_id, clicked_id, now],
    )?;
    Ok(())
}

/// Top `n` most-activated cards (by activation count, ties broken by most
/// recent activation), joined against `entries` for full content. Rows
/// whose `clicked_result_id` no longer exists in `entries` (e.g. a card
/// removed from a pack) are dropped by the JOIN.
#[instrument(skip(conn))]
pub fn popular(conn: &Connection, n: usize) -> Result<Vec<PopularHit>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.pack_id, e.title, e.syntax, e.description, e.source, \
                e.source_url, e.examples_json \
         FROM search_log s JOIN entries e ON e.id = s.clicked_result_id \
         WHERE s.clicked_result_id IS NOT NULL \
         GROUP BY s.clicked_result_id \
         ORDER BY COUNT(*) DESC, MAX(s.ts) DESC \
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![n as i64], |row| {
        let examples_json: String = row.get(7)?;
        let examples: Vec<Example> = serde_json::from_str(&examples_json).unwrap_or_default();
        Ok(PopularHit {
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
             VALUES (?1, ?1, '1.0.0', 'curated', 'MIT', 0) ON CONFLICT(id) DO NOTHING",
            params![pack],
        )
        .expect("pack");
        conn.execute(
            "INSERT INTO entries(id, pack_id, title, syntax, description, \
                examples_json, tags_json, source) \
             VALUES (?1, ?2, ?3, ?4, 'desc', '[]', '[]', 'curated')",
            params![id, pack, format!("title-{id}"), format!("syntax-{id}")],
        )
        .expect("entry");
    }

    #[test]
    fn record_then_popular_orders_by_count() {
        let (_d, conn) = open();
        seed_entry(&conn, "git-a", "git");
        seed_entry(&conn, "git-b", "git");
        // a activated 3×, b once → a ranks first.
        for _ in 0..3 {
            record(&conn, "stash", Some("git-a"), Some("git-a")).expect("record");
        }
        record(&conn, "branch", Some("git-b"), Some("git-b")).expect("record");
        let pop = popular(&conn, 8).expect("popular");
        assert_eq!(pop.len(), 2);
        assert_eq!(pop[0].id, "git-a");
        assert_eq!(pop[1].id, "git-b");
        assert_eq!(pop[0].syntax, "syntax-git-a");
    }

    #[test]
    fn popular_drops_orphan_clicked_ids() {
        let (_d, conn) = open();
        // Log an activation for an id that has no entries row.
        record(&conn, "ghost", Some("missing"), Some("missing")).expect("record");
        let pop = popular(&conn, 8).expect("popular");
        assert!(pop.is_empty(), "orphan clicked_result_id must not surface");
    }

    #[test]
    fn empty_query_is_noop() {
        let (_d, conn) = open();
        record(&conn, "   ", Some("git-a"), Some("git-a")).expect("record");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_log", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 0);
    }

    #[test]
    fn null_clicked_id_logs_query_but_not_popular() {
        let (_d, conn) = open();
        // Zero-result activation: query logged, no clicked id.
        record(&conn, "nomatch xyzzy", None, None).expect("record");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_log", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 1);
        assert!(popular(&conn, 8).expect("popular").is_empty());
    }
}
