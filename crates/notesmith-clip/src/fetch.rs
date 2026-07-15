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

/// Result of a successful raw fetch: final URL, bytes, and content type.
#[derive(Debug, Clone)]
pub struct FetchedBytes {
    /// The final URL after following redirects.
    pub final_url: String,
    /// The raw response body.
    pub bytes: Vec<u8>,
    /// The `Content-Type` header value, when present.
    pub content_type: Option<String>,
}

/// Optional overrides for a single request: HTTP method/body, extra request
/// headers, and a `User-Agent` override. `Default` yields a plain `GET` with
/// the limits' `User-Agent` and no extra headers.
///
/// Used by the YouTube source to POST to YouTube's InnerTube API with the
/// `Origin`/`Referer`/`User-Agent` headers the (unofficial) endpoint requires
/// ([ADR 0020](../../docs/adr/0020-web-clipper.md) §8).
#[derive(Debug, Clone, Default)]
pub struct RequestSpec {
    /// When set, issue a `POST` with this `(body, content_type)`; otherwise `GET`.
    pub post_body: Option<(Vec<u8>, String)>,
    /// Extra request headers to set (e.g. `Origin`, `Referer`).
    pub extra_headers: Vec<(String, String)>,
    /// Overrides the limits' `User-Agent` for this request.
    pub user_agent: Option<String>,
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
    let raw = fetch_bytes(url, limits).await?;
    Ok(FetchedPage {
        final_url: raw.final_url,
        html: String::from_utf8_lossy(&raw.bytes).into_owned(),
    })
}

/// Fetch raw bytes from `url` (e.g. an image), following redirects manually with
/// SSRF validation and size/time bounds. Shares the redirect/SSRF loop with
/// [`fetch_html`].
pub async fn fetch_bytes(url: &str, limits: &FetchLimits) -> Result<FetchedBytes, ClipError> {
    fetch_request(url, &RequestSpec::default(), limits).await
}

/// POST `body` (with `content_type`) to `url` and return the raw response
/// bytes, following redirects manually with SSRF validation and size/time
/// bounds. `extra_headers` and `user_agent` let callers satisfy APIs (such as
/// YouTube's InnerTube endpoint) that require specific `Origin`/`Referer`/UA
/// headers ([ADR 0020](../../docs/adr/0020-web-clipper.md) §8).
pub async fn fetch_json_post(
    url: &str,
    body: Vec<u8>,
    content_type: &str,
    extra_headers: Vec<(String, String)>,
    user_agent: Option<String>,
    limits: &FetchLimits,
) -> Result<FetchedBytes, ClipError> {
    let spec = RequestSpec {
        post_body: Some((body, content_type.to_string())),
        extra_headers,
        user_agent,
    };
    fetch_request(url, &spec, limits).await
}

/// Core fetch loop: issues the request described by `spec`, following redirects
/// manually with per-hop SSRF validation and size/time bounds.
pub async fn fetch_request(
    url: &str,
    spec: &RequestSpec,
    limits: &FetchLimits,
) -> Result<FetchedBytes, ClipError> {
    let mut current = Url::parse(url).map_err(|e| ClipError::InvalidUrl(e.to_string()))?;
    let user_agent = spec.user_agent.as_deref().unwrap_or(&limits.user_agent);

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
            .user_agent(user_agent)
            .resolve_to_addrs(&host, &addrs)
            .build()
            .map_err(|e| ClipError::Fetch(e.to_string()))?;

        let mut req = match &spec.post_body {
            Some((body, content_type)) => client
                .post(current.clone())
                .header(reqwest::header::CONTENT_TYPE, content_type)
                .body(body.clone()),
            None => client.get(current.clone()),
        };
        for (name, value) in &spec.extra_headers {
            req = req.header(name.as_str(), value.as_str());
        }

        let resp = req
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

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let final_url = current.to_string();
        let bytes = read_body_capped(resp, limits.max_bytes).await?;
        return Ok(FetchedBytes {
            final_url,
            bytes,
            content_type,
        });
    }

    Err(ClipError::Fetch(format!(
        "too many redirects (limit {})",
        limits.max_redirects
    )))
}

async fn read_body_capped(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, ClipError> {
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
    Ok(buf)
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
