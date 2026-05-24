//! Test that golden-vault frontmatter parses into the generic Frontmatter map.

use notesmith_core::Frontmatter;
use std::fs;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn parse_frontmatter(content: &str) -> Option<Frontmatter> {
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("---")?;
    let yaml = &rest[..end];
    serde_yaml::from_str(yaml).ok()
}

fn read_and_parse(relative_path: &str) -> Frontmatter {
    let path = golden_vault().join(relative_path);
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    parse_frontmatter(&content)
        .unwrap_or_else(|| panic!("Failed to parse frontmatter from {relative_path}"))
}

#[test]
fn daily_note_preserves_generic_fields() {
    let fm = read_and_parse("Inbox/Daily/2025-01-15.md");
    assert_eq!(fm.get_str("type"), Some("daily"));
    assert_eq!(fm.get_string("date"), Some("2025-01-15".to_string()));
    assert!(fm.tags().contains(&"daily".to_string()));
}

#[test]
fn meeting_note_keeps_customer_and_meeting_kind() {
    let fm = read_and_parse("Customers/Acme/Internal Meetings/2025-01-15 Internal Sync.md");
    assert_eq!(fm.get_str("type"), Some("meeting"));
    assert_eq!(fm.get_str("meeting-kind"), Some("internal"));
    assert_eq!(fm.get_str("customer"), Some("[[Acme Corp]]"));
}

#[test]
fn stream_note_keeps_status_and_owner() {
    let fm = read_and_parse("Customers/Acme/Streams/Migration to v2.md");
    assert_eq!(fm.get_str("type"), Some("stream"));
    assert_eq!(fm.get_str("status"), Some("In Progress"));
    assert_eq!(fm.get_str("owner"), Some("me"));
}

#[test]
fn customer_note_preserves_state() {
    let fm = read_and_parse("Customers/Acme/Acme Corp.md");
    assert_eq!(fm.get_str("type"), Some("customer"));
    assert_eq!(fm.get_str("state"), Some("Active"));
}

#[test]
fn generic_note_preserves_unknown_frontmatter() {
    let fm = read_and_parse("General/Prototype Notes.md");
    assert_eq!(fm.get_str("type"), Some("note"));
    assert_eq!(fm.get_str("_icon"), Some("🔬"));
    assert!(fm.has_field("tags"));
}
