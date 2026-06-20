use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use hotdoc_core::cli;
use hotdoc_core::golden;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut kv: HashMap<String, String> = HashMap::new();
    let positional: Vec<String> = args
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            if i == 0 || a.starts_with("--") {
                None
            } else {
                Some(a.clone())
            }
        })
        .collect();
    cli::parse_kv(&args[1..], &mut kv);

    let mut exit_code = 0u8;
    match positional.first().map(String::as_str) {
        Some("index") => {
            let packs = kv
                .get("packs")
                .map(PathBuf::from)
                .unwrap_or_else(cli::default_packs_dir);
            let out = kv
                .get("out")
                .map(PathBuf::from)
                .unwrap_or_else(cli::default_index_dir);
            if let Err(e) = cli::cmd_index(&packs, &out) {
                eprintln!("index: {e:#}");
                exit_code = 1;
            }
        }
        Some("query") => {
            let query = match positional.get(1) {
                Some(q) => q.clone(),
                None => {
                    eprintln!("usage: hotdoc-cli query <text> [--limit N] [--index PATH]");
                    return ExitCode::from(2);
                }
            };
            let limit: usize = kv.get("limit").and_then(|s| s.parse().ok()).unwrap_or(8);
            let index = kv
                .get("index")
                .map(PathBuf::from)
                .unwrap_or_else(cli::default_index_dir);
            if let Err(e) = cli::cmd_query(&query, &index, limit) {
                eprintln!("query: {e:#}");
                exit_code = 1;
            }
        }
        Some("copy") => {
            let query = match positional.get(1) {
                Some(q) => q.clone(),
                None => {
                    eprintln!("usage: hotdoc-cli copy <text> [--index PATH]");
                    return ExitCode::from(2);
                }
            };
            let index = kv
                .get("index")
                .map(PathBuf::from)
                .unwrap_or_else(cli::default_index_dir);
            if let Err(e) = cli::cmd_copy(&query, &index) {
                eprintln!("copy: {e:#}");
                exit_code = 1;
            }
        }
        Some("bench") => {
            let golden_path = kv
                .get("queries")
                .map(PathBuf::from)
                .unwrap_or_else(golden::default_golden_path);
            let index = kv
                .get("index")
                .map(PathBuf::from)
                .unwrap_or_else(cli::default_index_dir);
            if let Err(e) = golden::cmd_bench(&golden_path, &index) {
                eprintln!("bench: {e:#}");
                exit_code = 1;
            }
        }
        _ => usage(),
    }
    ExitCode::from(exit_code)
}

fn usage() {
    eprintln!(
        "hotdoc-cli: index | query <text> | copy <text> | bench\n  \
         index [--packs PATH] [--out PATH]\n  \
         query <text> [--limit N] [--index PATH]\n  \
         copy  <text> [--index PATH]\n  \
         bench [--queries PATH] [--index PATH]"
    );
}
