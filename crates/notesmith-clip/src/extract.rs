//! Article extraction: HTML in, clean markdown + metadata out.
//!
//! Uses `dom_smoothie` (a Rust port of Mozilla's readability.js) to isolate the
//! article body and metadata, then `htmd` to convert the extracted HTML to
//! Markdown. All input HTML is untrusted per
//! [ADR 0009](../../docs/adr/0009-resilience-to-malformed-content.md): this
//! function never panics and degrades to an error rather than aborting.

use dom_smoothie::{Config, Readability};

use crate::error::ClipError;
use crate::url::canonicalize_url;

/// Upper bound on total elements dom_smoothie will parse. Guards against very
/// large (wide) documents; realistic articles are far below this.
const MAX_ELEMENTS: usize = 50_000;

/// Upper bound on HTML tag nesting depth. dom_smoothie's readability scoring is
/// super-linear in nesting depth, so an adversarially deep document (e.g. 5000
/// nested `<div>`s) can hang the parser even with few total elements. We reject
/// such documents up front. Realistic pages nest well under 100 levels.
const MAX_NESTING_DEPTH: usize = 400;

/// HTML void elements: they never nest and have no closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Cheap, conservative maximum tag-nesting-depth scan over raw HTML.
///
/// Approximate by design: it never builds a DOM (which is what we are trying to
/// avoid feeding to the expensive parser) and errs toward *under*-counting void
/// and self-closing tags so it does not false-positive on wide, flat documents.
fn max_nesting_depth(html: &str) -> usize {
    let bytes = html.as_bytes();
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // Skip comments, doctype, CDATA, processing instructions.
        if bytes[i..].starts_with(b"<!") || bytes[i..].starts_with(b"<?") {
            i += 1;
            continue;
        }
        // Find the end of the tag.
        let Some(rel_end) = bytes[i..].iter().position(|&b| b == b'>') else {
            break;
        };
        let end = i + rel_end;
        let inner = &html[i + 1..end];
        let trimmed = inner.trim_start();

        if let Some(name) = trimmed.strip_prefix('/') {
            // Closing tag.
            let _ = name;
            depth = depth.saturating_sub(1);
        } else if inner.trim_end().ends_with('/') {
            // Self-closing tag: no depth change.
        } else {
            let name: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if !name.is_empty() && !VOID_ELEMENTS.contains(&name.as_str()) {
                depth += 1;
                max_depth = max_depth.max(depth);
            }
        }
        i = end + 1;
    }
    max_depth
}

/// A single extracted clip: metadata plus the article body as Markdown.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipDocument {
    /// Canonicalized source URL (deduplication key).
    pub source_url: String,
    /// Article title (falls back to the host if extraction yields none).
    pub title: String,
    /// Byline / author, when detected.
    pub author: Option<String>,
    /// Published time string as reported by the source, when detected.
    pub published: Option<String>,
    /// Short excerpt / description, when detected.
    pub excerpt: Option<String>,
    /// Site name, when detected.
    pub site_name: Option<String>,
    /// Article body converted to Markdown.
    pub markdown: String,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| "Untitled".to_string())
}

