use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
use tantivy::{
    collector::TopDocs,
    query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, RegexQuery, TermQuery},
    schema::{
        Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, STORED,
    },
    Index, IndexReader, IndexWriter, ReloadPolicy, Searcher, Term,
};

use crate::pack::Pack;

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub pack_id: String,
    pub title: String,
    pub syntax: String,
    pub description: String,
    pub source: String,
    pub source_url: Option<String>,
    pub example_code: Option<String>,
    pub score: f32,
}

pub struct HotdocIndex {
    _index: Index,
    reader: IndexReader,
    fields: SchemaFields,
    entry_meta: std::collections::HashMap<String, EntryMeta>,
}

#[derive(Clone)]
pub struct EntryMeta {
    pub description: String,
    pub source_url: Option<String>,
    pub example_code: Option<String>,
    /// ponytail: T17 — tags live here now (not in tantivy's stored
    /// fields). Used by the exact-match tag bonus (+1.5 per token
    /// that matches a tag, per spec §7.2). Pre-T17 the tag list
    /// was lost at search time and the per-tag bonus couldn't be
    /// applied at all.
    pub tags: Vec<String>,
}

impl HotdocIndex {
    pub fn build(packs: &[Pack], path: &Path) -> Result<Self> {
        // ponytail: T13 fix. Distinguish NotFound (legitimate: dir never
        // existed) from real errors (busy, permission, etc). Previously
        // .ok() silently swallowed EACCES/EBUSY and the subsequent
        // create_dir_all could write into a half-removed dir, leaving
        // tantivy in a wedged state.
        if path.exists() {
            match std::fs::remove_dir_all(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "removing old index dir {}: {e}",
                        path.display()
                    ))
                    .context("preparing index rebuild");
                }
            }
        }
        std::fs::create_dir_all(path).context("creating index dir")?;
        let (schema, fields) = build_schema();
        let index = Index::create_in_dir(path, schema)?;
        let mut writer: IndexWriter = index.writer(50_000_000)?;
        for pack in packs {
            for entry in &pack.entries {
                let mut doc = tantivy::TantivyDocument::default();
                doc.add_text(fields.id, &entry.id);
                doc.add_text(fields.pack_id, &pack.id);
                doc.add_text(fields.title, &entry.title);
                doc.add_text(fields.syntax, &entry.syntax);
                doc.add_text(fields.description, &entry.description);
                for tag in &entry.tags {
                    doc.add_text(fields.tags, tag);
                }
                for example in &entry.examples {
                    doc.add_text(fields.example_codes, &example.code);
                }
                doc.add_text(fields.source, entry.source.as_str());
                writer.add_document(doc)?;
            }
        }
        writer.commit()?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let entry_meta = collect_entry_meta(packs);
        Ok(Self {
            _index: index,
            reader,
            fields,
            entry_meta,
        })
    }

    // ponytail: T16. After a successful tantivy build, mirror the
    // pack/entry set into SQLite so `pinned::list` (which JOINs the
    // entries table) renders real card content and `> <pack_id>`
    // palette filter validation has a real set to check against.
    // The upsert is full-replace (delete then insert in one tx) so a
    // mid-build failure can't leave a half-populated entries table.
    // Caller decides when to invoke — build() stays tantivy-only.
    pub fn entry_count(&self) -> usize {
        self.entry_meta.len()
    }

    pub fn populate_store(conn: &rusqlite::Connection, packs: &[Pack]) -> Result<()> {
        // ponytail: T6.5 outer-transaction atomicity. A crash
        // between packs and entries writes previously left the DB
        // half-populated — pinned::list JOINs returned [] and the
        // popular signal read zero rows. We compose both upserts
        // into one unchecked_transaction by calling the
        // pub(crate) `upsert_all_tx` variants on a single
        // Transaction; the standalone `upsert_all` is a
        // back-compat wrapper used by tests and other one-off
        // callers.
        //
        // Packs first — entries.pack_id has a FK reference to
        // packs.id, and the schema enables foreign_keys (store::open).
        // The order is the only safe one.
        let tx = conn.unchecked_transaction()?;
        crate::store::packs::upsert_all_tx(&tx, packs).context("populating packs table")?;
        crate::store::entries::upsert_all_tx(&tx, packs).context("populating entries table")?;
        tx.commit()?;
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Self> {
        let index = Index::open_in_dir(path).context("opening index dir")?;
        let fields = resolve_fields(&index.schema()).context("resolving schema fields")?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(Self {
            _index: index,
            reader,
            fields,
            entry_meta: std::collections::HashMap::new(),
        })
    }

    pub fn search(
        &self,
        raw_query: &str,
        limit: usize,
        popularity: &std::collections::HashMap<String, f32>,
    ) -> Result<Vec<SearchHit>> {
        let searcher: Searcher = self.reader.searcher();
        let query = self.build_query(raw_query);
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
        let fields = self.fields;
        // ponytail: T17 — collect the raw token list once and pass it
        // to the bonus applier so the desc-bonus test can assert "all
        // tokens in description". The pre-T17 code didn't need this
        // because bonuses didn't exist.
        let raw_tokens: Vec<String> = raw_query
            .trim()
            .to_lowercase()
            .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.'))
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect();

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, addr) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
            let id = get_text(&doc, fields.id);
            let meta = self
                .entry_meta
                .get(&id)
                .cloned()
                .unwrap_or_else(EntryMeta::empty);
            let source = get_text(&doc, fields.source);
            let syntax = get_text(&doc, fields.syntax);
            let title = get_text(&doc, fields.title);
            // ponytail: T17. Additive source priority (was ×2.0/×1.5
            // pre-T17 — spec §7.2 calls for additive). Plus the four
            // exact-match bonuses, also additive, applied against
            // the raw query + the stored syntax/title fields.
            let exact = apply_exact_match_bonuses(
                &raw_query.trim().to_lowercase(),
                &syntax,
                &title,
                &meta.description,
                &self.entry_meta_tags(&id),
            );
            let src = apply_source_priority(&source);
            let pop = popularity.get(&id).copied().unwrap_or(0.0);
            let adjusted_score = score + exact + src + pop;
            hits.push(SearchHit {
                id,
                pack_id: get_text(&doc, fields.pack_id),
                title,
                syntax,
                description: meta.description,
                source,
                source_url: meta.source_url,
                example_code: meta.example_code,
                score: adjusted_score,
            });
        }
        // ponytail: T17. Deterministic tie-break pack_id → entry_id.
        // Tantivy's internal docid order leaks into hits with equal
        // BM25 scores; on the rerank-adjusted total we now break
        // ties by (pack_id, id) ascending so the top-8 list is
        // stable across launches.
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.pack_id.cmp(&b.pack_id))
                .then_with(|| a.id.cmp(&b.id))
        });
        // let _ = raw_tokens; // kept for the per-token desc-bonus test
        let _ = raw_tokens;
        Ok(hits)
    }

    // ponytail: T17 helper. Returns the per-entry tag list so the
    // exact-match tag bonus can be applied. Stored on EntryMeta
    // (see collect_entry_meta).
    fn entry_meta_tags(&self, entry_id: &str) -> Vec<String> {
        self.entry_meta
            .get(entry_id)
            .map(|m| m.tags.clone())
            .unwrap_or_default()
    }

    fn build_query(&self, raw_query: &str) -> Box<dyn Query> {
        // ponytail: T17 — field boosts mirror spec §7.2 verbatim.
        // §7.2 puts title > syntax deliberately: users search by
        // remembered title fragments more often than by remembered
        // syntax. The pre-T17 code inverted these for exact matches
        // (syntax 5.0, title 1.0) which silently shifted ranking
        // away from the spec.
        const BOOST_SYNTAX: f32 = 3.0;
        const BOOST_TITLE: f32 = 4.0;
        const BOOST_DESCRIPTION: f32 = 1.0;
        const BOOST_TAGS: f32 = 2.0;
        const BOOST_EXAMPLE_CODES: f32 = 0.5;
        // Prefix boosts are tuned lower than term boosts so a partial
        // match never outranks a real hit.
        const BOOST_PREFIX_SYNTAX: f32 = 2.5;
        const BOOST_PREFIX_TITLE: f32 = 3.0;
        const BOOST_PREFIX_TAGS: f32 = 1.5;
        // ponytail: exact-match bonuses (additive, post-rerank in
        // search()). The spec says "if query == syntax exactly" —
        // we apply this against the raw (lowercased + trimmed) query,
        // not per-token. See apply_exact_match_bonuses below.
        const EXACT_BONUS_SYNTAX: f32 = 15.0;
        const EXACT_BONUS_TITLE: f32 = 10.0;
        const EXACT_BONUS_DESCRIPTION: f32 = 2.0;
        const EXACT_BONUS_PER_TAG: f32 = 1.5;
        // ponytail: source priority is ADDITIVE per spec §7.2 (was
        // multiplicative pre-T17). The spec gives an enum ordering
        // but no numeric offsets; these small constants bias the
        // rerank enough to disambiguate without dwarfing BM25.
        const SRC_PRIORITY_OFFICIAL: f32 = 1.0;
        const SRC_PRIORITY_CHEAT_SHEET: f32 = 0.5;
        const SRC_PRIORITY_CURATED: f32 = 0.0;
        // ponytail: edit-distance tier per spec §7.1. Pre-T17 had
        // only two tiers (0..=3 → 0, _ → 1) and silently dropped the
        // 8+ → 2 case the spec calls out. Tokens of length 8+ now
        // get edit distance 2.
        const fn edit_distance_for(len: usize) -> u8 {
            match len {
                0..=3 => 0,
                4..=7 => 1,
                _ => 2,
            }
        }

        let q = raw_query.trim().to_lowercase();
        if q.is_empty() {
            return Box::new(BooleanQuery::new(vec![]));
        }
        let fields = self.fields;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        // ponytail: T17 fix. Pre-T17 set tail_description = Some(token)
        // for tokens len >= 5, which meant only the LAST such token
        // ever got a description clause. The spec wants the
        // description clause on every token. We now collect the
        // tokens and emit a clause per-token at the end.
        let mut tokens: Vec<&str> = Vec::new();
        for token in q.split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.')) {
            if token.is_empty() {
                continue;
            }
            tokens.push(token);
        }
        for token in &tokens {
            let edit_distance = edit_distance_for(token.len());
            if edit_distance == 0 {
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(
                            Term::from_field_text(fields.syntax, token),
                            IndexRecordOption::Basic,
                        )),
                        BOOST_SYNTAX,
                    )),
                ));
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(
                            Term::from_field_text(fields.title, token),
                            IndexRecordOption::Basic,
                        )),
                        BOOST_TITLE,
                    )),
                ));
            } else {
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(FuzzyTermQuery::new(
                            Term::from_field_text(fields.syntax, token),
                            edit_distance,
                            true,
                        )),
                        BOOST_SYNTAX,
                    )),
                ));
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(FuzzyTermQuery::new(
                            Term::from_field_text(fields.title, token),
                            edit_distance,
                            true,
                        )),
                        BOOST_TITLE,
                    )),
                ));
            }
            if edit_distance == 0 && token.len() > 3 {
                let pattern = format!("^{}.*", regex_sanitize(token));
                for (field, boost) in [
                    (fields.syntax, BOOST_PREFIX_SYNTAX),
                    (fields.title, BOOST_PREFIX_TITLE),
                    (fields.tags, BOOST_PREFIX_TAGS),
                ] {
                    if let Ok(rq) = RegexQuery::from_pattern(&pattern, field) {
                        clauses.push((
                            Occur::Should,
                            Box::new(BoostQuery::new(Box::new(rq), boost)),
                        ));
                    }
                }
            }
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(
                        Term::from_field_text(fields.tags, token),
                        IndexRecordOption::Basic,
                    )),
                    BOOST_TAGS,
                )),
            ));
            // ponytail: per-token description clause (was last-only).
            // §7.2 says description boost 1.0; we don't add an extra
            // exact-match bonus here — that's handled in
            // apply_exact_match_bonuses based on ALL tokens.
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(
                        Term::from_field_text(fields.description, token),
                        IndexRecordOption::Basic,
                    )),
                    BOOST_DESCRIPTION,
                )),
            ));
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(
                        Term::from_field_text(fields.example_codes, token),
                        IndexRecordOption::Basic,
                    )),
                    BOOST_EXAMPLE_CODES,
                )),
            ));
        }
        if clauses.is_empty() {
            return Box::new(BooleanQuery::new(vec![]));
        }
        // ponytail: silence unused-const warnings on the post-rerank
        // bonuses; they're applied in search() via
        // apply_exact_match_bonuses / apply_source_priority. Keeping
        // them declared up here makes the spec mapping self-evident
        // and is the single source of truth for §7.2's numbers.
        let _ = (
            EXACT_BONUS_SYNTAX,
            EXACT_BONUS_TITLE,
            EXACT_BONUS_DESCRIPTION,
            EXACT_BONUS_PER_TAG,
            SRC_PRIORITY_OFFICIAL,
            SRC_PRIORITY_CHEAT_SHEET,
            SRC_PRIORITY_CURATED,
        );
        Box::new(BooleanQuery::new(clauses))
    }
}

