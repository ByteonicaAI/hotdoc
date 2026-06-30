use crate::pack::EntrySource;
use crate::ranker::normalize::NormalizedEntry;
use crate::ranker::parse_query::ParsedQuery;

/// ponytail: WS-A.2 §9.1 — f64 chosen over integer scores so weights can
/// be tuned in small increments without a re-multiply pass. The
/// popularity/source tie-breaks are sub-unit so integer math would force
/// scaling that obscures the per-tier contribution.
pub type Score = f64;

// ponytail: tier weights verbatim from ranker-design-2026-06-29.md §4.
// Bumping any of these changes behaviour for every ranking decision —
// always update the spike doc first.
const TOOL_MATCH: Score = 40.0;
const TOOL_HARD_FILTER: Score = f64::NEG_INFINITY;

const INTENT_EXACT: Score = 25.0;
const INTENT_PREFIX: Score = 18.0;
const INTENT_DESCRIPTION: Score = 5.0;
const INTENT_MISSING: Score = -25.0;

const ALL_INTENT_COVERED_BONUS: Score = 30.0;

// ponytail: made pub(crate) so ranker/tests.rs can assert weighted totals
// without re-typing magic numbers — keeps the spec table and the test
// honest to the same values.
pub(crate) const FIELD_WEIGHT_TITLE: Score = 4.0;
pub(crate) const FIELD_WEIGHT_SYNTAX: Score = 3.0;
pub(crate) const FIELD_WEIGHT_TAGS: Score = 2.0;
pub(crate) const FIELD_WEIGHT_ALIAS: Score = 4.0;
const FIELD_WEIGHT_DESCRIPTION: Score = 1.0;

const EXACT_MATCH_SYNTAX: Score = 100.0;
const EXACT_MATCH_TITLE: Score = 80.0;
const EXACT_MATCH_ALIAS: Score = 75.0;
const EXACT_MATCH_CAP: Score = 100.0;

const PHRASE_ADJACENT: Score = 20.0;
const PHRASE_QUERY_ORDER: Score = 10.0;

const FUZZY_PER_RESCUE: Score = 10.0;
pub(crate) const FUZZY_RESCUE_CAP: Score = 10.0;
const FUZZY_MIN_LEN: usize = 4;
const FUZZY_MAX_DIST: usize = 2;

