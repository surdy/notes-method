//! URL canonicalization for clip deduplication.
//!
//! Canonicalization removes tracking parameters and normalizes equivalent forms
//! so that re-clipping the same page maps to the same `source_url`, per
//! [ADR 0019](../../docs/adr/0019-media-ingestion-pipeline.md) §6.

use url::Url;

/// Query-parameter name prefixes that are always tracking noise.
const TRACKING_PREFIXES: &[&str] = &["utm_", "utm-"];

/// Exact query-parameter names that are tracking noise.
const TRACKING_EXACT: &[&str] = &[
    "fbclid", "gclid", "gclsrc", "dclid", "msclkid", "mc_cid", "mc_eid", "igshid", "ref_src",
    "ref_url", "yclid", "_hsenc", "_hsmi", "vero_id", "wickedid", "twclid",
];

fn is_tracking_param(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    TRACKING_PREFIXES.iter().any(|p| lower.starts_with(p))
        || TRACKING_EXACT.iter().any(|t| *t == lower)
}

/// Canonicalize a URL for use as a deduplication key.
///
/// - Lowercases the scheme and host.
/// - Drops the fragment.
/// - Removes tracking query parameters (`utm_*`, `fbclid`, `gclid`, ...).
/// - Sorts remaining query parameters for stable ordering.
/// - Strips a trailing slash from non-root paths.
///
/// Returns the original input unchanged if it cannot be parsed as a URL.
pub fn canonicalize_url(input: &str) -> String {
    let trimmed = input.trim();
    let Ok(mut url) = Url::parse(trimmed) else {
        return trimmed.to_string();
    };

    url.set_fragment(None);

    // Filter and sort query parameters.
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if kept.is_empty() {
        url.set_query(None);
    } else {
        let mut sorted = kept;
        sorted.sort();
        let mut serializer = url.query_pairs_mut();
        serializer.clear();
        for (k, v) in &sorted {
            serializer.append_pair(k, v);
        }
        drop(serializer);
    }

    // Lowercase host (scheme is already lowercased by the parser).
    if let Some(host) = url.host_str() {
        let lowered = host.to_ascii_lowercase();
        if lowered != host {
            let _ = url.set_host(Some(&lowered));
        }
    }

    // Strip a single trailing slash from non-root paths.
    let path = url.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        url.set_path(path.trim_end_matches('/'));
    }

    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utm_and_click_ids() {
        let got =
            canonicalize_url("https://example.com/post?utm_source=x&utm_medium=y&fbclid=abc&id=42");
        assert_eq!(got, "https://example.com/post?id=42");
    }

    #[test]
    fn drops_fragment() {
        let got = canonicalize_url("https://example.com/post#section-2");
        assert_eq!(got, "https://example.com/post");
    }

    #[test]
    fn lowercases_host() {
        let got = canonicalize_url("https://Example.COM/Path");
        assert_eq!(got, "https://example.com/Path");
    }

    #[test]
    fn removes_trailing_slash_but_keeps_root() {
        assert_eq!(
            canonicalize_url("https://example.com/a/b/"),
            "https://example.com/a/b"
        );
        assert_eq!(
            canonicalize_url("https://example.com/"),
            "https://example.com/"
        );
    }

    #[test]
    fn sorts_remaining_params_for_stable_key() {
        let a = canonicalize_url("https://example.com/p?b=2&a=1");
        let b = canonicalize_url("https://example.com/p?a=1&b=2");
        assert_eq!(a, b);
    }

    #[test]
    fn tracking_only_query_is_dropped_entirely() {
        assert_eq!(
            canonicalize_url("https://example.com/p?utm_source=x"),
            "https://example.com/p"
        );
    }

    #[test]
    fn non_url_returned_unchanged() {
        assert_eq!(canonicalize_url("not a url"), "not a url");
    }
}
