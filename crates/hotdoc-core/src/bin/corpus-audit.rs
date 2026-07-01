//! corpus-audit — read-only curation audit for pack files.
//!
//! Scans packs/curate/*.json and reports three classes of curation gaps:
//!   1. Exact syntax duplicates (exact-syntax-dup)
//!   2. Weak metadata: no aliases (aggregate) / thin tags (per-entry)
//!   3. Title near-duplicates via Jaccard similarity on title tokens
//!
//! Output drives Tasks 4.2 (dup resolution) and 4.3 (alias curation).
//! NEVER touches the Tantivy/SQLite index — pure pack-file analysis.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use serde::Serialize;

use hotdoc_core::cli;
use hotdoc_core::pack::{self, Entry};
use hotdoc_core::ranker::normalize_entry;

// ─── Finding ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct Finding {
    pack_id: String,
    entry_id: String,
    issue: String,
    detail: String,
}

// ─── Pure check functions ─────────────────────────────────────────────────────

/// Group entries by exact `syntax` string; emit one `Finding` per group with
/// more than one member. High precision: flags only true textual duplicates.
/// Findings are sorted deterministically by (pack_id, entry_id).
fn exact_syntax_dups(entries: &[(&str, &Entry)]) -> Vec<Finding> {
    let mut by_syntax: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (pack_id, entry) in entries {
        by_syntax
            .entry(entry.syntax.clone())
            .or_default()
            .push((pack_id.to_string(), entry.id.clone()));
    }

    let mut findings: Vec<Finding> = by_syntax
        .into_iter()
        .filter(|(_, group)| group.len() > 1)
        .map(|(syntax, mut group)| {
            group.sort_unstable();
            let ids = group
                .iter()
                .map(|(p, e)| format!("{p}/{e}"))
                .collect::<Vec<_>>()
                .join(", ");
            Finding {
                pack_id: group[0].0.clone(),
                entry_id: group[0].1.clone(),
                issue: "exact-syntax-dup".to_string(),
                detail: format!("{} entries share syntax {:?}: {}", group.len(), syntax, ids),
            }
        })
        .collect();

    findings.sort_by(|a, b| (&a.pack_id, &a.entry_id).cmp(&(&b.pack_id, &b.entry_id)));
    findings
}

/// Flag entries with empty `aliases` (one aggregate Finding) and/or
/// `tags.len() < min_tags` (one per-entry Finding each).
///
/// The no-alias case is aggregated because currently ALL 731 entries in the
/// corpus have empty aliases — emitting 731 individual rows would obscure the
/// signal. One headline suffices; the alias-curation task (4.3) will act on it.
fn weak_metadata(entries: &[(&str, &Entry)], min_tags: usize) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    // ── Aggregate no-alias headline ────────────────────────────────────────
    let no_alias_count = entries.iter().filter(|(_, e)| e.aliases.is_empty()).count();
    if no_alias_count > 0 {
        findings.push(Finding {
            pack_id: "*".to_string(),
            entry_id: "*".to_string(),
            issue: "no-aliases".to_string(),
            detail: format!("{}/{} cards have no aliases", no_alias_count, entries.len()),
        });
    }

    // ── Per-entry thin-tag findings ────────────────────────────────────────
    let mut thin: Vec<Finding> = entries
        .iter()
        .filter(|(_, e)| e.tags.len() < min_tags)
        .map(|(pack_id, entry)| Finding {
            pack_id: pack_id.to_string(),
            entry_id: entry.id.clone(),
            issue: "thin-tags".to_string(),
            detail: format!("{} tag(s), need >= {}", entry.tags.len(), min_tags),
        })
        .collect();
    thin.sort_by(|a, b| (&a.pack_id, &a.entry_id).cmp(&(&b.pack_id, &b.entry_id)));
    findings.extend(thin);
    findings
}

/// Return a canonical ordered pair (min, max) for deduplicating symmetric pairs.
fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Jaccard similarity between two token sets: |A ∩ B| / |A ∪ B|.
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let inter = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    inter as f64 / union as f64
}

