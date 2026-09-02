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
}
