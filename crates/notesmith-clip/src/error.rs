//! Error type for the clip pipeline.

use thiserror::Error;

/// Errors produced while fetching, extracting, or rendering a clip.
#[derive(Debug, Error)]
pub enum ClipError {
    /// The URL was malformed or used an unsupported scheme.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// The target resolved to a non-public address (SSRF guard).
    #[error("blocked: {0}")]
    Blocked(String),

    /// Network fetch failed, timed out, or exceeded limits.
    #[error("fetch failed: {0}")]
    Fetch(String),

    /// The response body was larger than the configured limit.
    #[error("response too large: {0} bytes exceeds limit {1}")]
    TooLarge(usize, usize),

    /// Content extraction failed to produce a usable article.
    #[error("extraction failed: {0}")]
    Extract(String),
}