/// Flag pairs of entries whose title-token Jaccard similarity ≥ `threshold`.
///
/// Skips:
///   - self-pairs (implicit — outer loop starts at `j = i + 1`)
///   - pairs already flagged as exact-syntax dups (passed in `syntax_dup_pairs`)
///
/// Reuses `normalize_entry` from the existing ranker so tokenisation is
/// consistent with search rather than adding a second tokenizer.
fn title_near_dups(
    entries: &[(&str, &Entry)],
    threshold: f64,
    syntax_dup_pairs: &HashSet<(String, String)>,
) -> Vec<Finding> {
    // Build (pack_id, entry_id, title_token_set) for every entry once.
    let normalized: Vec<(String, String, HashSet<String>)> = entries
        .iter()
        .map(|(pack_id, entry)| {
            let norm = normalize_entry(pack_id, entry);
            let title_set: HashSet<String> = norm.title_tokens.into_iter().collect();
            (pack_id.to_string(), entry.id.clone(), title_set)
        })
        .collect();

    let mut findings: Vec<Finding> = Vec::new();

    for i in 0..normalized.len() {
        for j in (i + 1)..normalized.len() {
            let (pack_i, id_i, tokens_i) = &normalized[i];
            let (pack_j, id_j, tokens_j) = &normalized[j];

            // Skip pairs already reported as exact-syntax dups.
            // Keys are pack-qualified ("pack/entry_id") to avoid false skips
            // when two different packs share an entry_id.
            if syntax_dup_pairs.contains(&pair_key(
                &format!("{pack_i}/{id_i}"),
                &format!("{pack_j}/{id_j}"),
            )) {
                continue;
            }

            let score = jaccard(tokens_i, tokens_j);
            if score >= threshold {
                findings.push(Finding {
                    pack_id: pack_i.clone(),
                    entry_id: id_i.clone(),
                    issue: "title-near-dup".to_string(),
                    detail: format!("Jaccard {score:.3} with {pack_j}/{id_j}"),
                });
            }
        }
    }

    findings.sort_by(|a, b| (&a.pack_id, &a.entry_id).cmp(&(&b.pack_id, &b.entry_id)));
    findings
}

// ─── Text report printer ──────────────────────────────────────────────────────

fn print_text_report(
    dup_findings: &[Finding],
    meta_findings: &[Finding],
    near_findings: &[Finding],
    min_tags: usize,
    threshold: f64,
) {
    println!(
        "=== EXACT SYNTAX DUPLICATES ({} groups) ===",
        dup_findings.len()
    );
    if dup_findings.is_empty() {
        println!("  (none)");
    } else {
        println!("{:<20} {:<45} DETAIL", "PACK", "ENTRY-ID");
        for f in dup_findings {
            println!("{:<20} {:<45} {}", f.pack_id, f.entry_id, f.detail);
        }
    }
    println!();

    let no_alias: Vec<&Finding> = meta_findings
        .iter()
        .filter(|f| f.issue == "no-aliases")
        .collect();
    let thin_tag: Vec<&Finding> = meta_findings
        .iter()
        .filter(|f| f.issue == "thin-tags")
        .collect();
    println!("=== WEAK METADATA (min-tags={min_tags}) ===");
    if let Some(f) = no_alias.first() {
        println!("  HEADLINE: {}", f.detail);
    }
    println!("  Thin-tag entries ({}):", thin_tag.len());
    if thin_tag.is_empty() {
        println!("    (none)");
    } else {
        println!("  {:<20} {:<45} DETAIL", "PACK", "ENTRY-ID");
        for f in thin_tag {
            println!("  {:<20} {:<45} {}", f.pack_id, f.entry_id, f.detail);
        }
    }
    println!();

    println!(
        "=== TITLE NEAR-DUPS (threshold={threshold:.2}, {} pairs) ===",
        near_findings.len()
    );
    if near_findings.is_empty() {
        println!("  (none)");
    } else {
        println!("{:<20} {:<45} DETAIL", "PACK", "ENTRY-ID");
        for f in near_findings {
            println!("{:<20} {:<45} {}", f.pack_id, f.entry_id, f.detail);
        }
    }
}

