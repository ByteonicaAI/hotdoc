//! Forward-only SQLite migration runner (PRD §10, FR-I5).
//!
//! Each entry in `STEP_SQL` maps a schema version number to the SQL that
//! advances from the previous version to that version. Steps are applied in
//! order, each in its own transaction. On success the `meta.schema_version`
//! key is bumped atomically within that transaction.
//!
//! Failure returns `StoreError::MigrationFailed { step, message }` — the
//! caller (Tauri `run()`) maps this to the FR-I5 rebuild path: delete the DB
//! and start fresh.

use rusqlite::{params, Connection};

use crate::store::{Result, StoreError};

/// Step definitions: (version, sql). Ordered ascending; each is applied in a
/// single transaction. The v1 baseline encodes the full current DDL so a
/// fresh DB and a migrated-from-v0 DB converge to the same schema.
const STEP_SQL: &[(u32, &str)] = &[
    (1, super::SCHEMA_SQL),
    (2, "ALTER TABLE recents ADD COLUMN copied_syntax TEXT;"),
];

pub fn run(conn: &Connection) -> Result<()> {
    run_steps(conn, STEP_SQL)
}

/// Inner runner — accepts a custom step list so tests can inject failing SQL.
fn run_steps(conn: &Connection, steps: &[(u32, &str)]) -> Result<()> {
    let current = current_version(conn)?;
    for &(step, sql) in steps {
        if step <= current {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)
            .map_err(|e| StoreError::MigrationFailed {
                step,
                message: format!("{e}"),
            })?;
        // Bump version inside the same transaction — atomic with the DDL.
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![step.to_string()],
        )
        .map_err(|e| StoreError::MigrationFailed {
            step,
            message: format!("version bump: {e}"),
        })?;
        tx.commit()?;
        tracing::debug!(step, "migration applied");
    }
    Ok(())
}

/// Read `meta.schema_version` as a u32.
/// Returns 0 for a brand-new DB (meta table absent or empty).
fn current_version(conn: &Connection) -> Result<u32> {
    match conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(v) => Ok(v.parse().unwrap_or_else(|e| {
            tracing::warn!(
                value = %v,
                error = %e,
                "schema_version failed to parse; falling back to 0 (full migration replay)"
            );
            0
        })),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
        Err(e) if is_no_such_table(&e) => Ok(0),
        Err(e) => Err(e.into()),
    }
}

fn is_no_such_table(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(_, Some(msg)) if msg.contains("no such table")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let conn = crate::store::open(&dir.path().join("hotdoc.sqlite")).expect("open");
        (dir, conn)
    }

    #[test]
    fn fresh_db_migrates_to_v1() {
        let (_d, conn) = open_fresh();
        run(&conn).expect("migration must succeed on fresh DB");
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("schema_version row");
        assert_eq!(v, "2", "schema_version must be 2 after all migrations");
        // All 7 spec §9.2 tables must exist.
        for table in &[
            "meta",
            "packs",
            "entries",
            "recents",
            "pinned",
            "search_log",
            "settings",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |r| r.get(0),
                )
                .expect("sqlite_master query");
            assert_eq!(n, 1, "table {table} must exist after v1 migration");
        }
    }

    #[test]
    fn already_migrated_db_is_noop() {
        let (_d, conn) = open_fresh();
        run(&conn).expect("first run");
        run(&conn).expect("second run must be a no-op");
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("schema_version");
        assert_eq!(v, "2");
    }

    #[test]
    fn v1_already_set_skips_step() {
        // Simulate a DB that was already at v1 (legacy code path).
        let (_d, conn) = open_fresh();
        // Apply baseline manually.
        conn.execute_batch(crate::store::SCHEMA_SQL).expect("ddl");
        conn.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', '1')",
            [],
        )
        .expect("seed version");
        // run() skips step 1 (already at v1) but applies step 2 (ALTER TABLE).
        run(&conn).expect("step 2 applies");
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("version");
        assert_eq!(v, "2");
    }

    #[test]
    fn migration_failure_returns_typed_error_no_corruption() {
        let (_d, conn) = open_fresh();
        // Apply all real steps so the DB is at v2.
        run(&conn).expect("v2");
        // Inject a failing v3 step — bad SQL that SQLite will reject.
        const BAD_SQL: &str = "THIS IS INTENTIONALLY BAD SQL FOR TESTING;";
        let alter_sql = "ALTER TABLE recents ADD COLUMN copied_syntax TEXT;";
        let result = run_steps(
            &conn,
            &[(1, crate::store::SCHEMA_SQL), (2, alter_sql), (3, BAD_SQL)],
        );
        assert!(
            matches!(result, Err(StoreError::MigrationFailed { step: 3, .. })),
            "expected MigrationFailed at step 3, got: {result:?}"
        );
        // DB must be fully readable after the failed migration (no corruption).
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .expect("settings must be readable after failed migration");
        assert_eq!(n, 0);
        // schema_version must still be at 2 (the failed step was rolled back).
        let v: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("schema_version after failure");
        assert_eq!(v, "2", "failed step must not bump schema_version");
    }

    #[test]
    fn fresh_and_migrated_dbs_have_identical_schema() {
        // A fresh DB (run migrations from v0) and a DB that had the schema
        // applied manually must have the same table list.
        let (_d, conn_migrated) = open_fresh();
        run(&conn_migrated).expect("migrate");

        let (_d2, conn_fresh) = open_fresh();
        conn_fresh
            .execute_batch(crate::store::SCHEMA_SQL)
            .expect("apply DDL directly");

        let tables_for = |conn: &Connection| -> Vec<String> {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .expect("prepare");
            stmt.query_map([], |r| r.get::<_, String>(0))
                .expect("query")
                .map(|r| r.expect("row"))
                .collect()
        };

        let t_migrated = tables_for(&conn_migrated);
        let t_fresh = tables_for(&conn_fresh);
        assert_eq!(
            t_migrated, t_fresh,
            "migrated-from-v0 and direct-DDL schemas must be identical"
        );
    }
}
