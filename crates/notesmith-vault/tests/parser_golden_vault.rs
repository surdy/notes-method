use notesmith_core::{LinkType, Note, StatusGroup, VaultEngine, VaultName, VaultPath};
use notesmith_vault::{NativeVaultEngine, parse_note};
use std::fs;

fn golden_vault() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn read_note(relative_path: &str) -> String {
    let path = golden_vault().join(relative_path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()))
}

fn parse(relative_path: &str) -> Note {
    let content = read_note(relative_path);
    parse_note(
        &VaultName::new("test"),
        &VaultPath::new(relative_path),
        &content,
    )
}

#[test]
fn daily_note_frontmatter() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    assert_eq!(parsed.vault.as_str(), "test");
    assert_eq!(parsed.path.as_str(), "Inbox/Daily/2025-01-15.md");
    assert!(!parsed.hash.is_empty());
    assert!(parsed.frontmatter.is_some());
    assert!(parsed.raw_frontmatter.is_some());
    assert!(!parsed.body.starts_with("---"));
}

#[test]
fn note_without_frontmatter() {
    let parsed = parse("Customers/Acme/Acme Corp.md");
    assert!(parsed.frontmatter.is_some());
}

#[test]
fn wikilinks_in_customer_note() {
    let parsed = parse("Customers/Acme/Acme Corp.md");
    let wikilinks: Vec<_> = parsed
        .links
        .iter()
        .filter(|l| l.link_type == LinkType::WikiLink)
        .collect();
    assert!(
        !wikilinks.is_empty(),
        "Should find wikilinks in customer note"
    );
}

#[test]
fn wikilink_with_alias() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    let aliased: Vec<_> = parsed
        .links
        .iter()
        .filter(|l| l.link_type == LinkType::WikiLink && l.display_text.is_some())
        .collect();
    assert!(!aliased.is_empty(), "Should find aliased wikilinks");
}

#[test]
fn wikilinks_inside_code_blocks_are_ignored() {
    let parsed = parse("General/OFM Edge Cases.md");
    let targets: Vec<_> = parsed.links.iter().map(|l| l.target.as_str()).collect();
    assert!(
        !targets.contains(&"ShouldNotParse"),
        "Links inside code blocks must be ignored"
    );
    assert!(
        !targets.contains(&"NotALink"),
        "Links inside fenced code blocks must be ignored"
    );
}

#[test]
fn embed_in_note() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    let embeds: Vec<_> = parsed
        .links
        .iter()
        .filter(|l| l.link_type == LinkType::Embed)
        .collect();
    assert!(!embeds.is_empty(), "Should find embeds");
}

#[test]
fn inline_fields_in_daily_note() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    assert!(
        !parsed.inline_fields.is_empty(),
        "Daily note should have inline fields on tasks"
    );
}

#[test]
fn inline_field_key_value() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    let customer_fields: Vec<_> = parsed
        .inline_fields
        .iter()
        .filter(|f| f.key == "owner")
        .collect();
    assert!(
        !customer_fields.is_empty(),
        "Should find owner inline fields"
    );
}

#[test]
fn all_seven_task_statuses_parsed() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    let statuses: std::collections::HashSet<_> =
        parsed.tasks.iter().map(|t| t.status_char).collect();
    assert!(statuses.contains(&' '), "Missing Todo");
    assert!(statuses.contains(&'/'), "Missing InProgress");
    assert!(statuses.contains(&'b'), "Missing Blocked");
    assert!(statuses.contains(&'w'), "Missing Waiting");
    assert!(statuses.contains(&'h'), "Missing OnHold");
    assert!(statuses.contains(&'x'), "Missing Done");
    assert!(statuses.contains(&'-'), "Missing Cancelled");
}

#[test]
fn task_status_groups_resolve_from_default_map() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    assert!(
        parsed
            .tasks
            .iter()
            .any(|task| task.status_group == StatusGroup::Done)
    );
    assert!(
        parsed
            .tasks
            .iter()
            .any(|task| task.status_group == StatusGroup::Open)
    );
}

#[test]
fn task_content_keeps_unparsed_inline_metadata() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    assert!(
        parsed
            .tasks
            .iter()
            .any(|task| task.content.contains("📅 2025-01-16"))
    );
    assert!(parsed.tasks.iter().any(|task| task.content.contains("🔼")));
}

#[test]
fn task_content_hash() {
    let parsed = parse("Inbox/Daily/2025-01-15.md");
    for task in &parsed.tasks {
        assert!(
            task.content_hash.is_some(),
            "Every task should have a content hash"
        );
    }
}

#[test]
fn block_references() {
    let parsed = parse("General/OFM Edge Cases.md");
    let with_id: Vec<_> = parsed
        .blocks
        .iter()
        .filter(|b| b.block_id.is_some())
        .collect();
    assert!(!with_id.is_empty(), "Should find block references with ^id");
}

#[test]
fn external_links() {
    let parsed = parse("General/OFM Edge Cases.md");
    let external: Vec<_> = parsed
        .links
        .iter()
        .filter(|l| l.link_type == LinkType::ExternalLink)
        .collect();
    assert!(!external.is_empty(), "Should find external markdown links");
}

#[test]
fn scan_golden_vault() {
    let engine = NativeVaultEngine;
    let vault_root = golden_vault();
    let notes = engine.scan(&vault_root).unwrap();

    assert!(
        notes.len() >= 20,
        "Should find at least 20 notes, got {}",
        notes.len()
    );

    for note in &notes {
        assert!(
            !note.hash.is_empty(),
            "Note {} should have a hash",
            note.path
        );
    }

    let with_fm: Vec<_> = notes.iter().filter(|n| n.frontmatter.is_some()).collect();
    assert!(
        with_fm.len() >= 15,
        "Most notes should have frontmatter, got {}",
        with_fm.len()
    );
}

#[test]
fn scan_skips_hidden_directories() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let vault = tmp.path();

    fs::create_dir_all(vault.join(".notesmith")).unwrap();
    fs::create_dir_all(vault.join(".obsidian")).unwrap();
    fs::write(vault.join("note.md"), "---\ntype: note\n---\n# Hello").unwrap();
    fs::write(vault.join(".notesmith/vault.toml"), "name = \"test\"").unwrap();
    fs::write(vault.join(".obsidian/config.json"), "{}").unwrap();
    fs::write(vault.join(".obsidian/hidden.md"), "# Hidden").unwrap();

    let engine = NativeVaultEngine;
    let notes = engine.scan(vault).unwrap();
    assert_eq!(notes.len(), 1, "Should only find the non-hidden note");
    assert_eq!(notes[0].path.as_str(), "note.md");
}
