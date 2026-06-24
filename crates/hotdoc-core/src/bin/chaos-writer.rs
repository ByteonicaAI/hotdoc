/// Chaos-writer binary: writes to a SQLite DB in a tight loop until killed.
/// Used by the NFR-8 WAL chaos test to simulate a mid-write SIGKILL.
/// Usage: chaos-writer <db_path>
fn main() {
    let db_path = std::env::args()
        .nth(1)
        .expect("usage: chaos-writer <db_path>");
    let conn = hotdoc_core::store::open(std::path::Path::new(&db_path)).expect("open");
    hotdoc_core::store::migrate(&conn).expect("migrate");
    let mut i: u64 = 0;
    loop {
        let query = format!("chaos-query-{i}");
        let _ = hotdoc_core::store::recents::record(&conn, &query, None);
        i = i.wrapping_add(1);
    }
}
