//! Shared time helpers for the store layer. Single source of truth for
//! "now" so every timestamp in `pinned_at`, `last_used_at`, `indexed_at`
//! reads identically.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since UNIX epoch. Used by `pinned::add`, `recents::record`,
/// and `packs::upsert_all` for their `*_at` columns. Returns 0 when the
/// system clock is before the epoch — effectively a poison value that
/// surfaces as "very old" in MRU sorts; matches the prior `unwrap_or(0)`
/// behavior at the call sites this helper replaced (T19).
#[inline]
pub fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_now_ms_increases_monotonically() {
        let t0 = unix_now_ms();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let t1 = unix_now_ms();
        assert!(t1 > t0, "expected t1 > t0 (got {t0} then {t1})");
    }
}
