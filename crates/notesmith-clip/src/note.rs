//! Render a [`ClipDocument`] into a Markdown note with provenance frontmatter.
//!
//! Frontmatter follows [ADR 0019](../../docs/adr/0019-media-ingestion-pipeline.md)
//! §3: `source_url`, `source_type: article`, `title`, `author`/`published` when
//! known, and `ingested_at`. Clips also carry `tags: [inbox, ...]` so the
//! existing routing engine can file them ([ADR 0020](../../docs/adr/0020-web-clipper.md)).

use chrono::{DateTime, Local};
use serde_yaml::Value;

use crate::extract::ClipDocument;
use crate::template::{ClipTemplate, render_with_template};

/// The `source_type` value used for web-article clips.
pub const SOURCE_TYPE_ARTICLE: &str = "article";

/// Resolve the tag list for a clip: mandatory `inbox` first, then de-duplicated
/// non-empty extras. Returns both YAML values (for frontmatter) and strings
/// (for template context).
fn resolve_tags(extra_tags: &[String]) -> (Vec<Value>, Vec<String>) {
    let mut strings = vec!["inbox".to_string()];
    for tag in extra_tags {
        let tag = tag.trim();
        if !tag.is_empty() && tag != "inbox" && !strings.iter().any(|t| t == tag) {
            strings.push(tag.to_string());
        }
    }
    let values = strings.iter().cloned().map(Value::from).collect();
    (values, strings)
}

/// Render `doc` as a full Markdown note (frontmatter + body).
///
/// `extra_tags` are appended after the mandatory `inbox` tag. `now` is injected
/// so callers (and tests) control the `ingested_at` timestamp.
pub fn render_note(doc: &ClipDocument, extra_tags: &[String], now: DateTime<Local>) -> String {
    render_note_with_template(doc, extra_tags, now, None)
}

/// Render `doc` as a full Markdown note, optionally applying a per-domain
/// `template` ([`ClipTemplate`]). When `template` is `None`, this is identical
/// to [`render_note`].
pub fn render_note_with_template(
    doc: &ClipDocument,
    extra_tags: &[String],
    now: DateTime<Local>,
    template: Option<&ClipTemplate>,
) -> String {
    let ingested_at = now.to_rfc3339();
    let (tag_values, tag_strings) = resolve_tags(extra_tags);
    render_with_template(doc, &ingested_at, &tag_values, &tag_strings, template)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> ClipDocument {
        ClipDocument {
            source_url: "https://example.com/post".to_string(),
            title: "Hello: A Title".to_string(),
            author: Some("Jane Doe".to_string()),
            published: Some("2026-07-01T10:00:00Z".to_string()),
            excerpt: None,
            site_name: Some("Example".to_string()),
            markdown: "# Hello\n\nBody text.".to_string(),
        }
    }

    fn fixed_now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 7, 9, 2, 30, 0).unwrap()
    }

    #[test]
    fn renders_frontmatter_and_body() {
        let note = render_note(&sample(), &[], fixed_now());
        assert!(note.starts_with("---\n"));
        assert!(note.contains("source_url: https://example.com/post"));
        assert!(note.contains("source_type: article"));
        assert!(note.contains("author: Jane Doe"));
        assert!(note.contains("published: 2026-07-01T10:00:00Z"));
        assert!(note.contains("ingested_at:"));
        assert!(note.contains("Body text."));
    }

    #[test]
    fn always_tagged_inbox_and_dedupes_extra_inbox() {
        let note = render_note(
            &sample(),
            &["research".to_string(), "inbox".to_string()],
            fixed_now(),
        );
        assert!(note.contains("- inbox"));
        assert!(note.contains("- research"));
        // `inbox` must appear exactly once in the tag list.
        assert_eq!(note.matches("- inbox").count(), 1);
    }

    #[test]
    fn omits_optional_fields_when_absent() {
        let mut doc = sample();
        doc.author = None;
        doc.published = None;
        let note = render_note(&doc, &[], fixed_now());
        assert!(!note.contains("author:"));
        assert!(!note.contains("published:"));
    }

    #[test]
    fn title_with_colon_is_quoted_valid_yaml() {
        let note = render_note(&sample(), &[], fixed_now());
        let fm = note
            .strip_prefix("---\n")
            .and_then(|s| s.split("\n---\n").next())
            .unwrap();
        // Round-trips as valid YAML.
        let parsed: Value = serde_yaml::from_str(fm).unwrap();
        assert_eq!(parsed["title"].as_str().unwrap(), "Hello: A Title");
    }
}
