use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::{
    schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED},
    Index, IndexReader, IndexWriter, ReloadPolicy,
};

use crate::pack::{Entry, Pack};

// ponytail: 5.3. Sidecar file name for the full corpus (pairs of
// (pack_id, Entry)). Mirrors the entry_meta sidecar pattern — lets
// `open()` repopulate the in-memory ranker's working set without
// re-parsing pack JSONs on every CLI invocation.
const ENTRIES_SIDECAR: &str = "hotdoc_entries.json";

// ponytail: below this corpus size, the full in-memory scan (p50≈2ms at
// 731 entries, 8× under NFR-3) is already optimal and Tantivy retrieval
// can only LOSE recall (BM25 misses typos the ranker would rescue). So
// retrieval is dormant here and engages only when the corpus grows past
// the point where full-scan would threaten the latency budget.
const RETRIEVAL_FULLSCAN_MAX: usize = 5000;
// ponytail: when the candidate set is this thin, fall back to a full scan
// so the ranker's fuzzy/prefix rescue still sees every entry.
const RETRIEVAL_MIN_CANDIDATES: usize = 16;
// ponytail: Tantivy candidate cap — generous so the ranker, not BM25,
// decides order.
const RETRIEVAL_CANDIDATE_CAP: usize = 256;

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
    // ponytail: 5.3 — `_index`, `reader`, and `fields` are kept on
    // the struct for the lifetime of the tantivy index (writer +
    // reader hold filesystem handles). Tantivy retrieval is now WIRED
    // (Task 2.6, architecture doc §7 Option B): `reader`/`fields` are
    // load-bearing — `retrieve_candidates()` reads them to supply BM25
    // candidates ABOVE RETRIEVAL_FULLSCAN_MAX. At v1 scale the corpus
    // is below the threshold so retrieval stays dormant and search()
    // takes the full-scan branch (golden byte-identical).
    _index: Index,
    reader: IndexReader,
    fields: SchemaFields,
    // ponytail: 5.3 — full corpus for the in-memory ranker. Pairs of
    // (pack_id, Entry). Persisted to hotdoc_entries.json sidecar so
    // `open()` doesn't have to re-parse pack JSONs on every CLI run.
    // Tantivy stays in the struct as a retrieval-side index for
    // future use; search() no longer touches it.
    entries: Vec<(String, Entry)>,
    // ponytail: normalized corpus built ONCE at build()/open(). The
    // search hot path scores against this slice rather than re-tokenizing
    // 731 entries per keystroke (the prior per-query normalize was the
    // real latency cost, not the scan itself).
    normalized: Vec<crate::ranker::NormalizedEntry>,
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
        // ponytail: 5.3 — also persist the full corpus. The
        // in-memory ranker needs the entire (pack_id, Entry) set on
        // every search; loading packs from disk per query would
        // blow the NFR-3 latency budget. Pair shape matches
        // `ranker::score_query`'s input.
        let entries: Vec<(String, Entry)> = packs
            .iter()
            .flat_map(|p| p.entries.iter().map(|e| (p.id.clone(), e.clone())))
            .collect();
        write_entries_sidecar(path, &entries).context("writing entries sidecar")?;
        let normalized = build_normalized(&entries);
        Ok(Self {
            _index: index,
            reader,
            fields,
            entries,
            normalized,
        })
    }

    // ponytail: T16. After a successful tantivy build, mirror the
    // pack/entry set into SQLite so `pinned::list` (which JOINs the
    // entries table) renders real card content and `> <pack_id>`
    // palette filter validation has a real set to check against.
    // The upsert is full-replace (delete then insert in one tx) so a
    // mid-build failure can't leave a half-populated entries table.
    // Caller decides when to invoke — build() stays tantivy-only.
    /// ponytail: true entry count = number of (pack_id, Entry) pairs, NOT
    /// a deduped id-map length. Two packs sharing an entry id both count;
    /// this equals `sum(packs.entries.len())`. The SQLite IPC count dedups
    /// by id (`ON CONFLICT(id)`), so the two agree only when entry ids are
    /// globally unique — which real packs guarantee.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
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

    // ponytail: Approach B (sidecar JSON) — the in-memory ranker's corpus
    // can't ride in tantivy's own meta.json (a schema descriptor we don't
    // own), so build() writes hotdoc_entries.json next to the tantivy
    // index and open() reads it back. Signature stays (path) — no upstream
    // call-site changes. A missing or version-mismatched sidecar → Err so
    // the Tauri resolver's open-failure fallthrough (in
    // index_resolver::resolve_one) rebuilds. Direct CLI users get a clear
    // error pointing them at `hotdoc-cli index`.
    pub fn open(path: &Path) -> Result<Self> {
        let index = Index::open_in_dir(path).context("opening index dir")?;
        let fields = resolve_fields(&index.schema()).context("resolving schema fields")?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let entries = read_entries_sidecar(path).context("reading entries sidecar")?;
        let normalized = build_normalized(&entries);
        Ok(Self {
            _index: index,
            reader,
            fields,
            entries,
            normalized,
        })
    }

    /// Deterministic, deduped pack_ids over the FULL corpus. Tool
    /// detection in `parse_query` depends entirely on this set, so it
    /// must always be derived from `self.normalized` (never a candidate
    /// subset) — the M1 invariant.
    fn full_corpus_pack_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self.normalized.iter().map(|n| n.pack_id.clone()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// BM25 query terms (intents + options) drawn from an already-parsed
    /// query. Pure projection — does NOT parse, so `search()` can parse
    /// the query exactly once (M2).
    fn bm25_terms_of(parsed: &crate::ranker::ParsedQuery) -> Vec<String> {
        let mut terms: Vec<String> = parsed.intents.clone();
        terms.extend(parsed.options.iter().cloned());
        terms
    }

    /// Extract BM25 query terms from a raw query string. Test seam reused
    /// by the injectable-threshold tests (`retrieve_candidates_with_max`).
    #[cfg(test)]
    fn bm25_terms_for(&self, raw: &str) -> Vec<String> {
        let parsed = crate::ranker::parse_query(raw, &self.full_corpus_pack_ids());
        Self::bm25_terms_of(&parsed)
    }

    /// Tantivy BM25 candidate retrieval with an injectable fullscan threshold.
    /// Returns indices into `self.normalized`, or `None` to signal "score the
    /// full corpus" (corpus ≤ fullscan_max, thin candidates, or empty terms).
    /// Production callers use `retrieve_candidates`; tests can pass `fullscan_max=0`
    /// to force the BM25 path on a small hermetic corpus.
    fn retrieve_candidates_with_max(
        &self,
        terms: &[String],
        fullscan_max: usize,
    ) -> Option<Vec<usize>> {
        use tantivy::collector::TopDocs;
        use tantivy::query::{BooleanQuery, Occur, Query, TermQuery};
        use tantivy::schema::document::Value;
        use tantivy::schema::IndexRecordOption;
        use tantivy::tokenizer::TokenStream;
        use tantivy::Term;

        if self.normalized.len() <= fullscan_max || terms.is_empty() {
            return None;
        }
        let searcher = self.reader.searcher();
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for t in terms {
            for field in [
                self.fields.title,
                self.fields.syntax,
                self.fields.tags,
                self.fields.description,
            ] {
                // M5: build candidate terms through the field's OWN analyzer
                // so query tokens fold identically to what Tantivy indexed.
                // `parse_query` keeps `--hard`/`foo-bar` as one token and
                // retains >40-byte tokens, but the index analyzer splits on
                // `-` (SimpleTokenizer) and drops long tokens
                // (RemoveLongFilter(40)). Feeding the raw token to
                // `Term::from_field_text` looks for a literal the index never
                // stored, silently losing candidates on the >5000 BM25 path.
                // If the analyzer is unavailable, skip this field (a clause
                // less → at worst a full-scan fallback, never a wrong hit).
                let mut analyzer = match self._index.tokenizer_for_field(field) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let mut stream = analyzer.token_stream(t);
                while stream.advance() {
                    let term = Term::from_field_text(field, &stream.token().text);
                    clauses.push((
                        Occur::Should,
                        Box::new(TermQuery::new(term, IndexRecordOption::WithFreqs)),
                    ));
                }
            }
        }
        if clauses.is_empty() {
            return None;
        }
        let query = BooleanQuery::new(clauses);
        let top = searcher
            .search(&query, &TopDocs::with_limit(RETRIEVAL_CANDIDATE_CAP))
            .ok()?;
        // ponytail: map retrieved doc ids back to normalized indices via the
        // stored entry id. Build a one-time id→index map.
        let id_to_idx: std::collections::HashMap<&str, usize> = self
            .normalized
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let id_field = self.fields.id;
        let mut idxs = Vec::with_capacity(top.len());
        for (_score, addr) in top {
            let doc: tantivy::TantivyDocument = searcher.doc(addr).ok()?;
            if let Some(val) = doc.get_first(id_field).and_then(|v| v.as_str()) {
                if let Some(&i) = id_to_idx.get(val) {
                    idxs.push(i);
                }
            }
        }
        if idxs.len() < RETRIEVAL_MIN_CANDIDATES {
            return None; // thin candidates → fall back to full scan
        }
        Some(idxs)
    }

    /// Tantivy BM25 candidate retrieval. One-line wrapper around
    /// `retrieve_candidates_with_max` using the production threshold.
    fn retrieve_candidates(&self, terms: &[String]) -> Option<Vec<usize>> {
        self.retrieve_candidates_with_max(terms, RETRIEVAL_FULLSCAN_MAX)
    }

    pub fn search(&self, raw_query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let raw = raw_query.trim();
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        // ponytail: parse ONCE against full-corpus pack_ids. The same
        // `parsed` drives BM25 retrieval AND scoring, so tool detection is
        // stable regardless of which rows survive retrieval (M1), and the
        // dormant path no longer parses twice (M2).
        let parsed = crate::ranker::parse_query(raw, &self.full_corpus_pack_ids());
        let terms = Self::bm25_terms_of(&parsed);

        let ranked = match self.retrieve_candidates(&terms) {
            Some(idxs) => {
                let subset: Vec<crate::ranker::NormalizedEntry> = idxs
                    .into_iter()
                    .map(|i| self.normalized[i].clone())
                    .collect();
                crate::ranker::score_query_normalized_with(&subset, &parsed)
            }
            None => crate::ranker::score_query_normalized_with(&self.normalized, &parsed),
        };
        // ponytail: 5.3 — O(1) entry lookup by id for SearchHit
        // construction. RankedHit carries NormalizedEntry (id +
        // pack_id + token sets) but not the original strings; we need
        // the Entry for description/syntax/title/source_url/example_code.
        // 731 entries × ~30 bytes per key is a 20 KB hashmap — trivial.
        let by_id: std::collections::HashMap<&str, &Entry> = self
            .entries
            .iter()
            .map(|(_, e)| (e.id.as_str(), e))
            .collect();
        let hits: Vec<SearchHit> = ranked
            .into_iter()
            .take(limit)
            .filter_map(|h| {
                let entry = by_id.get(h.entry.id.as_str())?;
                Some(SearchHit {
                    id: entry.id.clone(),
                    pack_id: h.entry.pack_id.clone(),
                    title: entry.title.clone(),
                    syntax: entry.syntax.clone(),
                    description: entry.description.clone(),
                    source: h.entry.source.as_str().to_string(),
                    source_url: entry.source_url.clone(),
                    example_code: entry.examples.first().map(|ex| ex.code.clone()),
                    score: h.score as f32,
                })
            })
            .collect();
        Ok(hits)
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

const ENTRIES_SIDECAR_VERSION: u32 = 1;

// ponytail: 5.3 — entries sidecar. Wrapped in a struct so the JSON
// shape is self-documenting; serde otherwise serialises
// Vec<(String, Entry)> as a flat array of [pack_id, entry] pairs,
// which is harder to read and forward-incompatible (no place to add
// metadata later without breaking readers).
#[derive(Serialize, Deserialize)]
struct EntriesSidecar {
    // ponytail: bumped whenever the on-disk Entry shape changes in a way
    // a prior reader would misinterpret. open() rejects a mismatch so the
    // resolver rebuilds rather than ranking a stale/misread corpus.
    version: u32,
    entries: Vec<(String, Entry)>,
}

fn write_entries_sidecar(dir: &Path, entries: &[(String, Entry)]) -> Result<()> {
    let path = dir.join(ENTRIES_SIDECAR);
    let tmp = dir.join(format!("{ENTRIES_SIDECAR}.tmp"));
    let sidecar = EntriesSidecar {
        version: ENTRIES_SIDECAR_VERSION,
        entries: entries.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&sidecar).context("serializing entries sidecar")?;
    // ponytail: write to a sibling temp then rename — rename is atomic on
    // the same filesystem, so a crash/ENOSPC mid-write can never leave a
    // truncated sidecar that open() would parse as a partial corpus.
    std::fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

fn read_entries_sidecar(dir: &Path) -> Result<Vec<(String, Entry)>> {
    let path = dir.join(ENTRIES_SIDECAR);
    let raw = std::fs::read(&path).with_context(|| {
        format!(
            "reading {} (re-run `hotdoc-cli index` to rebuild)",
            path.display()
        )
    })?;
    let sidecar: EntriesSidecar =
        serde_json::from_slice(&raw).context("parsing entries sidecar")?;
    if sidecar.version != ENTRIES_SIDECAR_VERSION {
        anyhow::bail!(
            "entries sidecar version {} != expected {} (re-run `hotdoc-cli index`)",
            sidecar.version,
            ENTRIES_SIDECAR_VERSION
        );
    }
    Ok(sidecar.entries)
}

fn build_normalized(entries: &[(String, Entry)]) -> Vec<crate::ranker::NormalizedEntry> {
    entries
        .iter()
        .map(|(pid, e)| crate::ranker::normalize_entry(pid, e))
        .collect()
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
        let hits = idx.search("git stash", 8).expect("search");
        assert!(!hits.is_empty(), "expected hits for 'git stash'");
        assert_eq!(hits[0].id, "git-stash", "top hit should be git-stash");
    }

    #[test]
    fn git_stash_pop_top_hit() {
        let idx = fresh_index();
        let hits = idx.search("git stash pop", 8).expect("search");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "git-stash-pop");
    }

    #[test]
    fn fuzzy_typo_finds_target() {
        let idx = fresh_index();
        let hits = idx.search("git stsh pop", 8).expect("search");
        assert!(
            hits.iter().any(|h| h.id == "git-stash-pop"),
            "typo 'stsh' should still resolve to git-stash-pop; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn prefix_match_finds_target() {
        let idx = fresh_index();
        let hits = idx.search("git stas", 8).expect("search");
        assert!(
            hits.iter().any(|h| h.id == "git-stash"),
            "prefix 'stas' should resolve to git-stash; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn short_token_skips_prefix_clause() {
        let idx = fresh_index();
        let hits = idx.search("git st", 8).expect("search");
        for h in &hits {
            assert_ne!(h.id, "git-st", "no card has id 'git-st'");
        }
    }

    #[test]
    fn empty_query_returns_empty() {
        let idx = fresh_index();
        let hits = idx.search("   ", 8).expect("search");
        assert!(hits.is_empty(), "empty query should yield zero results");
    }

    #[test]
    fn simple_tokenizer_splits_hyphenated_syntax() {
        let idx = fresh_index();
        let hits = idx.search("reset soft head", 8).expect("search");
        assert!(
            hits.iter().take(3).any(|h| h.id == "git-reset-soft-head-1"),
            "SimpleTokenizer should split 'git-reset-soft-head-1' so 'reset soft head' hits it; got {:?}",
            hits.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_match_boost_lifts_docker_logs_over_logs_tail() {
        let idx = fresh_index();
        let hits = idx.search("docker logs", 8).expect("search");
        // With the full 18-pack corpus, BM25 TF variance can push
        // cross-pack entries (e.g. aws-ecr-login has "docker" twice) above
        // docker-logs at position 1. The invariant we lock is that
        // docker-logs appears in the top 5 and above docker-logs-tail.
        let docker_logs_pos = hits.iter().position(|h| h.id == "docker-logs");
        let docker_logs_tail_pos = hits.iter().position(|h| h.id == "docker-logs-tail");
        assert!(
            docker_logs_pos.is_some_and(|p| p < 5),
            "docker-logs should appear in top 5; got {:?}",
            hits.iter().take(5).map(|h| &h.id).collect::<Vec<_>>()
        );
        if let (Some(a), Some(b)) = (docker_logs_pos, docker_logs_tail_pos) {
            assert!(a < b, "docker-logs should rank above docker-logs-tail");
        }
    }

    #[test]
    fn source_priority_lifts_official_over_curated_sibling() {
        let idx = fresh_index();
        let hits = idx.search("kubectl get pods", 8).expect("search");
        // kubectl-get-pods is source:"official" in the curated pack.
        // It should appear as the top result, beating any curated siblings.
        assert_eq!(
            hits.first().map(|h| h.id.as_str()),
            Some("kubectl-get-pods"),
            "source-priority boost should put the official card above its curated sibling; got {:?}",
            hits.iter().take(3).map(|h| &h.id).collect::<Vec<_>>()
        );
        // Confirm it is indeed the official entry.
        assert_eq!(hits[0].source, "official");
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
                aliases: vec![],
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
        let hits = idx.search("git stash", 8).expect("search");
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
        let hits = idx.search("kubernates", 8).expect("search");
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
        let hits = idx.search("kubectl get pods", 8).expect("search");
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
    fn open_rejects_wrong_sidecar_version() {
        let dir = std::env::temp_dir().join(format!(
            "hotdoc-ver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        HotdocIndex::build(&packs_for_test(), &dir).expect("build");
        // Corrupt the sidecar version.
        let p = dir.join(ENTRIES_SIDECAR);
        let raw = std::fs::read_to_string(&p).expect("read sidecar");
        let bumped = raw.replacen("\"version\": 1", "\"version\": 999", 1);
        std::fs::write(&p, bumped).expect("rewrite sidecar");
        let res = HotdocIndex::open(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(res.is_err(), "open must reject an unknown sidecar version");
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
        let hits = idx.search("x", 8).expect("search");
        assert!(hits.len() >= 2);
        // With identical BM25, pack_id "aaa" should sort before "zzz"
        let first_pack = &hits[0].pack_id;
        let second_pack = &hits[1].pack_id;
        assert!(
            first_pack < second_pack,
            "deterministic tie-break failed: first={first_pack} second={second_pack}"
        );
    }

    #[test]
    fn retrieve_candidates_with_max_forces_bm25_branch() {
        // Small corpus — well below RETRIEVAL_FULLSCAN_MAX but with enough
        // "git" entries (21 total) that the BM25 result set survives the
        // RETRIEVAL_MIN_CANDIDATES (16) thin-candidate guard.
        //
        // Pack id is "alpha" (NOT "git") so parse_query does NOT consume
        // "git" as the `tool` slot.  That keeps "git", "commit", "amend"
        // all in `intents`, giving BM25 terms that match every entry.
        let mut packs = Vec::new();
        for i in 0..20_usize {
            let (p, _) = make_entry_with(
                &format!("git-cmd-{i}"),
                "alpha",
                &format!("git cmd-{i}"),
                &format!("Git Cmd {i}"),
                "git utility",
                crate::pack::EntrySource::Curated,
                vec![],
            );
            packs.push(p);
        }
        let (target, _) = make_entry_with(
            "git-commit-amend",
            "alpha",
            "git commit --amend",
            "Git Commit Amend",
            "amend the most recent commit",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        packs.push(target);
        let idx = t17_build(&packs);

        let terms = idx.bm25_terms_for("git commit amend");
        // fullscan_max=0 → corpus.len() (21) > 0 → BM25 branch engages.
        let cands = idx.retrieve_candidates_with_max(&terms, 0);
        assert!(
            cands.is_some(),
            "must take BM25 branch when corpus.len() > fullscan_max"
        );
        let ids: Vec<&str> = cands
            .expect("Some")
            .iter()
            .map(|&i| idx.normalized[i].id.as_str())
            .collect();
        assert!(
            ids.contains(&"git-commit-amend"),
            "BM25 candidates must include the target; got {ids:?}"
        );
        // Real threshold on this small corpus → full scan (dormant branch).
        assert!(
            idx.retrieve_candidates(&terms).is_none(),
            "real threshold must keep retrieval dormant on a small corpus"
        );
    }

    #[test]
    fn bm25_terms_match_hyphenated_tokens() {
        // M5: BM25 query terms must pass through the index analyzer so a
        // hyphenated query option like `--hard` folds to the same `hard`
        // token Tantivy indexed. Building `Term::from_field_text(field,
        // "--hard")` directly (pre-fix) looks for a literal `--hard` token
        // that the SimpleTokenizer never produces → the target is invisible
        // to the BM25 branch even though its syntax clearly contains it.
        //
        // pack_id "git" → parse_query consumes "git" as the tool slot, so
        // the live BM25 terms are ["reset", "--hard"]. The 20 filler rows
        // match "reset" (NOT "hard"), clearing RETRIEVAL_MIN_CANDIDATES so
        // the BM25 branch returns Some in BOTH worlds. The ONLY bridge to
        // the target is the analyzer mapping "--hard" → "hard"; its
        // searchable fields deliberately omit a bare "reset"/"hard" query
        // term so its presence hinges solely on the hyphen fold.
        let mut packs = Vec::new();
        for i in 0..20_usize {
            let (p, _) = make_entry_with(
                &format!("git-reset-hunk-{i}"),
                "git",
                &format!("git reset hunk-{i}"),
                &format!("Reset Hunk {i}"),
                "unstage a reset hunk",
                crate::pack::EntrySource::Curated,
                vec![],
            );
            packs.push(p);
        }
        let (target, _) = make_entry_with(
            "git-reset-hard",
            "git",
            "git switch --hard <ref>",
            "Hard Switch",
            "force overwrite the working tree using the hard flag",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        packs.push(target);
        let idx = t17_build(&packs);

        let terms = idx.bm25_terms_for("git reset --hard");
        let cands = idx
            .retrieve_candidates_with_max(&terms, 0)
            .expect("bm25 branch");
        let ids: Vec<&str> = cands
            .iter()
            .map(|&i| idx.normalized[i].id.as_str())
            .collect();
        assert!(
            ids.contains(&"git-reset-hard"),
            "hyphenated `--hard` must fold to the indexed `hard` token; got {ids:?}"
        );
    }

    #[test]
    fn scoring_uses_full_corpus_pack_ids_above_threshold() {
        // M1: the above-threshold path scores a candidate SUBSET. Tool
        // detection must use pack_ids from the FULL corpus, never the
        // subset — otherwise a query token like "beta" goes unrecognized
        // whenever no candidate row happens to be in the "beta" pack, and
        // the TOOL_HARD_FILTER pack scoping silently disappears.
        //
        // Full corpus: many "alpha" rows + one "beta" row. The candidate
        // subset is deliberately beta-sparse (alpha rows only), mimicking
        // a BM25 result set that missed the weak beta entry.
        let mut full: Vec<crate::ranker::NormalizedEntry> = Vec::new();
        let mut alpha_subset: Vec<crate::ranker::NormalizedEntry> = Vec::new();
        for i in 0..5_usize {
            let (p, _) = make_entry_with(
                &format!("alpha-thing-{i}"),
                "alpha",
                &format!("thing widget {i}"),
                &format!("Thing {i}"),
                "configure a thing",
                crate::pack::EntrySource::Curated,
                vec![],
            );
            let n = crate::ranker::normalize_entry(&p.id, &p.entries[0]);
            full.push(n.clone());
            alpha_subset.push(n);
        }
        let (beta_pack, _) = make_entry_with(
            "beta-thing",
            "beta",
            "beta thing",
            "Beta Thing",
            "a beta thing",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        full.push(crate::ranker::normalize_entry(
            &beta_pack.id,
            &beta_pack.entries[0],
        ));

        // Full-corpus pack_ids include "beta" → parse recognizes the tool.
        let full_pack_ids: Vec<String> = {
            let mut v: Vec<String> = full.iter().map(|n| n.pack_id.clone()).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        let parsed = crate::ranker::parse_query("beta thing", &full_pack_ids);
        assert_eq!(
            parsed.tool.as_deref(),
            Some("beta"),
            "full-corpus parse must recognize 'beta' as the tool"
        );

        // Score the beta-sparse subset with the full-corpus parse. Every
        // alpha row must be hard-filtered (NEG_INFINITY → dropped), so no
        // finite-scored hit may belong to any pack other than "beta".
        let hits = crate::ranker::score_query_normalized_with(&alpha_subset, &parsed);
        assert!(
            hits.iter().all(|h| h.entry.pack_id == "beta"),
            "alpha rows must be hard-filtered when tool=beta is recognized \
             from the full corpus; leaked: {:?}",
            hits.iter().map(|h| &h.entry.id).collect::<Vec<_>>()
        );
        // And the alpha-only subset yields zero survivors (tool filter bites).
        assert!(
            hits.is_empty(),
            "beta-sparse subset must produce no finite hits under tool=beta"
        );
    }

    #[test]
    fn retrieval_engages_above_threshold_and_fuzzy_falls_back() {
        // Build a synthetic corpus larger than RETRIEVAL_FULLSCAN_MAX so
        // the tantivy candidate path activates. A clean query must hit its
        // card via candidates; a typo'd query must still resolve via the
        // full-scan fallback (fuzzy recall preserved).
        let mut packs = Vec::new();
        for i in 0..(super::RETRIEVAL_FULLSCAN_MAX + 50) {
            let (p, _) = make_entry_with(
                &format!("card-{i}"),
                "alpha",
                &format!("widget {i} configure"),
                &format!("Widget {i}"),
                "configure a widget",
                crate::pack::EntrySource::Curated,
                vec![],
            );
            packs.push(p);
        }
        // A distinctive target for the fuzzy case.
        let (target, _) = make_entry_with(
            "kubernetes-card",
            "beta",
            "kubernetes orchestrate",
            "Kubernetes",
            "container orchestrator",
            crate::pack::EntrySource::Curated,
            vec![],
        );
        packs.push(target);
        let idx = t17_build(&packs);

        // Clean query → candidate path returns the right card.
        let clean = idx.search("widget 7 configure", 8).expect("search");
        assert!(
            clean.iter().any(|h| h.id == "card-7"),
            "clean query must find its card"
        );

        // Typo query (BM25 won't match "kubernates") → fallback full-scan
        // lets the ranker's fuzzy rescue resolve it.
        let typo = idx.search("kubernates", 8).expect("search");
        assert!(
            typo.iter().any(|h| h.id == "kubernetes-card"),
            "typo must still resolve via fallback; got {:?}",
            typo.iter().map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    // M1 end-to-end guard (production search() path).
    //
    // Background: before commit 6b6989e, search() derived pack_ids from the
    // BM25 candidate SUBSET rather than the full corpus. When the target tool
    // pack ("ziptool") was absent from the BM25 candidates, parse_query could
    // not recognize "ziptool" as a tool token, so TOOL_HARD_FILTER was never
    // applied and the noise pack's entries leaked into results.
    //
    // Why the BM25 subset is alpha-only here:
    // The target pack ("ziptool") has 3 entries that contain NO "download"
    // token in any indexed field (title, syntax, description, tags). BM25
    // retrieval searches for "download" (the only intent term on the fixed
    // path) and returns the top-256 alpha entries, all of which have
    // "download" prominently in title + syntax. The 3 ziptool entries score
    // 0 for the "download" BM25 term and never appear in candidates.
    //
    // Fixed path (search parses against full corpus):
    //   full_corpus_pack_ids = ["alpha", "ziptool"]
    //   parse_query("ziptool download", ...) → tool="ziptool", intents=["download"]
    //   BM25 subset = 256 alpha entries
    //   score subset with tool="ziptool" → TOOL_HARD_FILTER for every alpha
    //   entry → zero finite hits. Assertion holds vacuously (empty slice).
    //
    // Buggy path (search parses against subset):
    //   subset pack_ids = ["alpha"]
    //   parse_query("ziptool download", ["alpha"]) → tool=None ("ziptool"
    //   absent from subset), intents=["ziptool","download"]
    //   Score 256 alpha entries with tool=None → no hard filter.
    //   Per alpha entry: "download" → INTENT_EXACT(25) + field_weighted(8);
    //   "ziptool" → INTENT_MISSING(-25). Total = 8 > 0 → finite.
    //   256 alpha entries survive → assertion `pack_id == "ziptool"` fails.
    #[test]
    fn above_threshold_search_scopes_hits_to_target_pack_m1() {
        let dir = tempfile::tempdir().expect("create tempdir for m1 e2e test");
        let mut packs = Vec::new();

        // Noise pack: RETRIEVAL_FULLSCAN_MAX+50 entries, all with "download"
        // in title + syntax + description → strong, uniform BM25 signal.
        // This guarantees the BM25 candidate set is well above the 16-entry
        // thin-candidate floor and is entirely populated by alpha entries.
        for i in 0..(super::RETRIEVAL_FULLSCAN_MAX + 50) {
            let (p, _) = make_entry_with(
                &format!("alpha-{i}"),
                "alpha",
                &format!("alpha download cmd {i}"),
                &format!("Alpha Download {i}"),
                "download a file from the network",
                crate::pack::EntrySource::Curated,
                vec![],
            );
            packs.push(p);
        }

        // Target pack: 3 entries with NO "download" in any indexed field.
        // They will not appear in the BM25 candidate set for "download",
        // making the candidate subset alpha-only — the M1 adversarial shape.
        for i in 0..3_usize {
            let (p, _) = make_entry_with(
                &format!("ziptool-archive-{i}"),
                "ziptool",
                &format!("ziptool archive files {i}"),
                &format!("Ziptool Archive {i}"),
                "compress and extract archive files",
                crate::pack::EntrySource::Curated,
                vec![],
            );
            packs.push(p);
        }

        let idx = HotdocIndex::build(&packs, dir.path()).expect("build m1 e2e index");

        assert!(
            idx.entry_count() > super::RETRIEVAL_FULLSCAN_MAX,
            "corpus must exceed RETRIEVAL_FULLSCAN_MAX to exercise the BM25 \
             candidate branch; got {}",
            idx.entry_count()
        );

        // Production search() call — no injectable seam, no hand-crafted
        // ParsedQuery. This is the path the fix guards.
        let hits = idx.search("ziptool download", 8).expect("search m1 e2e");

        // Every returned hit must belong to the target pack. On the fixed
        // path this holds vacuously (the alpha-only BM25 subset is entirely
        // hard-filtered by tool="ziptool", leaving zero finite hits). On the
        // buggy path 256 alpha entries survive with score ≈ 8.0, and their
        // pack_id "alpha" != "ziptool" causes this assertion to fail.
        assert!(
            hits.iter().all(|h| h.pack_id == "ziptool"),
            "search must not return non-ziptool hits (M1 regression); got: {:?}",
            hits.iter()
                .map(|h| (h.id.as_str(), h.pack_id.as_str()))
                .collect::<Vec<_>>()
        );
    }
}