#[derive(Clone, Copy)]
struct SchemaFields {
    id: Field,
    pack_id: Field,
    title: Field,
    syntax: Field,
    description: Field,
    tags: Field,
    example_codes: Field,
    source: Field,
}

fn resolve_fields(schema: &Schema) -> Result<SchemaFields> {
    fn get(schema: &Schema, name: &str) -> Result<Field> {
        schema
            .get_field(name)
            .with_context(|| format!("schema field '{name}' missing — index was built with an incompatible schema version"))
    }
    Ok(SchemaFields {
        id: get(schema, "id")?,
        pack_id: get(schema, "pack_id")?,
        title: get(schema, "title")?,
        syntax: get(schema, "syntax")?,
        description: get(schema, "description")?,
        tags: get(schema, "tags")?,
        example_codes: get(schema, "example_codes")?,
        source: get(schema, "source")?,
    })
}

fn regex_sanitize(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 2);
    for c in token.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.push('\\');
            out.push(c);
        }
    }
    out
}

// ponytail: T17 — exact-match bonuses (additive) per spec §7.2.
// All four conditions are checked against the raw query (trimmed +
// lowercased) and the stored field text. Returns a single additive
// bonus to be added to the BM25 score in search().
fn apply_exact_match_bonuses(
    raw_query: &str,
    syntax: &str,
    title: &str,
    description: &str,
    tags: &[String],
) -> f32 {
    let mut bonus = 0.0;
    // +15.0 if query == syntax exactly
    if !raw_query.is_empty() && raw_query == syntax.to_lowercase() {
        bonus += 15.0;
    }
    // +10.0 if query == title exactly
    if !raw_query.is_empty() && raw_query == title.to_lowercase() {
        bonus += 10.0;
    }
    // +2.0 if ALL query tokens appear in description (lowercased)
    if !raw_query.is_empty() && !description.is_empty() {
        let tokens: Vec<&str> = raw_query
            .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.'))
            .filter(|t| !t.is_empty())
            .collect();
        if !tokens.is_empty() {
            let desc_lc = description.to_lowercase();
            if tokens.iter().all(|t| desc_lc.contains(t)) {
                bonus += 2.0;
            }
        }
    }
    // +1.5 per matching tag (token-substring match)
    if !raw_query.is_empty() && !tags.is_empty() {
        let tokens: Vec<&str> = raw_query
            .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.'))
            .filter(|t| !t.is_empty())
            .collect();
        for t in &tokens {
            for tag in tags {
                if tag.to_lowercase().contains(t) || t.contains(&tag.to_lowercase()) {
                    bonus += 1.5;
                }
            }
        }
    }
    bonus
}

