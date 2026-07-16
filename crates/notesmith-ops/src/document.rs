//! Document ingestion for the `read_document` MCP tool.
//!
//! A **thin wrapper** over [`notesmith_document`] ([ADR 0019](../../docs/adr/0019-media-ingestion-pipeline.md)
//! §PDF/EPUB): it resolves a vault-relative path safely inside the vault root,
//! reads the raw bytes, and calls [`notesmith_document::parse_document`] to get
//! extracted text, chunks, provenance metadata, and a normalized note body. It
//! never mutates the vault — persisting the normalized note is the caller's job
//! (the MCP dispatch composes `create_note` when `save:true`), so a read-only
//! surface can safely expose the extraction without any write capability.
//!
//! All parsing is panic-isolated per document inside `notesmith-document`
//! (ADR 0009): a malformed or encrypted file returns a typed error rather than
//! aborting, and never crosses a multi-note boundary.

use std::path::{Component, Path, PathBuf};

use notesmith_document::{
    ChunkOptions, ParsedDocument, parse_document, render_document_note_parts,
};
use serde_json::{Value, json};

use crate::Result;

/// Resolve a vault-relative `path` to an absolute path strictly inside
/// `vault_root`, rejecting traversal, absolute, and Windows-prefixed paths.
fn resolve_within_vault(vault_root: &Path, path: &str) -> Result<PathBuf> {
    if path.is_empty()
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains(':')
    {
        anyhow::bail!("invalid document path: {path:?}");
    }

    let mut resolved = vault_root.to_path_buf();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(value) => resolved.push(value),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                anyhow::bail!("invalid document path: {path:?}");
            }
        }
    }
    Ok(resolved)
}

/// Read and parse a document at a vault-relative `path`, returning its
/// structured extraction plus a normalized note body (never written here).
///
/// `vault_root` is the absolute vault root; `path` is validated to stay inside
/// it. The bytes are read and handed to [`parse_document`]; the result is mapped
/// to a JSON value carrying `source_path`, `source_type`, metadata, chunk list,
/// and `note_markdown` (the caller persists it only when the user opts in).
pub fn read_document(vault_root: &Path, path: &str) -> Result<Value> {
    let absolute = resolve_within_vault(vault_root, path)?;
    let bytes = std::fs::read(&absolute)
        .map_err(|error| anyhow::anyhow!("cannot read document {path:?}: {error}"))?;
    let filename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let parsed = parse_document(filename, &bytes, &ChunkOptions::default())?;
    Ok(parsed_document_to_value(path, &parsed))
}

/// Map a [`ParsedDocument`] to the `read_document` tool's JSON result.
///
/// Pure and I/O-free so it can be unit-tested with constructed values. The
/// `frontmatter`, `body`, and `note_markdown` fields together let the caller
/// persist the normalized note through the gated `create_note` path (structured
/// `frontmatter` + `body`) or preview it verbatim (`note_markdown`).
pub fn parsed_document_to_value(source_path: &str, parsed: &ParsedDocument) -> Value {
    let note = render_document_note_parts(source_path, parsed, chrono::Utc::now());
    let chunks = parsed
        .chunks
        .iter()
        .map(|chunk| {
            json!({
                "index": chunk.index,
                "char_start": chunk.char_start,
                "char_end": chunk.char_end,
                "text": chunk.text,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "source_path": source_path,
        "source_type": parsed.meta.source_type,
        "title": parsed.meta.title,
        "author": parsed.meta.author,
        "unit_label": parsed.meta.unit_label,
        "unit_count": parsed.meta.unit_count,
        "chunk_count": parsed.chunks.len(),
        "text": parsed.text,
        "chunks": chunks,
        "frontmatter": Value::Object(note.frontmatter),
        "body": note.body,
        "note_markdown": note.markdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notesmith_document::{DocumentChunk, DocumentMeta};

    fn sample_parsed() -> ParsedDocument {
        ParsedDocument {
            meta: DocumentMeta {
                source_type: "pdf".to_string(),
                title: Some("Report".to_string()),
                author: Some("Jane".to_string()),
                unit_label: "page".to_string(),
                unit_count: 2,
            },
            text: "Body.".to_string(),
            chunks: vec![DocumentChunk {
                index: 0,
                char_start: 0,
                char_end: 5,
                text: "Body.".to_string(),
            }],
        }
    }

    #[test]
    fn value_carries_metadata_chunks_and_note() {
        let value = parsed_document_to_value("attachments/report.pdf", &sample_parsed());
        assert_eq!(value["source_type"], "pdf");
        assert_eq!(value["source_path"], "attachments/report.pdf");
        assert_eq!(value["title"], "Report");
        assert_eq!(value["unit_count"], 2);
        assert_eq!(value["chunk_count"], 1);
        assert_eq!(value["chunks"][0]["text"], "Body.");
        assert!(
            value["note_markdown"]
                .as_str()
                .unwrap()
                .contains("source_type: pdf")
        );
    }

    #[test]
    fn rejects_path_traversal() {
        let root = Path::new("/tmp/vault");
        assert!(resolve_within_vault(root, "../secret.pdf").is_err());
        assert!(resolve_within_vault(root, "a/../../secret.pdf").is_err());
        assert!(resolve_within_vault(root, "/etc/passwd").is_err());
        assert!(resolve_within_vault(root, "a\\b.pdf").is_err());
        assert!(resolve_within_vault(root, "C:foo.pdf").is_err());
        assert!(resolve_within_vault(root, "").is_err());
    }

    #[test]
    fn resolves_nested_relative_path() {
        let root = Path::new("/tmp/vault");
        let resolved = resolve_within_vault(root, "attachments/report.pdf").unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/vault/attachments/report.pdf"));
    }

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        let root = Path::new("/tmp/definitely-missing-vault-xyz");
        let err = read_document(root, "nope.pdf").unwrap_err();
        assert!(err.to_string().contains("cannot read document"));
    }
}
