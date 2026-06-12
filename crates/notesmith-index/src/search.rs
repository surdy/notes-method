use std::path::Path;

use anyhow::Context;
use notesmith_core::Note;
use serde::{Deserialize, Serialize};
use tantivy::{
    Index, IndexReader, ReloadPolicy, Term,
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, SchemaBuilder, TEXT, TantivyDocument, Value},
    snippet::SnippetGenerator,
};

use crate::indexer::extract_note_metadata;

const INDEX_WRITER_MEMORY_BUDGET_BYTES: usize = 50_000_000;
const TITLE_BOOST: f32 = 2.0;

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    schema: Schema,
    vault_name_field: Field,
    path_field: Field,
    title_field: Field,
    body_field: Field,
    note_type_field: Field,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub vault_name: String,
    pub path: String,
    pub title: String,
    pub note_type: String,
    pub score: f32,
    pub snippet: String,
}

impl SearchIndex {
    pub fn open(index_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(index_dir)?;
        let schema = build_schema();
        let directory = tantivy::directory::MmapDirectory::open(index_dir)?;
        let index = Index::open_or_create(directory, schema.clone())?;
        Self::from_index(index, schema)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        let schema = build_schema();
        let index = Index::create_in_ram(schema.clone());
        Self::from_index(index, schema)
    }

