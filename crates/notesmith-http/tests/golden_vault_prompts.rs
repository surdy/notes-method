//! Golden-vault agent-prompt config is tested like code (issue #288): the
//! daily-briefing flow is entirely vault config (prompt + template + jobs
//! entry), so these tests keep its SQL valid against the real index schema
//! and its managed-section markers well-formed. Kit↔fixture byte-identity is
//! enforced separately by notesmith-kit's `kit_matches_golden_vault` tests.

use std::path::PathBuf;

use notesmith_core::VaultEngine;
use notesmith_http::prompt_render::parse_prompt_template;
use notesmith_index::VaultCache;
use notesmith_query::execute_sql;
use notesmith_vault::NativeVaultEngine;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_vault() -> PathBuf {
    repo_root().join("golden-vault")
}

fn work_notes_kit() -> PathBuf {
    repo_root().join("kits/work-notes")
}

fn indexed_golden_vault() -> VaultCache {
    let notes = NativeVaultEngine.scan(&golden_vault()).unwrap();
    let cache = VaultCache::open_in_memory().unwrap();
    cache.reindex("golden-vault", &notes).unwrap();
    cache
}

#[test]
fn daily_note_prompt_context_queries_execute_against_the_index() {
    let template =
        std::fs::read_to_string(golden_vault().join(".notesmith/prompts/daily-note.md")).unwrap();
    let (queries, body) = parse_prompt_template(&template).unwrap();
    assert!(
        !queries.is_empty(),
        "the briefing prompt must declare context queries"
    );

    let cache = indexed_golden_vault();
    for query in &queries {
        execute_sql(&cache, &query.sql).unwrap_or_else(|error| {
            panic!("context query {:?} failed: {error}", query.name);
        });
        assert!(
            body.contains(&format!("{{{{ {} }}}}", query.name)),
            "query {:?} has no {{{{ {} }}}} placeholder in the prompt body",
            query.name,
            query.name
        );
    }
    assert!(
        body.contains("{{ today }}"),
        "prompt must use {{{{ today }}}}"
    );
}

/// The `<!-- notesmith:section:begin/end <id> -->` pairs in a template body
/// must be balanced and properly nested-free: every begin has a matching end
/// for the same id, in order, with no duplicate ids.
fn assert_marker_pairs(content: &str, expected_ids: &[&str], context: &str) {
    let mut open: Option<String> = None;
    let mut seen: Vec<String> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("<!-- notesmith:section:begin ") {
            let id = rest
                .strip_suffix(" -->")
                .unwrap_or_else(|| panic!("{context}: malformed begin marker line {line:?}"));
            assert!(open.is_none(), "{context}: nested begin marker {id:?}");
            assert!(
                !seen.iter().any(|s| s == id),
                "{context}: duplicate section id {id:?}"
            );
            open = Some(id.to_string());
        } else if let Some(rest) = line.strip_prefix("<!-- notesmith:section:end ") {
            let id = rest
                .strip_suffix(" -->")
                .unwrap_or_else(|| panic!("{context}: malformed end marker line {line:?}"));
            match open.take() {
                Some(begin) if begin == id => seen.push(begin),
                Some(begin) => panic!("{context}: end {id:?} does not match begin {begin:?}"),
                None => panic!("{context}: end marker {id:?} without a begin"),
            }
        }
    }
    assert!(open.is_none(), "{context}: unclosed begin marker {open:?}");
    assert_eq!(
        seen, expected_ids,
        "{context}: managed sections differ from the briefing contract"
    );
}

#[test]
fn daily_template_ships_the_briefing_managed_sections() {
    let expected = [
        "briefing/meetings",
        "briefing/email",
        "briefing/tasks",
        "briefing/attention",
    ];
    for root in [golden_vault(), work_notes_kit()] {
        let path = root.join(".notesmith/templates/daily.md");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_marker_pairs(&content, &expected, &path.display().to_string());
    }
}

