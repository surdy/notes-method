//! notesmith-tasks: Status transitions, content-hash anchored toggling, and task insertion.

use chrono::NaiveDate;
use notesmith_core::{TaskPriority, TaskStatus};
use regex::Regex;
use std::sync::OnceLock;

// ── Error types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("cannot transition from {from} to {to}")]
pub struct TransitionError {
    pub from: TaskStatus,
    pub to: TaskStatus,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ToggleError {
    #[error("no task with hash '{hash}' found in note")]
    TaskNotFound { hash: String },
    #[error("hash '{hash}' matches {count} tasks; cannot toggle unambiguously")]
    HashCollision { hash: String, count: usize },
    #[error("{0}")]
    InvalidTransition(TransitionError),
}

impl From<TransitionError> for ToggleError {
    fn from(err: TransitionError) -> Self {
        ToggleError::InvalidTransition(err)
    }
}

// ── Options for adding a task ─────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct AddTaskOptions {
    pub due: Option<NaiveDate>,
    pub customer: Option<String>,
    pub stream: Option<String>,
    pub owner: Option<String>,
    pub priority: Option<TaskPriority>,
}

// ── Status transition table ───────────────────────────────────────────────────

/// Return the allowed next states for a given status.
///
/// See the notes-method plan §10.2 for the full transition table.
pub fn allowed_transitions(from: TaskStatus) -> &'static [TaskStatus] {
    use TaskStatus::*;
    match from {
        Todo => &[InProgress, Blocked, Waiting, OnHold, Done],
        InProgress => &[Done, Blocked, Waiting, OnHold],
        Blocked => &[Todo, InProgress, Done],
        Waiting => &[Todo, InProgress, Done],
        OnHold => &[Todo, InProgress, Done],
        Done => &[Todo],
        Cancelled => &[Todo],
    }
}

/// Check whether a status transition is permitted.
pub fn validate_transition(from: TaskStatus, to: TaskStatus) -> Result<(), TransitionError> {
    if allowed_transitions(from).contains(&to) {
        Ok(())
    } else {
        Err(TransitionError { from, to })
    }
}

// ── Content-hash anchored toggling ────────────────────────────────────────────

/// Find the task identified by `task_hash` in `content`, validate the
/// transition to `new_status`, rewrite that single line in place, and return
/// the updated content.
///
/// The hash must match exactly one task line; if it matches zero or more than
/// one, an error is returned instead.
pub fn toggle_task(
    content: &str,
    task_hash: &str,
    new_status: TaskStatus,
) -> Result<String, ToggleError> {
    let re = task_line_regex();
    let lines = split_preserving_endings(content);

    // Collect indices of lines whose hash matches task_hash AND are task lines.
    let matches: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let stripped = strip_line_ending(line);
            let hash = blake3::hash(stripped.as_bytes()).to_hex().to_string();
            hash == task_hash && re.is_match(stripped)
        })
        .map(|(idx, _)| idx)
        .collect();

    match matches.len() {
        0 => Err(ToggleError::TaskNotFound {
            hash: task_hash.to_string(),
        }),
        n if n > 1 => Err(ToggleError::HashCollision {
            hash: task_hash.to_string(),
            count: n,
        }),
        _ => {
            let idx = matches[0];
            let line = lines[idx];
            let stripped = strip_line_ending(line);
            let ending = &line[stripped.len()..];

            let caps = re.captures(stripped).expect("already matched");
            let marker_char = caps
                .name("marker")
                .and_then(|m| m.as_str().chars().next())
                .expect("marker group always present");

            let current_status =
                TaskStatus::from_marker(marker_char).ok_or_else(|| ToggleError::TaskNotFound {
                    hash: task_hash.to_string(),
                })?;

            validate_transition(current_status, new_status)?;

            let indent = caps.name("indent").map_or("", |m| m.as_str());
            let task_content = caps.name("content").map_or("", |m| m.as_str());
            let new_line = format!("{indent}- [{}] {task_content}{ending}", new_status.marker());

            let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            result[idx] = new_line;
            Ok(result.concat())
        }
    }
}

/// Compute the content hash for a raw task line (matches the parser's computation).
pub fn task_content_hash(raw_line: &str) -> String {
    let stripped = strip_line_ending(raw_line);
    blake3::hash(stripped.as_bytes()).to_hex().to_string()
}

// ── Adding tasks ──────────────────────────────────────────────────────────────

