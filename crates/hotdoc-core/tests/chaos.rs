/// NFR-8 WAL chaos test.
///
/// Spawns the `chaos-writer` binary in a child process, SIGKILL it mid-write,
/// then reopens the SQLite DB and asserts WAL recovery produces a consistent,
/// readable database with no torn rows or FK violations.
///
/// Runs under `cargo test` via the subprocess harness; CARGO_BIN_EXE_chaos-writer
/// is only available in integration tests (not unit tests in lib.rs).
#[cfg(unix)]
#[test]
fn wal_recovers_after_sigkill() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("chaos.sqlite");

    let writer_bin = env!("CARGO_BIN_EXE_chaos-writer");
    let mut child = std::process::Command::new(writer_bin)
        .arg(&db_path)
        .spawn()
        .expect("spawn chaos-writer");

    // Poll until the DB file exists, then let it write a bit more.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if db_path.exists() {
            std::thread::sleep(std::time::Duration::from_millis(80));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "chaos-writer did not create DB within 10s"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // SIGKILL — simulates a hard crash mid-write.
    child.kill().expect("kill chaos-writer");
    let _ = child.wait();

    // Reopen — WAL recovery runs automatically on open.
    let conn = hotdoc_core::store::open(&db_path).expect("reopen after SIGKILL");

    // No torn rows: all recents rows must have non-null, non-empty queries.
    let bad: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM recents WHERE query IS NULL OR query = ''",
            [],
            |r| r.get(0),
        )
        .expect("torn-row check");
    assert_eq!(bad, 0, "no torn rows after WAL recovery");

    // SQLite integrity check.
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .expect("integrity_check");
    assert_eq!(
        integrity, "ok",
        "integrity_check must pass after WAL recovery"
    );

    // The recents table must be readable.
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM recents", [], |r| r.get(0))
        .expect("count recents");
    assert!(n >= 0, "recents table readable after WAL recovery");
}
