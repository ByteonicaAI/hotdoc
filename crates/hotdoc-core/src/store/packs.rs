//! Packs table (spec §9.2). Read-only listing via [`list_ids`] in M3;
//! T16 adds [`upsert_all`] so the index pipeline can mirror the on-disk
//! pack set into SQLite after a successful tantivy build. Without this,
//! `pinned::list` (which JOINs `entries`) and the `> <pack_id>` palette
//! filter validation have no real set to check against.

use rusqlite::{params, Connection, Transaction};
use serde::Serialize;
use tracing::instrument;

use crate::pack::Pack;
use crate::store::Result;

#[derive(Serialize, Debug)]
pub struct PackMeta {
    pub id: String,
    pub name: String,
    pub license: String,
    pub homepage: String,
}

#[instrument(skip(conn))]
pub fn list_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM packs ORDER BY id")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[instrument(skip(conn))]
pub fn list_metas(conn: &Connection) -> Result<Vec<PackMeta>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, COALESCE(license,''), COALESCE(homepage,'') \
         FROM packs ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PackMeta {
            id: row.get(0)?,
            name: row.get(1)?,
            license: row.get(2)?,
            homepage: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Replace every row in `packs` with the current on-disk pack set.
/// Companion to `store::entries::upsert_all`; both run in the
/// `populate_store` path invoked by the index pipeline (T16). Full
/// replace is correct because the tantivy index is also a full rebuild
/// — no diff state lives in SQLite. Wrapped in a transaction so a
/// mid-build failure can't leave a half-populated table.
///
/// For atomic composition into a caller-owned transaction (see
/// `HotdocIndex::populate_store`, M4.5-T6.5), use [`upsert_all_tx`]
/// directly. `upsert_all` here is the standalone wrapper.
///
/// ponytail: T15 — full-wipe semantics (`DELETE FROM packs` then
/// re-insert). Intentional for the rebuild path. DO NOT use for
/// partial pack updates — pair with `entries::upsert_all` in the
/// same transaction or you will leave dangling entries whose
/// `pack_id → packs.id` FK fails on the next SELECT. See gap-analysis
/// §M4.5 for the disposition.
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
    tx.execute("DELETE FROM packs", [])?;
    let now = crate::store::time::unix_now_ms();
    for pack in packs {
        tx.execute(
            "INSERT INTO packs(id, name, version, source, license, homepage, indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(id) DO UPDATE SET \
                name = excluded.name, \
                version = excluded.version, \
                source = excluded.source, \
                license = excluded.license, \
                homepage = excluded.homepage, \
                indexed_at = excluded.indexed_at",
            params![
                pack.id,
                pack.name,
                pack.version,
                pack.source,
                pack.license,
                pack.homepage,
                now,
            ],
        )?;
    }
    Ok(())
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

    #[test]
    fn list_ids_returns_inserted_packs_in_order() {
        let (_d, conn) = open();
        conn.execute(
            "INSERT INTO packs(id, name, version, source, license, indexed_at) \
             VALUES ('git', 'Git', '1.0.0', 'curated', 'MIT', 0)",
            [],
        )
        .expect("insert git");
        conn.execute(
            "INSERT INTO packs(id, name, version, source, license, indexed_at) \
             VALUES ('docker', 'Docker', '1.0.0', 'curated', 'MIT', 0)",
            [],
        )
        .expect("insert docker");

        let ids = list_ids(&conn).expect("list_ids");
        assert_eq!(ids, vec!["docker".to_string(), "git".to_string()]);
    }

    #[test]
    fn list_ids_empty_when_table_unpopulated() {
        let (_d, conn) = open();
        let ids = list_ids(&conn).expect("list_ids");
        assert!(ids.is_empty());
    }
}