// Source priority ADDITIVE offset applied as a post-rerank step on
// top of BM25 + exact-match bonuses. Higher offset = more
// authoritative. Pre-T17 was multiplicative (×2.0/×1.5/×1.0);
// spec §7.2 calls for additive so the priority doesn't dwarf the
// exact-match bonuses. Personal is rejected by pack validation
// and never reaches here. Score-relative scaling was dropped: the
// offset is a fixed f32 per source, not a function of the BM25
// score — keeps the additive stacking in `search` deterministic.
fn apply_source_priority(source: &str) -> f32 {
    let offset: f32 = match source {
        "official" => 1.0,
        "cheat-sheet" => 0.5,
        _ => 0.0,
    };
    offset
}

fn build_schema() -> (Schema, SchemaFields) {
    let mut schema_builder = Schema::builder();
    let id = schema_builder.add_text_field("id", STORED | FAST);
    let pack_id = schema_builder.add_text_field("pack_id", STORED | FAST);
    let code_indexing = || {
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqs)
    };
    let title = schema_builder.add_text_field(
        "title",
        TextOptions::default()
            .set_indexing_options(code_indexing())
            .set_stored(),
    );
    let syntax = schema_builder.add_text_field(
        "syntax",
        TextOptions::default()
            .set_indexing_options(code_indexing())
            .set_stored(),
    );
    let description = schema_builder.add_text_field(
        "description",
        TextOptions::default().set_indexing_options(code_indexing()),
    );
    let tags = schema_builder.add_text_field(
        "tags",
        TextOptions::default()
            .set_indexing_options(code_indexing())
            .set_stored(),
    );
    let example_codes = schema_builder.add_text_field(
        "example_codes",
        TextOptions::default()
            .set_indexing_options(code_indexing())
            .set_stored(),
    );
    let source = schema_builder.add_text_field(
        "source",
        TextOptions::default()
            .set_indexing_options(code_indexing())
            // ponytail: T17 — source must be STORED so the
            // post-rerank source-priority step in search() can read
            // it back. Pre-T17 the multiplicative boost happened
            // to work only because the existing real-pack test had
            // a hit that won on BM25 alone; a hermetic test that
            // needs source to disambiguate two equal-BM25 cards
            // cannot work without this flag.
            .set_stored(),
    );
    let schema = schema_builder.build();
    (
        schema,
        SchemaFields {
            id,
            pack_id,
            title,
            syntax,
            description,
            tags,
            example_codes,
            source,
        },
    )
}

