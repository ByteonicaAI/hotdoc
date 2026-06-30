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
    // task-5.1b "plain command wins": a CURATED plain card must beat an
    // OFFICIAL variant carrying an uncovered command token (`compose`),
    // purely via the command-modifier penalty. Field weights are identical
    // ("logs" hits syntax+title+tags on both), and the variant even has the
    // +1 official-source bonus — so without the penalty the variant would
    // win. The -2 penalty (one uncovered `compose` token) overcomes the +1
    // bonus, giving the plain card a net 1.0 lead. No alias crutch: the
    // penalty alone is the deciding signal.
    let entries = vec![
        (
            "docker".into(),
            entry(
                "docker-logs",
                "docker",
                "docker logs <container>",
                "View container logs",
                &["logs"],
                &[],
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
                "View compose logs",
                &["logs", "compose"],
                &[],
                "",
                EntrySource::Official, // +1 source bonus the penalty must overcome
            ),
        ),
    ];
    let hits = score_query(&entries, "docker logs");
    assert!(!hits.is_empty());
    assert_eq!(
        hits[0].entry.id, "docker-logs",
        "plain docker-logs must outrank the OFFICIAL docker-compose-logs via the penalty"
    );
    let plain = hits
        .iter()
        .find(|h| h.entry.id == "docker-logs")
        .expect("plain entry must survive");
    let compose = hits
        .iter()
        .find(|h| h.entry.id == "docker-compose-logs")
        .expect("compose entry must survive");
    // The penalty must be the load-bearing signal: the variant carries a
    // negative command_specificity, the plain card carries zero.
    assert_eq!(
        plain.breakdown.command_specificity, 0.0,
        "plain card has no uncovered command token"
    );
    assert!(
        compose.breakdown.command_specificity < 0.0,
        "compose variant must carry a negative command-specificity penalty, got {}",
        compose.breakdown.command_specificity
    );
    assert!(
        plain.score > compose.score + 1e-9,
        "plain must outscore the official variant, got {} vs {}",
        plain.score,
        compose.score
    );
}

