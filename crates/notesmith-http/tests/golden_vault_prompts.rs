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

// ── Meeting prefill (integrations plan, feature 1) ───────────────────────────

/// The `context_queries` the meeting templates declare, as written in the
/// fixture. Reading them back rather than restating them here is what keeps
/// this test honest when the templates change.
fn meeting_template_queries(template: &str) -> Vec<(String, String)> {
    let contents =
        std::fs::read_to_string(golden_vault().join(format!(".notesmith/templates/{template}.md")))
            .unwrap();
    let (frontmatter, _) = notesmith_vault::extract_frontmatter(&contents);
    let yaml: serde_yaml::Value = serde_yaml::from_str(&frontmatter.expect("frontmatter")).unwrap();
    yaml.get("context_queries")
        .and_then(|value| value.as_mapping())
        .expect("meeting templates must declare context_queries")
        .iter()
        .map(|(key, value)| {
            (
                key.as_str().unwrap().to_string(),
                value.as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// The prefill queries are the only thing standing between the hook and the
/// real schema — the hook itself never touches SQL. Prove they execute, return
/// the columns `meeting-prefill.py` reads, and stay read-only.
#[test]
fn meeting_template_context_queries_execute_against_the_index() {
    let cache = indexed_golden_vault();
    let mut checked = 0;

    for template in ["internal-meeting", "external-meeting"] {
        let queries = meeting_template_queries(template);
        assert_eq!(
            queries.len(),
            2,
            "{template} must declare both prefill queries"
        );

        for (name, sql) in queries {
            let result = execute_sql(&cache, &sql)
                .unwrap_or_else(|error| panic!("{template}/{name} failed: {error}\nSQL: {sql}"));
            checked += 1;

            let expected: &[&str] = match name.as_str() {
                "calendar_events" => &["path", "event_id", "start", "end", "audience", "organizer"],
                "calendar_event_members" => &["path", "key", "ordinal", "value"],
                other => panic!("unexpected prefill query {other}"),
            };
            assert_eq!(
                result.columns, expected,
                "{template}/{name} must return the columns meeting-prefill.py reads"
            );
        }
    }

    assert_eq!(checked, 4, "expected four prefill queries");
}

/// Both meeting templates must ask the *same* questions — the hook is shared,
/// so a query that drifts on one template silently changes the other's answers.
#[test]
fn both_meeting_templates_share_the_same_prefill_queries() {
    assert_eq!(
        meeting_template_queries("internal-meeting"),
        meeting_template_queries("external-meeting"),
    );
    assert_eq!(
        std::fs::read_to_string(golden_vault().join(".notesmith/scripts/meeting-prefill.sh"))
            .unwrap(),
        std::fs::read_to_string(work_notes_kit().join(".notesmith/scripts/meeting-prefill.sh"))
            .unwrap(),
    );
}

/// The event note the prefill queries look for is the one calendar-sync writes.
/// Pin the shape here so a change to either side is caught: the golden-vault
/// fixture event must be visible to the candidate query when its day is today.
#[test]
fn the_prefill_candidate_query_shape_matches_calendar_sync_notes() {
    let cache = indexed_golden_vault();
    // Same query, with the day window pinned around the fixture's date instead
    // of `now` — the fixture event is deliberately in the past.
    let sql = meeting_template_queries("internal-meeting")
        .into_iter()
        .find(|(name, _)| name == "calendar_events")
        .unwrap()
        .1
        .replace("date('now', 'localtime', '-1 day')", "'2026-08-03'")
        .replace("date('now', 'localtime', '+1 day')", "'2026-08-05'");
    assert!(
        !sql.contains("now"),
        "the pinned query must not still depend on the clock: {sql}"
    );

    let result = execute_sql(&cache, &sql).unwrap();
    assert_eq!(
        result.rows.len(),
        1,
        "the golden-vault calendar event must be a prefill candidate"
    );
    let row = &result.rows[0];
    assert_eq!(
        row[0].as_str(),
        Some("Calendar/2026/08/2026-08-04 0930 Acme Corp sync.md")
    );
    assert_eq!(row[1].as_str(), Some("AAMkAGI2-golden-0001"));
    assert_eq!(row[2].as_str(), Some("2026-08-04T09:30:00"));
    assert_eq!(row[3].as_str(), Some("2026-08-04T10:00:00"));
    assert_eq!(row[4].as_str(), Some("external"));
    assert_eq!(row[5].as_str(), Some("alice@acme.com"));
}

/// Runs the prefill hook's embedded `--self-test` (no network, no cache) so its
/// pure logic — the ±10m window, nearest-start selection, typed values winning
/// over the calendar, and the degrade-to-blank path — has real coverage.
/// Skipped gracefully when python3 is not on PATH.
#[test]
fn meeting_prefill_hook_self_test_passes() {
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
        .arg(".notesmith/scripts/meeting-prefill.py")
        .arg("--self-test")
        .current_dir(golden_vault())
        .output()
        .expect("failed to spawn meeting-prefill.py");

    assert!(
        output.status.success(),
        "hook self-test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("OK"),
        "hook self-test did not print OK"
    );
}

/// The shim is what the engine actually executes (`sh <script>`), and it must
/// still answer when python3 is missing — a broken toolchain must not stop a
/// meeting note being created.
#[test]
fn the_prefill_shim_degrades_when_python_is_missing() {
    let output = std::process::Command::new("/bin/sh")
        .arg(".notesmith/scripts/meeting-prefill.sh")
        .current_dir(golden_vault())
        // An empty PATH is the strongest form of "python3 is missing"; the
        // shim uses only shell builtins before it gives up, so it still works.
        .env("PATH", "")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to spawn the prefill shim");

    assert!(output.status.success(), "the shim must exit 0");
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the shim must always emit JSON");
    assert_eq!(parsed.get("event_matched"), Some(&serde_json::json!(false)));
}

/// The Teams transcript bridge (ADR 0025's 2026-09-04 amendment). `join_url` is
/// what lets transcript-sync reach a meeting's transcripts at all, so pin that
/// calendar-sync persists it and that it is queryable from the index.
#[test]
fn calendar_event_notes_persist_the_teams_join_url() {
    let cache = indexed_golden_vault();
    let sql = "SELECT n.path AS path, f.value AS join_url \
               FROM v_notes n \
               JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path \
               WHERE f.key = 'join_url'";
    let result = execute_sql(&cache, sql).unwrap();
    assert_eq!(
        result.rows.len(),
        1,
        "the golden-vault online meeting must carry a join_url"
    );
    assert!(
        result.rows[0][1]
            .as_str()
            .unwrap()
            .starts_with("https://teams.microsoft.com/l/meetup-join/"),
        "{:?}",
        result.rows[0]
    );
}

// ── transcript-sync (ADR 0025 Decision 4) ────────────────────────────────────

/// The connector reads occurrences from the local cache rather than
/// re-querying calendarView, so these queries are its only contract with the
/// index. Prove they execute and return the columns it reads.
#[test]
fn transcript_sync_queries_execute_against_the_index() {
    let cache = indexed_golden_vault();

    let occurrences = "SELECT n.path AS path, \
         MAX(CASE WHEN f.key = 'event_id' THEN f.value END) AS event_id, \
         MAX(CASE WHEN f.key = 'start' THEN f.value END) AS start, \
         MAX(CASE WHEN f.key = 'end' THEN f.value END) AS end, \
         MAX(CASE WHEN f.key = 'join_url' THEN f.value END) AS join_url \
         FROM v_notes n \
         JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path \
         WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event') \
         GROUP BY n.path";
    let result = execute_sql(&cache, occurrences).unwrap();
    assert_eq!(
        result.columns,
        vec!["path", "event_id", "start", "end", "join_url"]
    );
    assert_eq!(
        result.rows.len(),
        1,
        "the golden-vault event must be a transcript-sync candidate"
    );
    assert!(
        result.rows[0][4]
            .as_str()
            .unwrap()
            .starts_with("https://teams."),
        "the candidate must carry the join_url bridge: {:?}",
        result.rows[0]
    );

    // The dedup lookup: a transcript already ingested must not be re-created.
    let dedup = "SELECT note_path AS path FROM v_field_values \
                 WHERE key = 'source_url' AND value = 'teams:none'";
    assert!(execute_sql(&cache, dedup).unwrap().rows.is_empty());

    // The meeting back-link lookup.
    let meeting = "SELECT note_path AS path FROM v_field_values \
                   WHERE key = 'event_id' AND note_path IN \
                   (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'meeting') \
                   AND value = 'AAMkAGI2-golden-0001'";
    execute_sql(&cache, meeting).expect("meeting back-link query must be valid SQL");
}

/// The connector's pure logic — occurrence matching, the ambiguity guard, UTC
/// conversion, note identity — offline. Skipped when python3 is absent.
#[test]
fn transcript_sync_connector_self_test_passes() {
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
        .arg(".notesmith/connectors/transcript-sync.py")
        .arg("--self-test")
        .current_dir(golden_vault())
        .output()
        .expect("failed to spawn transcript-sync.py");

    assert!(
        output.status.success(),
        "connector self-test failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK"));
}

/// transcript-sync must run after calendar-sync: it matches transcripts against
/// the event notes that job writes, and an event with no synced `join_url` has
/// no bridge to its transcript at all.
#[test]
fn golden_vault_orders_transcript_sync_after_calendar_sync() {
    let config = std::fs::read_to_string(golden_vault().join(".notesmith/vault.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&config).unwrap();
    let jobs = parsed["jobs"].as_array().expect("jobs array");

    let transcript = jobs
        .iter()
        .find(|job| job.get("name").and_then(|n| n.as_str()) == Some("transcript-sync"))
        .expect("transcript-sync job missing");
    assert_eq!(
        transcript.get("command").and_then(|c| c.as_str()),
        Some(".notesmith/connectors/transcript-sync.py")
    );
    assert_eq!(
        transcript.get("enabled").and_then(|e| e.as_bool()),
        Some(false),
        "connectors ship disabled — they need the workiq CLI"
    );
    let after: Vec<&str> = transcript["after"]
        .as_array()
        .expect("transcript-sync must declare `after`")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(after, vec!["calendar-sync"]);
}
