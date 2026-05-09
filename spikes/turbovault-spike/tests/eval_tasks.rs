//! Test TurboVault task parsing with all 7 custom statuses and emoji metadata.

use std::{fs, path::PathBuf};

use turbovault_core::{TaskItem, TaskPriority, task_parser::parse_task_line};
use turbovault_parser::ParsedContent;

#[allow(dead_code)]
fn golden_vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../golden-vault")
}

fn fixture_content(relative_path: &str) -> String {
    fs::read_to_string(golden_vault().join(relative_path)).expect("fixture should be readable")
}

fn parse_fixture(relative_path: &str) -> ParsedContent {
    ParsedContent::parse(&fixture_content(relative_path))
}

fn find_task<'a>(tasks: &'a [TaskItem], content: &str) -> Option<&'a TaskItem> {
    tasks.iter().find(|task| task.content == content)
}

#[test]
fn parses_todo_task() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");
    let task =
        find_task(&parsed.tasks, "Review PR for API changes").expect("todo task should parse");

    assert!(!task.is_completed);
}

#[test]
fn parses_done_task() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");
    let task = find_task(&parsed.tasks, "Send weekly report").expect("done task should parse");

    assert!(task.is_completed);
}

#[test]
fn parses_in_progress_task() {
    let content = fixture_content("Inbox/Daily/2025-01-15.md");
    let parsed = ParsedContent::parse(&content);

    assert_eq!(
        content
            .lines()
            .filter(|line| line.trim_start().starts_with("- ["))
            .count(),
        7
    );
    assert_eq!(parsed.tasks.len(), 2);
    // EXPECTED: Not detected by TurboVault's full-document parser.
    assert!(find_task(&parsed.tasks, "Draft Q1 planning doc").is_none());

    let standalone = parse_task_line("- [/] Draft Q1 planning doc ⏳ 2025-01-20")
        .expect("standalone task parser should accept [/] status");
    assert_eq!(standalone.status, '/');
    assert_eq!(standalone.description, "Draft Q1 planning doc");
    assert_eq!(standalone.scheduled.as_deref(), Some("2025-01-20"));
}

#[test]
fn parses_cancelled_task() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");

    // EXPECTED: Not detected by TurboVault's full-document parser.
    assert!(find_task(&parsed.tasks, "Cancelled standup (holiday)").is_none());

    let standalone = parse_task_line("- [-] Cancelled standup (holiday)")
        .expect("standalone task parser should accept [-] status");
    assert_eq!(standalone.status, '-');
    assert_eq!(standalone.description, "Cancelled standup (holiday)");
}

#[test]
fn parses_blocked_task() {
    let parsed = parse_fixture("Customers/Acme/Streams/Migration to v2.md");

    // EXPECTED: Not detected by TurboVault.
    assert!(find_task(&parsed.tasks, "Blocked on auth service upgrade").is_none());
    assert!(parse_task_line("- [b] Blocked on auth service upgrade 📅 2025-01-25").is_err());
}

#[test]
fn parses_waiting_task() {
    let parsed = parse_fixture("Customers/Acme/Streams/Migration to v2.md");

    // EXPECTED: Not detected by TurboVault.
    assert!(
        find_task(
            &parsed.tasks,
            "Awaiting customer sign-off on breaking changes"
        )
        .is_none()
    );
    assert!(
        parse_task_line("- [w] Awaiting customer sign-off on breaking changes 🛫 2025-02-15")
            .is_err()
    );
}

#[test]
fn parses_on_hold_task() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");

    // EXPECTED: Not detected by TurboVault.
    assert!(find_task(&parsed.tasks, "On hold pending legal review").is_none());
    assert!(parse_task_line("- [h] On hold pending legal review ⏫").is_err());
}

#[test]
fn parses_task_due_date_emoji() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");
    let task =
        find_task(&parsed.tasks, "Review PR for API changes").expect("todo task should parse");

    assert_eq!(
        task.due_date.map(|date| date.to_string()),
        Some("2025-01-16".to_string())
    );
}

#[test]
fn parses_task_priority_emoji() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");
    let task =
        find_task(&parsed.tasks, "Review PR for API changes").expect("todo task should parse");

    assert_eq!(task.priority, TaskPriority::Medium);
}

#[test]
fn parses_task_scheduled_emoji() {
    let parsed = parse_fixture("Customers/Acme/Streams/Migration to v2.md");

    // EXPECTED: Metadata is lost because the [/] task is not detected by TurboVault.
    assert!(find_task(&parsed.tasks, "Testing in staging").is_none());

    let standalone = parse_task_line("- [/] Testing in staging ⏳ 2025-01-20")
        .expect("standalone task parser should recover scheduled metadata");
    assert_eq!(standalone.scheduled.as_deref(), Some("2025-01-20"));
}

#[test]
fn parses_task_start_emoji() {
    let parsed = parse_fixture("Customers/Acme/Streams/Migration to v2.md");

    // EXPECTED: Metadata is lost because the [w] task is not detected by TurboVault.
    assert!(
        find_task(
            &parsed.tasks,
            "Awaiting customer sign-off on breaking changes"
        )
        .is_none()
    );
    assert!(
        parse_task_line("- [w] Awaiting customer sign-off on breaking changes 🛫 2025-02-15")
            .is_err()
    );
}

#[test]
fn parses_task_done_emoji() {
    let parsed = parse_fixture("Inbox/Daily/2025-01-15.md");
    let task = find_task(&parsed.tasks, "Send weekly report").expect("done task should parse");

    assert_eq!(
        task.done_date.map(|date| date.to_string()),
        Some("2025-01-15".to_string())
    );
}
