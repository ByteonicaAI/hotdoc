//! Property/fuzz tests for the ranker.
//!
//! Proves: (1) score_query never panics and returns only finite scores,
//! (2) the total order is invariant under input permutation,
//! (3) degenerate queries are handled without panic and produce finite scores.

use hotdoc_core::pack::{Entry, EntrySource};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Entry strategy
// ---------------------------------------------------------------------------

/// Strategy that builds a single `(pack_id, Entry)` with the given index `i`
/// baked into the entry id so it is globally unique within a generated vec.
fn arb_entry(i: usize) -> impl Strategy<Value = (String, Entry)> {
    (
        "[a-z]{1,8}",                                   // pack_id
        "[a-zA-Z0-9 ]{0,32}",                           // title
        "[a-zA-Z0-9 ]{1,16}", // syntax (non-empty; validate() would reject empty but score_query doesn't care)
        "[a-zA-Z0-9 ]{0,64}", // description
        prop::collection::vec("[a-z]{1,8}", 0..4usize), // tags
    )
        .prop_map(move |(pack_id, title, syntax, description, tags)| {
            let entry = Entry {
                id: format!("e{i}"),
                title,
                syntax,
                description,
                examples: vec![],
                tags,
                aliases: vec![],
                source: EntrySource::Curated,
                source_url: None,
            };
            (pack_id, entry)
        })
}

/// Strategy producing a vec of `(pack_id, Entry)` pairs with unique entry ids.
fn arb_entries(range: std::ops::Range<usize>) -> impl Strategy<Value = Vec<(String, Entry)>> {
    range.prop_flat_map(|len| {
        let strats: Vec<_> = (0..len).map(arb_entry).collect();
        strats
    })
}

/// Extract lowercase word tokens from an entry's key fields (pack_id, title,
/// syntax words, tags). Used to build overlap queries so tool-detection (+40)
/// and intent-match (ExactCommand/Prefix) paths actually fire.
fn content_words(pack_id: &str, entry: &Entry) -> Vec<String> {
    let mut words: Vec<String> = vec![pack_id.to_lowercase()];
    for w in entry.title.split_whitespace() {
        let w = w.to_lowercase();
        if !w.is_empty() {
            words.push(w);
        }
    }
    for w in entry.syntax.split_ascii_whitespace() {
        let w: String = w
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        if !w.is_empty() {
            words.push(w);
        }
    }
    for tag in &entry.tags {
        let t = tag.to_lowercase();
        if !t.is_empty() {
            words.push(t);
        }
    }
    words
}

/// Query strategy: ~50% probability picks 1–3 tokens drawn from `words`
/// (entry content overlap, so tool-detection and intent-match paths fire),
/// ~50% generates arbitrary text including non-ASCII (adversarial coverage).
fn arb_query(words: Vec<String>) -> BoxedStrategy<String> {
    let overlapping: BoxedStrategy<String> = if words.is_empty() {
        Just(String::new()).boxed()
    } else {
        prop::collection::vec(prop::sample::select(words), 1usize..=3usize)
            .prop_map(|ws| ws.join(" "))
            .boxed()
    };

    // [^\n]{0,64} generates arbitrary Unicode (incl. non-ASCII) up to 64 chars.
    let arbitrary: BoxedStrategy<String> = prop::string::string_regex("[^\n]{0,64}")
        .expect("valid regex for arbitrary adversarial query text")
        .boxed();

    prop_oneof![
        1 => overlapping,
        1 => arbitrary,
    ]
    .boxed()
}

/// Strategy: (entries with ≥ 1 element, query that overlaps content ~50%).
///
/// Fixes Property 1 vacuousness: non-empty entry list ensures the scoring
/// loop always runs; 50% overlap queries force tool-detection and intent-match
/// paths instead of tokenizing to nothing.
fn arb_entries_and_query(
    range: std::ops::Range<usize>,
) -> impl Strategy<Value = (Vec<(String, Entry)>, String)> {
    arb_entries(range).prop_flat_map(|entries| {
        let words = content_words(&entries[0].0, &entries[0].1);
        let entries_clone = entries.clone();
        arb_query(words).prop_map(move |q| (entries_clone.clone(), q))
    })
}

