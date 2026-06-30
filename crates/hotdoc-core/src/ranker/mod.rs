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

/// Rank precomputed normalized entries against a raw query. This is the
/// hot path: `normalized` is built ONCE at index build/open, never per
/// query. `pack_ids` is derived from the normalized set for tool
/// detection.
pub fn score_query_normalized(normalized: &[NormalizedEntry], raw: &str) -> Vec<RankedHit> {
    let pack_ids: Vec<String> = {
        let mut v: Vec<String> = normalized.iter().map(|n| n.pack_id.clone()).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let parsed = parse_query(raw, &pack_ids);
    score_query_normalized_with(normalized, &parsed)
}

/// Rank precomputed normalized entries against an already-parsed query.
///
/// Identical scoring body to `score_query_normalized`, but takes a
/// `ParsedQuery` directly instead of deriving pack_ids and parsing
/// internally. This is the M1 fix: when `normalized` is a candidate
/// SUBSET (the above-threshold path), pack_ids derived from that subset
/// can omit a query token's pack, silently disabling the tool hard
/// filter. Callers must pass `parsed` built against the FULL corpus so
/// tool detection is stable regardless of which rows survived retrieval.
pub fn score_query_normalized_with(
    normalized: &[NormalizedEntry],
    parsed: &ParsedQuery,
) -> Vec<RankedHit> {
    let mut hits: Vec<RankedHit> = normalized
        .iter()
        .map(|n| {
            let breakdown = score(n, parsed);
            let total = breakdown.total;
            let confidence = score::classify(n, parsed, &breakdown);
            RankedHit {
                entry: n.clone(),
                score: total,
                confidence,
                breakdown,
            }
        })
        .collect();

    hits.retain(|h| h.score.is_finite());
    sort_ranked(&mut hits);
    hits
}

/// Convenience wrapper that normalizes `(pack_id, Entry)` pairs on the
/// fly. Used by ranker unit tests and any caller that doesn't hold a
/// precomputed normalized set. Production search uses
/// `score_query_normalized` against the index's cached normals.
pub fn score_query(entries: &[(String, Entry)], raw: &str) -> Vec<RankedHit> {
    let normalized: Vec<NormalizedEntry> = entries
        .iter()
        .map(|(pid, e)| normalize_entry(pid, e))
        .collect();
    score_query_normalized(&normalized, raw)
}
