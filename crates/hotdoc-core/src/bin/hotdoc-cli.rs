use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use hotdoc_core::cli;
use hotdoc_core::golden;

fn main() -> ExitCode {
    let _ = hotdoc_core::logging::init();

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
            let limit: usize = match kv.get("limit") {
                Some(s) => match s.parse() {
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("query: --limit {:?} is not a number: {e}", s);
                        return ExitCode::from(2);
                    }
                },
                None => 8,
            };
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
            // ponytail: bare `--adversarial` == true; `--adversarial=<bool>` parses
            // the value so `--adversarial=false` actually disables the filter
            // (presence-only detection silently enabled it before).
            let adversarial = args.iter().any(|a| a == "--adversarial")
                || args
                    .iter()
                    .find_map(|a| a.strip_prefix("--adversarial="))
                    .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                    .unwrap_or(false);
            let golden_path = kv
                .get("queries")
                .map(PathBuf::from)
                .unwrap_or_else(golden::default_golden_path);
            let index = kv
                .get("index")
                .map(PathBuf::from)
                .unwrap_or_else(cli::default_index_dir);
            if let Err(e) = golden::cmd_bench(&golden_path, &index, adversarial) {
                eprintln!("bench: {e:#}");
                exit_code = 1;
            }
        }
        Some("eval") => {
            // ponytail: bare `--adversarial` == true; `--adversarial=<bool>` parses
            // the value so `--adversarial=false` actually disables the filter
            // (mirrors the same logic used in `bench`).
            let adversarial = args.iter().any(|a| a == "--adversarial")
                || args
                    .iter()
                    .find_map(|a| a.strip_prefix("--adversarial="))
                    .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                    .unwrap_or(false);
            let queries = kv.get("queries").map(String::as_str);
            let index = kv.get("index").map(String::as_str);
            if let Err(e) = golden::cmd_eval(queries, index, adversarial) {
                eprintln!("eval: {e:#}");
                exit_code = 1;
            }
        }
        Some("toggle") => {
            if let Err(e) = cli::cmd_toggle() {
                eprintln!(
                    "toggle: {e:#}\n\
                     hint: is the hotdoc server running? Launch the app first, then bind your DE shortcut to `hotdoc-cli toggle`."
                );
                exit_code = 1;
            }
        }
        _ => usage(),
    }
    ExitCode::from(exit_code)
}

fn usage() {
    eprintln!(
        "hotdoc-cli: index | query <text> | copy <text> | bench | eval | toggle\n  \
         index  [--packs PATH] [--out PATH]\n  \
         query  <text> [--limit N] [--index PATH]\n  \
         copy   <text> [--index PATH]\n  \
         bench  [--queries PATH] [--index PATH] [--adversarial]\n  \
         eval   [--queries PATH] [--index PATH] [--adversarial]\n  \
         toggle  send a show/hide signal to the running hotdoc app (bind to your DE shortcut)"
    );
}