/// Append a new To Do task to the end of `content` and return the updated string.
///
/// Inline fields (`[customer:: ...]`, `[stream:: ...]`, `[owner:: ...]`) are
/// appended before emoji metadata so the indexer can pick them up.
pub fn add_task(content: &str, description: &str, opts: &AddTaskOptions) -> String {
    let mut task = format!("- [ ] {description}");

    if let Some(customer) = &opts.customer {
        task.push_str(&format!(" [customer:: {customer}]"));
    }
    if let Some(stream) = &opts.stream {
        task.push_str(&format!(" [stream:: {stream}]"));
    }
    if let Some(owner) = &opts.owner {
        task.push_str(&format!(" [owner:: {owner}]"));
    }
    if let Some(due) = opts.due {
        task.push_str(&format!(" 📅 {due}"));
    }
    if let Some(priority) = opts.priority {
        let emoji = match priority {
            TaskPriority::Highest => "⏫",
            TaskPriority::High => "🔼",
            TaskPriority::Medium => "🔼",
            TaskPriority::Low => "🔽",
            TaskPriority::Lowest => "🔽",
        };
        task.push(' ');
        task.push_str(emoji);
    }
    task.push('\n');

    let separator = if content.ends_with('\n') || content.is_empty() {
        ""
    } else {
        "\n"
    };
    format!("{content}{separator}{task}")
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn task_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?P<indent>\s*)- \[(?P<marker>.)\] (?P<content>.*)$")
            .expect("valid task line regex")
    })
}

