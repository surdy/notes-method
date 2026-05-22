//! Vault menu helpers: pure functions for menu id encoding and decoding.
//!
//! Vault submenu entries live on both the system tray and the application
//! menu. Their ids encode the vault name so a single `handle_menu_event`
//! arm can dispatch to `open_vault_window`. We URL-encode the name so
//! arbitrary unicode and punctuation survive a round-trip through Tauri's
//! `MenuId` (which is a free-form string but historically picky about
//! shell-like characters).

/// Prefix for menu ids that open a registered vault.
pub const OPEN_VAULT_PREFIX: &str = "open_vault::";

/// Menu id for the "Open Folder…" / "Open Folder as Vault…" entry.
pub const OPEN_FOLDER_AS_VAULT_ID: &str = "open_folder_as_vault";

/// Encode a vault name into a menu id.
pub fn encode_open_vault_id(vault: &str) -> String {
    let mut out = String::with_capacity(OPEN_VAULT_PREFIX.len() + vault.len());
    out.push_str(OPEN_VAULT_PREFIX);
    for ch in vault.chars() {
        if is_unreserved(ch) {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for byte in ch.encode_utf8(&mut buf).as_bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

/// Reverse of [`encode_open_vault_id`]. Returns `None` for ids that don't
/// match the prefix or that are malformed.
pub fn decode_open_vault_id(id: &str) -> Option<String> {
    let rest = id.strip_prefix(OPEN_VAULT_PREFIX)?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_value(bytes[i + 1])?;
            let lo = hex_value(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn is_unreserved(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~')
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(10 + byte - b'a'),
        b'A'..=b'F' => Some(10 + byte - b'A'),
        _ => None,
    }
}

/// Validate a user-supplied vault display name for `open_folder_as_vault`.
///
/// Returns `Err(message)` for the frontend to surface inline. Rules:
/// - non-empty after trimming
/// - no path separators
/// - no leading dot (avoids hidden-file collisions in any future on-disk
///   sidecar)
/// - not in the `existing` set (case-sensitive — matches how
///   `GlobalConfig::vaults` keys them today)
pub fn validate_vault_display_name<I, S>(name: &str, existing: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Vault name must not be empty".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("Vault name must not contain path separators".to_string());
    }
    if trimmed.starts_with('.') {
        return Err("Vault name must not start with '.'".to_string());
    }
    for candidate in existing {
        if candidate.as_ref() == trimmed {
            return Err(format!("Vault '{trimmed}' is already registered"));
        }
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_passes_through_unreserved_chars() {
        assert_eq!(encode_open_vault_id("personal"), "open_vault::personal");
        assert_eq!(encode_open_vault_id("work-2024"), "open_vault::work-2024");
    }

    #[test]
    fn encode_percent_encodes_spaces_and_punctuation() {
        assert_eq!(encode_open_vault_id("My Notes"), "open_vault::My%20Notes");
        assert_eq!(encode_open_vault_id("a/b"), "open_vault::a%2Fb");
    }

    #[test]
    fn encode_percent_encodes_non_ascii() {
        assert_eq!(encode_open_vault_id("ñ"), "open_vault::%C3%B1");
    }

    #[test]
    fn decode_round_trips_for_ascii() {
        let id = encode_open_vault_id("personal");
        assert_eq!(decode_open_vault_id(&id).as_deref(), Some("personal"));
    }

    #[test]
    fn decode_round_trips_for_spaces_and_unicode() {
        for vault in ["My Notes", "ñ", "a/b", "Émojis 🚀", "with:colon"] {
            let id = encode_open_vault_id(vault);
            assert_eq!(
                decode_open_vault_id(&id).as_deref(),
                Some(vault),
                "round-trip failed for {vault:?}"
            );
        }
    }

    #[test]
    fn decode_returns_none_for_non_matching_id() {
        assert_eq!(decode_open_vault_id("not_a_vault_id"), None);
        assert_eq!(decode_open_vault_id("open_folder_as_vault"), None);
        assert_eq!(decode_open_vault_id(""), None);
    }

    #[test]
    fn decode_returns_none_for_malformed_percent_escape() {
        assert_eq!(decode_open_vault_id("open_vault::ab%2"), None);
        assert_eq!(decode_open_vault_id("open_vault::ab%ZZ"), None);
    }

    #[test]
    fn validate_rejects_empty_or_whitespace() {
        assert!(validate_vault_display_name("", std::iter::empty::<&str>()).is_err());
        assert!(validate_vault_display_name("   ", std::iter::empty::<&str>()).is_err());
    }

    #[test]
    fn validate_rejects_path_separators_and_dotfiles() {
        assert!(validate_vault_display_name("a/b", std::iter::empty::<&str>()).is_err());
        assert!(validate_vault_display_name("a\\b", std::iter::empty::<&str>()).is_err());
        assert!(validate_vault_display_name(".hidden", std::iter::empty::<&str>()).is_err());
    }

    #[test]
    fn validate_rejects_duplicates() {
        let existing = ["personal", "work"];
        let err = validate_vault_display_name("personal", existing.iter()).unwrap_err();
        assert!(err.contains("already registered"), "got: {err}");
    }

    #[test]
    fn validate_trims_and_returns_normalised_name() {
        assert_eq!(
            validate_vault_display_name("  newvault  ", std::iter::empty::<&str>()).unwrap(),
            "newvault"
        );
    }
}
