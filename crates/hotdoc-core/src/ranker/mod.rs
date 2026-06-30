//! WS-A.2 — deterministic in-memory command ranker.
//!
//! Phase A lands the module in isolation (this file + normalize /
//! parse_query / score / tests). Phase B wires `score_query` into
//! `HotdocIndex::search()`. Until then, no behavior elsewhere in the
//! crate depends on it.

pub mod normalize;
pub mod parse_query;
pub mod score;

#[cfg(test)]
mod tests;

pub use normalize::{normalize_entry, NormalizedEntry};
pub use parse_query::{parse_query, ParsedQuery, STOPWORDS};
pub use score::{score, sort_ranked, Confidence, CoverageTier, RankedHit, ScoreBreakdown};

use crate::pack::Entry;

/// ponytail: the facade takes raw `(pack_id, Entry)` pairs (what the
/// index builder holds) and returns ranked hits. HotdocIndex::search
/// will pass `self.entries` and clamp to its IPC `limit` after
/// `score_query` returns. `score_query` does NOT apply the limit — the
/// caller decides how many hits to surface.
pub fn score_query(entries: &[(String, Entry)], raw: &str) -> Vec<RankedHit> {
    let pack_ids: Vec<String> = entries.iter().map(|(pid, _)| pid.clone()).collect();
    let parsed = parse_query(raw, &pack_ids);

    let mut hits: Vec<RankedHit> = entries
        .iter()
        .map(|(pack_id, entry)| {
            let normalized = normalize_entry(pack_id, entry);
            let breakdown = score(&normalized, &parsed);
            let total = breakdown.total;
            let confidence = score::classify(&normalized, &parsed, &breakdown);
            RankedHit {
                entry: normalized,
                score: total,
                confidence,
                breakdown,
            }
        })
        .collect();

    // ponytail: NEG_INFINITY entries (tool hard-filter) sink to the
    // bottom by step 1 of `sort_ranked`. Drop them from the result so
    // callers see only finite-scored hits, keeping the contract simple.
    hits.retain(|h| h.score.is_finite());
    sort_ranked(&mut hits);
    hits
}
