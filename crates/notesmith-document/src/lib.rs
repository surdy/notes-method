//! notesmith-document: local PDF/EPUB parsing into text chunks + normalized
//! notes, for the `read_document` MCP tool (ADR 0019 §2, issue #205).
//!
//! This is a **pure-Rust, no-native-dependency** document source: `pdf-extract`
//! (+ `lopdf`) for PDF text and `epub` (+ `htmd`) for EPUB chapters. There is no
//! OCR, so image-only/scanned PDFs extract little or no text and degrade to an
//! [`DocumentError::Empty`] rather than failing loudly.
//!
//! Per [ADR 0009](../../docs/adr/0009-resilience-to-malformed-content.md) all
//! input bytes are untrusted: parsing is panic-isolated (`catch_unwind`) and
//! every failure mode is a typed, non-fatal [`DocumentError`] the caller can
//! `warn`-and-skip. No `unwrap`/`expect`/`?` escapes on document-derived bytes.

use std::panic::{AssertUnwindSafe, catch_unwind};

use chrono::{DateTime, Utc};
use serde::Serialize;

/// `source_type` frontmatter value for PDF documents.
pub const SOURCE_TYPE_PDF: &str = "pdf";
/// `source_type` frontmatter value for EPUB documents.
pub const SOURCE_TYPE_EPUB: &str = "epub";

/// Default chunk target size in characters (~400 tokens, matching the
/// ADR 0018/0019 chunk boundary of ~256–512 tokens).
pub const DEFAULT_CHUNK_CHARS: usize = 1600;

/// The kind of document, inferred from the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Pdf,
    Epub,
}

impl DocumentKind {
    /// The `source_type` frontmatter value for this kind.
    pub fn source_type(self) -> &'static str {
        match self {
            DocumentKind::Pdf => SOURCE_TYPE_PDF,
            DocumentKind::Epub => SOURCE_TYPE_EPUB,
        }
    }

    /// The label for a structural unit of this kind (`page` / `chapter`).
    pub fn unit_label(self) -> &'static str {
        match self {
            DocumentKind::Pdf => "page",
            DocumentKind::Epub => "chapter",
        }
    }

    /// Infer the document kind from a filename's extension, case-insensitively.
    pub fn from_filename(name: &str) -> Option<Self> {
        let ext = name.rsplit('.').next()?.to_ascii_lowercase();
        match ext.as_str() {
            "pdf" => Some(DocumentKind::Pdf),
            "epub" => Some(DocumentKind::Epub),
            _ => None,
        }
    }
}

/// A typed, non-fatal failure to parse a document. Callers log a `WARN` and
/// skip the document rather than propagating.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    /// The file extension is not a supported document type.
    #[error("unsupported document type: {0}")]
    Unsupported(String),
    /// The document is encrypted / password-protected.
    #[error("document is encrypted and cannot be read")]
    Encrypted,
    /// The document parsed but contained no extractable text (e.g. a scanned
    /// image-only PDF, which would require OCR).
    #[error("document contained no extractable text")]
    Empty,
    /// The document was malformed or the parser failed.
    #[error("failed to parse {kind} document: {reason}")]
    Parse { kind: &'static str, reason: String },
}

/// Provenance metadata extracted from a document.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentMeta {
    /// `source_type`: `pdf` or `epub`.
    pub source_type: String,
    /// Document title, if present in the metadata.
    pub title: Option<String>,
    /// Document author/creator, if present in the metadata.
    pub author: Option<String>,
    /// `page` (PDF) or `chapter` (EPUB).
    pub unit_label: String,
    /// Number of pages (PDF) or chapters (EPUB).
    pub unit_count: usize,
}

/// A single text chunk with character offsets into the normalized text.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocumentChunk {
    /// Zero-based chunk index.
    pub index: usize,
    /// Inclusive start character offset into the normalized text.
    pub char_start: usize,
    /// Exclusive end character offset into the normalized text.
    pub char_end: usize,
    /// The chunk text.
    pub text: String,
}

/// A parsed document: provenance metadata, the full normalized text, and the
/// chunked view of that text.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub meta: DocumentMeta,
    pub text: String,
    pub chunks: Vec<DocumentChunk>,
}