/// Strip `\r\n` or `\n` from the end of a line (matches the vault parser).
fn strip_line_ending(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

/// Split `content` into a vec of slices, each including its trailing `\n`
/// (or `\r\n`). A final unterminated segment is included as-is.
fn split_preserving_endings(content: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (idx, ch) in content.char_indices() {
        if ch == '\n' {
            out.push(&content[start..idx + 1]);
            start = idx + 1;
        }
    }
    if start < content.len() {
        out.push(&content[start..]);
    }
    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use TaskStatus::*;

    // ── Slice 1: Status transitions ───────────────────────────────────────────

    #[test]
    fn todo_can_transition_to_five_states() {
        let allowed = allowed_transitions(Todo);
        assert!(allowed.contains(&InProgress));
        assert!(allowed.contains(&Blocked));
        assert!(allowed.contains(&Waiting));
        assert!(allowed.contains(&OnHold));
        assert!(allowed.contains(&Done));
        assert!(!allowed.contains(&Cancelled));
        assert!(!allowed.contains(&Todo));
    }

    #[test]
    fn in_progress_can_close_or_block() {
        let allowed = allowed_transitions(InProgress);
        assert!(allowed.contains(&Done));
        assert!(allowed.contains(&Blocked));
        assert!(allowed.contains(&Waiting));
        assert!(allowed.contains(&OnHold));
        assert!(!allowed.contains(&Todo)); // can't go back to Todo from InProgress
    }

    #[test]
    fn blocked_waiting_on_hold_can_reopen_or_done() {
        for status in [Blocked, Waiting, OnHold] {
            let allowed = allowed_transitions(status);
            assert!(allowed.contains(&Todo), "{status} should allow Todo");
            assert!(
                allowed.contains(&InProgress),
                "{status} should allow InProgress"
            );
            assert!(allowed.contains(&Done), "{status} should allow Done");
        }
    }

    #[test]
    fn done_and_cancelled_can_only_reopen() {
        for status in [Done, Cancelled] {
            let allowed = allowed_transitions(status);
            assert_eq!(allowed, &[Todo], "{status} should only allow Todo");
        }
    }

    #[test]
    fn validate_transition_ok_for_allowed() {
        assert!(validate_transition(Todo, InProgress).is_ok());
        assert!(validate_transition(InProgress, Done).is_ok());
        assert!(validate_transition(Done, Todo).is_ok());
    }

    #[test]
    fn validate_transition_err_for_disallowed() {
        let err = validate_transition(InProgress, Todo).unwrap_err();
        assert_eq!(err.from, InProgress);
        assert_eq!(err.to, Todo);

        assert!(validate_transition(Done, InProgress).is_err());
        assert!(validate_transition(Todo, Cancelled).is_err());
    }

    // ── Slice 2: Toggle task by hash ──────────────────────────────────────────

    fn line_hash(line: &str) -> String {
        task_content_hash(line)
    }

    #[test]
    fn toggle_rewrites_the_matching_task_line() {
        let line = "- [ ] Fix the bug";
        let content = format!("{line}\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, InProgress).unwrap();
        assert_eq!(result, "- [/] Fix the bug\n");
    }

    #[test]
    fn toggle_preserves_other_lines() {
        let line = "- [ ] Task A";
        let content = format!("Some intro text\n{line}\n- [/] Task B\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, Done).unwrap();
        assert_eq!(result, "Some intro text\n- [x] Task A\n- [/] Task B\n");
    }

    #[test]
    fn toggle_preserves_indented_tasks() {
        let line = "  - [ ] Nested task";
        let content = format!("{line}\n");
        let hash = line_hash(line);

        let result = toggle_task(&content, &hash, Blocked).unwrap();
        assert_eq!(result, "  - [b] Nested task\n");
    }

    #[test]
    fn toggle_returns_not_found_when_hash_missing() {
        let content = "- [ ] Some task\n";
        let err = toggle_task(content, "deadbeef", InProgress).unwrap_err();
        assert!(matches!(err, ToggleError::TaskNotFound { .. }));
    }

    #[test]
    fn toggle_returns_collision_for_duplicate_lines() {
        let line = "- [ ] Duplicate task";
        let content = format!("{line}\n{line}\n");
        let hash = line_hash(line);

        let err = toggle_task(&content, &hash, Done).unwrap_err();
        assert!(matches!(err, ToggleError::HashCollision { count: 2, .. }));
    }

    #[test]
    fn toggle_returns_invalid_transition_for_disallowed_status() {
        let line = "- [/] In progress task";
        let content = format!("{line}\n");
        let hash = line_hash(line);

        let err = toggle_task(&content, &hash, Todo).unwrap_err();
        assert!(matches!(err, ToggleError::InvalidTransition(_)));
    }

    #[test]
    fn toggle_works_without_trailing_newline() {
        let line = "- [ ] No trailing newline";
        let hash = line_hash(line);

        let result = toggle_task(line, &hash, Done).unwrap();
        assert_eq!(result, "- [x] No trailing newline");
    }

    #[test]
    fn toggle_preserves_crlf_endings() {
        let line = "- [ ] Windows line ending";
        let content = format!("{line}\r\n");
        let hash = line_hash(&format!("{line}")); // hash of the stripped line

        let result = toggle_task(&content, &hash, InProgress).unwrap();
        assert_eq!(result, "- [/] Windows line ending\r\n");
    }

    // ── Slice 3: Add task ─────────────────────────────────────────────────────

    #[test]
    fn add_task_appends_simple_todo() {
        let result = add_task("", "Fix the bug", &AddTaskOptions::default());
        assert_eq!(result, "- [ ] Fix the bug\n");
    }

    #[test]
    fn add_task_appends_after_existing_content_with_newline() {
        let content = "# Heading\n\nSome text\n";
        let result = add_task(content, "New task", &AddTaskOptions::default());
        assert_eq!(result, "# Heading\n\nSome text\n- [ ] New task\n");
    }

    #[test]
    fn add_task_inserts_separator_when_no_trailing_newline() {
        let content = "Some text";
        let result = add_task(content, "New task", &AddTaskOptions::default());
        assert_eq!(result, "Some text\n- [ ] New task\n");
    }

    #[test]
    fn add_task_includes_due_date_emoji() {
        let due = NaiveDate::from_ymd_opt(2025, 3, 15).unwrap();
        let opts = AddTaskOptions {
            due: Some(due),
            ..Default::default()
        };
        let result = add_task("", "Task with due", &opts);
        assert_eq!(result, "- [ ] Task with due 📅 2025-03-15\n");
    }

    #[test]
    fn add_task_includes_inline_fields_before_emoji() {
        let opts = AddTaskOptions {
            customer: Some("Acme".to_string()),
            stream: Some("Migration to v2".to_string()),
            ..Default::default()
        };
        let result = add_task("", "Plan migration", &opts);
        assert_eq!(
            result,
            "- [ ] Plan migration [customer:: Acme] [stream:: Migration to v2]\n"
        );
    }

    #[test]
    fn add_task_includes_priority_emoji() {
        let opts = AddTaskOptions {
            priority: Some(TaskPriority::High),
            ..Default::default()
        };
        let result = add_task("", "Urgent task", &opts);
        assert_eq!(result, "- [ ] Urgent task 🔼\n");
    }

    // ── Slice 4: Hash computation is stable ───────────────────────────────────

    #[test]
    fn task_content_hash_strips_line_ending_before_hashing() {
        let with_newline = "- [ ] Task A\n";
        let without = "- [ ] Task A";
        // Both should produce the same hash (endings stripped)
        assert_eq!(task_content_hash(with_newline), task_content_hash(without));
    }
}