/// Extract a [`ClipDocument`] from raw HTML fetched at `url`.
///
/// `url` is used to resolve relative links and as the canonical source URL.
pub fn extract_from_html(html: &str, url: &str) -> Result<ClipDocument, ClipError> {
    let source_url = canonicalize_url(url);

    let depth = max_nesting_depth(html);
    if depth > MAX_NESTING_DEPTH {
        return Err(ClipError::Extract(format!(
            "html nesting too deep ({depth} > {MAX_NESTING_DEPTH})"
        )));
    }

    let cfg = Config {
        max_elements_to_parse: MAX_ELEMENTS,
        ..Config::default()
    };
    let mut readability = Readability::new(html, Some(url), Some(cfg))
        .map_err(|e| ClipError::Extract(format!("readability init failed: {e}")))?;

    let article = readability
        .parse()
        .map_err(|e| ClipError::Extract(format!("readability parse failed: {e}")))?;

    let content_html = article.content.to_string();
    let markdown = htmd::convert(&content_html)
        .map_err(|e| ClipError::Extract(format!("html to markdown failed: {e}")))?
        .trim()
        .to_string();

    if markdown.is_empty() {
        return Err(ClipError::Extract(
            "no article content could be extracted".to_string(),
        ));
    }

    let title = {
        let t = article.title.trim();
        if t.is_empty() {
            host_of(&source_url)
        } else {
            t.to_string()
        }
    };

    Ok(ClipDocument {
        source_url,
        title,
        author: non_empty(article.byline),
        published: non_empty(article.published_time),
        excerpt: non_empty(article.excerpt),
        site_name: non_empty(article.site_name),
        markdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"<!DOCTYPE html>
<html>
<head>
  <title>Test Article Title</title>
  <meta name="author" content="Jane Doe">
  <meta property="article:published_time" content="2026-07-01T10:00:00Z">
</head>
<body>
  <nav>Home About Contact</nav>
  <article>
    <h1>Test Article Title</h1>
    <p>This is the first substantial paragraph of the article body. It has
    enough text that a readability algorithm will treat it as the main content
    rather than boilerplate navigation or advertising chrome.</p>
    <p>Here is a second paragraph, also with a reasonable amount of prose so the
    extractor scores this container as the article. It continues for a while to
    ensure the content heuristic selects it.</p>
  </article>
  <footer>Copyright 2026</footer>
</body>
</html>"#;

    #[test]
    fn extracts_title_and_body() {
        let doc = extract_from_html(ARTICLE, "https://example.com/post").unwrap();
        assert_eq!(doc.title, "Test Article Title");
        assert_eq!(doc.source_url, "https://example.com/post");
        assert!(doc.markdown.contains("first substantial paragraph"));
        assert!(doc.markdown.contains("second paragraph"));
        // Boilerplate should be dropped.
        assert!(!doc.markdown.contains("Home About Contact"));
        assert!(!doc.markdown.contains("Copyright 2026"));
    }

    #[test]
    fn empty_body_is_extraction_error() {
        let err =
            extract_from_html("<html><body></body></html>", "https://example.com/x").unwrap_err();
        assert!(matches!(err, ClipError::Extract(_)));
    }

    #[test]
    fn malformed_html_does_not_panic() {
        // Unclosed tags, stray braces, broken nesting.
        let junk = "<html><body><article><p>hi <div><span>{{ oops </article";
        let _ = extract_from_html(junk, "https://example.com/j");
    }

    #[test]
    fn pathological_deep_nesting_is_rejected_fast_not_hung() {
        // Adversarial: thousands of nested <div>s would hang the readability
        // scorer. The depth guard must reject it quickly instead.
        let deep = format!(
            "<html><body>{}text{}</body></html>",
            "<div>".repeat(5000),
            "</div>".repeat(5000)
        );
        let start = std::time::Instant::now();
        let err = extract_from_html(&deep, "https://example.com/deep").unwrap_err();
        assert!(matches!(err, ClipError::Extract(_)));
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "depth guard must bail quickly, took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn depth_scan_ignores_void_and_self_closing_tags() {
        // A wide, flat document of void/self-closing tags has depth ~1 and must
        // not trip the nesting guard.
        let flat = format!(
            "<html><body>{}</body></html>",
            "<img src=\"x\"><br/><hr>".repeat(2000)
        );
        assert!(max_nesting_depth(&flat) < MAX_NESTING_DEPTH);
    }

    #[test]
    fn depth_scan_counts_real_nesting() {
        assert_eq!(max_nesting_depth("<a><b><c></c></b></a>"), 3);
        assert!(max_nesting_depth(&"<div>".repeat(600)) > MAX_NESTING_DEPTH);
    }

    #[test]
    fn title_falls_back_to_host_when_missing() {
        let html = r#"<html><body><article>
            <p>Body text long enough to be considered the main article content by
            the readability heuristic used for extraction in this test case.</p>
            <p>A second paragraph of sufficient length to reinforce the main
            content selection so extraction succeeds without a title element.</p>
            </article></body></html>"#;
        let doc = extract_from_html(html, "https://news.example.org/a").unwrap();
        assert!(!doc.title.is_empty());
    }
}
