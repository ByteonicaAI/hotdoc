use anyhow::{Context, Result};
use std::fs;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// ponytail: spec FR-G1 + audit §4.1. Single init() entry point, called
// once from src-tauri::run() and from hotdoc-cli's main(). try_init()
// returns Err on a duplicate global subscriber — the bench binaries do
// not call this; they use eprintln!/println! directly to avoid polluting
// the user's log dir on every CI run.
pub fn init() -> Result<()> {
    let log_dir = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("no data dir"))?
        .join("hotdoc")
        .join("logs");
    fs::create_dir_all(&log_dir).context("creating log dir")?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "hotdoc.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = if cfg!(debug_assertions) {
        EnvFilter::builder()
            .parse("hotdoc_core=debug,hotdoc=trace")
            .context("parsing debug filter")?
    } else {
        EnvFilter::builder().parse_lossy("RUST_LOG")
    };

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::Layer::new().with_writer(std::io::stderr).pretty())
        .with(
            fmt::Layer::new()
                .with_writer(file_writer)
                .with_ansi(false)
                .json(),
        );

    registry
        .try_init()
        .map_err(|e| anyhow::anyhow!("init tracing subscriber: {e}"))?;
    // Hold the guard for the lifetime of the program by leaking it —
    // the non-blocking writer's worker thread dies when the guard drops.
    std::mem::forget(_guard);
    Ok(())
}
