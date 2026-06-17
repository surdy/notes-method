//! Helpers for computing per-vault window labels and URLs.
//!
//! Each Tauri window is bound to a single vault **on a single daemon** at
//! creation time. The window label is `main:<slug>-<short-hash>` where the slug
//! is a lossy sanitisation of the vault name and the short hash disambiguates
//! between vaults whose slugs would otherwise collide (e.g. `Foo Bar`,
//! `foo-bar`, `foo_bar` all reduce to slug `foo-bar`).
//!
//! Identity is **server-qualified**: the canonical key is a [`VaultKey`] of
//! `(server_id, vault)`, so the same vault name hosted by two different daemons
//! (e.g. a local vault and a remote vault both named `personal`) yields two
//! distinct windows. For the reserved local server (`servers::LOCAL_ID`) the
//! label is byte-for-byte identical to the legacy name-only label, so existing
//! windows and persisted geometry keep matching across the upgrade.

use crate::app_url::{FrontendMode, app_window_url};

/// Prefix used by every vault-bound window label.
pub const VAULT_WINDOW_LABEL_PREFIX: &str = "main:";

/// Length of the short hash suffix appended to vault window labels.
const HASH_SUFFIX_LEN: usize = 8;

/// Canonical, server-qualified identity of a vault window.
///
/// A window is uniquely identified by the daemon that hosts the vault
/// (`server_id`) plus the vault name. `server_id` is the stable id minted by
/// [`crate::servers`] (it survives server rename/URL edits); the reserved
/// [`crate::servers::LOCAL_ID`] denotes the local daemon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VaultKey {
    /// Stable id of the daemon hosting the vault (`servers::LOCAL_ID` = local).
    pub server_id: String,
    /// Vault name as registered on that daemon.
    pub vault: String,
}

impl VaultKey {
    /// Identity for `vault` on the daemon identified by `server_id`.
    pub fn new(server_id: impl Into<String>, vault: impl Into<String>) -> Self {
        Self {
            server_id: server_id.into(),
            vault: vault.into(),
        }
    }

    /// Identity for a vault on the local daemon.
    pub fn local(vault: impl Into<String>) -> Self {
        Self::new(crate::servers::LOCAL_ID, vault)
    }

    /// True when this identity refers to the local daemon.
    pub fn is_local(&self) -> bool {
        self.server_id == crate::servers::LOCAL_ID
    }
}

/// String fed to [`short_hash`] to make a label unique per `(server_id, vault)`.
///
/// For the local server this is exactly the vault name, so local labels match
/// the legacy name-only scheme byte-for-byte. For remote servers the stable
/// `server_id` is prefixed with a unit-separator (`U+001F`) — a character the
/// slug-form `server_id` can never contain, keeping the encoding injective.
fn canonical_identity(key: &VaultKey) -> String {
    if key.is_local() {
        key.vault.clone()
    } else {
        format!("{}\u{1f}{}", key.server_id, key.vault)
    }
}

/// Compute the canonical window label for a vault on the local daemon.
///
/// Thin shim over [`vault_window_label_for_key`] with the local server id. The
/// output is unchanged from the historical name-only scheme.
pub fn vault_window_label(vault: &str) -> String {
    vault_window_label_for_key(&VaultKey::local(vault))
}

/// Compute the canonical window label for a server-qualified vault identity.
///
/// The returned label is guaranteed to differ between distinct identities —
/// across vault names *and* across servers — even when their sanitised slugs
/// collide. For the local server it equals [`vault_window_label`] for the same
/// vault name.
pub fn vault_window_label_for_key(key: &VaultKey) -> String {
    let slug = slug_for_vault(&key.vault);
    let hash = short_hash(&canonical_identity(key));
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
    fn local_key_label_matches_legacy_name_label() {
        // The name-only shim must not change local-vault labels: existing
        // windows/persisted geometry keyed by the legacy label must still match.
        for name in ["personal", "work", "foo-bar", "日本", "Foo Bar"] {
            assert_eq!(
                vault_window_label_for_key(&VaultKey::local(name)),
                vault_window_label(name),
            );
        }
    }

    #[test]
    fn same_vault_distinct_servers_yield_distinct_labels() {
        let a = vault_window_label_for_key(&VaultKey::new("server-a", "personal"));
        let b = vault_window_label_for_key(&VaultKey::new("server-b", "personal"));
        let local = vault_window_label_for_key(&VaultKey::local("personal"));
        assert_ne!(a, b);
        assert_ne!(a, local);
        assert_ne!(b, local);
    }

    #[test]
    fn same_server_and_vault_is_stable() {
        let key = VaultKey::new("server-a", "personal");
        assert_eq!(
            vault_window_label_for_key(&key),
            vault_window_label_for_key(&key),
        );
    }

    #[test]
    fn remote_label_keeps_vault_slug_prefix() {
        let label = vault_window_label_for_key(&VaultKey::new("server-a", "Work Notes"));
        assert!(label.starts_with("main:work-notes-"), "got {label}");
    }

    #[test]
    fn cross_server_duplicate_after_slug_collision_still_distinct() {
        // Names that slug-collide, spread across servers — all labels distinct.
        let a = vault_window_label_for_key(&VaultKey::new("s1", "foo-bar"));
        let b = vault_window_label_for_key(&VaultKey::new("s2", "Foo Bar"));
        let c = vault_window_label_for_key(&VaultKey::local("foo_bar"));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn vault_key_local_constructor_uses_local_id() {
        assert!(VaultKey::local("x").is_local());
        assert_eq!(VaultKey::local("x").server_id, crate::servers::LOCAL_ID);
        assert!(!VaultKey::new("remote", "x").is_local());
    }

    #[test]
    fn remote_label_with_empty_slug_uses_hash_only() {
        let label = vault_window_label_for_key(&VaultKey::new("server-a", "日本"));
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
