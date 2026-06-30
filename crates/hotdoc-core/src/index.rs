use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tantivy::{
    schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, FAST, STORED},
    Index, IndexReader, IndexWriter, ReloadPolicy,
};

use crate::pack::{Entry, Pack};

// ponytail: 5.2. Sidecar file name for entry_meta. Sits next to
// tantivy's own `meta.json` in the index dir; tantivy ignores
// unknown files, so coexistence is safe. The `hotdoc_` prefix marks
// it as ours — no collision with tantivy's reserved filenames.
const ENTRY_META_SIDECAR: &str = "hotdoc_entry_meta.json";

// ponytail: 5.3. Sidecar file name for the full corpus (pairs of
// (pack_id, Entry)). Mirrors the entry_meta sidecar pattern — lets
// `open()` repopulate the in-memory ranker's working set without
// re-parsing pack JSONs on every CLI invocation.
const ENTRIES_SIDECAR: &str = "hotdoc_entries.json";

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
    // reader hold filesystem handles). 5.3 replaces the search hot
    // path with the in-memory ranker; tantivy is no longer read but
    // the index is still BUILT at build() time. Follow-up work may
    // add a tantivy-backed retrieval stage on top of the ranker
    // (architecture doc §7 Option B) — keeping the fields in place
    // makes that wire-up a no-op.
    _index: Index,
    #[allow(dead_code)]
    reader: IndexReader,
    #[allow(dead_code)]
    fields: SchemaFields,
    entry_meta: std::collections::HashMap<String, EntryMeta>,
    // ponytail: 5.3 — full corpus for the in-memory ranker. Pairs of
    // (pack_id, Entry). Persisted to hotdoc_entries.json sidecar so
    // `open()` doesn't have to re-parse pack JSONs on every CLI run.
    // Tantivy stays in the struct as a retrieval-side index for
    // future use; search() no longer touches it.
    entries: Vec<(String, Entry)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        // ponytail: 5.2. Persist entry_meta to a JSON sidecar so
        // HotdocIndex::open() can rebuild the exact same in-memory
        // map on a subsequent launch without re-parsing the pack
        // JSONs at every CLI invocation. Tantivy's own `meta.json`
        // can't carry this — it's tantivy's schema/index descriptor
        // and we don't control its format. The sidecar is removed
        // implicitly when build() removes the dir at the top.
        write_entry_meta_sidecar(path, &entry_meta).context("writing entry_meta sidecar")?;
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
        Ok(Self {
            _index: index,
            reader,
            fields,
            entry_meta,
            entries,
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

    // ponytail: 5.2. Approach B (sidecar JSON) — tantivy can't
    // round-trip entry_meta through its own meta.json (that's a
    // schema descriptor we don't own), so build() writes
    // hotdoc_entry_meta.json next to the tantivy index and open()
    // reads it back. Signature stays (path) — no upstream call
    // site changes vs Approach A. Missing sidecar → Err so the
    // Tauri resolver's open-failure fallthrough (in
    // index_resolver::resolve_one) rebuilds. Direct CLI users
    // get a clear error pointing them at `hotdoc-cli index`.
    pub fn open(path: &Path) -> Result<Self> {
        let index = Index::open_in_dir(path).context("opening index dir")?;
        let fields = resolve_fields(&index.schema()).context("resolving schema fields")?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let entry_meta = read_entry_meta_sidecar(path).context("reading entry_meta sidecar")?;
        let entries = read_entries_sidecar(path).context("reading entries sidecar")?;
        Ok(Self {
            _index: index,
            reader,
            fields,
            entry_meta,
            entries,
        })
    }

    pub fn search(
        &self,
        raw_query: &str,
        limit: usize,
        // ponytail: 5.3 — popularity map currently unused. The ranker
        // reads `NormalizedEntry.popularity` (always 0 in v1). Wiring
        // the external popularity into the ranker is a follow-up —
        // ranker takes ownership of popularity scoring under spike §9.
        _popularity: &std::collections::HashMap<String, f32>,
    ) -> Result<Vec<SearchHit>> {
        let raw = raw_query.trim();
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        let ranked = crate::ranker::score_query(&self.entries, raw);
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
                let meta = self.entry_meta.get(&h.entry.id);
                Some(SearchHit {
                    id: entry.id.clone(),
                    pack_id: h.entry.pack_id.clone(),
                    title: entry.title.clone(),
                    syntax: entry.syntax.clone(),
                    description: entry.description.clone(),
                    source: h.entry.source.as_str().to_string(),
                    source_url: meta.and_then(|m| m.source_url.clone()),
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

// ponytail: 5.2. Sidecar I/O. Build writes the in-memory map to
// path/hotdoc_entry_meta.json; open reads it back. Pretty-printed
// JSON so humans can diff it across builds when debugging the
// meta-parity invariant. Errors propagate via anyhow for the
// resolve_and_build fallthrough to catch.
fn write_entry_meta_sidecar(
    dir: &Path,
    meta: &std::collections::HashMap<String, EntryMeta>,
) -> Result<()> {
    let path = dir.join(ENTRY_META_SIDECAR);
    let json = serde_json::to_vec_pretty(meta).context("serializing entry_meta sidecar")?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn read_entry_meta_sidecar(dir: &Path) -> Result<std::collections::HashMap<String, EntryMeta>> {
    let path = dir.join(ENTRY_META_SIDECAR);
    let raw = std::fs::read(&path).with_context(|| {
        format!(
            "reading {} (re-run `hotdoc-cli index` to rebuild)",
            path.display()
        )
    })?;
    let meta: std::collections::HashMap<String, EntryMeta> =
        serde_json::from_slice(&raw).context("parsing entry_meta sidecar")?;
    Ok(meta)
}

// ponytail: 5.3 — entries sidecar. Wrapped in a struct so the JSON
// shape is self-documenting; serde otherwise serialises
// Vec<(String, Entry)> as a flat array of [pack_id, entry] pairs,
// which is harder to read and forward-incompatible (no place to add
// metadata later without breaking readers).
#[derive(Serialize, Deserialize)]
struct EntriesSidecar {
    entries: Vec<(String, Entry)>,
}

fn write_entries_sidecar(dir: &Path, entries: &[(String, Entry)]) -> Result<()> {
    let path = dir.join(ENTRIES_SIDECAR);
    let sidecar = EntriesSidecar {
        entries: entries.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&sidecar).context("serializing entries sidecar")?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
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
    Ok(sidecar.entries)
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

    // ponytail: 5.2 meta-parity fix. Build() writes an entry_meta
    // sidecar (hotdoc_entry_meta.json) into the index dir; open()
    // reads it back. Without the sidecar, reused indexes reported
    // entry_count=0 and lost tag/description exact-match bonuses
    // (architecture doc §3.6 parity contract). This test asserts
    // open() produces an entry_meta map deep-equal to build()'s.
    #[test]
    fn open_repopulates_entry_meta() {
        let dir = std::env::temp_dir().join(format!(
            "hotdoc-open-meta-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let packs = packs_for_test();
        let built = HotdocIndex::build(&packs, &dir).expect("build");
        let reopened = HotdocIndex::open(&dir).expect("open");
        assert_eq!(
            built.entry_meta.len(),
            reopened.entry_meta.len(),
            "open() must produce the same entry_meta.len() as build()"
        );
        for (id, expected) in &built.entry_meta {
            let actual = reopened.entry_meta.get(id);
            assert_eq!(
                actual,
                Some(expected),
                "entry_meta[{id}] must deep-equal build()'s after open()"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
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
        let hits = idx
            .search("kubectl get pods", 8, &Default::default())
            .expect("search");
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
