//! Download remote images referenced in clipped Markdown and rewrite the links
//! to point at locally-saved copies.
//!
//! Runs after extraction: it scans the Markdown for `![alt](url)` image links,
//! fetches each remote `http(s)` image (SSRF-guarded and bounded, sharing the
//! same [`fetch_bytes`](crate::fetch::fetch_bytes) path as the page fetch), and
//! rewrites the links to a caller-provided attachments prefix. The caller
//! persists the returned bytes into the vault.
//!
//! Untrusted-input policy ([ADR 0009](../../docs/adr/0009-resilience-to-malformed-content.md)):
//! a single failed image download never aborts the clip — the original remote
//! URL is left in place and the next image is attempted.

use std::collections::HashMap;

use url::Url;

use crate::fetch::{FetchLimits, fetch_bytes};

/// Maximum number of images downloaded per clip. Bounds fan-out on
/// image-heavy pages.
const MAX_IMAGES: usize = 50;

/// An image downloaded for a clip, ready to be written into the vault.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadedImage {
    /// Filename (no directory component) the link was rewritten to.
    pub filename: String,
    /// Raw image bytes.
    pub bytes: Vec<u8>,
}

/// Parsed `![alt](target)` occurrence in the Markdown source.
struct ImageRef {
    /// Full matched substring, e.g. `![alt](https://x/y.png)`.
    full: String,
    /// Alt text between the brackets.
    alt: String,
    /// Link target between the parentheses (may include a title).
    target: String,
}

