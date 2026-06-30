use crate::pack::{Entry, EntrySource};

/// ponytail: WS-A.2 — the ranker reads a flat `Vec<String>` of tokens per
/// field, not the raw strings, so tokenization runs once at build/load
/// time rather than per query. Drops punctuation/case and preserves
/// order (phrase matchers like `phrase_order` rely on token position).
#[derive(Debug, Clone)]
pub struct NormalizedEntry {
    pub id: String,
    pub pack_id: String,
    pub title_tokens: Vec<String>,
    pub syntax_tokens: Vec<String>,
    pub description_tokens: Vec<String>,
    pub tag_tokens: Vec<String>,
    pub example_tokens: Vec<String>,
    /// Flat token list across all aliases; boundary between aliases is
    /// intentionally lost (spike §9 decision 2 → v1 substring approach).
    pub aliases: Vec<String>,
    pub source: EntrySource,
}

/// Lowercase ASCII alphanumeric token split. Preserves order and
/// duplicates (phrase ordering cares about position, membership tests
/// ignore duplicates).
fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Derive a `NormalizedEntry` from a `pack::Entry`. Pure: no I/O, no
/// dependencies. Called once per entry at index build time (Phase B) and
/// once per test fixture in `tests.rs`.
pub fn normalize_entry(pack_id: &str, entry: &Entry) -> NormalizedEntry {
    let title_tokens = tokenize(&entry.title);
    let syntax_tokens = tokenize(&entry.syntax);
    let description_tokens = tokenize(&entry.description);
    let tag_tokens: Vec<String> = entry
        .tags
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    let mut example_tokens = Vec::new();
    for ex in &entry.examples {
        example_tokens.extend(tokenize(&ex.description));
        example_tokens.extend(tokenize(&ex.code));
    }

    let mut aliases: Vec<String> = Vec::new();
    for alias in &entry.aliases {
        if alias.is_empty() {
            continue;
        }
        aliases.extend(tokenize(alias));
    }

    NormalizedEntry {
        id: entry.id.clone(),
        pack_id: pack_id.to_string(),
        title_tokens,
        syntax_tokens,
        description_tokens,
        tag_tokens,
        example_tokens,
        aliases,
        source: entry.source.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(syntax: &str, title: &str, tags: &[&str]) -> Entry {
        Entry {
            id: "x".into(),
            title: title.into(),
            syntax: syntax.into(),
            description: String::new(),
            examples: vec![],
            tags: tags.iter().map(|s| s.to_string()).collect(),
            aliases: vec![],
            source: EntrySource::Curated,
            source_url: None,
        }
    }

    #[test]
    fn tokenize_lowercases_and_splits_on_punct() {
        assert_eq!(
            tokenize("git reset --soft HEAD~1"),
            vec!["git", "reset", "soft", "head", "1"]
        );
        assert_eq!(
            tokenize("return 301/302 url, rewrite regex"),
            vec!["return", "301", "302", "url", "rewrite", "regex"]
        );
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn normalize_tokenizes_each_field_independently() {
        let e = entry(
            "git reset --soft HEAD~1",
            "Undo Last Commit",
            &["reset", "Soft", "uncommit"],
        );
        let n = normalize_entry("git", &e);
        assert_eq!(n.id, "x");
        assert_eq!(n.pack_id, "git");
        assert_eq!(n.syntax_tokens, vec!["git", "reset", "soft", "head", "1"]);
        assert_eq!(n.title_tokens, vec!["undo", "last", "commit"]);
        assert_eq!(n.tag_tokens, vec!["reset", "soft", "uncommit"]);
    }

    #[test]
    fn normalize_flattens_aliases_into_single_token_list() {
        let mut e = entry("x", "T", &[]);
        e.aliases = vec!["undo last commit".into(), "uncommit keep changes".into()];
        let n = normalize_entry("git", &e);
        assert_eq!(
            n.aliases,
            vec!["undo", "last", "commit", "uncommit", "keep", "changes"]
        );
    }

    #[test]
    fn normalize_collects_examples() {
        let mut e = entry("x", "T", &[]);
        e.examples = vec![crate::pack::Example {
            description: "show running".into(),
            code: "docker ps -a".into(),
        }];
        let n = normalize_entry("docker", &e);
        assert!(n.example_tokens.contains(&"running".to_string()));
        assert!(n.example_tokens.contains(&"docker".to_string()));
        assert!(n.example_tokens.contains(&"ps".to_string()));
    }
}