#[test]
fn score_command_specificity_breaks_score_ties_over_id() {
    // task-5.1b: command-specificity is now a SCORE PENALTY in `total`, not a
    // pure tie-break. Two cards that would otherwise tie on every component
    // are now separated by the penalty: the LESS specific card ("x foo
    // extra", carrying the unasked-for `extra`) loses `COMMAND_SPECIFICITY_
    // PENALTY` and so scores BELOW the plain card — even though its
    // alphabetically-SMALLER id ("aaa-extra") would otherwise win the
    // deterministic id fallback. This proves the penalty overrides the id
    // tie-break by lowering the score outright.
    let entries = vec![
        (
            "x".into(),
            entry(
                "aaa-extra", // sorts first by id — but is LESS specific
                "x",
                "x foo extra",
                "T",
                &["foo"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "x".into(),
            entry(
                "zzz-plain", // sorts last by id — but is MORE specific
                "x",
                "x foo",
                "T",
                &["foo"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
    ];
    let hits = score_query(&entries, "x foo");
    // The plain card must now score STRICTLY higher — the penalty separates
    // what was previously a score tie.
    assert!(
        hits[0].score > hits[1].score + 1e-9,
        "plain card must outscore the less-specific one, got {} vs {}",
        hits[0].score,
        hits[1].score
    );
    assert_eq!(
        hits[0].entry.id, "zzz-plain",
        "more-specific card must win via the penalty despite its larger id"
    );
}

#[test]
fn score_systemctl_restart_specificity_ordering() {
    // task-5.1b owner decision: `systemctl-restart` is the decisive #1
    // (exact title match). `systemctl-reload-or-restart` and
    // `systemctl-try-restart` are SYMMETRIC — equal score, and each carries
    // exactly ONE uncovered command token (`reload` vs `try`). The command-
    // modifier penalty is now a SCORE term, but it hits both variants by the
    // SAME -2 (one uncovered token each), so it cannot separate them — they
    // stay tied and order only by the deterministic id fallback. The golden
    // `forbidden_top3:[try-restart]` was removed (owner): try-restart is a
    // legitimate restart variant symmetric with the accepted reload-or-
    // restart, and restart already wins rank 1.
    let entries = vec![
        (
            "systemctl".into(),
            entry(
                "systemctl-restart",
                "systemctl",
                "systemctl restart name",
                "restart service",
                &["services", "control"],
                &[],
                "",
                EntrySource::Curated,
            ),
        ),
        (
            "systemctl".into(),
            entry(
                "systemctl-reload-or-restart",
                "systemctl",
                "systemctl reload-or-restart <unit>",
                "Reload or restart unit",
                &["reload", "restart", "service"],
                &[],
                "",
                EntrySource::Official,
            ),
        ),
        (
            "systemctl".into(),
            entry(
                "systemctl-try-restart",
                "systemctl",
                "systemctl try-restart <unit>",
                "Restart only if active",
                &["restart", "active", "service"],
                &[],
                "",
                EntrySource::Official,
            ),
        ),
    ];
    let hits = score_query(&entries, "systemctl restart service");
    assert_eq!(
        hits[0].entry.id, "systemctl-restart",
        "the plain restart card must be the decisive first result"
    );

    // Symmetry: both variants carry exactly one uncovered command token, so
    // the specificity penalty is identical and cannot evict try-restart.
    let parsed = parse_query("systemctl restart service", &all_pack_ids(&entries));
    let normalized = normalize_all(&entries);
    let reload = normalized
        .iter()
        .find(|n| n.id == "systemctl-reload-or-restart")
        .expect("reload present");
    let tryr = normalized
        .iter()
        .find(|n| n.id == "systemctl-try-restart")
        .expect("try present");
    assert_eq!(
        crate::ranker::score::uncovered_command_count(reload, &parsed),
        1,
        "reload-or-restart carries one uncovered command token (reload)"
    );
    assert_eq!(
        crate::ranker::score::uncovered_command_count(tryr, &parsed),
        crate::ranker::score::uncovered_command_count(reload, &parsed),
        "try-restart and reload-or-restart are specificity-symmetric"
    );
    // Both variants tie on score AND on specificity → the determinism
    // fallback (entry id) orders reload above try; try stays rank 3.
    let try_pos = hits
        .iter()
        .position(|h| h.entry.id == "systemctl-try-restart")
        .expect("try survives");
    assert_eq!(
        try_pos, 2,
        "try-restart legitimately remains rank 3 (symmetric)"
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

#[test]
fn classify_exact_via_alias_is_exact() {
    // Whole-query == alias string → Confidence::Exact via alias-75 bonus.
    // ponytail: title "Roll Back Changes" tokenises to ["roll","back","changes"]
    // and syntax "git reset --soft HEAD~1" tokenises to ["git","reset","soft","head","1"];
    // neither join equals "undo last commit", so EXACT_MATCH_TITLE (+80) and
    // EXACT_MATCH_SYNTAX (+100) do NOT fire. Only the alias exact bonus (+75)
    // contributes, giving b.exact_match == 75. This FAILS under the old
    // threshold-80 path (75 < 80) and PASSES under the new threshold-75 path —
    // true regression protection for the alias-75 fix.
    let e = entry(
        "git-reset-soft-head-1",
        "git",
        "git reset --soft HEAD~1",
        "Roll Back Changes",
        &[],
        &["undo last commit"],
        "",
        EntrySource::Curated,
    );
    let n = normalize_entry("git", &e);
    let q = parse_query("undo last commit", &["git".into()]);
    let b = score(&n, &q);
    assert_eq!(
        b.exact_match, 75.0,
        "only alias-75 must fire; got exact_match={} (title or syntax may have leaked)",
        b.exact_match
    );
    assert_eq!(score::classify(&n, &q, &b), Confidence::Exact);
}

#[test]
fn classify_fuzzy_rescued_tool_match_is_strong() {
    // "kubectl roolout restrt" — typos rescued by fuzzy; tool matches.
    // Must be Strong, not Weak.
    let e = entry(
        "kubectl-rollout-restart",
        "kubectl",
        "kubectl rollout restart",
        "Restart rollout",
        &["rollout", "restart"],
        &[],
        "",
        EntrySource::Official,
    );
    let n = normalize_entry("kubectl", &e);
    let q = parse_query("kubectl roolout restrt", &["kubectl".into()]);
    let b = score(&n, &q);
    assert!(b.fuzzy_rescue > 0.0, "precondition: fuzzy rescue fired");
    assert_eq!(score::classify(&n, &q, &b), Confidence::Strong);
}

#[test]
fn classify_description_only_no_tool_is_medium() {
    // All intents covered, but only in description, and no tool match.
    // Per the Medium doc: "covered somewhere but NOT entirely in command fields".
    let e = entry(
        "xyz",
        "openssl",
        "xyz",
        "Zzz",
        &[],
        &[],
        "rotate the encryption key",
        EntrySource::Curated,
    );
    let n = normalize_entry("openssl", &e);
    let q = parse_query("rotate key", &["git".into()]); // tool not matched
    let b = score(&n, &q);
    assert_eq!(score::classify(&n, &q, &b), Confidence::Medium);
}

#[test]
fn classify_tool_only_is_weak() {
    let e = entry(
        "git-stash",
        "git",
        "git stash",
        "Stash",
        &[],
        &[],
        "",
        EntrySource::Curated,
    );
    let n = normalize_entry("git", &e);
    let q = parse_query("git zzzzzz", &["git".into()]); // zzzzzz matches nothing
    let b = score(&n, &q);
    assert_eq!(score::classify(&n, &q, &b), Confidence::Weak);
}

#[test]
fn fuzzy_rescue_accumulates_two_typos() {
    // Two typo'd intents, both rescued → 20 (2 × 10), not flat 10.
    let e = entry(
        "kubectl-rollout-restart",
        "kubectl",
        "kubectl rollout restart",
        "Rollout restart",
        &["rollout", "restart"],
        &[],
        "",
        EntrySource::Official,
    );
    let n = normalize_entry("kubectl", &e);
    let q = parse_query("roolout restrt", &["kubectl".into()]);
    let b = score(&n, &q);
    assert!(
        (b.fuzzy_rescue - 20.0).abs() < 0.001,
        "two rescued typos should sum to 20, got {}",
        b.fuzzy_rescue
    );
}

// ---- WS task-5.2: confidence gate for gibberish / low-confidence ----

fn confidence_gate_corpus() -> Vec<(String, Entry)> {
    vec![(
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
    )]
}

#[test]
fn score_gibberish_query_returns_empty_via_confidence_gate() {
    // Gibberish: no tool, no field match anywhere. The top hit is
    // Weak confidence with a net-negative score (intent-missing penalty
    // dominates). The confidence gate must empty the result list rather
    // than surface a deterministic tie-break "winner".
    let entries = confidence_gate_corpus();
    let hits = score_query(&entries, "asdfqwer");
    assert!(
        hits.is_empty(),
        "gibberish must return empty after the confidence gate, got {:?}",
        hits.iter()
            .map(|h| (&h.entry.id, h.score))
            .collect::<Vec<_>>()
    );
}

#[test]
fn score_legit_weak_query_survives_confidence_gate() {
    // Regression guard: a tool-only match ("git" matches the pack, the
    // intent token is uncovered) is Weak confidence but earns a net
    // POSITIVE score (TOOL_MATCH outweighs INTENT_MISSING). It must NOT
    // be emptied — the gate only drops Weak hits below the score floor.
    let entries = confidence_gate_corpus();
    let hits = score_query(&entries, "git xyzzy");
    assert!(
        !hits.is_empty(),
        "a positive-score weak tool-match must survive the confidence gate"
    );
    assert_eq!(
        hits[0].confidence,
        Confidence::Weak,
        "guard precondition: top hit is Weak"
    );
    assert!(
        hits[0].score > 0.0,
        "guard precondition: top hit scores above the floor"
    );
}