/// Options controlling how extracted text is chunked.
#[derive(Debug, Clone)]
pub struct ChunkOptions {
    /// Approximate maximum characters per chunk.
    pub target_chars: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            target_chars: DEFAULT_CHUNK_CHARS,
        }
    }
}

/// Parse `bytes` of a document named `filename` into normalized text + chunks.
///
/// The `filename` is used only to infer the [`DocumentKind`] from its extension.
/// Returns a typed [`DocumentError`] (never panics) on any failure.
pub fn parse_document(
    filename: &str,
    bytes: &[u8],
    opts: &ChunkOptions,
) -> Result<ParsedDocument, DocumentError> {
    let kind = DocumentKind::from_filename(filename).ok_or_else(|| {
        DocumentError::Unsupported(
            filename
                .rsplit('.')
                .next()
                .filter(|ext| *ext != filename)
                .unwrap_or("(none)")
                .to_string(),
        )
    })?;

    let (raw_text, meta) = match kind {
        DocumentKind::Pdf => parse_pdf(bytes)?,
        DocumentKind::Epub => parse_epub(bytes)?,
    };

    let text = normalize_text(&raw_text);
    if text.is_empty() {
        return Err(DocumentError::Empty);
    }

    let chunks = chunk_text(&text, opts);
    Ok(ParsedDocument { meta, text, chunks })
}

fn parse_pdf(bytes: &[u8]) -> Result<(String, DocumentMeta), DocumentError> {
    // `pdf-extract` can panic on some malformed PDFs; isolate it so a bad file
    // degrades to a typed error instead of taking down the caller (ADR 0009).
    let extraction = catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem_by_pages(bytes)
    }))
    .map_err(|_| DocumentError::Parse {
        kind: "pdf",
        reason: "parser panicked on malformed input".to_string(),
    })?;

    let pages = match extraction {
        Ok(pages) => pages,
        Err(error) => {
            let reason = error.to_string();
            if reason.to_lowercase().contains("encrypt") {
                return Err(DocumentError::Encrypted);
            }
            return Err(DocumentError::Parse {
                kind: "pdf",
                reason,
            });
        }
    };

    let unit_count = pages.len();
    let text = pages.join("\n\n");
    let meta = DocumentMeta {
        source_type: SOURCE_TYPE_PDF.to_string(),
        title: None,
        author: None,
        unit_label: DocumentKind::Pdf.unit_label().to_string(),
        unit_count,
    };
    Ok((text, meta))
}

fn parse_epub(bytes: &[u8]) -> Result<(String, DocumentMeta), DocumentError> {
    use std::io::Cursor;

    let owned = bytes.to_vec();
    let parsed = catch_unwind(AssertUnwindSafe(move || {
        let mut doc = epub::doc::EpubDoc::from_reader(Cursor::new(owned))
            .map_err(|error| error.to_string())?;
        let title = doc.mdata("title").map(|m| m.value.clone());
        let author = doc.mdata("creator").map(|m| m.value.clone());
        let unit_count = doc.get_num_chapters();

        let mut sections = Vec::new();
        loop {
            if let Some((content, _mime)) = doc.get_current_str() {
                // A single malformed chapter must not abort the whole document.
                let md = htmd::convert(&content).unwrap_or_default();
                let trimmed = md.trim();
                if !trimmed.is_empty() {
                    sections.push(trimmed.to_string());
                }
            }
            if !doc.go_next() {
                break;
            }
        }
        Ok::<_, String>((sections.join("\n\n"), title, author, unit_count))
    }))
    .map_err(|_| DocumentError::Parse {
        kind: "epub",
        reason: "parser panicked on malformed input".to_string(),
    })?;

    let (text, title, author, unit_count) = parsed.map_err(|reason| DocumentError::Parse {
        kind: "epub",
        reason,
    })?;

    let meta = DocumentMeta {
        source_type: SOURCE_TYPE_EPUB.to_string(),
        title,
        author,
        unit_label: DocumentKind::Epub.unit_label().to_string(),
        unit_count,
    };
    Ok((text, meta))
}

