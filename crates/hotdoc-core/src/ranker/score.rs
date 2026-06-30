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
// ponytail: cap = 2 × FUZZY_PER_RESCUE so up to two typo'd intents can
// each earn +10 (ranker-design §4 "+10 per rescue"). A third rescue is
// capped — fuzzy stays a rescue, never an intent-exact-grade win (25).
pub(crate) const FUZZY_RESCUE_CAP: Score = 20.0;
const FUZZY_MIN_LEN: usize = 4;
const FUZZY_MAX_DIST: usize = 2;
// ponytail: a 1-char intent prefix-matches an enormous token set and
// inflates field weights with noise; require ≥2 chars to count as a
// prefix hit. Exact (full-token) matches are unaffected.
const PREFIX_MIN_LEN: usize = 2;

// ponytail: float epsilon so a stored exact bonus that round-tripped
// through f32 (index.rs SearchHit.score) still classifies into the right
// tier bucket. 0.5 is half the smallest tie-break increment.
const EXACT_TIER_EPSILON: Score = 0.5;

const SOURCE_OFFICIAL: Score = 1.0;
const SOURCE_CHEATSHEET: Score = 0.5;

// ponytail: command-specificity — a card carrying command/sub-command
// tokens the user did NOT ask for (`docker compose logs` for query
// `docker logs`, `systemctl try-restart` for `systemctl restart`) is less
// specific than the plainer card. Owner decision (task-5.1b, "plain command
// wins"): this is a SMALL BOUNDED SCORE PENALTY added into `total`, not just
// a tie-break. The magnitude must exceed the +1 official-source bonus so a
// curated plain card beats an official variant carrying one extra modifier
// token — and be as SMALL as possible to minimise collateral reordering.
// 2.0 is the minimum that clears +1 with a 1.0 margin per uncovered token.
// The count is capped (MAX_TOKENS) so a long multi-arg syntax cannot pile up
// an unbounded penalty. The `sort_ranked` tie-break stage is retained as a
// harmless secondary key (now redundant for score-separated entries).
const COMMAND_SPECIFICITY_PENALTY: Score = 2.0;
const COMMAND_SPECIFICITY_MAX_TOKENS: usize = 3;

// ponytail: task-5.2 — confidence gate floor. A `Weak`-confidence top hit
// scoring below this floor carries no net positive evidence and is
// emptied (gibberish, e.g. `asdfqwer`). Measured on the full 731-entry
// corpus (2026-07-01): the two gibberish golden queries top out at Weak
// scores of -74.0 and -24.0 (intent-missing penalties dominate, no tool
// or field match); the LOWEST-scoring legitimate golden query tops out at
// +43.0 (Strong) and the lowest legitimate *Weak* query at +77.0. 0.0 is
// the principled boundary strictly between the populations: every
// legitimate query nets positive, gibberish nets negative. The 24-point
// margin below and 43-point margin above leave no overlap. Only `Weak`
// confidence is gated, so Strong/Medium/Exact hits are never emptied.
pub(crate) const CONFIDENCE_SCORE_FLOOR: Score = 0.0;

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
    /// Negative-or-zero penalty for uncovered command tokens (see
    /// `COMMAND_SPECIFICITY_PENALTY`). Stored so the tie-break ladder and
    /// tests can read the raw uncovered-token count back out.
    pub command_specificity: Score,
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

    // 9. Command-specificity — count of extra unasked-for command tokens,
    // expressed as a negative-or-zero penalty. Owner decision (task-5.1b,
    // "plain command wins"): this IS added into `total` so a plain card beats
    // an official variant carrying an extra modifier (`docker logs` →
    // docker-logs over docker-compose-logs; `docker ps` → docker-ps over
    // docker-compose-ps). The earlier task-5.1 "irreconcilable" note no
    // longer holds — the owner reversed the `docker ps` golden so plain wins
    // there too, removing the contradiction. The `uncovered_command_count`
    // GATE (returns 0 unless an intent covers a command token) keeps queries
    // that don't match a sub-command from being perturbed. `charged` is
    // finite (`min` of two finite values), so `command_specificity` is never
    // NaN/-inf.
    //
    // task-5.1b cap (robustness): the SCORE contribution is bounded at one
    // uncovered token (-2.0 max) so a multi-token canonical answer cannot be
    // hard-demoted below a near-tied competitor by the -6 floor. Every
    // measured win (docker compose, systemctl) is a single uncovered token
    // (-2.0), so no golden outcome changes. `command_specificity` retains the
    // full-count penalty and is used only by the `sort_ranked` tie-break stage
    // to finely order score-tied variants (finer ordering preserved).
    let uncovered = uncovered_command_count(entry, query);
    let charged = uncovered.min(COMMAND_SPECIFICITY_MAX_TOKENS);
    b.command_specificity = -(charged as Score) * COMMAND_SPECIFICITY_PENALTY;
    // Score contribution capped at one uncovered token; tie-break keeps full count.
    let command_penalty = -(uncovered.min(1) as Score) * COMMAND_SPECIFICITY_PENALTY;

    b.total = b.tool_scope
        + b.intent_coverage
        + b.all_intent_covered
        + b.field_weighted
        + b.exact_match
        + b.phrase_order
        + b.fuzzy_rescue
        + b.source_tiebreak
        + command_penalty;

    b
}

