//! Settings table CRUD (spec FR-X2). Lightweight key/value store; values
//! are stored as TEXT and parsed by the caller (light/dark/system theme,
//! "true"/"false" for recents_enabled + autostart, etc.).

use rusqlite::{params, Connection};
use std::collections::HashMap;
use tracing::instrument;

use crate::store::Result;

#[instrument(skip(conn))]
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    let v: Option<String> = match conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.into()),
    };
    Ok(v)
}

#[instrument(skip(conn, value))]
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[instrument(skip(conn))]
pub fn all(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = HashMap::new();
    for r in rows {
        let (k, v) = r?;
        out.insert(k, v);
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
    fn get_returns_none_for_missing_key() {
        let (_d, conn) = open();
        assert!(get(&conn, "missing").expect("get").is_none());
    }

    #[test]
    fn set_then_get_round_trip() {
        let (_d, conn) = open();
        set(&conn, "hotkey", "Ctrl+Shift+K").expect("set");
        assert_eq!(
            get(&conn, "hotkey").expect("get"),
            Some("Ctrl+Shift+K".to_string())
        );
    }

    #[test]
    fn set_overwrites_existing() {
        let (_d, conn) = open();
        set(&conn, "theme", "light").expect("set");
        set(&conn, "theme", "dark").expect("set");
        assert_eq!(get(&conn, "theme").expect("get"), Some("dark".to_string()));
    }

    #[test]
    fn all_returns_every_row() {
        let (_d, conn) = open();
        set(&conn, "hotkey", "Ctrl+Shift+K").expect("set");
        set(&conn, "theme", "dark").expect("set");
        set(&conn, "recents_enabled", "true").expect("set");
        let m = all(&conn).expect("all");
        assert_eq!(m.get("hotkey").map(String::as_str), Some("Ctrl+Shift+K"));
        assert_eq!(m.get("theme").map(String::as_str), Some("dark"));
        assert_eq!(m.get("recents_enabled").map(String::as_str), Some("true"));
        assert_eq!(m.len(), 3);
    }

    // ponytail: T19 (audit §3.4 error visibility). Pre-T19 the
    // `.ok()` swallowed any DB error and returned `None` — settings
    // reads became silent no-ops under DB corruption instead of
    // surfacing a propagated `StoreError::Db`. Drop the `settings`
    // table to force a non-`QueryReturnedNoRows` error.
    #[test]
    fn get_propagates_db_errors() {
        let (_d, conn) = open();
        // Happy path: missing key returns `None`.
        assert!(get(&conn, "missing").expect("happy path get").is_none());
        conn.execute("DROP TABLE settings", []).expect("drop table");
        let err = get(&conn, "any").expect_err("must propagate a non-QueryReturnedNoRows error");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("no such table")
                || msg.to_lowercase().contains("settings")
                || msg.to_lowercase().contains("query"),
            "expected a schema/query error, got: {msg}"
        );
    }
}
