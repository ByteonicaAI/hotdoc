use super::*;
use crate::pack::{Entry, EntrySource};
use crate::ranker::normalize::NormalizedEntry;
use crate::ranker::parse_query::ParsedQuery;
use crate::ranker::score::{
    score, sort_ranked, ScoreBreakdown, FIELD_WEIGHT_ALIAS, FIELD_WEIGHT_SYNTAX, FIELD_WEIGHT_TAGS,
    FIELD_WEIGHT_TITLE, FUZZY_RESCUE_CAP,
};
use std::collections::HashSet;

// ponytail: every test in this file builds entries via the inline
// `entry` helper instead of loading real packs — adversarial cases
// need predictable fields, and the real `nginx-redirect` for instance
// doesn't carry `http`/`https` tags. Keep the ranker honest against
// the spike §8 fixtures, not against drifting pack data.

// ponytail: 8 positional args is verbose but keeps every test fixture
// read top-to-bottom as a single declaration. Splitting into a
// builder would obscure the input matrix adversarial cases depend on.
#[allow(clippy::too_many_arguments)]
fn entry(
    id: &str,
    // ponytail: ignored — the *real* pack_id comes from the tuple wrapper
    // at the call site. Keeping the positional arg avoids reshaping every
    // call site for an unused detail.
    _pack_id: &str,
    syntax: &str,
    title: &str,
    tags: &[&str],
    aliases: &[&str],
    description: &str,
    source: EntrySource,
) -> Entry {
    Entry {
        id: id.into(),
        title: title.into(),
        syntax: syntax.into(),
        description: description.into(),
        examples: vec![],
        tags: tags.iter().map(|s| s.to_string()).collect(),
        aliases: aliases.iter().map(|s| s.to_string()).collect(),
        source,
        source_url: None,
    }
}

fn all_pack_ids(entries: &[(String, Entry)]) -> Vec<String> {
    let mut s: HashSet<String> = entries.iter().map(|(p, _)| p.clone()).collect();
    let mut v: Vec<String> = s.drain().collect();
    v.sort();
    v
}

fn normalize_all(entries: &[(String, Entry)]) -> Vec<NormalizedEntry> {
    entries.iter().map(|(p, e)| normalize_entry(p, e)).collect()
}

// ---- parse_query scenario tests (mirror of task-list §3) ----

#[test]
fn parse_query_classifies_tool_intent_stopword() {
    let q = parse_query("git reset", &["git".into()]);
    assert_eq!(q.tool.as_deref(), Some("git"));
    assert_eq!(q.intents, vec!["reset"]);
    assert!(q.options.is_empty());
    assert!(q.stopwords.is_empty());
}

#[test]
fn parse_query_handles_reversed_order() {
    // Both orders: tool is the first hit on pack_ids, intent is whatever
    // is left. `reset git` → tool=git, intents=[reset]. `git reset` →
    // tool=git, intents=[reset]. Identical shape regardless of order.
    let a = parse_query("git reset", &["git".into()]);
    let b = parse_query("reset git", &["git".into()]);
    assert_eq!(a.tool, b.tool);
    assert_eq!(a.intents, b.intents);
    assert_eq!(b.intents, vec!["reset"]);
}

#[test]
fn parse_query_filters_stopwords() {
    let q = parse_query("nginx redirect http to https", &["nginx".into()]);
    assert_eq!(q.tool.as_deref(), Some("nginx"));
    assert_eq!(q.intents, vec!["redirect", "http", "https"]);
    assert_eq!(q.stopwords, vec!["to"]);
}

#[test]
fn parse_query_recognizes_options() {
    let q = parse_query("docker logs --since", &["docker".into()]);
    assert_eq!(q.tool.as_deref(), Some("docker"));
    assert_eq!(q.intents, vec!["logs"]);
    assert_eq!(q.options, vec!["--since"]);
}

// ---- score adversarial tests (spike §8) ----

