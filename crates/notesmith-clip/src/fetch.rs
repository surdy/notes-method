//! Bounded, SSRF-guarded HTTP fetch for clips.
//!
//! Redirects are followed manually so that **every hop** is re-validated by the
//! SSRF guard and pinned to its resolved public address (defeating DNS
//! rebinding). Timeout, redirect count, and response size are all bounded, per
//! [ADR 0020](../../docs/adr/0020-web-clipper.md) §6.

use std::time::Duration;

use reqwest::redirect::Policy;
use url::Url;

use crate::error::ClipError;
use crate::ssrf::resolve_public_addrs;

/// Limits applied to a single clip fetch.
#[derive(Debug, Clone)]
pub struct FetchLimits {
    /// Per-request timeout.
    pub timeout: Duration,
    /// Maximum response body size in bytes.
    pub max_bytes: usize,
    /// Maximum number of redirect hops to follow.
    pub max_redirects: u32,
    /// `User-Agent` header sent with each request.
    pub user_agent: String,
}

impl Default for FetchLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            max_bytes: 5 * 1024 * 1024,
            max_redirects: 5,
            user_agent: format!("Notesmith-Clip/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// Result of a successful fetch: the final (post-redirect) URL and HTML body.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    /// The final URL after following redirects.
    pub final_url: String,
    /// The response body as a UTF-8 (lossy) string.
    pub html: String,
}

fn require_http(url: &Url) -> Result<(), ClipError> {
    match url.scheme() {
        "http" | "https" => Ok(()),
        other => Err(ClipError::InvalidUrl(format!(
            "unsupported scheme: {other}"
        ))),
    }
}

/// Fetch HTML from `url`, following redirects manually with SSRF validation and
/// size/time bounds.
pub async fn fetch_html(url: &str, limits: &FetchLimits) -> Result<FetchedPage, ClipError> {
    let mut current = Url::parse(url).map_err(|e| ClipError::InvalidUrl(e.to_string()))?;

    for _ in 0..=limits.max_redirects {
        require_http(&current)?;
        let host = current
            .host_str()
            .ok_or_else(|| ClipError::InvalidUrl("missing host".to_string()))?
            .to_string();
        let port = current
            .port_or_known_default()
            .ok_or_else(|| ClipError::InvalidUrl("missing port".to_string()))?;

        let addrs = resolve_public_addrs(&host, port)?;

        let client = reqwest::Client::builder()
            .timeout(limits.timeout)
            .redirect(Policy::none())
            .user_agent(&limits.user_agent)
            .resolve_to_addrs(&host, &addrs)
            .build()
            .map_err(|e| ClipError::Fetch(e.to_string()))?;

        let resp = client
            .get(current.clone())
            .send()
            .await
            .map_err(|e| ClipError::Fetch(e.to_string()))?;

        let status = resp.status();
        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ClipError::Fetch(format!("redirect {status} without location")))?;
            current = current
                .join(location)
                .map_err(|e| ClipError::InvalidUrl(format!("bad redirect target: {e}")))?;
            continue;
        }

        if !status.is_success() {
            return Err(ClipError::Fetch(format!("http status {status}")));
        }

        let final_url = current.to_string();
        let html = read_body_capped(resp, limits.max_bytes).await?;
        return Ok(FetchedPage { final_url, html });
    }

    Err(ClipError::Fetch(format!(
        "too many redirects (limit {})",
        limits.max_redirects
    )))
}

async fn read_body_capped(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<String, ClipError> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ClipError::Fetch(e.to_string()))?
    {
        if buf.len() + chunk.len() > max_bytes {
            return Err(ClipError::TooLarge(buf.len() + chunk.len(), max_bytes));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let err = fetch_html("file:///etc/passwd", &FetchLimits::default())
            .await
            .unwrap_err();
        assert!(matches!(err, ClipError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn blocks_loopback_before_network() {
        let err = fetch_html("http://127.0.0.1:80/", &FetchLimits::default())
            .await
            .unwrap_err();
        assert!(matches!(err, ClipError::Blocked(_)));
    }

    #[tokio::test]
    async fn blocks_private_literal() {
        let err = fetch_html(
            "http://169.254.169.254/latest/meta-data/",
            &FetchLimits::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ClipError::Blocked(_)));
    }
}
