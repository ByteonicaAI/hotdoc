//! Entries table CRUD (spec §9.2). Populated by the index pipeline
//! (`HotdocIndex::build` calls `upsert_all`) so `pinned::list` can JOIN
//! against full card content (FR-P1), and `list_packs` (via the `packs`
//! table) reflects what's actually indexed. Without this, every
//! "pinned card" renders as empty (the JOIN returns nothing).
//!
//! ponytail: T16 closes the audit §1.6 HIGH finding that the indexer
//! wrote only Tantivy and left SQLite empty. The upsert is full-replace
//! per pack because the index is full-replace too (T13 deferral note):
//! cheaper than diffing and the entry set is bounded by the on-disk
//! pack files.

use rusqlite::{params, Connection, Transaction};
use tracing::instrument;

use crate::pack::Pack;
use crate::store::Result;

/// Replace all entries for `pack.id` in the `entries` table with the
/// current pack's entries. Called from the index pipeline after a
/// successful tantivy build.
#[instrument(skip(conn, pack))]
pub fn upsert_pack(conn: &Connection, pack: &Pack) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    // ponytail: delete then insert is correct for a full-replace pipeline
    // (no per-card diffing). Wrap in a transaction so a mid-loop failure
    // doesn't leave a half-populated entries table — T19's atomicity
    // work is the same pattern.
    tx.execute("DELETE FROM entries WHERE pack_id = ?1", params![pack.id])?;
    for entry in &pack.entries {
        let examples_json = serde_json::to_string(&entry.examples).unwrap_or_else(|_| "[]".into());
        let tags_json = serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "INSERT INTO entries( \
                id, pack_id, title, syntax, description, \
                examples_json, tags_json, source, source_url) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(id) DO UPDATE SET \
                pack_id = excluded.pack_id, \
                title = excluded.title, \
                syntax = excluded.syntax, \
                description = excluded.description, \
                examples_json = excluded.examples_json, \
                tags_json = excluded.tags_json, \
                source = excluded.source, \
                source_url = excluded.source_url",
            params![
                entry.id,
                pack.id,
                entry.title,
                entry.syntax,
                entry.description,
                examples_json,
                tags_json,
                entry.source.as_str(),
                entry.source_url,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Replace all entries for `packs` in one transaction. Entry point used
/// by `HotdocIndex::build` (T16) and the index resolver (T14).
///
/// For atomic composition into a caller-owned transaction (see
/// `HotdocIndex::populate_store`, M4.5-T6.5), use [`upsert_all_tx`]
/// directly. `upsert_all` here is the standalone wrapper.
#[instrument(skip_all)]
pub fn upsert_all(conn: &Connection, packs: &[Pack]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    upsert_all_tx(&tx, packs)?;
    tx.commit()?;
    Ok(())
}

/// Upsert body extracted from [`upsert_all`] so callers can compose
/// it into their own transaction (the `populate_store` outer-tx
/// path is the only current caller). The function does not commit
/// or roll back — the owning `Transaction` does. `pub(crate)` because
/// the only consumer is `HotdocIndex::populate_store` in the same
/// crate; the standalone [`upsert_all`] is the public surface for
/// tests and other standalone callers.
#[instrument(skip_all)]
pub(crate) fn upsert_all_tx(tx: &Transaction<'_>, packs: &[Pack]) -> Result<()> {
    for pack in packs {
        tx.execute("DELETE FROM entries WHERE pack_id = ?1", params![pack.id])?;
        for entry in &pack.entries {
            let examples_json =
                serde_json::to_string(&entry.examples).unwrap_or_else(|_| "[]".into());
            let tags_json = serde_json::to_string(&entry.tags).unwrap_or_else(|_| "[]".into());
            tx.execute(
                "INSERT INTO entries(\
                    id, pack_id, title, syntax, description, \
                    examples_json, tags_json, source, source_url) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT(id) DO UPDATE SET \
                    pack_id = excluded.pack_id, \
                    title = excluded.title, \
                    syntax = excluded.syntax, \
                    description = excluded.description, \
                    examples_json = excluded.examples_json, \
                    tags_json = excluded.tags_json, \
                    source = excluded.source, \
                    source_url = excluded.source_url",
                params![
                    entry.id,
                    pack.id,
                    entry.title,
                    entry.syntax,
                    entry.description,
                    examples_json,
                    tags_json,
                    entry.source.as_str(),
                    entry.source_url,
                ],
            )?;
        }
    }
    Ok(())
}

// silence dead_code on the From impl — kept for future migrations
fn _example_unused() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{Entry, EntrySource, Example};
    use crate::store::migrate;
    use crate::store::open;

    fn open_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = open(&dir.path().join("hotdoc.sqlite")).expect("open");
        migrate(&conn).expect("migrate");
        (dir, conn)
    }

    fn seed_pack_row(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO packs(id, name, version, source, license, indexed_at) \
             VALUES (?1, ?1, '1.0.0', 'curated', 'MIT', 0) \
             ON CONFLICT(id) DO NOTHING",
            params![id],
        )
        .expect("seed pack row");
    }

    fn make_pack(id: &str, entries: Vec<Entry>) -> Pack {
        Pack {
            id: id.to_string(),
            name: id.to_string(),
            version: "1.0.0".to_string(),
            source: "test".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            entries,
        }
    }

    fn make_entry(id: &str, syntax: &str, desc: &str) -> Entry {
        Entry {
            id: id.to_string(),
            title: format!("title-{id}"),
            syntax: syntax.to_string(),
            description: desc.to_string(),
            examples: vec![Example {
                description: "ex".into(),
                code: format!("code-{id}"),
            }],
            tags: vec!["t1".into()],
            aliases: vec![],
            source: EntrySource::Curated,
            source_url: None,
        }
    }

    #[test]
    fn upsert_all_inserts_every_entry() {
        let (_d, conn) = open_db();
        seed_pack_row(&conn, "git");
        let packs = vec![make_pack(
            "git",
            vec![
                make_entry("git-1", "x", "d"),
                make_entry("git-2", "y", "d2"),
            ],
        )];
        upsert_all(&conn, &packs).expect("upsert");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE pack_id = 'git'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 2);
    }

    #[test]
    fn upsert_all_replaces_stale_entries() {
        let (_d, conn) = open_db();
        seed_pack_row(&conn, "git");
        // first pass: 2 entries
        upsert_all(
            &conn,
            &[make_pack(
                "git",
                vec![
                    make_entry("git-1", "x", "d"),
                    make_entry("git-2", "y", "d2"),
                ],
            )],
        )
        .expect("first");
        // second pass: 1 entry (replaces, doesn't add)
        upsert_all(
            &conn,
            &[make_pack("git", vec![make_entry("git-1", "x", "d")])],
        )
        .expect("second");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE pack_id = 'git'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "stale entries should be replaced, not appended");
    }

    #[test]
    fn upsert_pack_deletes_then_inserts_atomically() {
        let (_d, conn) = open_db();
        seed_pack_row(&conn, "git");
        upsert_pack(
            &conn,
            &make_pack("git", vec![make_entry("git-1", "x", "d")]),
        )
        .expect("first upsert");
        // second upsert with no entries → table entry for 'git' should be gone
        upsert_pack(&conn, &make_pack("git", vec![])).expect("empty upsert");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE pack_id = 'git'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 0, "pack with no entries should clear the table rows");
    }
}
