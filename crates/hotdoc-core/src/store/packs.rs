//! Packs table (spec §9.2). Read-only listing via [`list_ids`] in M3;
//! T16 adds [`upsert_all`] so the index pipeline can mirror the on-disk
//! pack set into SQLite after a successful tantivy build. Without this,
//! `pinned::list` (which JOINs `entries`) and the `> <pack_id>` palette
//! filter validation have no real set to check against.

use rusqlite::{params, Connection};
use tracing::instrument;

use crate::pack::Pack;
use crate::store::Result;

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

/// Replace every row in `packs` with the current on-disk pack set.
/// Companion to `store::entries::upsert_all`; both run in the
/// `populate_store` path invoked by the index pipeline (T16). Full
/// replace is correct because the tantivy index is also a full rebuild
/// — no diff state lives in SQLite. Wrapped in a transaction so a
/// mid-build failure can't leave a half-populated table.
#[instrument(skip_all)]
pub fn upsert_all(conn: &Connection, packs: &[Pack]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM packs", [])?;
    let now = unix_now();
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
    tx.commit()?;
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
