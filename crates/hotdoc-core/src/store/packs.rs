//! Packs table (spec §9.2). Read-only in M3: the table is created by
//! `migrate()` but is not yet populated (v1.1's persistent-index-reuse
//! work writes into it). T6's command palette uses `list_ids()` to know
//! which `> <pack_id>` filters are valid; in M3 the set is always empty,
//! so unknown pack ids fall through to search.

use rusqlite::Connection;
use tracing::instrument;

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
