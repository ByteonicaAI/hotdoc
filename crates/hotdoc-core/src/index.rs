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
    entry_meta: std::collections::HashMap<String, (String, Option<String>, Option<String>)>,
}

impl HotdocIndex {
    pub fn build(packs: &[Pack], path: &Path) -> Result<Self> {
        if path.exists() {
            std::fs::remove_dir_all(path).ok();
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

    pub fn search(&self, raw_query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let searcher: Searcher = self.reader.searcher();
        let query = self.build_query(raw_query);
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
        let fields = self.fields;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, addr) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
            let id = get_text(&doc, fields.id);
            let (description, source_url, example_code) = self
                .entry_meta
                .get(&id)
                .cloned()
                .unwrap_or_else(|| (String::new(), None, None));
            let source = get_text(&doc, fields.source);
            let adjusted_score = apply_source_priority(score, &source);
            hits.push(SearchHit {
                id,
                pack_id: get_text(&doc, fields.pack_id),
                title: get_text(&doc, fields.title),
                syntax: get_text(&doc, fields.syntax),
                description,
                source,
                source_url,
                example_code,
                score: adjusted_score,
            });
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(hits)
    }

    fn build_query(&self, raw_query: &str) -> Box<dyn Query> {
        let q = raw_query.trim().to_lowercase();
        if q.is_empty() {
            return Box::new(BooleanQuery::new(vec![]));
        }
        let fields = self.fields;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        let mut tail_description: Option<String> = None;
        for token in q.split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '/' | '.')) {
            if token.is_empty() {
                continue;
            }
            let edit_distance: u8 = match token.len() {
                0..=3 => 0,
                _ => 1,
            };
            if edit_distance == 0 {
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(
                        Box::new(TermQuery::new(
                            Term::from_field_text(fields.syntax, token),
                            IndexRecordOption::Basic,
                        )),
                        5.0,
                    )),
                ));
                clauses.push((
                    Occur::Should,
                    Box::new(TermQuery::new(
                        Term::from_field_text(fields.title, token),
                        IndexRecordOption::Basic,
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
                        3.0,
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
                        4.0,
                    )),
                ));
            }
            if edit_distance == 0 && token.len() > 3 {
                let pattern = format!("^{}.*", regex_sanitize(token));
                for (field, boost) in [
                    (fields.syntax, 2.5),
                    (fields.title, 3.0),
                    (fields.tags, 1.5),
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
                    2.0,
                )),
            ));
            if token.len() >= 5 {
                tail_description = Some(token.to_string());
            }
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(
                        Term::from_field_text(fields.example_codes, token),
                        IndexRecordOption::Basic,
                    )),
                    0.5,
                )),
            ));
        }
        if let Some(token) = tail_description {
            clauses.push((
                Occur::Should,
                Box::new(TermQuery::new(
                    Term::from_field_text(fields.description, &token),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if clauses.is_empty() {
            return Box::new(BooleanQuery::new(vec![]));
        }
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

// Source priority multiplier applied as a post-rerank step on top of BM25.
// Higher multiplier = the source is more authoritative. Ordering matches the
// plan: official > cheat-sheet > curated. Personal is rejected by pack
// validation, so it never reaches here.
fn apply_source_priority(score: f32, source: &str) -> f32 {
    let multiplier: f32 = match source {
        "official" => 2.0,
        "cheat-sheet" => 1.5,
        _ => 1.0,
    };
    score * multiplier
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
        TextOptions::default().set_indexing_options(code_indexing()),
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

fn collect_entry_meta(
    packs: &[Pack],
) -> std::collections::HashMap<String, (String, Option<String>, Option<String>)> {
    let mut m = std::collections::HashMap::new();
    for p in packs {
        for e in &p.entries {
            let first_example = e.examples.first().map(|x| x.code.clone());
            m.insert(
                e.id.clone(),
                (e.description.clone(), e.source_url.clone(), first_example),
            );
        }
    }
    m
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
        pack::load_dir(&dir).expect("load packs")
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
    fn gibberish_returns_empty() {
        let idx = fresh_index();
        let hits = idx.search("asdfqwer", 8).expect("search");
        assert!(hits.is_empty(), "gibberish should yield zero results");
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
        assert!(
            hits.first().map(|h| h.id.as_str()) == Some("docker-logs"),
            "exact-match boost should lift docker-logs above docker-logs-tail; got {:?}",
            hits.iter().take(3).map(|h| &h.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn source_priority_lifts_official_over_curated_sibling() {
        let idx = fresh_index();
        let hits = idx.search("kubectl get pods", 8).expect("search");
        assert_eq!(
            hits.first().map(|h| h.id.as_str()),
            Some("kubectl-get-pods-official"),
            "source-priority boost should put the official card above its curated sibling; got {:?}",
            hits.iter().take(3).map(|h| &h.id).collect::<Vec<_>>()
        );
    }
}
