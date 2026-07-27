//! Scaffolding the Work Notes kit must produce a vault that actually works —
//! not just a directory of files. These tests apply the kit into a temp vault
//! and then drive the real engines over the result (#275).

use std::path::Path;

use notesmith_kit::{ApplyOptions, Kit};

fn apply_into(root: &Path) -> notesmith_kit::ApplyReport {
    Kit::builtin("work-notes")
        .expect("work-notes kit should be built in")
        .apply(root, &ApplyOptions::for_vault("work"))
        .expect("applying the kit should succeed")
}

#[test]
fn builtin_registry_exposes_the_work_notes_kit() {
    let ids: Vec<&str> = Kit::all().iter().map(|kit| kit.id()).collect();
    assert!(ids.contains(&"work-notes"), "got {ids:?}");
    assert!(Kit::builtin("nope").is_none());

    let kit = Kit::builtin("work-notes").unwrap();
    assert!(!kit.description().is_empty());
    assert!(
        kit.files().len() >= 15,
        "kit should carry config, templates and dashboards, got {}",
        kit.files().len()
    );
}

#[test]
fn apply_writes_config_templates_dashboards_and_folders() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    let report = apply_into(root);

    for relative in [
        ".notesmith/vault.toml",
        ".notesmith/fields.toml",
        ".notesmith/routing.yaml",
        ".notesmith/skill.md",
        ".notesmith/templates/internal-meeting.md",
        ".notesmith/templates/external-meeting.md",
        ".notesmith/templates/stream.md",
        ".notesmith/templates/customer.md",
        ".notesmith/templates/person.md",
        ".notesmith/templates/daily.md",
        ".notesmith/templates/weekly.md",
        ".notesmith/templates/quarterly.md",
        ".notesmith/templates/generic-note.md",
        "Dashboards/Home.md",
    ] {
        assert!(root.join(relative).is_file(), "missing {relative}");
    }

    for folder in [
        "Inbox",
        "Meetings",
        "Streams",
        "Customers",
        "People",
        "Daily",
        "Weekly",
        "Quarterly",
        "Dashboards",
    ] {
        assert!(root.join(folder).is_dir(), "missing folder {folder}");
    }

    assert!(report.skipped.is_empty());
    assert_eq!(
        report.written.len(),
        Kit::builtin("work-notes").unwrap().files().len()
    );
}

#[test]
fn vault_name_is_substituted_into_the_config() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let config = notesmith_config::VaultConfig::load_from_vault(root).unwrap();
    assert_eq!(config.name, "work");
    assert_eq!(config.capture.folder, "Inbox");

    let daily = config.periodic.daily.as_ref().unwrap();
    assert_eq!(daily.folder, "Daily");
    assert_eq!(daily.filename, "{{ date }}");
    assert!(config.periodic.weekly.is_some());
    assert!(config.periodic.quarterly.is_some());
}

#[test]
fn a_scaffolded_vault_routes_meetings_streams_and_people() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let engine = notesmith_routing::RoutingEngine::load(root)
        .expect("the scaffolded routing.yaml should load");

    let meeting = engine
        .preview(
            "Inbox/2026-07-24 - Acme - Check-in.md",
            "---\nkind: meeting\naudience: external\ndate: 2026-07-24\n---\n# Check-in\n",
        )
        .unwrap();
    assert_eq!(
        meeting.destination,
        "Meetings/2026/07/2026-07-24 - Acme - Check-in.md"
    );

    let stream = engine
        .preview(
            "Inbox/Renewal.md",
            "---\nkind: stream\nstatus: active\n---\n# Renewal\n",
        )
        .unwrap();
    assert_eq!(stream.destination, "Streams/Renewal.md");

    let person = engine
        .preview("Inbox/Jane Doe.md", "---\nkind: person\n---\n# Jane\n")
        .unwrap();
    assert_eq!(person.destination, "People/Jane Doe.md");
}

#[test]
fn a_scaffolded_vault_renders_its_templates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), None);
    let templates = engine.list_templates().unwrap();
    assert_eq!(templates.len(), 9, "kit ships nine templates");

    let prompts = std::collections::HashMap::from([
        ("title".to_string(), "Kickoff".to_string()),
        ("customer".to_string(), "Acme Corp".to_string()),
    ]);
    let rendered = engine.render("external-meeting", &prompts).unwrap();
    assert!(rendered.path.starts_with("Inbox/"));
    assert!(rendered.content.contains("kind: meeting"));
    assert!(rendered.content.contains("- \"[[Acme Corp]]\""));
}