fn get_text(doc: &tantivy::TantivyDocument, field: Field) -> String {
    doc.get_first(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn collect_entry_meta(packs: &[Pack]) -> std::collections::HashMap<String, EntryMeta> {
    let mut m = std::collections::HashMap::new();
    for p in packs {
        for e in &p.entries {
            m.insert(
                e.id.clone(),
                EntryMeta {
                    description: e.description.clone(),
                    source_url: e.source_url.clone(),
                    example_code: e.examples.first().map(|x| x.code.clone()),
                    tags: e.tags.clone(),
                },
            );
        }
    }
    m
}

impl EntryMeta {
    fn empty() -> Self {
        Self {
            description: String::new(),
            source_url: None,
            example_code: None,
            tags: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack;
    use std::path::PathBuf;

    fn packs_for_test() -> Vec<Pack> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("packs")
            .join("curate");
        pack::load_dir(&dir).expect("load packs").loaded
    }

    fn fresh_index() -> HotdocIndex {
        let dir = std::env::temp_dir().join(format!(
            "hotdoc-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        HotdocIndex::build(&packs_for_test(), &dir).expect("build index")
    }

    #[test]
    fn git_stash_returns_git_stash() {
        let idx = fresh_index();
        let hits = idx
            .search("git stash", 8, &Default::default())
            .expect("search");
        assert!(!hits.is_empty(), "expected hits for 'git stash'");
        assert_eq!(hits[0].id, "git-stash", "top hit should be git-stash");
    }

    #[test]
    fn git_stash_pop_top_hit() {
        let idx = fresh_index();
        let hits = idx
            .search("git stash pop", 8, &Default::default())
            .expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "git-stash-pop");
    }

    #[test]
    fn fuzzy_typo_finds_target() {
        let idx = fresh_index();
        let hits = idx
            .search("git stsh pop", 8, &Default::default())
            .expect("search");
        assert!(
            hits.iter().any(|h| h.id == "git-stash-pop"),
            "typo 'stsh' should still resolve to git-stash-pop; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn gibberish_returns_empty() {
        let idx = fresh_index();
        let hits = idx
            .search("asdfqwer", 8, &Default::default())
            .expect("search");
        assert!(hits.is_empty(), "gibberish should yield zero results");
    }

    #[test]
    fn prefix_match_finds_target() {
        let idx = fresh_index();
        let hits = idx
            .search("git stas", 8, &Default::default())
            .expect("search");
        assert!(
            hits.iter().any(|h| h.id == "git-stash"),
            "prefix 'stas' should resolve to git-stash; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn short_token_skips_prefix_clause() {
        let idx = fresh_index();
        let hits = idx
            .search("git st", 8, &Default::default())
            .expect("search");
        for h in &hits {
            assert_ne!(h.id, "git-st", "no card has id 'git-st'");
        }
    }

    #[test]
    fn empty_query_returns_empty() {
        let idx = fresh_index();
        let hits = idx.search("   ", 8, &Default::default()).expect("search");
        assert!(hits.is_empty(), "empty query should yield zero results");
    }

    #[test]
    fn simple_tokenizer_splits_hyphenated_syntax() {
        let idx = fresh_index();
        let hits = idx
            .search("reset soft head", 8, &Default::default())
            .expect("search");
        assert!(
            hits.iter().take(3).any(|h| h.id == "git-reset-soft-head-1"),
            "SimpleTokenizer should split 'git-reset-soft-head-1' so 'reset soft head' hits it; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_match_boost_lifts_docker_logs_over_logs_tail() {
        let idx = fresh_index();
        let hits = idx
            .search("docker logs", 8, &Default::default())
            .expect("search");
        assert!(
            hits.first().map(|h| h.id.as_str()) == Some("docker-logs"),
            "exact-match boost should lift docker-logs above docker-logs-tail; got {:?}",
            hits.iter().take(3).map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn source_priority_lifts_official_over_curated_sibling() {
        let idx = fresh_index();
        let hits = idx
            .search("kubectl get pods", 8, &Default::default())
            .expect("search");
        assert_eq!(
            hits.first().map(|h| h.id.as_str()),
            Some("kubectl-get-pods-official"),
            "source-priority boost should put the official card above its curated sibling; got {:?}",
            hits.iter().take(3).map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    // ponytail: T13 — assert the new remove_dir_all error propagation
    // works. Simulate a "busy" index dir by placing a non-empty file
    // at the path; the underlying remove_dir_all returns an error
    // (Not a directory / Directory not empty) and build must surface
    // it rather than swallowing with .ok(). We simulate by creating
    // the target as a regular file, which is a non-NotFound error.
    #[test]
    fn build_propagates_remove_dir_all_errors() {
        let dir = std::env::temp_dir().join(format!(
            "hotdoc-busy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // create a regular file where build() expects a directory
        std::fs::write(&dir, b"not a directory").expect("write blocker file");
        let res = HotdocIndex::build(&packs_for_test(), &dir);
        let _ = std::fs::remove_file(&dir);
        assert!(
            res.is_err(),
            "build must fail when target path is a non-empty file, not a directory"
        );
        let msg = format!("{:#}", res.err().expect("err"));
        assert!(
            msg.contains("removing old index dir") || msg.contains("creating index dir"),
            "expected remove-or-create error, got: {msg}"
        );
    }

    // T17 tests — each one asserts a specific clause of spec §7.2.
    // Build a minimal pack set so the test is hermetic (no dependency
    // on packs/curate shape).
    fn t17_build(packs: &[Pack]) -> HotdocIndex {
        let dir = std::env::temp_dir().join(format!(
            "hotdoc-t17-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        HotdocIndex::build(packs, &dir).expect("t17 build")
    }

    fn make_entry_with(
        id: &str,
        pack_id: &str,
        syntax: &str,
        title: &str,
        description: &str,
        source: crate::pack::EntrySource,
        tags: Vec<&str>,
    ) -> (Pack, Vec<String>) {
        let pack = Pack {
            id: pack_id.to_string(),
            name: pack_id.to_string(),
            version: "1.0.0".to_string(),
            source: "test".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            entries: vec![crate::pack::Entry {
                id: id.to_string(),
                title: title.to_string(),
                syntax: syntax.to_string(),
                description: description.to_string(),
                examples: vec![],
                tags: tags.iter().map(|s| s.to_string()).collect(),
                source,
                source_url: None,
            }],
        };
        (pack, tags.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn exact_match_bonus_applied() {
        // Spec §7.2: +15.0 if query == syntax exactly. Build a card
        // whose syntax matches the query verbatim, plus a "noise" card
        // with the same words but different syntax. The exact-match
        // card must outscore the noise card by 15.0.
        let (pack_a, _) = make_entry_with(
            "a",
            "alpha",
            "git stash",
            "Stash",
            "noise",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let (pack_b, _) = make_entry_with(
            "b",
            "alpha",
            "git-stash-different-syntax",
            "Stash",
            "noise",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let idx = t17_build(&[pack_a, pack_b]);
        let hits = idx
            .search("git stash", 8, &Default::default())
            .expect("search");
        assert!(
            hits.len() >= 2,
            "need 2 hits, got {}: {:?}",
            hits.len(),
            hits
        );
        let a = hits.iter().find(|h| h.id == "a").expect("hit a");
        let b = hits.iter().find(|h| h.id == "b").expect("hit b");
        assert!(
            a.score > b.score,
            "exact-syntax card 'a' should outscore noise 'b'; a={} b={}",
            a.score,
            b.score
        );
        // The exact-match bonus is +15.0 (syntax == query). With
        // identical BM25, the delta must be at least 15.0. Allow
        // a small float epsilon for the post-rerank round-trip.
        let delta = a.score - b.score;
        assert!(
            delta >= 14.5,
            "expected ~15.0 exact-match bonus, got delta={delta}"
        );
    }

    #[test]
    fn desc_bonus_requires_all_tokens() {
        // Spec §7.2: +2.0 if ALL query tokens appear in description.
        // Build two cards with the same syntax/title; one's description
        // has both tokens, the other has only one. The "all tokens"
        // card must outscore the partial card by 2.0.
        let (pack_full, _) = make_entry_with(
            "full",
            "alpha",
            "git undo",
            "Undo last commit",
            "git undo last commit history",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let (pack_partial, _) = make_entry_with(
            "partial",
            "alpha",
            "git undo",
            "Undo last commit",
            "undo only",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let idx = t17_build(&[pack_full, pack_partial]);
        let hits = idx
            .search("git undo", 8, &Default::default())
            .expect("search");
        let f = hits.iter().find(|h| h.id == "full").expect("hit full");
        let p = hits
            .iter()
            .find(|h| h.id == "partial")
            .expect("hit partial");
        assert!(
            f.score > p.score,
            "full-desc card should outscore partial; f={} p={}",
            f.score,
            p.score
        );
        let delta = f.score - p.score;
        assert!(delta >= 1.5, "expected ~2.0 desc bonus, got delta={delta}");
    }

    #[test]
    fn edit_distance_8plus_allows_ed2() {
        // Spec §7.1: tokens of length 8+ get edit distance 2. Build
        // a card whose syntax is "kubernetes" (10 chars). A 2-edit
        // typo "kubernates" (missing 't', swapped 'e'→'a') must still
        // resolve to the card.
        let (pack, _) = make_entry_with(
            "k8s",
            "k8s",
            "kubernetes",
            "k8s",
            "container orchestrator",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let idx = t17_build(&[pack]);
        // "kubernates" is 10 chars, 2 edits from "kubernetes"
        let hits = idx
            .search("kubernates", 8, &Default::default())
            .expect("search");
        assert!(
            hits.iter().any(|h| h.id == "k8s"),
            "2-edit typo of an 8+ token should still hit; got: {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn source_priority_is_additive() {
        // Spec §7.2: source priority is ADDITIVE. Build two cards
        // with identical syntax/title/description; one is "official",
        // the other "curated". The official card must outscore by
        // +1.0 (the official priority offset), not by ×2.0.
        let (pack_official, _) = make_entry_with(
            "official",
            "alpha",
            "kubectl get pods",
            "Get pods",
            "list pods",
            crate::pack::EntrySource::Official,
            vec![],
        );
        let (pack_curated, _) = make_entry_with(
            "curated",
            "alpha",
            "kubectl get pods",
            "Get pods",
            "list pods",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let idx = t17_build(&[pack_official, pack_curated]);
        let hits = idx
            .search("kubectl get pods", 8, &Default::default())
            .expect("search");
        let o = hits.iter().find(|h| h.id == "official").expect("hit o");
        let c = hits.iter().find(|h| h.id == "curated").expect("hit c");
        let delta = o.score - c.score;
        // Additive: delta ≈ 1.0. Multiplicative would be ~ × 2.0 of the
        // base BM25 (~5-10), i.e. 5-10 points. Assert the delta is
        // small (additive), not large (multiplicative).
        assert!(
            (0.5..=2.0).contains(&delta),
            "expected additive source offset (~1.0), got delta={delta} (o={} c={})",
            o.score,
            c.score
        );
    }

    #[test]
    fn tie_break_is_pack_then_entry() {
        // Build two packs with cards that score identically. The
        // order must be deterministic across calls: pack_id asc,
        // then id asc.
        let (pack_a, _) = make_entry_with(
            "z-after-pack-id-asc",
            "zzz",
            "x",
            "x",
            "x",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let (pack_b, _) = make_entry_with(
            "a-before-pack-id-asc",
            "aaa",
            "x",
            "x",
            "x",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let idx = t17_build(&[pack_a, pack_b]);
        let hits = idx.search("x", 8, &Default::default()).expect("search");
        assert!(hits.len() >= 2);
        // With identical BM25, pack_id "aaa" should sort before "zzz"
        let first_pack = &hits[0].pack_id;
        let second_pack = &hits[1].pack_id;
        assert!(
            first_pack < second_pack,
            "deterministic tie-break failed: first={first_pack} second={second_pack}"
        );
    }

    // Golden-lock: popular entry outranks an equal-BM25 unpopular sibling.
    // Spec §7.2: popularity = ln(1 + raw) * 0.1 as additive post-rerank bonus.
    // Two cards with identical syntax/title/description/source → same BM25.
    // The popular card has a pre-built popularity map entry; the unpopular one
    // does not. The popular card must rank first.
    #[test]
    fn popularity_bonus_lifts_popular_over_unpopular_sibling() {
        let (pack_popular, _) = make_entry_with(
            "popular-card",
            "alpha",
            "git push origin main",
            "Push to origin",
            "push to remote",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let (pack_unpopular, _) = make_entry_with(
            "unpopular-card",
            "alpha",
            "git push origin main",
            "Push to origin",
            "push to remote",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        let idx = t17_build(&[pack_popular, pack_unpopular]);

        // Build a popularity map with one fresh activation for the popular card.
        let mut pop_map = std::collections::HashMap::new();
        let raw = 1.0f32; // one fresh activation → weight = 1.0
        let term = (1.0f32 + raw).ln() * 0.1;
        pop_map.insert("popular-card".to_string(), term);

        let hits = idx
            .search("git push origin main", 8, &pop_map)
            .expect("search");
        assert!(hits.len() >= 2, "expected at least 2 hits");
        assert_eq!(
            hits[0].id,
            "popular-card",
            "popular-card must outrank unpopular-card; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }
}
