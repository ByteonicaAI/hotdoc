use crate::pack::{Entry, EntrySource};
use crate::ranker::parse_query::STOPWORDS;

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
    /// Significant *command* tokens drawn from the raw syntax: the literal
    /// sub-command words (`compose`, `try`, `restart`) a user would type,
    /// with `<placeholder>` arguments, `-`/`--option` flags, and stopwords
    /// stripped. Unlike `syntax_tokens` (which folds `<unit>` → `unit`),
    /// this is built from the *raw* syntax so placeholders can be told
    /// apart from literal command words. The ranker uses it for
    /// command-specificity: a card with extra, unasked-for command tokens
    /// (e.g. `docker compose logs` vs `docker logs`) is less specific to a
    /// query that omits them. The matched tool token is NOT stripped here —
    /// the ranker excludes it at comparison time against the parsed query.
    pub command_tokens: Vec<String>,
    pub source: EntrySource,
}

/// Lowercase Unicode alphanumeric token split. Preserves order and
/// duplicates (phrase ordering cares about position, membership tests
/// ignore duplicates). Splits on any non-alphanumeric Unicode scalar so
/// accented terms (`café`) survive intact.
fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Extract the *command-path prefix* of a raw syntax string: the literal
/// sub-command words that come BEFORE the first argument. Concretely, walk
/// the first syntax variant left-to-right and stop at the first word that
/// is a `<placeholder>` or a `-`/`--flag`; everything up to there is the
/// command path (`docker compose logs`, `systemctl try-restart`,
/// `git reset`). Surviving words are tokenized like every other field
/// (hyphen-split, lowercased, punctuation-stripped) so `reload-or-restart`
/// becomes `[reload, restart]`; stopwords (`or`) are dropped.
///
/// Built from RAW syntax — `syntax_tokens` cannot be reused because it has
/// already flattened `<unit>` to `unit`, erasing the placeholder marker.
///
/// The prefix rule is deliberate: bare OPERANDS that some curated cards
/// write without angle brackets (`git reset --hard HEAD`, `tmux
/// new-session -s name`) sit after a flag, so they are excluded and do not
/// masquerade as command identity. This keeps the command-specificity
/// tie-break free of argument noise (see ranker/score.rs sort_ranked).
fn command_tokens_from_syntax(syntax: &str) -> Vec<String> {
    // Comma-separated alternatives share a command path; the first variant
    // is representative.
    let first_variant = syntax.split(',').next().unwrap_or(syntax);
    let mut out = Vec::new();
    for word in first_variant.split_whitespace() {
        if word.starts_with('<') || word.starts_with('-') {
            break;
        }
        for t in tokenize(word) {
            if !STOPWORDS.contains(&t.as_str()) {
                out.push(t);
            }
        }
    }
    out
}

/// Derive a `NormalizedEntry` from a `pack::Entry`. Pure: no I/O, no
/// dependencies. Called once per entry at index build time (Phase B) and
/// once per test fixture in `tests.rs`.
pub fn normalize_entry(pack_id: &str, entry: &Entry) -> NormalizedEntry {
    let title_tokens = tokenize(&entry.title);
    let syntax_tokens = tokenize(&entry.syntax);
    let command_tokens = command_tokens_from_syntax(&entry.syntax);
    let description_tokens = tokenize(&entry.description);
    let tag_tokens: Vec<String> = entry
        .tags
        .iter()
        .map(|t| t.to_lowercase())
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
        command_tokens,
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

    #[test]
    fn tokenize_preserves_unicode_alphanumerics() {
        assert_eq!(tokenize("café RÉSUMÉ"), vec!["café", "résumé"]);
        assert_eq!(tokenize("naïve_DÉJÀ"), vec!["naïve", "déjà"]);
    }

    #[test]
    fn command_tokens_are_the_command_path_prefix() {
        // Sub-command words before the first flag/placeholder are kept...
        assert_eq!(
            command_tokens_from_syntax("docker compose logs -f <service>"),
            vec!["docker", "compose", "logs"]
        );
        assert_eq!(
            command_tokens_from_syntax("systemctl try-restart <unit>"),
            vec!["systemctl", "try", "restart"]
        );
        // ...while bare OPERANDS that sit after a flag are excluded, so they
        // cannot masquerade as command identity (the git/tmux tie noise).
        assert_eq!(
            command_tokens_from_syntax("git reset --hard HEAD"),
            vec!["git", "reset"]
        );
        assert_eq!(
            command_tokens_from_syntax("git reset --soft HEAD~1"),
            vec!["git", "reset"]
        );
        // Multi-variant (comma) syntaxes use the first variant's path.
        assert_eq!(
            command_tokens_from_syntax("tmux new-session -s name, tmux new-session -d -s name"),
            vec!["tmux", "new", "session"]
        );
        // Stopwords inside a hyphenated command are dropped.
        assert_eq!(
            command_tokens_from_syntax("systemctl reload-or-restart <unit>"),
            vec!["systemctl", "reload", "restart"]
        );
    }
}
