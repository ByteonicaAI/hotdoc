/// ponytail: the canonical stopword list mirrors search-ranking-redesign
/// §5.2 — kept verbatim so the spike-doc, the test fixtures, and
/// production packs all describe the same `to/from/with/...` filter.
/// Adding a word here is a *behavior* change visible to every query.
pub const STOPWORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "and", "or", "with", "from", "is", "be", "by",
    "at", "as", "into",
];

/// Structurally-typed query. Splitting tool/intent/option/stopword at
/// parse time means the score function works on tokens, not strings,
/// and test assertions can poke the classification directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedQuery {
    pub raw: String,
    pub tool: Option<String>,
    pub intents: Vec<String>,
    pub options: Vec<String>,
    pub stopwords: Vec<String>,
}

fn is_stopword(tok: &str) -> bool {
    STOPWORDS.contains(&tok)
}

/// ponytail: a single tool is taken from the **first** pack-id match;
/// a second `git` later in the query degrades to an intent. Spike §3
/// rule — keeps `reset git` and `git reset` parsing identically.
pub fn parse_query(raw: &str, pack_ids: &[String]) -> ParsedQuery {
    let tokens: Vec<String> = raw
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_ascii_lowercase())
        .collect();

    let mut tool: Option<String> = None;
    let mut intents: Vec<String> = Vec::new();
    let mut options: Vec<String> = Vec::new();
    let mut stopwords: Vec<String> = Vec::new();

    for tok in tokens {
        if tool.is_none() && pack_ids.iter().any(|p| p == &tok) {
            tool = Some(tok);
            continue;
        }
        if is_stopword(&tok) {
            stopwords.push(tok);
            continue;
        }
        // ponytail: option tokens must keep their leading dash, so the
        // tokenizer splits on whitespace and everything that's not
        // alphanumeric OR `-`. The spike's "split on non-alphanumeric"
        // would flatten `--since` to `since` and break option detection.
        if tok.starts_with('-') && tok.len() >= 2 {
            options.push(tok);
            continue;
        }
        intents.push(tok);
    }

    ParsedQuery {
        raw: raw.to_string(),
        tool,
        intents,
        options,
        stopwords,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_ids() -> Vec<String> {
        vec![
            "git".into(),
            "docker".into(),
            "nginx".into(),
            "kubectl".into(),
            "jq".into(),
        ]
    }

    #[test]
    fn parses_tool_intent() {
        let q = parse_query("git reset", &pack_ids());
        assert_eq!(q.tool.as_deref(), Some("git"));
        assert_eq!(q.intents, vec!["reset"]);
        assert!(q.options.is_empty());
        assert!(q.stopwords.is_empty());
    }

    #[test]
    fn order_does_not_matter_for_tool_and_intent() {
        let a = parse_query("git reset", &pack_ids());
        let b = parse_query("reset git", &pack_ids());
        assert_eq!(a.tool, b.tool);
        // Both shapes collapse to (tool=git, intents=[reset]).
        assert_eq!(a.intents, vec!["reset"]);
        assert_eq!(b.intents, vec!["reset"]);
    }

    #[test]
    fn stopwords_are_filtered() {
        let q = parse_query("nginx redirect http to https", &pack_ids());
        // nginx is in pack_ids → tool. `to` is a stopword. Everything else
        // is intent.
        assert_eq!(q.tool.as_deref(), Some("nginx"));
        assert_eq!(q.intents, vec!["redirect", "http", "https"]);
        assert_eq!(q.stopwords, vec!["to"]);
    }

    #[test]
    fn options_are_recognized() {
        let q = parse_query("docker logs --since", &pack_ids());
        assert_eq!(q.tool.as_deref(), Some("docker"));
        assert_eq!(q.intents, vec!["logs"]);
        assert_eq!(q.options, vec!["--since"]);
    }

    #[test]
    fn unknown_tool_falls_back_to_intent() {
        // `terraform` is not in pack_ids for this test; treat as intent.
        let q = parse_query("terraform plan", &["git".into()]);
        assert_eq!(q.tool, None);
        assert_eq!(q.intents, vec!["terraform", "plan"]);
    }

    #[test]
    fn second_tool_match_becomes_intent() {
        // `docker` then `kubectl`. First wins as tool; second as intent.
        let q = parse_query("docker kubectl logs", &pack_ids());
        assert_eq!(q.tool.as_deref(), Some("docker"));
        assert_eq!(q.intents, vec!["kubectl", "logs"]);
    }
}
