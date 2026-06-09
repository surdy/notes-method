//! Helpers for computing per-vault window labels and URLs.
//!
//! Each Tauri window is bound to a single vault at creation time. The window
//! label is `main:<slug>-<short-hash>` where the slug is a lossy sanitisation
//! of the vault name and the short hash disambiguates between vaults whose
//! slugs would otherwise collide (e.g. `Foo Bar`, `foo-bar`, `foo_bar` all
//! reduce to slug `foo-bar`).

use crate::app_url::{FrontendMode, app_window_url};

/// Prefix used by every vault-bound window label.
pub const VAULT_WINDOW_LABEL_PREFIX: &str = "main:";

/// Length of the short hash suffix appended to vault window labels.
const HASH_SUFFIX_LEN: usize = 8;

/// Compute the canonical window label for a vault.
///
/// The returned label is guaranteed to differ between distinct vault names,
/// even when their sanitised slugs collide.
pub fn vault_window_label(vault: &str) -> String {
    let slug = slug_for_vault(vault);
    let hash = short_hash(vault);
    if slug.is_empty() {
        format!("{VAULT_WINDOW_LABEL_PREFIX}{hash}")
    } else {
        format!("{VAULT_WINDOW_LABEL_PREFIX}{slug}-{hash}")
    }
}

/// Returns true if the given window label was produced by [`vault_window_label`].
pub fn is_vault_window_label(label: &str) -> bool {
    label.starts_with(VAULT_WINDOW_LABEL_PREFIX)
}

/// Sanitise a vault name into a slug usable as part of a window label.
///
/// The slug keeps ASCII lowercase letters and digits. Every other character
/// becomes `-`. Runs of `-` collapse to one and leading/trailing `-` are
/// trimmed. The slug alone is **not** collision-proof — callers must
/// combine it with [`short_hash`] for unique labels.
pub fn slug_for_vault(vault: &str) -> String {
    let mut out = String::with_capacity(vault.len());
    let mut last_dash = true;
    for ch in vault.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        if mapped == '-' {
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
        } else {
            out.push(mapped);
            last_dash = false;
        }
    }
    // trim trailing '-'
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Stable 8-hex-character hash of a vault name.
///
/// Uses FNV-1a over the UTF-8 bytes of the vault name. The exact algorithm
/// is an implementation detail; what matters is determinism across launches
/// and a sufficiently low collision rate within a single user's vault set.
pub fn short_hash(vault: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in vault.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let hex = format!("{hash:016x}");
    hex[..HASH_SUFFIX_LEN].to_string()
}

/// Build the webview URL for a vault, given the daemon base URL.
///
/// Returns a string like `http://127.0.0.1:27183/app/?vault=<encoded>`.
pub fn vault_app_url(daemon_base: &str, vault: &str) -> String {
    app_window_url(daemon_base, Some(vault), FrontendMode::Daemon)
}

/// Build the webview URL for the onboarding/no-vault flow.
pub fn onboarding_app_url(daemon_base: &str) -> String {
    app_window_url(daemon_base, None, FrontendMode::Daemon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_lowercases_ascii_alphanumeric() {
        assert_eq!(slug_for_vault("Work"), "work");
        assert_eq!(slug_for_vault("FOO123"), "foo123");
    }

    #[test]
    fn slug_replaces_separators_and_collapses_runs() {
        assert_eq!(slug_for_vault("Foo Bar"), "foo-bar");
        assert_eq!(slug_for_vault("foo_bar"), "foo-bar");
        assert_eq!(slug_for_vault("foo-bar"), "foo-bar");
        assert_eq!(slug_for_vault("foo  bar"), "foo-bar");
        assert_eq!(slug_for_vault("foo / bar"), "foo-bar");
    }

    #[test]
    fn slug_trims_trailing_separators() {
        assert_eq!(slug_for_vault("foo!"), "foo");
        assert_eq!(slug_for_vault("foo --- "), "foo");
    }

    #[test]
    fn slug_handles_non_ascii_as_separator() {
        // Non-ASCII gets dropped to '-' which then collapses.
        assert_eq!(slug_for_vault("café"), "caf");
        assert_eq!(slug_for_vault("日本"), "");
    }

    #[test]
    fn short_hash_is_deterministic() {
        let a = short_hash("work");
        let b = short_hash("work");
        assert_eq!(a, b);
        assert_eq!(a.len(), HASH_SUFFIX_LEN);
    }

    #[test]
    fn short_hash_differs_for_distinct_inputs() {
        assert_ne!(short_hash("foo-bar"), short_hash("Foo Bar"));
        assert_ne!(short_hash("foo-bar"), short_hash("foo_bar"));
        assert_ne!(short_hash("a"), short_hash("b"));
    }

    #[test]
    fn vault_window_label_collision_proof_across_slug_collisions() {
        // The three vault names below all slug to "foo-bar" — labels must still differ.
        let a = vault_window_label("foo-bar");
        let b = vault_window_label("Foo Bar");
        let c = vault_window_label("foo_bar");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
        assert!(a.starts_with("main:foo-bar-"));
        assert!(b.starts_with("main:foo-bar-"));
        assert!(c.starts_with("main:foo-bar-"));
    }

    #[test]
    fn vault_window_label_is_stable_across_calls() {
        assert_eq!(vault_window_label("work"), vault_window_label("work"));
    }

    #[test]
    fn vault_window_label_for_empty_slug_uses_hash_only() {
        let label = vault_window_label("日本");
        assert!(label.starts_with("main:"));
        assert!(!label.starts_with("main:-"));
        assert_eq!(label.len(), "main:".len() + HASH_SUFFIX_LEN);
    }

    #[test]
    fn is_vault_window_label_recognises_prefix() {
        assert!(is_vault_window_label("main:work-12345678"));
        assert!(!is_vault_window_label("main"));
        assert!(!is_vault_window_label("startup-splash"));
    }

    #[test]
    fn vault_app_url_appends_query() {
        assert_eq!(
            vault_app_url("http://127.0.0.1:27183", "work"),
            "http://127.0.0.1:27183/app/?vault=work"
        );
        assert_eq!(
            vault_app_url("http://127.0.0.1:27183/", "work"),
            "http://127.0.0.1:27183/app/?vault=work"
        );
    }

    #[test]
    fn vault_app_url_percent_encodes_special_chars() {
        assert_eq!(
            vault_app_url("http://x", "Foo Bar"),
            "http://x/app/?vault=Foo%20Bar"
        );
        assert_eq!(
            vault_app_url("http://x", "a&b=c"),
            "http://x/app/?vault=a%26b%3Dc"
        );
    }

    #[test]
    fn onboarding_app_url_has_no_query() {
        assert_eq!(
            onboarding_app_url("http://127.0.0.1:27183"),
            "http://127.0.0.1:27183/app/"
        );
        assert_eq!(
            onboarding_app_url("http://127.0.0.1:27183/"),
            "http://127.0.0.1:27183/app/"
        );
    }
}