#[test]
fn golden_vault_declares_the_daily_briefing_job() {
    let config = std::fs::read_to_string(golden_vault().join(".notesmith/vault.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&config).unwrap();
    let jobs = parsed
        .get("jobs")
        .and_then(|jobs| jobs.as_array())
        .expect("vault.toml must declare [[jobs]]");
    let briefing = jobs
        .iter()
        .find(|job| job.get("name").and_then(|n| n.as_str()) == Some("daily-briefing"))
        .expect("daily-briefing job missing");
    assert_eq!(
        briefing
            .get("agent")
            .and_then(|a| a.get("prompt"))
            .and_then(|p| p.as_str()),
        Some("daily-note")
    );
    assert_eq!(
        briefing
            .get("agent")
            .and_then(|a| a.get("allow_writes"))
            .and_then(|w| w.as_bool()),
        Some(true)
    );
    assert_eq!(briefing.get("at").and_then(|a| a.as_str()), Some("07:30"));
    assert_eq!(
        briefing.get("weekdays_only").and_then(|w| w.as_bool()),
        Some(true)
    );

    // The calendar-sync connector job exists as a sibling, and the briefing
    // declares `after = ["calendar-sync"]` (ADR 0025). The runner's `after`
    // validation requires the named job to exist among siblings, so these two
    // must stay wired together.
    let calendar = jobs
        .iter()
        .find(|job| job.get("name").and_then(|n| n.as_str()) == Some("calendar-sync"))
        .expect("calendar-sync job missing");
    assert_eq!(
        calendar.get("command").and_then(|c| c.as_str()),
        Some(".notesmith/connectors/calendar-sync.py")
    );
    assert_eq!(calendar.get("every").and_then(|e| e.as_str()), Some("15m"));
    let after: Vec<&str> = briefing
        .get("after")
        .and_then(|a| a.as_array())
        .expect("daily-briefing must declare `after`")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(after, vec!["calendar-sync"]);
}

/// The calendar-sync connector resolves attendee domains to customer wikilinks
/// with this exact query (mirrored in `.notesmith/connectors/calendar-sync.py`
/// as `DOMAIN_MAP_SQL`). Prove it stays valid against the real index schema and
/// that the golden-vault Acme domain resolves.
#[test]
fn calendar_sync_domain_map_query_resolves_acme() {
    let sql = "SELECT d.value AS domain, n.title AS title \
               FROM v_field_values d \
               JOIN v_notes n ON n.vault_name = d.vault_name AND n.path = d.note_path \
               WHERE d.key = 'domains'";
    let cache = indexed_golden_vault();
    let result = execute_sql(&cache, sql).unwrap();
    assert_eq!(result.columns, vec!["domain", "title"]);

    let acme = result
        .rows
        .iter()
        .find(|row| row.first().and_then(|v| v.as_str()) == Some("acme.com"));
    let acme = acme.expect("acme.com domain mapping missing from golden-vault");
    assert_eq!(
        acme.get(1).and_then(|v| v.as_str()),
        Some("Acme Corp"),
        "acme.com must resolve to the Acme Corp customer note title"
    );
}

/// Runs the connector's embedded `--self-test` (no network) so its pure logic
/// — path derivation, audience, customer mapping, frontmatter rendering — has
/// real coverage. Skipped gracefully when python3 is not on PATH.
#[test]
fn calendar_sync_connector_self_test_passes() {
    let python = "python3";
    if std::process::Command::new(python)
        .arg("--version")
        .output()
        .is_err()
    {
        println!("skipping: python3 not on PATH");
        return;
    }

    let output = std::process::Command::new(python)
        .arg(".notesmith/connectors/calendar-sync.py")
        .arg("--self-test")
        .current_dir(golden_vault())
        .output()
        .expect("failed to spawn calendar-sync.py");

    assert!(
        output.status.success(),
        "connector self-test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("OK"),
        "connector self-test did not print OK"
    );
}

/// Runs the email-summary connector's embedded `--self-test` (no network) so
/// its pure logic — sender/subject rendering, the empty case, and above all the
/// metadata-only boundary (no message body leaks into the output) — has real
/// coverage. Skipped gracefully when python3 is not on PATH.
#[test]
fn email_summary_connector_self_test_passes() {
    let python = "python3";
    if std::process::Command::new(python)
        .arg("--version")
        .output()
        .is_err()
    {
        println!("skipping: python3 not on PATH");
        return;
    }

    let output = std::process::Command::new(python)
        .arg(".notesmith/connectors/email-summary.py")
        .arg("--self-test")
        .current_dir(golden_vault())
        .output()
        .expect("failed to spawn email-summary.py");

    assert!(
        output.status.success(),
        "connector self-test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("OK"),
        "connector self-test did not print OK"
    );
}