#[test]
fn score_git_reset_ranks_reset_card_first() {
    let entries = vec![
        (
            "git".into(),
            entry(
                "git-stash",
                "git",
                "git stash",
                "Stash changes",
                &["stash", "save"],
                &[],
                "",
                EntrySource::Official,
            ),
        ),
        (
            "git".into(),
            entry(
                "git-reset-hard-head",
                "git",
                "git reset --hard HEAD",
                "Discard local changes",
                &["reset", "hard", "discard", "undo"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "git".into(),
            entry(
                "git-reset-soft-head-1",
                "git",
                "git reset --soft HEAD~1",
                "Undo last commit, keep changes",
                &["reset", "soft", "uncommit", "undo", "last", "commit"],
                &[],
                "",
                EntrySource::Official,
            ),
        ),
    ];
    let hits = score_query(&entries, "git reset");
    assert!(!hits.is_empty(), "git reset should produce hits");
    let top_id = &hits[0].entry.id;
    assert!(
        top_id == "git-reset-hard-head" || top_id == "git-reset-soft-head-1",
        "top hit must be a reset card, got {top_id:?}"
    );
    assert_ne!(
        top_id, "git-stash",
        "git-stash MUST NOT win — that's the spike §3.1 failure mode"
    );
    let stash_pos = hits.iter().position(|h| h.entry.id == "git-stash");
    if let Some(p) = stash_pos {
        assert!(
            p >= 2,
            "git-stash should rank below both reset cards (got position {p})"
        );
    }
}

#[test]
fn score_nginx_redirect_beats_jq_to_entries() {
    // Query omits the tool to test the case where the tool hard filter
    // can't help. The stopword-filtered `to` and the deeper intent
    // coverage must still rank nginx-redirect first.
    let entries = vec![
        (
            "nginx".into(),
            entry(
                "nginx-redirect",
                "nginx",
                "return 301 https; rewrite ^ http",
                "Redirect directive",
                &["redirect", "http", "https", "routing"],
                &[],
                "",
                EntrySource::Official,
            ),
        ),
        (
            "jq".into(),
            entry(
                "jq-to-entries",
                "jq",
                ".[] | to_entries",
                "Convert to entries",
                &["filter", "entries"],
                &[],
                "Filter and convert arrays",
                EntrySource::Curated,
            ),
        ),
    ];
    let hits = score_query(&entries, "redirect http to https");
    assert!(!hits.is_empty());
    assert_eq!(
        hits[0].entry.id, "nginx-redirect",
        "nginx-redirect must beat jq-to-entries even without a tool filter"
    );
}

#[test]
fn score_docker_logs_prefers_plain_over_compose() {
    let entries = vec![
        (
            "docker".into(),
            // ponytail: alias "view container logs" gives a 4× alias hit on
            // "logs" that the compose entry lacks — that's the signal the
            // spec formula needs to break the otherwise-identical field
            // weights when both cards carry "logs" in syntax/title/tags.
            entry(
                "docker-logs",
                "docker",
                "docker logs <container>",
                "View container logs",
                &["logs"],
                &["view container logs"],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "docker".into(),
            entry(
                "docker-compose-logs",
                "docker",
                "docker compose logs -f <service>",
                "View service logs",
                &["logs", "compose"],
                &["compose service management"],
                "",
                EntrySource::Curated,
            ),
        ),
    ];
    let hits = score_query(&entries, "docker logs");
    assert!(!hits.is_empty());
    assert_eq!(
        hits[0].entry.id, "docker-logs",
        "plain docker-logs must outrank docker-compose-logs when 'compose' is absent"
    );
    let compose_pos = hits
        .iter()
        .position(|h| h.entry.id == "docker-compose-logs")
        .expect("compose entry must survive");
    assert!(
        compose_pos >= 1,
        "docker-compose-logs must rank below docker-logs"
    );
}

#[test]
fn score_tool_filter_excludes_other_packs() {
    let entries = vec![
        (
            "git".into(),
            entry(
                "git-status",
                "git",
                "git status",
                "Status",
                &["status"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "nginx".into(),
            entry(
                "nginx-redirect",
                "nginx",
                "return 301",
                "Redirect",
                &["routing"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "jq".into(),
            entry(
                "jq-to-entries",
                "jq",
                ".[]",
                "Map",
                &[],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
    ];
    let hits = score_query(&entries, "git status");
    assert!(!hits.is_empty());
    assert_eq!(hits[0].entry.id, "git-status");
    for h in &hits {
        assert_eq!(
            h.entry.pack_id, "git",
            "non-git entries must be excluded by hard filter, found {}",
            h.entry.id
        );
    }
}

#[test]
fn score_alias_match_counts_as_command_field() {
    // The card knows `wibble` only via alias, not via syntax/title/tags.
    // field_weighted_hits must credit it as 4× alias (FIELD_WEIGHT_ALIAS).
    let entries = vec![(
        "git".into(),
        entry(
            "git-wibble",
            "git",
            "git status",
            "Status",
            &["status"],
            &["wibble"],
            "",
            EntrySource::Curated,
        ),
    )];
    let parsed = parse_query("git wibble", &all_pack_ids(&entries));
    let normalized = normalize_all(&entries);
    let b: ScoreBreakdown = score(&normalized[0], &parsed);
    assert!(
        (b.field_weighted - 4.0).abs() < 0.001,
        "alias match must contribute exactly 4×, got {}",
        b.field_weighted
    );
    // No other contribution: no syntax/title/tags/desc hits.
    assert_eq!(b.field_weighted, 4.0);
}

#[test]
fn score_tie_break_is_deterministic() {
    let entries = vec![
        (
            "git".into(),
            entry(
                "git-zzz",
                "git",
                "git zzz",
                "Z",
                &["zzz"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "git".into(),
            entry(
                "git-aaa",
                "git",
                "git aaa",
                "A",
                &["aaa"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "docker".into(),
            entry(
                "docker-zzz",
                "docker",
                "docker zzz",
                "Z",
                &["zzz"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
    ];

    fn run_once(e: &[(String, Entry)]) -> Vec<String> {
        let hits = score_query(e, "git aaa");
        hits.iter().map(|h| h.entry.id.clone()).collect()
    }

    let a = run_once(&entries);
    let b = run_once(&entries);
    assert_eq!(a, b, "two calls on the same input must order identically");
    // sanity: tool-filtered docker entries must not appear.
    assert!(a.iter().all(|id| !id.starts_with("docker")));
}

#[test]
fn score_empty_intents_returns_zero_intent_bonus() {
    // Query "git" alone: tool, no intents, no options, no stopwords.
    // The all_intent_covered bonus must NOT fire (would be a degenerate
    // bonus for "covering zero tokens").
    let entries = vec![(
        "git".into(),
        entry(
            "git-status",
            "git",
            "git status",
            "Status",
            &["status"],
            &[],
            "",
            EntrySource::Curated,
        ),
    )];
    let parsed = parse_query("git", &all_pack_ids(&entries));
    let normalized = normalize_all(&entries);
    let b = score(&normalized[0], &parsed);
    assert_eq!(
        b.all_intent_covered, 0.0,
        "no intents → no all_covered bonus"
    );
    assert!(
        b.total.is_finite(),
        "total must not be NaN/Inf for empty intents, got {}",
        b.total
    );
    assert_eq!(b.tool_scope, 40.0, "tool match still fires");
}

#[test]
fn score_kubectl_typo_uses_fuzzy_rescue() {
    // "roolout" is one edit from "rollout" → fuzzy-rescued from Missing.
    // "restrt" is one edit from "restart" → also rescued. Both intents
    // should score Missing in intent coverage (no exact/prefix match)
    // and trigger the separate +10 rescue component.
    let entries = vec![(
        "kubectl".into(),
        entry(
            "kubectl-rollout-restart",
            "kubectl",
            "kubectl rollout restart deployment/<name>",
            "Restart deployment",
            &["rollout", "restart", "deployment"],
            &[],
            "",
            EntrySource::Official,
        ),
    )];
    let parsed = parse_query("kubectl roolout restrt", &["kubectl".into()]);
    let normalized = normalize_all(&entries);
    let b = score(&normalized[0], &parsed);
    // Neither intent matches by exact or prefix → both Missing.
    let missing = b
        .tiers
        .iter()
        .filter(|(_, t)| *t == CoverageTier::Missing)
        .count();
    assert_eq!(missing, 2, "both typo'd intents should be Missing tier");
    // The fuzzy rescue component must contribute up to the cap.
    assert!(
        b.fuzzy_rescue > 0.0,
        "fuzzy_rescue should lift the missing penalty, got {}",
        b.fuzzy_rescue
    );
    assert!(
        b.fuzzy_rescue <= FUZZY_RESCUE_CAP + 0.001,
        "fuzzy_rescue cap holds at {} (got {})",
        FUZZY_RESCUE_CAP,
        b.fuzzy_rescue
    );
}

#[test]
fn score_query_drops_filtered_entries() {
    // The hard-filter sentinel is NEG_INFINITY. score_query keeps only
    // finite-scored hits so callers don't have to defend themselves.
    let entries = vec![
        (
            "git".into(),
            entry(
                "git-status",
                "git",
                "git status",
                "Status",
                &["status"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "nginx".into(),
            entry(
                "nginx-status",
                "nginx",
                "nginx -t",
                "Test config",
                &["status"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
    ];
    let hits = score_query(&entries, "git status");
    assert_eq!(hits.len(), 1, "non-git entry must be dropped");
    assert_eq!(hits[0].entry.id, "git-status");
}

#[test]
fn sort_ranked_breaks_source_then_pack_then_id() {
    // Two entries with identical scores — must order by official source,
    // then pack_id alphabetical, then entry id.
    let entries = vec![
        (
            "alpha".into(),
            entry(
                "alpha-1",
                "alpha",
                "alpha run",
                "Run",
                &["run"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "beta".into(),
            entry(
                "beta-1",
                "beta",
                "alpha run",
                "Run",
                &["run"],
                &[],
                "",
                EntrySource::Official, // beats curated
            ),
        ),
    ];
    let parsed_query = ParsedQuery {
        raw: "alpha run".into(),
        tool: None,
        intents: vec!["alpha".into(), "run".into()],
        options: vec![],
        stopwords: vec![],
    };
    let normalized = normalize_all(&entries);
    let mut hits: Vec<RankedHit> = normalized
        .iter()
        .map(|n| {
            let bd = score(n, &parsed_query);
            RankedHit {
                entry: n.clone(),
                score: bd.total,
                confidence: score::classify(n, &parsed_query, &bd),
                breakdown: bd,
            }
        })
        .collect();
    sort_ranked(&mut hits);
    // Both alpha-1 and beta-1 should survive and Official (beta-1) should
    // win because of the source tiebreak — even with the same tool scope
    // and similar fields, the source priority breaks the tie.
    assert_eq!(
        hits[0].entry.id, "beta-1",
        "Official source outranks Curated"
    );
}

#[test]
fn score_field_weights_match_spec() {
    // ponytail: each intent token appears in EXACTLY one field, so the
    // sum of weighted hits is unambiguous. Title tokens that look like
    // syntax tokens would double-fire — keep fields disjoint.
    let entries = vec![(
        "git".into(),
        entry(
            "git-weights",
            "git",
            "git foo extra", // syntax has "foo"
            "bar only",      // title has "bar"
            &["baz"],        // tags have "baz"
            &["alpha"],      // aliases have "alpha"
            "",
            EntrySource::Curated,
        ),
    )];
    let parsed = parse_query("git foo bar baz alpha", &["git".into()]);
    let normalized = normalize_all(&entries);
    let b = score(&normalized[0], &parsed);
    let expected =
        FIELD_WEIGHT_SYNTAX + FIELD_WEIGHT_TITLE + FIELD_WEIGHT_TAGS + FIELD_WEIGHT_ALIAS;
    assert!(
        (b.field_weighted - expected).abs() < 0.001,
        "expected {} for syntax+title+tags+alias hits (one per token), got {}",
        expected,
        b.field_weighted
    );
}

#[test]
fn score_classify_returns_strong_when_tool_matches_and_intents_in_cmd() {
    let entries = vec![(
        "git".into(),
        entry(
            "git-reset",
            "git",
            "git reset --soft",
            "Soft reset",
            &["reset", "soft"],
            &[],
            "",
            EntrySource::Official,
        ),
    )];
    let parsed = parse_query("git reset", &all_pack_ids(&entries));
    let normalized = normalize_all(&entries);
    let b = score(&normalized[0], &parsed);
    let conf = score::classify(&normalized[0], &parsed, &b);
    assert_eq!(conf, Confidence::Strong);
}

#[test]
fn score_classify_returns_weak_for_tool_only_match() {
    // tool matched but no intent coverage in any field.
    let entries = vec![(
        "git".into(),
        entry(
            "git-status",
            "git",
            "git status",
            "Status",
            &["status"],
            &[],
            "View the state of the working tree",
            EntrySource::Curated,
        ),
    )];
    let parsed = parse_query("git nonexistent", &all_pack_ids(&entries));
    let normalized = normalize_all(&entries);
    let b = score(&normalized[0], &parsed);
    let conf = score::classify(&normalized[0], &parsed, &b);
    assert_eq!(conf, Confidence::Weak);
}