/// Count an entry's significant command tokens that the query does NOT
/// account for. A command token is "covered" when it equals the matched
/// tool or one of the parsed intents; everything else (`compose`, `try`,
/// `reload`) is an extra modifier the user did not ask for. Placeholders
/// and option flags were already excluded when `command_tokens` was built,
/// so this only ever weighs literal sub-command identity. Lower = more
/// specific to the query.
///
/// GATE: returns 0 unless at least one command token is covered by a query
/// *intent*. Specificity is only meaningful when comparing cards that
/// actually match the user's sub-command intent — a garbage query, or one
/// that matched only on tool scope, must not perturb the deterministic
/// fallback ordering of non-matches (else the 50/50 baseline regresses).
pub(crate) fn uncovered_command_count(entry: &NormalizedEntry, query: &ParsedQuery) -> usize {
    let mut covered_by_intent = 0usize;
    let mut uncovered = 0usize;
    for t in &entry.command_tokens {
        if query.tool.as_deref() == Some(t.as_str()) {
            continue;
        }
        if query.intents.iter().any(|i| i == t) {
            covered_by_intent += 1;
        } else {
            uncovered += 1;
        }
    }
    if covered_by_intent == 0 {
        return 0;
    }
    uncovered
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
    if tok.len() < PREFIX_MIN_LEN {
        return false;
    }
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
        // 3. command-specificity: among score-tied entries, the one whose
        // command tokens are MORE fully covered by the query (fewer extra
        // unasked-for modifiers) wins. `command_specificity` is
        // `-penalty × charged_uncovered`, so the LARGER (closer-to-zero)
        // value is the more specific entry — compare a vs b descending.
        // Pure reordering of ties; the score stage already separated
        // decisively-scored results, so this cannot regress them.
        match b
            .breakdown
            .command_specificity
            .partial_cmp(&a.breakdown.command_specificity)
            .unwrap_or(std::cmp::Ordering::Equal)
        {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 4. source priority
        match source_priority(b.entry.source.clone()).cmp(&source_priority(a.entry.source.clone()))
        {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 5. pack_id ascending
        match a.entry.pack_id.cmp(&b.entry.pack_id) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        // 6. entry id ascending
        a.entry.id.cmp(&b.entry.id)
    });
}

fn exact_tier(exact_bonus: Score) -> u8 {
    if exact_bonus + EXACT_TIER_EPSILON >= EXACT_MATCH_SYNTAX {
        3
    } else if exact_bonus + EXACT_TIER_EPSILON >= EXACT_MATCH_TITLE {
        2
    } else if exact_bonus + EXACT_TIER_EPSILON >= EXACT_MATCH_ALIAS {
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
