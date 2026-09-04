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

// ── Meeting prefill (integrations plan, feature 1) ───────────────────────────

/// Writes a `kind: event` note shaped exactly like calendar-sync.py's output,
/// offset from `now`, and returns its vault path. A negative offset puts the
/// meeting already in progress — including across midnight, which is exactly
/// the case the candidate query's three-day window exists for.
fn write_event_note(root: &Path, offset_minutes: i64) -> (String, chrono::DateTime<chrono::Local>) {
    let start = chrono::Local::now() + chrono::Duration::minutes(offset_minutes);
    let end = start + chrono::Duration::minutes(30);
    let path = format!(
        "Calendar/{}/{}/{} {} Acme Q3 sync.md",
        start.format("%Y"),
        start.format("%m"),
        start.format("%Y-%m-%d"),
        start.format("%H%M"),
    );
    let note = format!(
        "---\nkind: event\nevent_id: AAMkAGI2-prefill-0001\nstart: {}\nend: {}\n\
         attendees: [\"alice@acme.com\", \"harpreet@corp.example.com\"]\n\
         audience: external\ncustomers: [\"[[Acme Corp]]\"]\norganizer: alice@acme.com\n\
         tags: [\"calendar\"]\n---\n\n<!-- Machine-owned calendar record -->\n",
        start.format("%Y-%m-%dT%H:%M:%S"),
        end.format("%Y-%m-%dT%H:%M:%S"),
    );
    let full = root.join(&path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, note).unwrap();
    (path, start)
}

/// Indexes the vault into an on-disk cache the template engine can query.
fn indexed_cache(root: &Path, cache_path: &Path) {
    let notes = notesmith_core::VaultEngine::scan(&notesmith_vault::NativeVaultEngine, root)
        .expect("vault should scan");
    let cache = notesmith_index::VaultCache::open(cache_path).unwrap();
    cache.reindex("kit", &notes).unwrap();
}

/// The whole feature end to end: a real calendar event in the cache, the real
/// kit templates, the real `sh`-invoked hook. Creating a meeting mid-call must
/// carry the event's title, customer, attendees and `event_id` across without
/// the user typing any of them.
#[test]
fn creating_a_meeting_during_a_call_prefills_from_the_calendar_event() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        println!("skipping: python3 not on PATH");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);
    // Started five minutes ago — we are in the meeting.
    let (event_path, event_start) = write_event_note(root, -5);

    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().join("cache.db");
    indexed_cache(root, &cache_path);

    let engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), Some(cache_path));
    // No prompts at all: everything below comes from the calendar.
    let rendered = engine
        .render("external-meeting", &std::collections::HashMap::new())
        .unwrap();

    // The note is dated by when the meeting *started*, not by today — those
    // differ for a call running across midnight, and the start is the truth.
    let event_day = event_start.format("%Y-%m-%d").to_string();
    assert_eq!(
        rendered.path,
        format!("Inbox/{event_day} - Acme Corp - Acme Q3 sync.md"),
        "path must take the event's date, customer and subject"
    );
    assert!(rendered.content.contains("kind: meeting"));
    assert!(rendered.content.contains("audience: external"));
    assert!(rendered.content.contains(&format!("date: {event_day}")));
    assert!(
        rendered.content.contains("- \"[[Acme Corp]]\""),
        "customer must resolve from the event's domain mapping:\n{}",
        rendered.content
    );
    assert!(
        rendered
            .content
            .contains("event_id: \"AAMkAGI2-prefill-0001\""),
        "the meeting must carry the event id transcript-sync will join on:\n{}",
        rendered.content
    );

    // Two-way link: the meeting points back at the machine-owned event note.
    let link_target = event_path
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".md");
    assert!(
        rendered
            .content
            .contains(&format!("event: \"[[{link_target}]]\"")),
        "expected a wikilink to {link_target} in:\n{}",
        rendered.content
    );

    // The roster lands in the body for enrichment — `attendees` stays an empty
    // wikilink list, because raw addresses are not `[[Person]]` links.
    assert!(rendered.content.contains("attendees: []"));
    assert!(rendered.content.contains("- alice@acme.com"));
    assert!(rendered.content.contains("- harpreet@corp.example.com"));
}