#[test]
fn a_scaffolded_vault_has_a_usable_field_registry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let registry = notesmith_index::FieldRegistry::load(root);
    assert_eq!(
        registry.get("kind").unwrap().field_type,
        notesmith_index::FieldType::Enum
    );
    assert_eq!(registry.get("customers").unwrap().multivalue, Some(true));
    // Advisory validation works against the shipped vocabulary.
    assert!(registry.validate("status", "active").is_none());
    assert!(registry.validate("status", "In Progress").is_some());
}

#[test]
fn every_dashboard_query_executes_against_a_scaffolded_vault() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let notes = notesmith_core::VaultEngine::scan(&notesmith_vault::NativeVaultEngine, root)
        .expect("scaffolded vault should scan");
    let cache = notesmith_index::VaultCache::open_in_memory().unwrap();
    cache.reindex("kit", &notes).unwrap();

    let mut checked = 0;
    let mut failures = Vec::new();
    for (relative, contents) in Kit::builtin("work-notes").unwrap().files() {
        if !relative.starts_with("Dashboards/") {
            continue;
        }
        for sql in notesmith_sql_blocks(contents) {
            checked += 1;
            if let Err(error) = notesmith_query::execute_sql(&cache, &sql) {
                failures.push(format!("{relative}\nSQL:\n{sql}\nError: {error}"));
            }
        }
    }

    assert!(
        checked >= 8,
        "expected dashboard SQL to check, got {checked}"
    );
    assert!(
        failures.is_empty(),
        "kit dashboards must execute on a fresh vault.\n\n{}",
        failures.join("\n\n---\n\n")
    );
}

#[test]
fn apply_is_idempotent_and_never_clobbers_local_edits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    // A hand-edited rule must survive a re-apply.
    let routing = root.join(".notesmith/routing.yaml");
    std::fs::write(&routing, "version: 1\nrules: []\n").unwrap();

    let second = apply_into(root);

    assert!(
        second.written.is_empty(),
        "re-applying should write nothing, wrote {:?}",
        second.written
    );
    assert_eq!(
        second.skipped.len(),
        Kit::builtin("work-notes").unwrap().files().len()
    );
    assert_eq!(
        std::fs::read_to_string(&routing).unwrap(),
        "version: 1\nrules: []\n",
        "a locally edited file must not be overwritten"
    );
}

#[test]
fn force_overwrites_and_dry_run_writes_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let routing = root.join(".notesmith/routing.yaml");
    std::fs::write(&routing, "version: 1\nrules: []\n").unwrap();

    let kit = Kit::builtin("work-notes").unwrap();

    // Dry run reports what *would* change without touching the disk.
    let preview = kit
        .apply(
            root,
            &ApplyOptions::for_vault("work").force(true).dry_run(true),
        )
        .unwrap();
    assert_eq!(preview.written.len(), kit.files().len());
    assert_eq!(
        std::fs::read_to_string(&routing).unwrap(),
        "version: 1\nrules: []\n",
        "dry run must not write"
    );

    let forced = kit
        .apply(root, &ApplyOptions::for_vault("work").force(true))
        .unwrap();
    assert_eq!(forced.written.len(), kit.files().len());
    assert!(forced.skipped.is_empty());
    assert!(
        std::fs::read_to_string(&routing)
            .unwrap()
            .contains("file-meeting"),
        "force should restore the kit's routing rules"
    );
}

#[test]
fn apply_into_a_populated_vault_leaves_existing_notes_alone() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("Inbox")).unwrap();
    std::fs::write(root.join("Inbox/Existing.md"), "# Existing\n").unwrap();
    std::fs::create_dir_all(root.join("Dashboards")).unwrap();
    std::fs::write(root.join("Dashboards/Home.md"), "# My own home\n").unwrap();

    let report = apply_into(root);

    assert_eq!(
        std::fs::read_to_string(root.join("Inbox/Existing.md")).unwrap(),
        "# Existing\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("Dashboards/Home.md")).unwrap(),
        "# My own home\n"
    );
    assert!(report.skipped.iter().any(|p| p == "Dashboards/Home.md"));
    assert!(report.written.iter().any(|p| p == ".notesmith/vault.toml"));
}

/// Same extraction the golden-vault fence test uses.
fn notesmith_sql_blocks(content: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut block_lines: Option<Vec<&str>> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(lines) = block_lines.as_mut() {
            if trimmed == "```" {
                let sql = lines.join("\n").trim().to_string();
                if !sql.is_empty() {
                    blocks.push(sql);
                }
                block_lines = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if let Some(info) = trimmed.strip_prefix("```") {
            let normalized = info.split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized.eq_ignore_ascii_case("notesmith sql") {
                block_lines = Some(Vec::new());
            }
        }
    }

    blocks
}