/// Collapse excessive blank lines and trim trailing whitespace so the extracted
/// text is stable for chunking and diffing.
fn normalize_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut blank_run = 0usize;
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(trimmed);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// Split `text` into chunks of roughly `opts.target_chars`, breaking on
/// paragraph (blank-line) boundaries where possible and never mid-`char`.
pub fn chunk_text(text: &str, opts: &ChunkOptions) -> Vec<DocumentChunk> {
    let target = opts.target_chars.max(1);
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let total = chars.len();
    let mut chunks = Vec::new();
    let mut start_char = 0usize; // index into `chars`
    let mut chunk_index = 0usize;

    while start_char < total {
        let mut end_char = (start_char + target).min(total);

        if end_char < total {
            // Prefer to break at the last paragraph/newline boundary within the
            // window so chunks stay semantically coherent.
            if let Some(boundary) = (start_char..end_char)
                .rev()
                .find(|&i| chars[i].1 == '\n')
                .filter(|&i| i > start_char)
            {
                end_char = boundary + 1;
            }
        }

        let byte_start = chars[start_char].0;
        let byte_end = if end_char >= total {
            text.len()
        } else {
            chars[end_char].0
        };

        let slice = text[byte_start..byte_end].trim();
        if !slice.is_empty() {
            chunks.push(DocumentChunk {
                index: chunk_index,
                char_start: start_char,
                char_end: end_char,
                text: slice.to_string(),
            });
            chunk_index += 1;
        }
        start_char = end_char;
    }

    chunks
}

/// A normalized document note split into its structured pieces so callers can
/// either persist it verbatim ([`RenderedNote::markdown`]) or route the
/// frontmatter/body through an existing note-creation path.
#[derive(Debug, Clone)]
pub struct RenderedNote {
    /// Provenance frontmatter as a JSON object (ADR 0019 §3), ready to hand to
    /// a note-creation API that expects structured frontmatter.
    pub frontmatter: serde_json::Map<String, serde_json::Value>,
    /// The note body (a `# heading` plus the normalized extracted text).
    pub body: String,
    /// The fully rendered note (`---` frontmatter + body), for verbatim writes
    /// or preview.
    pub markdown: String,
}

/// Build the structured pieces of a normalized note for a parsed document,
/// keyed by its vault-relative `source_path` (ADR 0019 §3).
pub fn render_document_note_parts(
    source_path: &str,
    doc: &ParsedDocument,
    ingested_at: DateTime<Utc>,
) -> RenderedNote {
    use serde_json::Value as JsonValue;

    let mut frontmatter = serde_json::Map::new();
    frontmatter.insert(
        "source_type".to_string(),
        JsonValue::from(doc.meta.source_type.clone()),
    );
    if let Some(title) = &doc.meta.title {
        frontmatter.insert("title".to_string(), JsonValue::from(title.clone()));
    }
    if let Some(author) = &doc.meta.author {
        frontmatter.insert("author".to_string(), JsonValue::from(author.clone()));
    }
    frontmatter.insert(
        "source_path".to_string(),
        JsonValue::from(source_path.to_string()),
    );
    frontmatter.insert(
        format!("{}_count", doc.meta.unit_label),
        JsonValue::from(doc.meta.unit_count),
    );
    frontmatter.insert(
        "ingested_at".to_string(),
        JsonValue::from(ingested_at.to_rfc3339()),
    );

    let heading = doc.meta.title.clone().unwrap_or_else(|| {
        source_path
            .rsplit('/')
            .next()
            .unwrap_or(source_path)
            .to_string()
    });
    let body = format!("# {heading}\n\n{}\n", doc.text);

    let yaml = serde_json::Value::Object(frontmatter.clone());
    let frontmatter_yaml = serde_yaml::to_string(&yaml).unwrap_or_default();
    let markdown = format!("---\n{frontmatter_yaml}---\n\n{body}");

    RenderedNote {
        frontmatter,
        body,
        markdown,
    }
}