    pub fn reindex(&self, vault_name: &str, notes: &[Note]) -> anyhow::Result<()> {
        let mut writer = self.writer()?;
        writer.delete_term(Term::from_field_text(self.vault_name_field, vault_name));

        for note in notes {
            writer.add_document(self.document_for(vault_name, note))?;
        }

        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn check_integrity(&self) -> anyhow::Result<bool> {
        match self.reader.reload() {
            Ok(()) => {
                let searcher = self.reader.searcher();
                let _ = searcher.segment_readers().len();
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    pub fn update_note(&self, vault_name: &str, note: &Note) -> anyhow::Result<()> {
        let mut writer = self.writer()?;
        writer.delete_term(Term::from_field_text(self.path_field, note.path.as_str()));
        writer.add_document(self.document_for(vault_name, note))?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn remove_note(&self, _vault_name: &str, path: &str) -> anyhow::Result<()> {
        let mut writer = self.writer()?;
        writer.delete_term(Term::from_field_text(self.path_field, path));
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        if limit == 0 || query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let mut parser =
            QueryParser::for_index(&self.index, vec![self.title_field, self.body_field]);
        parser.set_field_boost(self.title_field, TITLE_BOOST);
        let parsed_query = match parser.parse_query(query) {
            Ok(parsed) => parsed,
            Err(_) => {
                // Search queries are untrusted (agent / user input). Tantivy's
                // parser rejects field syntax and operator characters
                // (`:`, `+`, `(`, `"`, `~`, ...), so a natural-language query
                // like `note:foo` or `what's "the" plan?` would otherwise
                // surface as a hard error to the caller (e.g. an MCP tool
                // failure). Fall back to a sanitized, operator-free query so
                // the input degrades to a plain term search instead.
                let sanitized = sanitize_query(query);
                if sanitized.is_empty() {
                    return Ok(Vec::new());
                }
                match parser.parse_query(&sanitized) {
                    Ok(parsed) => {
                        tracing::debug!(
                            query,
                            sanitized,
                            "search query had reserved syntax; retried sanitized"
                        );
                        parsed
                    }
                    Err(error) => {
                        tracing::debug!(query, %error, "search query unparseable; returning no results");
                        return Ok(Vec::new());
                    }
                }
            }
        };
        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(limit))?;

        let mut body_snippets =
            SnippetGenerator::create(&searcher, &*parsed_query, self.body_field)?;
        body_snippets.set_max_num_chars(160);
        let mut title_snippets =
            SnippetGenerator::create(&searcher, &*parsed_query, self.title_field)?;
        title_snippets.set_max_num_chars(160);

        top_docs
            .into_iter()
            .map(|(score, doc_address)| {
                let document: TantivyDocument = searcher.doc(doc_address)?;
                let snippet = snippet_html(&body_snippets, &document);
                let snippet = if snippet.is_empty() {
                    snippet_html(&title_snippets, &document)
                } else {
                    snippet
                };

                Ok(SearchResult {
                    vault_name: stored_text(&document, self.vault_name_field),
                    path: stored_text(&document, self.path_field),
                    title: stored_text(&document, self.title_field),
                    note_type: stored_text(&document, self.note_type_field),
                    score,
                    snippet,
                })
            })
            .collect()
    }

    fn from_index(index: Index, schema: Schema) -> anyhow::Result<Self> {
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            vault_name_field: schema
                .get_field("vault_name")
                .context("missing vault_name field")?,
            path_field: schema.get_field("path").context("missing path field")?,
            title_field: schema.get_field("title").context("missing title field")?,
            body_field: schema.get_field("body").context("missing body field")?,
            note_type_field: schema
                .get_field("note_type")
                .context("missing note_type field")?,
            schema,
        })
    }

    fn writer(&self) -> anyhow::Result<tantivy::IndexWriter> {
        self.index
            .writer(INDEX_WRITER_MEMORY_BUDGET_BYTES)
            .map_err(anyhow::Error::from)
    }

    fn document_for(&self, vault_name: &str, note: &Note) -> TantivyDocument {
        let (note_type, title, _, _) = extract_note_metadata(note);
        doc!(
            self.vault_name_field => vault_name.to_string(),
            self.path_field => note.path.as_str().to_string(),
            self.title_field => title,
            self.body_field => note.body.clone(),
            self.note_type_field => note_type,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn schema(&self) -> &Schema {
        &self.schema
    }
}

fn build_schema() -> Schema {
    let mut builder = SchemaBuilder::default();
    builder.add_text_field("vault_name", STRING | STORED);
    builder.add_text_field("path", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("body", TEXT | STORED);
    builder.add_text_field("note_type", STRING | STORED);
    builder.build()
}

/// Reduce an untrusted query to plain, operator-free terms so it can be parsed
/// as a best-effort term search after the strict parse fails.
///
/// Tantivy's query syntax reserves a number of characters (`: + - ( ) { } [ ]
/// ^ " ~ * ? \ /` etc.) and the boolean keywords `AND` / `OR` / `NOT` / `IN`.
/// Natural-language search input from an agent or user routinely contains
/// these. We replace every non-alphanumeric character with whitespace (keeping
/// Unicode letters/digits) and drop bare boolean keywords, leaving a space-
/// separated bag of terms.
fn sanitize_query(query: &str) -> String {
    let cleaned: String = query
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|term| !matches!(*term, "AND" | "OR" | "NOT" | "IN"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn stored_text(document: &TantivyDocument, field: Field) -> String {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

fn snippet_html(generator: &SnippetGenerator, document: &TantivyDocument) -> String {
    let html = generator.snippet_from_doc(document).to_html();
    if strip_html(&html).trim().is_empty() {
        String::new()
    } else {
        html
    }
}

fn strip_html(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut inside_tag = false;

    for ch in text.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => stripped.push(ch),
            _ => {}
        }
    }

    stripped
}

#[cfg(test)]
mod tests {
    use notesmith_core::Note;

    use super::{SearchIndex, sanitize_query};

    #[test]
    fn check_integrity_reports_healthy_index() {
        let index = SearchIndex::open_in_memory().unwrap();
        index
            .reindex(
                "work",
                &[sample_note("Inbox/healthy.md", "healthy search index")],
            )
            .unwrap();

        assert!(index.check_integrity().unwrap());
    }

    #[test]
    fn search_matches_plain_terms() {
        let index = SearchIndex::open_in_memory().unwrap();
        index
            .reindex(
                "work",
                &[sample_note(
                    "Inbox/test.md",
                    "this is a test note about launches",
                )],
            )
            .unwrap();

        let results = index.search("test note", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "Inbox/test.md");
    }

    #[test]
    fn search_degrades_field_syntax_to_plain_terms() {
        // An agent often passes `field:value` style queries. Tantivy rejects an
        // unknown field, which previously surfaced as a hard error. It must now
        // degrade to a plain term search and still find the note.
        let index = SearchIndex::open_in_memory().unwrap();
        index
            .reindex(
                "work",
                &[sample_note(
                    "Inbox/test.md",
                    "this is a test note about launches",
                )],
            )
            .unwrap();

        let results = index.search("note:test", 10).expect("must not error");
        assert!(
            results.iter().any(|r| r.path == "Inbox/test.md"),
            "sanitized fallback should still match the note: {results:?}"
        );
    }

    #[test]
    fn search_tolerates_reserved_characters_without_erroring() {
        let index = SearchIndex::open_in_memory().unwrap();
        index
            .reindex(
                "work",
                &[sample_note("Inbox/cpp.md", "notes on C plus plus")],
            )
            .unwrap();

        for query in [
            "C++ (test)",
            "what's \"the\" plan?",
            "foo~bar^2",
            "AND OR NOT",
        ] {
            let result = index.search(query, 10);
            assert!(
                result.is_ok(),
                "query {query:?} should not error: {result:?}"
            );
        }
    }

    #[test]
    fn sanitize_query_strips_operators_and_boolean_keywords() {
        assert_eq!(sanitize_query("note:foo"), "note foo");
        assert_eq!(sanitize_query("C++ (test)"), "C test");
        assert_eq!(sanitize_query("a AND b OR c"), "a b c");
        assert_eq!(sanitize_query("+-*?\"~^"), "");
    }

    fn sample_note(path: &str, body: &str) -> Note {
        Note {
            vault: notesmith_core::VaultName::new("work"),
            path: path.into(),
            frontmatter: None,
            raw_frontmatter: None,
            body: body.to_string(),
            tasks: Vec::new(),
            links: Vec::new(),
            inline_fields: Vec::new(),
            blocks: Vec::new(),
            hash: format!("hash-{path}"),
        }
    }
}