/// Find Markdown image references (`![alt](target)`) in `markdown`.
///
/// Deliberately simple and allocation-light: it never builds a full Markdown
/// AST. It skips targets containing spaces-with-titles by taking the first
/// whitespace-delimited token as the URL.
fn find_image_refs(markdown: &str) -> Vec<ImageRef> {
    let bytes = markdown.as_bytes();
    let mut refs = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'!' && bytes[i + 1] == b'[' {
            // Find the closing `]` of the alt text.
            if let Some(alt_end) = markdown[i + 2..].find(']') {
                let alt_end = i + 2 + alt_end;
                // Require `(` immediately after `]`.
                if markdown.as_bytes().get(alt_end + 1) == Some(&b'(') {
                    if let Some(paren_end) = markdown[alt_end + 2..].find(')') {
                        let paren_end = alt_end + 2 + paren_end;
                        let alt = markdown[i + 2..alt_end].to_string();
                        let inner = markdown[alt_end + 2..paren_end].trim().to_string();
                        let full = markdown[i..paren_end + 1].to_string();
                        refs.push(ImageRef {
                            full,
                            alt,
                            target: inner,
                        });
                        i = paren_end + 1;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    refs
}

/// Extract the URL portion of a Markdown link target, dropping any optional
/// `"title"` suffix.
fn target_url(target: &str) -> &str {
    target.split_whitespace().next().unwrap_or(target)
}

/// Derive a stable, safe filename for `url` with `bytes` content.
///
/// The stem is a short hash of the canonical URL (so the same image dedupes
/// within a clip), and the extension is taken from the URL path or the
/// content type, defaulting to `.img`.
fn image_filename(url: &Url, content_type: Option<&str>) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    url.as_str().hash(&mut hasher);
    let stem = format!("{:016x}", hasher.finish());

    let ext = extension_from_path(url)
        .or_else(|| content_type.and_then(extension_from_content_type))
        .unwrap_or("img");
    format!("{stem}.{ext}")
}

fn extension_from_path(url: &Url) -> Option<&'static str> {
    let path = url.path();
    let dot = path.rfind('.')?;
    let raw = &path[dot + 1..];
    match raw.to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        "svg" => Some("svg"),
        "avif" => Some("avif"),
        "bmp" => Some("bmp"),
        _ => None,
    }
}

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
    let ct = content_type.split(';').next()?.trim().to_ascii_lowercase();
    match ct.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/avif" => Some("avif"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// Download the images referenced in `markdown` and rewrite their links.
///
/// - `base_url` resolves relative image targets.
/// - `asset_prefix` is prepended (with `/`) to each rewritten link, e.g.
///   `attachments/clips` → `![alt](attachments/clips/<file>)`.
/// - Only `http`/`https` targets are fetched; `data:` and already-relative
///   links are left untouched.
///
/// Returns the rewritten Markdown and the images to persist. Never errors: any
/// per-image failure leaves that link untouched.
pub async fn download_and_rewrite_images(
    markdown: &str,
    base_url: &str,
    asset_prefix: &str,
    limits: &FetchLimits,
) -> (String, Vec<DownloadedImage>) {
    let base = match Url::parse(base_url) {
        Ok(u) => u,
        Err(_) => return (markdown.to_string(), Vec::new()),
    };

    let refs = find_image_refs(markdown);
    let mut rewritten = markdown.to_string();
    let mut downloaded: Vec<DownloadedImage> = Vec::new();
    // Map canonical absolute URL → local filename, to dedupe repeats.
    let mut seen: HashMap<String, String> = HashMap::new();
    let prefix = asset_prefix.trim_matches('/');

    for image in refs {
        if downloaded.len() >= MAX_IMAGES {
            break;
        }
        let raw_url = target_url(&image.target);
        let Ok(abs) = base.join(raw_url) else {
            continue;
        };
        if abs.scheme() != "http" && abs.scheme() != "https" {
            continue;
        }

        let filename = if let Some(existing) = seen.get(abs.as_str()) {
            existing.clone()
        } else {
            let fetched = match fetch_bytes(abs.as_str(), limits).await {
                Ok(f) => f,
                Err(reason) => {
                    tracing::warn!(
                        image = %abs,
                        reason = %reason,
                        "clip image download failed; keeping remote link"
                    );
                    continue;
                }
            };
            let filename = image_filename(&abs, fetched.content_type.as_deref());
            downloaded.push(DownloadedImage {
                filename: filename.clone(),
                bytes: fetched.bytes,
            });
            seen.insert(abs.as_str().to_string(), filename.clone());
            filename
        };

        let local = if prefix.is_empty() {
            filename
        } else {
            format!("{prefix}/{filename}")
        };
        let replacement = format!("![{}]({})", image.alt, local);
        rewritten = rewritten.replacen(&image.full, &replacement, 1);
    }

    (rewritten, downloaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_image_refs() {
        let md = "Intro ![a](https://x/y.png) middle ![](https://x/z.jpg) end";
        let refs = find_image_refs(md);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].alt, "a");
        assert_eq!(target_url(&refs[0].target), "https://x/y.png");
        assert_eq!(refs[1].alt, "");
    }

    #[test]
    fn ignores_non_image_links() {
        let md = "A [link](https://x/y) not an image ![img](https://x/i.png)";
        let refs = find_image_refs(md);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].alt, "img");
    }

    #[test]
    fn target_url_drops_title() {
        assert_eq!(target_url("https://x/y.png \"a title\""), "https://x/y.png");
    }

    #[test]
    fn filename_uses_path_extension() {
        let url = Url::parse("https://x/a/b/pic.PNG").unwrap();
        assert!(image_filename(&url, None).ends_with(".png"));
    }

    #[test]
    fn filename_falls_back_to_content_type() {
        let url = Url::parse("https://x/image").unwrap();
        assert!(image_filename(&url, Some("image/jpeg")).ends_with(".jpg"));
    }

    #[test]
    fn filename_defaults_to_img() {
        let url = Url::parse("https://x/image").unwrap();
        assert!(image_filename(&url, None).ends_with(".img"));
    }

    #[tokio::test]
    async fn blocked_image_keeps_remote_link() {
        // A loopback image URL is SSRF-blocked; the link must survive untouched.
        let md = "![x](http://127.0.0.1/secret.png)";
        let (out, images) = download_and_rewrite_images(
            md,
            "https://example.com/post",
            "attachments",
            &FetchLimits::default(),
        )
        .await;
        assert_eq!(out, md);
        assert!(images.is_empty());
    }

    #[tokio::test]
    async fn no_images_is_noop() {
        let md = "Just text, no images.";
        let (out, images) = download_and_rewrite_images(
            md,
            "https://example.com/post",
            "attachments",
            &FetchLimits::default(),
        )
        .await;
        assert_eq!(out, md);
        assert!(images.is_empty());
    }
}