const SOURCE_OFFICIAL: Score = 1.0;
const SOURCE_CHEATSHEET: Score = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageTier {
    /// Exact token match in syntax/title/tag/alias fields.
    ExactCommand,
    /// Token matches a prefix of some field token.
    Prefix,
    /// Exact match only in description or examples (low-confidence).
    DescriptionOnly,
    /// No match anywhere — carries the missing-intent penalty until
    /// `score_fuzzy_rescue` either lifts it (+10 per rescue) or leaves
    /// it at -25.
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Whole-query exact match against syntax/title/alias.
    Exact,
    /// Tool matched AND every intent covered in command fields.
    Strong,
    /// All intents covered somewhere but not entirely in command fields.
    Medium,
    /// Tool-only match or only weak (description) coverage.
    Weak,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreBreakdown {
    pub tool_scope: Score,
    pub intent_coverage: Score,
    pub all_intent_covered: Score,
    pub field_weighted: Score,
    pub exact_match: Score,
    pub phrase_order: Score,
    pub fuzzy_rescue: Score,
    pub source_tiebreak: Score,
    pub total: Score,
    pub tiers: Vec<(String, CoverageTier)>,
}

#[derive(Debug, Clone)]
pub struct RankedHit {
    pub entry: NormalizedEntry,
    pub score: Score,
    pub confidence: Confidence,
    pub breakdown: ScoreBreakdown,
}

/// Score a single entry. Returns a `ScoreBreakdown` with `total` set to
/// `f64::NEG_INFINITY` for entries filtered out by the tool hard filter
/// — callers must drop them via `total.is_finite()`.
pub fn score(entry: &NormalizedEntry, query: &ParsedQuery) -> ScoreBreakdown {
    let mut b = ScoreBreakdown::default();

    // 1. Tool scope / hard filter.
    if let Some(t) = &query.tool {
        if entry.pack_id != *t {
            b.tool_scope = TOOL_HARD_FILTER;
            b.total = TOOL_HARD_FILTER;
            return b;
        }
        b.tool_scope = TOOL_MATCH;
    }

    // 2. Intent coverage per token.
    let mut all_in_command = !query.intents.is_empty();
    for intent in &query.intents {
        let tier = classify_intent(intent, entry);
        match tier {
            CoverageTier::ExactCommand => b.intent_coverage += INTENT_EXACT,
            CoverageTier::Prefix => b.intent_coverage += INTENT_PREFIX,
            CoverageTier::DescriptionOnly => {
                b.intent_coverage += INTENT_DESCRIPTION;
                all_in_command = false;
            }
            CoverageTier::Missing => {
                b.intent_coverage += INTENT_MISSING;
                all_in_command = false;
            }
        }
        b.tiers.push((intent.clone(), tier));
    }

    // 3. All-intent-covered bonus — strict command-field match required.
    if all_in_command {
        b.all_intent_covered = ALL_INTENT_COVERED_BONUS;
    }

    // 4. Field-weighted hits — intents + options × per-field weight.
    b.field_weighted = score_field_weighted(entry, query);

    // 5. Exact-match bonus: joined intents == a whole field string.
    b.exact_match = score_exact_match(entry, query);

    // 6. Phrase order in syntax/title/alias.
    b.phrase_order = score_phrase_order(entry, query);

    // 7. Fuzzy rescue for missing intents (capped).
    b.fuzzy_rescue = score_fuzzy_rescue(entry, &b.tiers);

    // 8. Source tiebreak.
    b.source_tiebreak = match entry.source {
        EntrySource::Official => SOURCE_OFFICIAL,
        EntrySource::CheatSheet => SOURCE_CHEATSHEET,
        EntrySource::Curated | EntrySource::Personal => 0.0,
    };

    b.total = b.tool_scope
        + b.intent_coverage
        + b.all_intent_covered
        + b.field_weighted
        + b.exact_match
        + b.phrase_order
        + b.fuzzy_rescue
        + b.source_tiebreak;

    b
}

fn classify_intent(intent: &str, entry: &NormalizedEntry) -> CoverageTier {
    if field_contains_exact(&entry.syntax_tokens, intent)
        || field_contains_exact(&entry.title_tokens, intent)
        || field_contains_exact(&entry.tag_tokens, intent)
        || field_contains_exact(&entry.aliases, intent)
    {
        return CoverageTier::ExactCommand;
    }
    if field_contains_prefix(&entry.syntax_tokens, intent)
        || field_contains_prefix(&entry.title_tokens, intent)
        || field_contains_prefix(&entry.tag_tokens, intent)
        || field_contains_prefix(&entry.aliases, intent)
    {
        return CoverageTier::Prefix;
    }
    if field_contains_exact(&entry.description_tokens, intent)
        || field_contains_exact(&entry.example_tokens, intent)
    {
        return CoverageTier::DescriptionOnly;
    }
    // ponytail: Levenshtein rescue runs AFTER classification so a
    // missing-tier intent can still earn +10 from `score_fuzzy_rescue`.
    // Treating fuzzy as a tier would double-count and inflate ranking;
    // treating it as rescue keeps "fuzzy is for rescue, not ranking
    // inflation" (ranker-design §4).
    CoverageTier::Missing
}

fn field_contains_exact(field: &[String], tok: &str) -> bool {
    field.iter().any(|t| t == tok)
}

fn field_contains_prefix(field: &[String], tok: &str) -> bool {
    field.iter().any(|t| t.starts_with(tok))
}

fn field_fuzzy_match(field: &[String], tok: &str) -> bool {
    if tok.len() < FUZZY_MIN_LEN {
        return false;
    }
    field
        .iter()
        .any(|t| t.len() >= FUZZY_MIN_LEN && levenshtein(t, tok) <= FUZZY_MAX_DIST && t != tok)
}

fn score_field_weighted(entry: &NormalizedEntry, query: &ParsedQuery) -> Score {
    let mut total = 0.0;
    for tok in query.intents.iter().chain(query.options.iter()) {
        if field_contains_exact(&entry.title_tokens, tok) {
            total += FIELD_WEIGHT_TITLE;
        }
        if field_contains_exact(&entry.syntax_tokens, tok) {
            total += FIELD_WEIGHT_SYNTAX;
        }
        if field_contains_exact(&entry.tag_tokens, tok) {
            total += FIELD_WEIGHT_TAGS;
        }
        if field_contains_exact(&entry.aliases, tok) {
            total += FIELD_WEIGHT_ALIAS;
        }
        if field_contains_exact(&entry.description_tokens, tok) {
            total += FIELD_WEIGHT_DESCRIPTION;
        }
    }
    total
}

fn score_exact_match(entry: &NormalizedEntry, query: &ParsedQuery) -> Score {
    if query.intents.is_empty() {
        return 0.0;
    }
    let joined = query.intents.join(" ");
    let mut total = 0.0;
    if !entry.syntax_tokens.is_empty() && entry.syntax_tokens.join(" ") == joined {
        total += EXACT_MATCH_SYNTAX;
    }
    if !entry.title_tokens.is_empty() && entry.title_tokens.join(" ") == joined {
        total += EXACT_MATCH_TITLE;
    }
    if !entry.aliases.is_empty() && entry.aliases.join(" ") == joined {
        total += EXACT_MATCH_ALIAS;
    }
    if total > EXACT_MATCH_CAP {
        EXACT_MATCH_CAP
    } else {
        total
    }
}

fn score_phrase_order(entry: &NormalizedEntry, query: &ParsedQuery) -> Score {
    if query.intents.len() < 2 {
        return 0.0;
    }
    let fields: [&[String]; 3] = [&entry.syntax_tokens, &entry.title_tokens, &entry.aliases];
    let mut found_adjacent = false;
    let mut found_in_order = false;
    for field in &fields {
        if found_adjacent {
            break;
        }
        if consecutive_subsequence(field, &query.intents) {
            found_adjacent = true;
            found_in_order = true;
            continue;
        }
        if !found_in_order && ordered_subsequence(field, &query.intents) {
            found_in_order = true;
        }
    }
    if found_adjacent {
        PHRASE_ADJACENT
    } else if found_in_order {
        PHRASE_QUERY_ORDER
    } else {
        0.0
    }
}

fn consecutive_subsequence(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    'outer: for i in 0..=haystack.len() - needle.len() {
        for j in 0..needle.len() {
            if haystack[i + j] != needle[j] {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

fn ordered_subsequence(haystack: &[String], needle: &[String]) -> bool {
    let mut idx = 0;
    for tok in haystack {
        if idx < needle.len() && tok == &needle[idx] {
            idx += 1;
        }
    }
    idx == needle.len()
}

fn score_fuzzy_rescue(entry: &NormalizedEntry, tiers: &[(String, CoverageTier)]) -> Score {
    let mut total = 0.0;
    for (intent, tier) in tiers {
        if *tier != CoverageTier::Missing {
            continue;
        }
        if intent.len() < FUZZY_MIN_LEN {
            continue;
        }
        if field_fuzzy_match(&entry.syntax_tokens, intent)
            || field_fuzzy_match(&entry.title_tokens, intent)
            || field_fuzzy_match(&entry.tag_tokens, intent)
        {
            total += FUZZY_PER_RESCUE;
            if total >= FUZZY_RESCUE_CAP {
                return FUZZY_RESCUE_CAP;
            }
        }
    }
    total
}

/// ponytail: std Levenshtein — O(n*m) on token length (≤ 32 in practice).
/// Using edit-distance directly (no Damerau) so simple typos like
/// `rolLout` vs `roolout` cost 2 (sub+sub) and stay within the cap. The
/// "exact token already present" fast-path is in `field_*` callers via
/// the `t != tok` guard — no need to special-case equality here.
fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

pub fn classify(entry: &NormalizedEntry, query: &ParsedQuery, b: &ScoreBreakdown) -> Confidence {
    // ponytail: an intent is "covered for confidence" if its static tier
    // is non-Missing OR it was lifted by fuzzy rescue. The static `tiers`
    // alone undercount coverage: a typo'd intent stays `Missing` even
    // after `score_fuzzy_rescue` adds points, which previously dropped
    // "kubectl roolout restrt" to Weak.
    let fuzzy_lifted = b.fuzzy_rescue > 0.0;
    let has_intents = !b.tiers.is_empty();
    let all_in_cmd = has_intents
        && b.tiers
            .iter()
            .all(|(_, t)| matches!(t, CoverageTier::ExactCommand | CoverageTier::Prefix));
    let all_anywhere = has_intents
        && b.tiers
            .iter()
            .all(|(_, t)| !matches!(t, CoverageTier::Missing));
    // Missing intents are tolerated for "covered anywhere" only when the
    // whole entry earned a fuzzy rescue (typo path).
    let all_covered = all_anywhere || (fuzzy_lifted && has_intents);

    // Any whole-query exact field match (syntax 100 / title 80 / alias 75)
    // is high-confidence — alias matches must not be excluded by the 80 cut.
    if b.exact_match >= EXACT_MATCH_ALIAS {
        return Confidence::Exact;
    }
    if query.tool.as_deref() == Some(entry.pack_id.as_str()) && (all_in_cmd || all_covered) {
        return Confidence::Strong;
    }
    if all_covered {
        return Confidence::Medium;
    }
    Confidence::Weak
}

/// Stable sort by score then deterministic tie-breakers. Stable to
/// bytes so the same input produces the same order on every platform.
pub fn sort_ranked(hits: &mut [RankedHit]) {
    hits.sort_by(|a, b| {
        // 1. score descending
        match b
            .score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
        {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 2. exact-match tier
        match exact_tier(b.breakdown.exact_match).cmp(&exact_tier(a.breakdown.exact_match)) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 3. source priority
        match source_priority(b.entry.source.clone()).cmp(&source_priority(a.entry.source.clone()))
        {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 4. pack_id ascending
        match a.entry.pack_id.cmp(&b.entry.pack_id) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 5. entry id ascending
        a.entry.id.cmp(&b.entry.id)
    });
}

fn exact_tier(exact_bonus: Score) -> u8 {
    if exact_bonus + 0.5 >= EXACT_MATCH_SYNTAX {
        3
    } else if exact_bonus + 0.5 >= EXACT_MATCH_TITLE {
        2
    } else if exact_bonus + 0.5 >= EXACT_MATCH_ALIAS {
        1
    } else {
        0
    }
}

fn source_priority(s: EntrySource) -> u8 {
    match s {
        EntrySource::Official => 4,
        EntrySource::CheatSheet => 3,
        EntrySource::Curated => 2,
        EntrySource::Personal => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein("git", "git"), 0);
        assert_eq!(levenshtein("git", "got"), 1);
        assert_eq!(levenshtein("roolout", "rollout"), 1);
        assert_eq!(levenshtein("restar", "restart"), 1);
        assert_eq!(levenshtein("git", "github"), 3);
    }
}