/// Render a normalized note (provenance frontmatter + body) for a parsed
/// document, keyed by its vault-relative `source_path` (ADR 0019 §3).
pub fn render_document_note(
    source_path: &str,
    doc: &ParsedDocument,
    ingested_at: DateTime<Utc>,
) -> String {
    render_document_note_parts(source_path, doc, ingested_at).markdown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_inference_is_case_insensitive() {
        assert_eq!(
            DocumentKind::from_filename("a.pdf"),
            Some(DocumentKind::Pdf)
        );
        assert_eq!(
            DocumentKind::from_filename("A.PDF"),
            Some(DocumentKind::Pdf)
        );
        assert_eq!(
            DocumentKind::from_filename("book.EpUb"),
            Some(DocumentKind::Epub)
        );
        assert_eq!(DocumentKind::from_filename("notes.md"), None);
        assert_eq!(DocumentKind::from_filename("noext"), None);
    }

    #[test]
    fn unsupported_extension_is_typed_error() {
        let err = parse_document("notes.txt", b"hello", &ChunkOptions::default()).unwrap_err();
        assert!(matches!(err, DocumentError::Unsupported(ext) if ext == "txt"));
    }

    #[test]
    fn malformed_pdf_degrades_without_panic() {
        let err = parse_document(
            "broken.pdf",
            b"%PDF-1.4 not a real pdf",
            &ChunkOptions::default(),
        )
        .unwrap_err();
        assert!(
            matches!(
                err,
                DocumentError::Parse { kind: "pdf", .. } | DocumentError::Empty
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn empty_bytes_do_not_panic() {
        for name in ["a.pdf", "a.epub"] {
            let err = parse_document(name, b"", &ChunkOptions::default()).unwrap_err();
            assert!(
                matches!(err, DocumentError::Parse { .. } | DocumentError::Empty),
                "{name}: {err:?}"
            );
        }
    }

    #[test]
    fn chunk_text_respects_target_and_covers_all_text() {
        let text = "para one line\n\npara two line\n\npara three line\n\npara four line";
        let chunks = chunk_text(text, &ChunkOptions { target_chars: 20 });
        assert!(chunks.len() > 1);
        // Offsets are monotonic and non-overlapping.
        for pair in chunks.windows(2) {
            assert!(pair[0].char_end <= pair[1].char_start);
        }
        // Reassembling the trimmed chunks recovers all non-whitespace content.
        let joined: String = chunks
            .iter()
            .flat_map(|c| c.text.chars())
            .filter(|c| !c.is_whitespace())
            .collect();
        let expected: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(joined, expected);
    }

    #[test]
    fn chunk_text_handles_multibyte_without_splitting_chars() {
        let text = "café ☕ ".repeat(50);
        let chunks = chunk_text(&text, &ChunkOptions { target_chars: 8 });
        assert!(!chunks.is_empty());
        // Every chunk is valid UTF-8 by construction (String), so this asserts
        // no panic occurred and offsets are sane.
        assert!(chunks.iter().all(|c| c.char_end >= c.char_start));
    }

    #[test]
    fn normalize_collapses_blank_runs() {
        let raw = "line one\n\n\n\nline two\n\n";
        assert_eq!(normalize_text(raw), "line one\n\nline two");
    }

    #[test]
    fn render_note_includes_provenance_frontmatter() {
        let doc = ParsedDocument {
            meta: DocumentMeta {
                source_type: SOURCE_TYPE_PDF.to_string(),
                title: Some("My Report".to_string()),
                author: Some("Jane".to_string()),
                unit_label: "page".to_string(),
                unit_count: 3,
            },
            text: "Body text.".to_string(),
            chunks: vec![],
        };
        let at = DateTime::parse_from_rfc3339("2026-07-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let note = render_document_note("attachments/report.pdf", &doc, at);
        assert!(note.contains("source_type: pdf"));
        assert!(note.contains("title: My Report"));
        assert!(note.contains("author: Jane"));
        assert!(note.contains("source_path: attachments/report.pdf"));
        assert!(note.contains("page_count: 3"));
        assert!(note.contains("ingested_at: 2026-07-15T12:00:00+00:00"));
        assert!(note.contains("# My Report"));
        assert!(note.contains("Body text."));
    }
}
