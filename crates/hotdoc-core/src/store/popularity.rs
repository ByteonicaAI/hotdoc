//! Recency-weighted activation counts for the §7.2 popularity ranking term.
//!
//! Formula (spec §7.2, locked 2026-06-22):
//!   `weight = 0.5^(age_days / 7.0)` per activation (7-day half-life).
//!   `raw = Σ weights` per entry; `term = ln(1 + raw) * 0.1`.
//! Orphan `clicked_result_id` not in `entries` are dropped via JOIN guard.

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::store::Result;

/// Build a recency-weighted score map from `search_log`.
///
/// `now_ms` is injected so tests can pass a fixed timestamp.
/// Returns an empty map if `search_log` is empty or all entries are orphaned.
pub fn weighted_counts(conn: &Connection, now_ms: i64) -> Result<HashMap<String, f32>> {
    let mut stmt = conn.prepare(
        "SELECT s.clicked_result_id, s.ts \
         FROM search_log s JOIN entries e ON e.id = s.clicked_result_id \
         WHERE s.clicked_result_id IS NOT NULL",
    )?;
    let rows = stmt.query_map(params![], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    let mut raw: HashMap<String, f32> = HashMap::new();
    for r in rows {
        let (id, ts) = r?;
        let age_ms = (now_ms - ts).max(0) as f64;
        let age_days = age_ms / (1000.0 * 60.0 * 60.0 * 24.0);
        let weight = 0.5f32.powf((age_days / 7.0) as f32);
        *raw.entry(id).or_insert(0.0) += weight;
    }

    let mut out = HashMap::with_capacity(raw.len());
    for (id, r) in raw {
        out.insert(id, (1.0f32 + r).ln() * 0.1);
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

    fn seed_log(conn: &Connection, entry_id: &str, ts_ms: i64) {
        conn.execute(
            "INSERT INTO search_log(query, first_result_id, clicked_result_id, ts) \
             VALUES ('q', ?1, ?1, ?2)",
            params![entry_id, ts_ms],
        )
        .expect("log");
    }

    // Fixed "now" for deterministic age calculations.
    const NOW: i64 = 1_000_000_000_000i64;

    #[test]
    fn fresh_activation_weight_is_one() {
        let (_d, conn) = open();
        seed_entry(&conn, "a", "pack");
        seed_log(&conn, "a", NOW); // age = 0 → weight = 0.5^0 = 1.0
        let m = weighted_counts(&conn, NOW).expect("counts");
        let expected = (1.0f32 + 1.0f32).ln() * 0.1;
        let got = *m.get("a").expect("entry a");
        assert!(
            (got - expected).abs() < 1e-5,
            "fresh activation: expected {expected}, got {got}"
        );
    }

    #[test]
    fn seven_day_old_activation_weight_is_half() {
        let (_d, conn) = open();
        seed_entry(&conn, "a", "pack");
        let seven_days_ms = 7 * 24 * 60 * 60 * 1000i64;
        seed_log(&conn, "a", NOW - seven_days_ms); // age = 7d → weight = 0.5
        let m = weighted_counts(&conn, NOW).expect("counts");
        let expected = (1.0f32 + 0.5f32).ln() * 0.1;
        let got = *m.get("a").expect("entry a");
        assert!(
            (got - expected).abs() < 1e-4,
            "7d-old: expected {expected}, got {got}"
        );
    }

    #[test]
    fn fourteen_day_old_activation_weight_is_quarter() {
        let (_d, conn) = open();
        seed_entry(&conn, "a", "pack");
        let fourteen_days_ms = 14 * 24 * 60 * 60 * 1000i64;
        seed_log(&conn, "a", NOW - fourteen_days_ms); // age = 14d → weight = 0.25
        let m = weighted_counts(&conn, NOW).expect("counts");
        let expected = (1.0f32 + 0.25f32).ln() * 0.1;
        let got = *m.get("a").expect("entry a");
        assert!(
            (got - expected).abs() < 1e-4,
            "14d-old: expected {expected}, got {got}"
        );
    }

    #[test]
    fn empty_log_returns_empty_map() {
        let (_d, conn) = open();
        let m = weighted_counts(&conn, NOW).expect("counts");
        assert!(m.is_empty(), "empty log must yield empty map");
    }

    #[test]
    fn orphan_clicked_id_is_dropped() {
        let (_d, conn) = open();
        seed_log(&conn, "ghost-entry", NOW); // no entries row → JOIN drops it
        let m = weighted_counts(&conn, NOW).expect("counts");
        assert!(m.is_empty(), "orphan must be dropped");
    }

    #[test]
    fn multiple_activations_accumulate() {
        let (_d, conn) = open();
        seed_entry(&conn, "a", "pack");
        for _ in 0..3 {
            seed_log(&conn, "a", NOW); // 3 fresh → raw = 3.0
        }
        let m = weighted_counts(&conn, NOW).expect("counts");
        let expected = (1.0f32 + 3.0f32).ln() * 0.1;
        let got = *m.get("a").expect("entry a");
        assert!(
            (got - expected).abs() < 1e-5,
            "3 fresh: expected {expected}, got {got}"
        );
    }

    #[test]
    fn zero_bonus_for_entry_with_no_activations() {
        let (_d, conn) = open();
        seed_entry(&conn, "a", "pack");
        // No search_log rows for "a" → must not appear in map → bonus = 0.
        let m = weighted_counts(&conn, NOW).expect("counts");
        assert_eq!(
            m.get("a"),
            None,
            "un-activated entry must not appear in map"
        );
    }
}
