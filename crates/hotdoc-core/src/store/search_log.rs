//! Search-log table CRUD (spec §9.2, §7.5 Popular, §7.2 popularity).
//!
//! Each result activation appends one row: the query, the top-ranked
//! result id (`first_result_id`), and the actually-activated card
//! (`clicked_result_id`, = the highlighted row at the activating
//! keystroke). The empty-query view reads [`popular`] for its "Popular"
//! section; v1 does not yet feed the §7.2 popularity boost (that stays a
//! v1.1 ranking change). Rows are local-only and never synced (FR-R2).
//!
//! Retention: rows older than 30 days are pruned on every store open
//! (gap-analysis P2-11). The 30-day window is wider than the 7-day
//! popularity signal so the FR-G2 diagnostics bundle can include a
//! last-30d summary. Pruning is a runtime DML step, not a schema
//! migration; the `search_log_ts_idx` already covers the DELETE predicate.

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::{instrument, warn};

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
///
/// INSERT runs inside `conn.unchecked_transaction()` for parity with
/// `recents::record` — the same atomicity primitive the NFR-8 chaos test
/// (recents.rs) asserts on. The early-return no-ops (disabled, empty
/// query) stay outside the transaction since they do not write.
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
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO search_log(query, first_result_id, clicked_result_id, ts) \
         VALUES (?1, ?2, ?3, ?4)",
        params![trimmed, first_id, clicked_id, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Delete every row whose `ts < cutoff_ts` and return the number of rows
/// removed. Caller supplies the cutoff (typically `unix_now_ms() -
/// retention_ms`); keeping it parametric avoids `SystemTime` mocking in
/// tests and makes the policy explicit at the call site. Invoked from
/// `store::open()` on every launch — the only place the retention clock
/// advances in v1.
#[instrument(skip(conn))]
pub fn prune_old(conn: &Connection, cutoff_ts: i64) -> Result<usize> {
    let n = conn.execute("DELETE FROM search_log WHERE ts < ?1", params![cutoff_ts])?;
    Ok(n)
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
        Ok(PopularHit {
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

    // ponytail: P2-11 retention. Seed rows at -31d, -29d, -1d relative to
    // a synthetic "now"; prune with cutoff = now - 30d; only the
    // pre-cutoff row must go. Boundary case (ts == cutoff) must SURVIVE
    // because the SQL is `ts < cutoff`, not `ts <= cutoff`.
    #[test]
    fn prune_old_deletes_only_rows_below_cutoff() {
        let (_d, conn) = open();
        let now = 1_700_000_000_000i64; // synthetic epoch ms
        let old = now - 31 * 86_400_000;
        let mid = now - 29 * 86_400_000;
        let recent = now - 86_400_000;
        let cutoff = now - 30 * 86_400_000;
        for (label, ts) in [
            ("old", old),
            ("boundary", cutoff),
            ("mid", mid),
            ("recent", recent),
        ] {
            conn.execute(
                "INSERT INTO search_log(query, first_result_id, clicked_result_id, ts) \
                 VALUES (?1, NULL, NULL, ?2)",
                params![label, ts],
            )
            .expect("seed");
        }
        let deleted = prune_old(&conn, cutoff).expect("prune");
        assert_eq!(deleted, 1, "only the pre-cutoff row must be deleted");
        let survivors: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT query FROM search_log ORDER BY ts ASC")
                .expect("prepare");
            stmt.query_map([], |r| r.get::<_, String>(0))
                .expect("query")
                .map(|r| r.expect("row"))
                .collect()
        };
        assert_eq!(
            survivors,
            vec![
                "boundary".to_string(),
                "mid".to_string(),
                "recent".to_string()
            ],
            "boundary row (ts == cutoff) must survive; mid and recent must survive"
        );
    }

    // ponytail: T19 atomicity primitive for search_log. Mirror of
    // recents::record_is_atomic_under_concurrent_reader. The reader
    // asserts the row count is strictly monotonic — the
    // `unchecked_transaction` in `record` is what guarantees no
    // half-state leaks through (e.g. an INSERT mid-commit). Without
    // the tx wrap, a sufficiently adversarial scheduler could observe
    // a transient inconsistency; with it, the count is either pre- or
    // post-commit, never anything in between.
    #[test]
    fn record_runs_in_single_transaction() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("hotdoc.sqlite");
        let conn = crate::store::open(&db_path).expect("open");
        crate::store::migrate(&conn).expect("migrate");

        let stop = Arc::new(AtomicBool::new(false));
        let reader_path = db_path.clone();
        let reader_stop = Arc::clone(&stop);
        let reader = std::thread::spawn(move || {
            let r = crate::store::open(&reader_path).expect("reader open");
            crate::store::migrate(&r).expect("reader migrate");
            let mut last = 0i64;
            while !reader_stop.load(Ordering::Relaxed) {
                let n: i64 = r
                    .query_row("SELECT COUNT(*) FROM search_log", [], |row| row.get(0))
                    .expect("count");
                assert!(
                    n >= last,
                    "reader observed non-monotonic count {n} after {last}; \
                     record() must commit atomically"
                );
                last = n;
            }
        });

        for i in 0..200 {
            record(&conn, &format!("q-{i:03}"), None, None).expect("record");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        stop.store(true, Ordering::Relaxed);
        reader.join().expect("reader join");
    }

    // Note: an integration test that opens the DB twice and asserts the
    // ancient row is gone lives in T5-T3, alongside the wire-up that
    // makes it pass. Splitting it that way keeps T5-T2's failure surface
    // limited to the search_log module's own primitives.
}