// ─── main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut kv: HashMap<String, String> = HashMap::new();
    cli::parse_kv(&args[1..], &mut kv);

    let packs_dir: PathBuf = kv
        .get("packs")
        .map(PathBuf::from)
        .unwrap_or_else(cli::default_packs_dir);

    let format = kv
        .get("format")
        .map(String::as_str)
        .unwrap_or("text")
        .to_string();
    let min_tags: usize = kv
        .get("min-tags")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_usize);
    let threshold: f64 = kv
        .get("threshold")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.8_f64);

    let report = pack::load_dir(&packs_dir).expect("corpus-audit: failed to open packs directory");

    if !report.failed.is_empty() {
        for (path, _errs) in &report.failed {
            eprintln!("corpus-audit: warning: failed to load {}", path.display());
        }
    }

    if report.loaded.is_empty() {
        eprintln!("corpus-audit: no packs loaded from {}", packs_dir.display());
        // Exit 0 — audit is informational, never a gate.
        return;
    }

    // Flatten into (pack_id, &Entry) pairs for analysis.
    let flat: Vec<(&str, &Entry)> = report
        .loaded
        .iter()
        .flat_map(|pack| pack.entries.iter().map(move |e| (pack.id.as_str(), e)))
        .collect();

    // ── Run checks ────────────────────────────────────────────────────────
    let dup_findings = exact_syntax_dups(&flat);

    // Build the exact-syntax-dup pair set so title_near_dups can skip them.
    // Keys are pack-qualified ("pack/entry_id") to prevent false skips when
    // two different packs happen to share an entry_id.
    let syntax_dup_pairs: HashSet<(String, String)> = {
        let mut by_syntax: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (pack_id, entry) in &flat {
            by_syntax
                .entry(entry.syntax.clone())
                .or_default()
                .push(format!("{pack_id}/{}", entry.id));
        }
        let mut pairs: HashSet<(String, String)> = HashSet::new();
        for group in by_syntax.values() {
            if group.len() > 1 {
                for i in 0..group.len() {
                    for j in (i + 1)..group.len() {
                        pairs.insert(pair_key(&group[i], &group[j]));
                    }
                }
            }
        }
        pairs
    };

    let meta_findings = weak_metadata(&flat, min_tags);
    let near_findings = title_near_dups(&flat, threshold, &syntax_dup_pairs);

    // ── Summary counts (used by both output paths) ────────────────────────
    let total_entries: usize = report.loaded.iter().map(|p| p.entries.len()).sum();
    let no_alias_count = flat.iter().filter(|(_, e)| e.aliases.is_empty()).count();
    let thin_tag_count = flat.iter().filter(|(_, e)| e.tags.len() < min_tags).count();

    // ── Output ────────────────────────────────────────────────────────────
    match format.as_str() {
        "json" => {
            // stdout must contain ONLY valid JSON so `jq`/programmatic consumers
            // can parse it. Route the summary to stderr instead.
            let all: Vec<&Finding> = dup_findings
                .iter()
                .chain(meta_findings.iter())
                .chain(near_findings.iter())
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&all)
                    .expect("corpus-audit: JSON serialization failed")
            );
            eprintln!();
            eprintln!("--- SUMMARY ---");
            eprintln!("Packs loaded             : {}", report.loaded.len());
            eprintln!("Total entries            : {total_entries}");
            eprintln!("Exact-syntax dup groups  : {}", dup_findings.len());
            eprintln!("No-alias entries         : {no_alias_count}/{total_entries}");
            eprintln!("Thin-tag entries         : {thin_tag_count}");
            eprintln!("Title near-dup pairs     : {}", near_findings.len());
        }
        _ => {
            print_text_report(
                &dup_findings,
                &meta_findings,
                &near_findings,
                min_tags,
                threshold,
            );
            println!();
            println!("--- SUMMARY ---");
            println!("Packs loaded             : {}", report.loaded.len());
            println!("Total entries            : {total_entries}");
            println!("Exact-syntax dup groups  : {}", dup_findings.len());
            println!("No-alias entries         : {no_alias_count}/{total_entries}");
            println!("Thin-tag entries         : {thin_tag_count}");
            println!("Title near-dup pairs     : {}", near_findings.len());
        }
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hotdoc_core::pack::{Entry, EntrySource};

    fn make_entry(id: &str, title: &str, syntax: &str, tags: &[&str], aliases: &[&str]) -> Entry {
        Entry {
            id: id.to_string(),
            title: title.to_string(),
            syntax: syntax.to_string(),
            description: String::new(),
            examples: Vec::new(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            source: EntrySource::Curated,
            source_url: None,
        }
    }

    // ── exact_syntax_dups ─────────────────────────────────────────────────

    #[test]
    fn test_exact_syntax_dups_detects_one_group() {
        let e1 = make_entry(
            "curl-head-1",
            "Fetch headers",
            "curl -I <url>",
            &["http"],
            &[],
        );
        let e2 = make_entry(
            "curl-head-2",
            "Inspect headers",
            "curl -I <url>",
            &["http"],
            &[],
        );
        let e3 = make_entry("curl-get", "GET request", "curl <url>", &["http"], &[]);
        let entries: Vec<(&str, &Entry)> = vec![("curl", &e1), ("curl", &e2), ("curl", &e3)];

        let findings = exact_syntax_dups(&entries);

        assert_eq!(
            findings.len(),
            1,
            "expected exactly 1 dup group, got: {findings:?}"
        );
        assert_eq!(findings[0].issue, "exact-syntax-dup");
        assert!(
            findings[0].detail.contains("curl-head-1")
                && findings[0].detail.contains("curl-head-2"),
            "detail should list both entry ids: {:?}",
            findings[0].detail
        );
    }

    #[test]
    fn test_exact_syntax_dups_no_dups() {
        let e1 = make_entry("git-log", "Show log", "git log", &["log"], &[]);
        let e2 = make_entry("git-status", "Show status", "git status", &["status"], &[]);
        let entries: Vec<(&str, &Entry)> = vec![("git", &e1), ("git", &e2)];

        let findings = exact_syntax_dups(&entries);

        assert!(
            findings.is_empty(),
            "expected no dups for distinct syntax, got: {findings:?}"
        );
    }

    #[test]
    fn test_exact_syntax_dups_cross_pack() {
        // Two entries in different packs sharing exact syntax.
        let e1 = make_entry("foo-ls", "List", "ls -la", &[], &[]);
        let e2 = make_entry("bar-ls", "List all", "ls -la", &[], &[]);
        let entries: Vec<(&str, &Entry)> = vec![("foo", &e1), ("bar", &e2)];

        let findings = exact_syntax_dups(&entries);

        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].detail.contains("foo/foo-ls") && findings[0].detail.contains("bar/bar-ls")
        );
    }

    // ── weak_metadata ─────────────────────────────────────────────────────

    #[test]
    fn test_weak_metadata_flags_empty_aliases() {
        // aliases: [] → aggregate no-aliases finding expected.
        let e = make_entry(
            "git-reset",
            "Undo commit",
            "git reset",
            &["reset", "undo"],
            &[],
        );
        let entries: Vec<(&str, &Entry)> = vec![("git", &e)];

        let findings = weak_metadata(&entries, 2);

        let alias_finding = findings.iter().find(|f| f.issue == "no-aliases");
        assert!(
            alias_finding.is_some(),
            "expected a no-aliases finding, got: {findings:?}"
        );
    }

    #[test]
    fn test_weak_metadata_flags_thin_tags() {
        // 1 tag < min_tags=2 → thin-tags finding.
        let e = make_entry("git-reset", "Undo commit", "git reset", &["reset"], &[]);
        let entries: Vec<(&str, &Entry)> = vec![("git", &e)];

        let findings = weak_metadata(&entries, 2);

        let thin = findings.iter().find(|f| f.issue == "thin-tags");
        assert!(
            thin.is_some(),
            "expected a thin-tags finding, got: {findings:?}"
        );
    }

    #[test]
    fn test_weak_metadata_no_alias_finding_when_aliases_present() {
        // Non-empty aliases → should NOT produce a no-aliases finding.
        let e = make_entry(
            "git-reset",
            "Undo commit",
            "git reset",
            &["reset", "undo"],
            &["undo last commit"],
        );
        let entries: Vec<(&str, &Entry)> = vec![("git", &e)];

        let findings = weak_metadata(&entries, 2);

        let alias_finding = findings.iter().find(|f| f.issue == "no-aliases");
        assert!(
            alias_finding.is_none(),
            "should NOT flag no-aliases when aliases are present: {findings:?}"
        );
    }

    #[test]
    fn test_weak_metadata_no_thin_tag_when_sufficient_tags() {
        // 3 tags >= min_tags=2 → no thin-tags finding.
        let e = make_entry(
            "git-log",
            "Log",
            "git log",
            &["log", "history", "commit"],
            &[],
        );
        let entries: Vec<(&str, &Entry)> = vec![("git", &e)];

        let findings = weak_metadata(&entries, 2);

        let thin = findings.iter().find(|f| f.issue == "thin-tags");
        assert!(
            thin.is_none(),
            "should NOT flag thin-tags when tags are sufficient: {findings:?}"
        );
    }

    // ── title_near_dups ───────────────────────────────────────────────────

    #[test]
    fn test_title_near_dups_flags_similar_titles() {
        // "Discard all local changes" vs "Discard all changes":
        // tokens: {discard,all,local,changes} ∩ {discard,all,changes} = 3
        //         {discard,all,local,changes} ∪ {discard,all,changes} = 4
        //         Jaccard = 3/4 = 0.75  →  flagged at threshold=0.75
        let e1 = make_entry(
            "git-checkout-dot",
            "Discard all local changes",
            "git checkout -- .",
            &["undo"],
            &[],
        );
        let e2 = make_entry(
            "git-restore",
            "Discard all changes",
            "git restore .",
            &["undo"],
            &[],
        );
        let entries: Vec<(&str, &Entry)> = vec![("git", &e1), ("git", &e2)];
        let empty_pairs: HashSet<(String, String)> = HashSet::new();

        let findings = title_near_dups(&entries, 0.75, &empty_pairs);

        assert!(
            !findings.is_empty(),
            "expected at least one near-dup finding for similar titles"
        );
        assert!(
            findings.iter().all(|f| f.issue == "title-near-dup"),
            "all findings should have issue=title-near-dup"
        );
    }

    #[test]
    fn test_title_near_dups_skips_exact_syntax_dup_pairs() {
        // Identical titles AND identical syntax → near-dup detection should skip
        // the pair because it was already reported as an exact-syntax dup.
        let e1 = make_entry("e1", "Discard all changes", "git restore .", &[], &[]);
        let e2 = make_entry("e2", "Discard all changes", "git restore .", &[], &[]);
        let entries: Vec<(&str, &Entry)> = vec![("git", &e1), ("git", &e2)];
        let mut dup_pairs: HashSet<(String, String)> = HashSet::new();
        // Keys are pack-qualified to match the production invariant.
        dup_pairs.insert(pair_key("git/e1", "git/e2"));

        let findings = title_near_dups(&entries, 0.5, &dup_pairs);

        assert!(
            findings.is_empty(),
            "should skip pairs already in exact-syntax dups, got: {findings:?}"
        );
    }

    #[test]
    fn test_title_near_dups_no_finding_for_distinct_titles() {
        let e1 = make_entry("git-log", "Show git log", "git log", &[], &[]);
        let e2 = make_entry("docker-ps", "List all containers", "docker ps", &[], &[]);
        let entries: Vec<(&str, &Entry)> = vec![("git", &e1), ("docker", &e2)];
        let empty: HashSet<(String, String)> = HashSet::new();

        let findings = title_near_dups(&entries, 0.8, &empty);

        assert!(
            findings.is_empty(),
            "unrelated titles should not produce near-dup findings, got: {findings:?}"
        );
    }

    #[test]
    fn test_pair_key_is_symmetric() {
        assert_eq!(pair_key("a", "b"), pair_key("b", "a"));
        assert_eq!(pair_key("z", "a"), ("a".to_string(), "z".to_string()));
    }
}
