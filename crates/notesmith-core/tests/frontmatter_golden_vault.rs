//! Test that golden-vault frontmatter parses into the generic Frontmatter map.
//!
//! The fixture follows the blessed Work Notes schema (`docs/example-work-notes-kit.md`):
//! `kind` is the canonical type field, and relationships live in frontmatter
//! lists of quoted wikilinks (`customers`, `streams`, `attendees`).

use notesmith_core::Frontmatter;
use serde_yaml::Value;
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

/// A list field as a `Vec<String>` — the shape every wikilink list field uses.
fn list_field(fm: &Frontmatter, key: &str) -> Vec<String> {
    match fm.fields.get(key) {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(String::from))
            .collect(),
        other => panic!("expected `{key}` to be a YAML sequence, got {other:?}"),
    }
}

#[test]
fn daily_note_preserves_generic_fields() {
    let fm = read_and_parse("Daily/2025-01-15.md");
    assert_eq!(fm.get_string("date"), Some("2025-01-15".to_string()));
    assert!(fm.tags().contains(&"daily".to_string()));
}

#[test]
fn meeting_note_uses_kind_audience_and_wikilink_lists() {
    let fm = read_and_parse("Meetings/2025/01/2025-01-15 - Internal Sync.md");

    assert_eq!(fm.get_str("kind"), Some("meeting"));
    assert_eq!(fm.get_str("audience"), Some("internal"));
    assert_eq!(fm.get_string("date"), Some("2025-01-15".to_string()));
    assert_eq!(list_field(&fm, "customers"), vec!["[[Acme Corp]]"]);
    assert_eq!(list_field(&fm, "streams"), vec!["[[Migration to v2]]"]);
    assert_eq!(
        list_field(&fm, "attendees"),
        vec!["[[Sarah Chen]]", "[[Mike Alvarez]]"]
    );

    // Dropped from the old schema.
    assert!(!fm.has_field("type"), "`type` is replaced by `kind`");
    assert!(
        !fm.has_field("meeting-kind"),
        "`meeting-kind` is replaced by `audience`"
    );
    assert!(
        !fm.has_field("customer"),
        "singular `customer` is replaced by the `customers` list"
    );
}

#[test]
fn external_meeting_has_exactly_one_customer_and_a_meeting_type() {
    let fm = read_and_parse("Meetings/2025/01/2025-01-14 - Acme Corp - Customer Check-in.md");

    assert_eq!(fm.get_str("audience"), Some("external"));
    assert_eq!(
        list_field(&fm, "customers").len(),
        1,
        "external meetings have exactly one customer"
    );
    assert_eq!(fm.get_str("meeting_type"), Some("status"));
}

#[test]
fn internal_meeting_can_span_multiple_customers_and_streams() {
    let fm = read_and_parse("Meetings/2025/01/2025-01-16 - Cross-customer Migration Review.md");

    assert_eq!(fm.get_str("audience"), Some("internal"));
    assert_eq!(
        list_field(&fm, "customers"),
        vec!["[[Acme Corp]]", "[[Globex]]"]
    );
    assert_eq!(
        list_field(&fm, "streams"),
        vec!["[[Migration to v2]]", "[[Platform Rollout]]"]
    );
    assert!(list_field(&fm, "attendees").len() >= 2);
}

#[test]
fn stream_note_uses_lowercase_status_vocabulary() {
    let fm = read_and_parse("Streams/Migration to v2.md");

    assert_eq!(fm.get_str("kind"), Some("stream"));
    assert_eq!(fm.get_str("status"), Some("active"));
    assert_eq!(fm.get_str("priority"), Some("P1"));
    assert_eq!(list_field(&fm, "customers"), vec!["[[Acme Corp]]"]);
    assert!(
        !fm.has_field("owner"),
        "stream `owner` is dropped — delegation is per-task"
    );
}

#[test]
fn internal_stream_carries_an_empty_customers_list() {
    let fm = read_and_parse("Streams/Internal - Support Process Redesign.md");

    assert_eq!(fm.get_str("kind"), Some("stream"));
    assert_eq!(fm.get_str("status"), Some("done"));
    assert!(
        list_field(&fm, "customers").is_empty(),
        "a zero-item list is a valid `customers` value"
    );
}

#[test]
fn customer_note_uses_kind_and_drops_state() {
    let fm = read_and_parse("Customers/Acme Corp/Acme Corp.md");

    assert_eq!(fm.get_str("kind"), Some("customer"));
    assert!(!fm.has_field("state"), "customer `state` is dropped");
    assert!(!fm.has_field("type"));
}

#[test]
fn account_note_links_back_to_its_customer() {
    let fm = read_and_parse("Customers/Acme Corp/Account Info.md");

    assert_eq!(fm.get_str("kind"), Some("account"));
    assert_eq!(list_field(&fm, "customers"), vec!["[[Acme Corp]]"]);
}

#[test]
fn person_note_carries_org_and_role() {
    let fm = read_and_parse("People/Jane Doe.md");

    assert_eq!(fm.get_str("kind"), Some("person"));
    assert_eq!(fm.get_str("org"), Some("[[Acme Corp]]"));
    assert_eq!(fm.get_str("role"), Some("CTO"));
}

#[test]
fn generic_note_preserves_unknown_frontmatter() {
    let fm = read_and_parse("General/Prototype Notes.md");
    assert_eq!(fm.get_str("_icon"), Some("🔬"));
    assert_eq!(fm.get_str("stage"), Some("discovery"));
    assert!(fm.has_field("tags"));
}