/// Strategy: (original entries ≥ 2, shuffled entries via prop_shuffle, query).
///
/// Fixes Property 2 vacuousness: ≥ 2 entries make the shuffle non-trivial;
/// `prop_shuffle` generates genuine permutations (not just reverse); the
/// same 50/50 query strategy is reused so scores are non-trivial.
#[allow(clippy::type_complexity)]
fn arb_shuffle_and_query(
) -> impl Strategy<Value = (Vec<(String, Entry)>, Vec<(String, Entry)>, String)> {
    arb_entries(2..10usize).prop_flat_map(|entries| {
        let words = content_words(&entries[0].0, &entries[0].1);
        let entries_orig = entries.clone();
        let shuffled_strat = Just(entries).prop_shuffle();
        let q_strat = arb_query(words);
        (shuffled_strat, q_strat).prop_map(move |(shuffled, q)| (entries_orig.clone(), shuffled, q))
    })
}

// ---------------------------------------------------------------------------
// Property 1: never panics, all scores finite
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn never_panics_and_scores_finite((entries, q) in arb_entries_and_query(1..8usize)) {
        let hits = hotdoc_core::ranker::score_query(&entries, &q);
        for h in &hits {
            prop_assert!(
                h.score.is_finite(),
                "non-finite score {} for entry {}",
                h.score,
                h.entry.id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Property 2: deterministic total order under input permutation
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn ordering_is_permutation_invariant((entries, shuffled, q) in arb_shuffle_and_query()) {
        let a: Vec<String> = hotdoc_core::ranker::score_query(&entries, &q)
            .into_iter()
            .map(|h| h.entry.id)
            .collect();
        let b: Vec<String> = hotdoc_core::ranker::score_query(&shuffled, &q)
            .into_iter()
            .map(|h| h.entry.id)
            .collect();

        prop_assert_eq!(
            a,
            b,
            "ordering differed after shuffling entry input order (query={:?})",
            q
        );
    }
}

// ---------------------------------------------------------------------------
// Property 3: degenerate queries never panic, all scores finite
// ---------------------------------------------------------------------------

#[test]
fn degenerate_queries_are_stable() {
    // Vary the 4 entries across different tools, titles, syntax, and tags so
    // degenerate queries exercise more `classify_intent` branches (not just
    // the id tie-break from identical content).
    let entries: Vec<(String, Entry)> = vec![
        (
            "git".to_string(),
            Entry {
                id: "e0".to_string(),
                title: "Undo last commit".to_string(),
                syntax: "git reset --soft HEAD~1".to_string(),
                description: "Move HEAD back one commit, keep changes staged".to_string(),
                examples: vec![],
                tags: vec!["reset".to_string(), "undo".to_string()],
                aliases: vec![],
                source: EntrySource::Curated,
                source_url: None,
            },
        ),
        (
            "docker".to_string(),
            Entry {
                id: "e1".to_string(),
                title: "List running containers".to_string(),
                syntax: "docker ps".to_string(),
                description: "Show only running containers".to_string(),
                examples: vec![],
                tags: vec!["list".to_string(), "containers".to_string()],
                aliases: vec![],
                source: EntrySource::Curated,
                source_url: None,
            },
        ),
        (
            "kubectl".to_string(),
            Entry {
                id: "e2".to_string(),
                title: "Apply configuration".to_string(),
                syntax: "kubectl apply -f manifest.yaml".to_string(),
                description: "Apply a configuration file to a cluster".to_string(),
                examples: vec![],
                tags: vec!["apply".to_string(), "deploy".to_string()],
                aliases: vec![],
                source: EntrySource::Curated,
                source_url: None,
            },
        ),
        (
            "nginx".to_string(),
            Entry {
                id: "e3".to_string(),
                title: "Reload nginx config".to_string(),
                syntax: "nginx -s reload".to_string(),
                description: "Reload configuration without stopping the server".to_string(),
                examples: vec![],
                tags: vec!["reload".to_string(), "config".to_string()],
                aliases: vec![],
                source: EntrySource::Curated,
                source_url: None,
            },
        ),
    ];

    for q in &["", "   ", "!!!", "the of a"] {
        let hits = hotdoc_core::ranker::score_query(&entries, q);
        for h in &hits {
            assert!(
                h.score.is_finite(),
                "degenerate query {:?} produced non-finite score {} for entry {}",
                q,
                h.score,
                h.entry.id,
            );
        }
    }
}