/// Typed values are not overruled: the hook fills blanks only.
#[test]
fn typed_prompts_win_over_the_calendar_event() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        println!("skipping: python3 not on PATH");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);
    let (_, event_start) = write_event_note(root, -5);

    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().join("cache.db");
    indexed_cache(root, &cache_path);

    let engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), Some(cache_path));
    let prompts = std::collections::HashMap::from([
        ("title".to_string(), "Renewal risk".to_string()),
        ("customer".to_string(), "Globex".to_string()),
    ]);
    let rendered = engine.render("external-meeting", &prompts).unwrap();

    let event_day = event_start.format("%Y-%m-%d").to_string();
    assert_eq!(
        rendered.path,
        format!("Inbox/{event_day} - Globex - Renewal risk.md")
    );
    assert!(rendered.content.contains("- \"[[Globex]]\""));
    assert!(!rendered.content.contains("Acme Corp"));
    // The event identity still attaches, so the note is still joinable.
    assert!(
        rendered
            .content
            .contains("event_id: \"AAMkAGI2-prefill-0001\"")
    );
}

/// No meeting in progress: the templates must still render a clean note from
/// the typed title alone, with no calendar residue and no `event_id`.
#[test]
fn meeting_templates_render_cleanly_with_no_calendar_event() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);
    // An event, but hours away — a candidate row the hook must still reject.
    write_event_note(root, 300);

    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().join("cache.db");
    indexed_cache(root, &cache_path);

    let engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), Some(cache_path));
    let prompts =
        std::collections::HashMap::from([("title".to_string(), "Ad hoc chat".to_string())]);

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let internal = engine.render("internal-meeting", &prompts).unwrap();
    assert_eq!(internal.path, format!("Inbox/{today} - Ad hoc chat.md"));
    assert!(internal.content.contains("audience: internal"));
    assert!(!internal.content.contains("event_id"));
    assert!(!internal.content.contains("Acme"));
    assert!(internal.content.contains("# "));

    let external = engine.render("external-meeting", &prompts).unwrap();
    assert_eq!(external.path, format!("Inbox/{today} - Ad hoc chat.md"));
    assert!(external.content.contains("customers: []"));
    assert!(external.content.contains("streams: []"));
    assert!(!external.content.contains("event_id"));
}

/// The hook is defensive by contract: with no cache at all (so no context
/// queries ran) the templates still render rather than erroring.
#[test]
fn meeting_templates_render_without_a_cache() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);

    let engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), None);
    let prompts = std::collections::HashMap::from([
        ("title".to_string(), "Kickoff".to_string()),
        ("customer".to_string(), "Acme Corp".to_string()),
    ]);

    let rendered = engine.render("external-meeting", &prompts).unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(
        rendered.path,
        format!("Inbox/{today} - Acme Corp - Kickoff.md")
    );
    assert!(rendered.content.contains("- \"[[Acme Corp]]\""));
}

/// A meeting note created from a prefilled template must still route by date
/// like any other — the added `event_id`/`event` keys must not break the rule.
#[test]
fn a_prefilled_meeting_note_still_routes() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        println!("skipping: python3 not on PATH");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    apply_into(root);
    let (_, event_start) = write_event_note(root, -5);

    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().join("cache.db");
    indexed_cache(root, &cache_path);

    let engine = notesmith_templates::TemplateEngine::new(root.to_path_buf(), Some(cache_path));
    let rendered = engine
        .render("external-meeting", &std::collections::HashMap::new())
        .unwrap();
    let router = notesmith_routing::RoutingEngine::load(root).expect("routing.yaml should load");
    let decision = router
        .preview(&rendered.path, &rendered.content)
        .expect("a prefilled meeting must still match the file-meeting rule");
    assert_eq!(
        decision.destination,
        format!(
            "Meetings/{}/{}/{}",
            event_start.format("%Y"),
            event_start.format("%m"),
            rendered.path.trim_start_matches("Inbox/")
        ),
        "routing files by the meeting's own `date`, which prefill took from the event"
    );
}
