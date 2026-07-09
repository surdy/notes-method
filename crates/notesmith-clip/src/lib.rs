//! notesmith-clip: server-side web clipping.
//!
//! Turns a URL into a clean Markdown note with provenance frontmatter. The
//! pipeline is `fetch (SSRF-guarded, bounded)` → `extract (readability →
//! markdown)` → `render (frontmatter + body)`. It is the shared extraction
//! library called by both the interactive clip endpoint and the batch
//! ingestion worker, per [ADR 0020](../docs/adr/0020-web-clipper.md).
//!
//! All fetched HTML is untrusted input: extraction never panics and degrades to
//! an error rather than aborting
//! ([ADR 0009](../docs/adr/0009-resilience-to-malformed-content.md)).

mod error;
mod extract;
mod fetch;
mod images;
mod note;
mod ssrf;
mod template;
mod url;

pub use error::ClipError;
pub use extract::{ClipDocument, extract_from_html};
pub use fetch::{FetchLimits, FetchedBytes, FetchedPage, fetch_bytes, fetch_html};
pub use images::{DownloadedImage, download_and_rewrite_images};
pub use note::{SOURCE_TYPE_ARTICLE, render_note, render_note_with_template};
pub use ssrf::{is_blocked_ip, resolve_public_addrs};
pub use template::{ClipTemplate, select_template};
pub use url::{canonicalize_url, host_of};

use chrono::Local;

/// Fetch `url` and extract it into a [`ClipDocument`].
///
/// Convenience wrapper over [`fetch_html`] + [`extract_from_html`]. The document
/// is keyed by the canonicalized *final* URL after redirects.
pub async fn clip_url(url: &str, limits: &FetchLimits) -> Result<ClipDocument, ClipError> {
    let page = fetch_html(url, limits).await?;
    extract_from_html(&page.html, &page.final_url)
}

/// Fetch, extract, and render `url` into a complete Markdown note.
///
/// `extra_tags` are appended after the mandatory `inbox` tag. Returns the
/// rendered note plus the [`ClipDocument`] (whose `source_url` is the dedup key).
pub async fn clip_url_to_note(
    url: &str,
    extra_tags: &[String],
    limits: &FetchLimits,
) -> Result<(String, ClipDocument), ClipError> {
    let doc = clip_url(url, limits).await?;
    let note = render_note(&doc, extra_tags, Local::now());
    Ok((note, doc))
}
