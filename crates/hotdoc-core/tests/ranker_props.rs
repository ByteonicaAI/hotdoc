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

// ---------------------------------------------------------------------------
// Property 1: never panics, all scores finite
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn never_panics_and_scores_finite(q in ".{0,64}", entries in arb_entries(0..8usize)) {
        let hits = hotdoc_core::ranker::score_query(&entries, &q);
        for h in &hits {
            prop_assert!(h.score.is_finite(), "non-finite score {} for entry {}", h.score, h.entry.id);
        }
    }
}

// ---------------------------------------------------------------------------
// Property 2: deterministic total order under input permutation
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn ordering_is_permutation_invariant(q in "[a-z ]{0,32}", entries in arb_entries(1..10usize)) {
        let mut shuffled = entries.clone();
        shuffled.reverse(); // deterministic permutation

        let a: Vec<String> = hotdoc_core::ranker::score_query(&entries, &q)
            .into_iter()
            .map(|h| h.entry.id)
            .collect();
        let b: Vec<String> = hotdoc_core::ranker::score_query(&shuffled, &q)
            .into_iter()
            .map(|h| h.entry.id)
            .collect();

        prop_assert_eq!(a, b, "ordering differed after reversing entry input order (query={:?})", q);
    }
}

// ---------------------------------------------------------------------------
// Property 3: degenerate queries never panic, all scores finite
// ---------------------------------------------------------------------------

#[test]
fn degenerate_queries_are_stable() {
    // Build a small fixed entry set so we exercise the scoring path.
    let entries: Vec<(String, Entry)> = (0..4)
        .map(|i| {
            (
                "git".to_string(),
                Entry {
                    id: format!("e{i}"),
                    title: "Undo last commit".to_string(),
                    syntax: "git reset --soft HEAD~1".to_string(),
                    description: "Move HEAD back one commit, keep changes staged".to_string(),
                    examples: vec![],
                    tags: vec!["reset".to_string(), "undo".to_string()],
                    aliases: vec![],
                    source: EntrySource::Curated,
                    source_url: None,
                },
            )
        })
        .collect();

    for q in &["", "   ", "!!!", "the of a"] {
        let hits = hotdoc_core::ranker::score_query(&entries, q);
        for h in &hits {
            assert!(
                h.score.is_finite(),
                "degenerate query {q:?} produced non-finite score {} for entry {}",
                h.score,
                h.entry.id,
            );
        }
    }
}
