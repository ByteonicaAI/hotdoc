use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::index::HotdocIndex;
use crate::pack;

pub fn cmd_index(packs_dir: &Path, out_dir: &Path) -> Result<()> {
    let packs = pack::load_dir(packs_dir)?;
    eprintln!("loaded {} packs from {}", packs.len(), packs_dir.display());
    let idx = HotdocIndex::build(&packs, out_dir)?;
    let total: usize = packs.iter().map(|p| p.entries.len()).sum();
    eprintln!("indexed {} entries at {}", total, out_dir.display());
    let _ = idx;
    Ok(())
}

pub fn cmd_query(raw_query: &str, index_dir: &Path, limit: usize) -> Result<()> {
    let idx = HotdocIndex::open(index_dir)?;
    let hits = idx.search(raw_query, limit)?;
    if hits.is_empty() {
        println!("(no results)");
        return Ok(());
    }
    for h in &hits {
        println!(
            "{}\t{}\t{}\t{:.4}\t{}",
            h.id,
            h.pack_id,
            h.syntax.replace('\t', " "),
            h.score,
            h.source
        );
    }
    Ok(())
}

pub fn cmd_copy(raw_query: &str, index_dir: &Path) -> Result<()> {
    let idx = HotdocIndex::open(index_dir)?;
    let hits = idx.search(raw_query, 1)?;
    let hit = hits.first().context("no results for query")?;
    let syntax = hit.syntax.clone();
    let mut clipboard = arboard::Clipboard::new().context("opening clipboard (no display?)")?;
    clipboard
        .set_text(syntax.clone())
        .context("writing to clipboard")?;
    println!("copied: {}", syntax);
    Ok(())
}

pub fn default_index_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hotdoc")
        .join("index")
}

pub fn default_packs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("packs")
        .join("curate")
}

pub fn parse_kv(args: &[String], map: &mut HashMap<String, String>) {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if let Some(eq) = a.strip_prefix("--") {
            if let Some((k, v)) = eq.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            } else if let Some(v) = iter.next() {
                map.insert(eq.to_string(), v.to_string());
            }
        }
    }
}
