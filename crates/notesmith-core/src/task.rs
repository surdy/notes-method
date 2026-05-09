use crate::link::SourcePosition;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// The 7 task statuses from the notes method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    /// `- [ ]` — To Do
    Todo,
    /// `- [/]` — In Progress
    InProgress,
    /// `- [b]` — Blocked
    Blocked,
    /// `- [w]` — Awaiting Customer / Waiting
    Waiting,
    /// `- [h]` — On Hold
    OnHold,
    /// `- [x]` — Done
    Done,
    /// `- [-]` — Cancelled
    Cancelled,
}

impl TaskStatus {
    /// The checkbox character for this status
    pub fn marker(&self) -> char {
        match self {
            TaskStatus::Todo => ' ',
            TaskStatus::InProgress => '/',
            TaskStatus::Blocked => 'b',
            TaskStatus::Waiting => 'w',
            TaskStatus::OnHold => 'h',
            TaskStatus::Done => 'x',
            TaskStatus::Cancelled => '-',
        }
    }

    /// Parse a checkbox character to a status
    pub fn from_marker(c: char) -> Option<Self> {
        match c {
            ' ' => Some(TaskStatus::Todo),
            '/' => Some(TaskStatus::InProgress),
            'b' => Some(TaskStatus::Blocked),
            'w' => Some(TaskStatus::Waiting),
            'h' => Some(TaskStatus::OnHold),
            'x' | 'X' => Some(TaskStatus::Done),
            '-' => Some(TaskStatus::Cancelled),
            _ => None,
        }
    }

    /// Whether this status is considered "open" (not done or cancelled)
    pub fn is_open(&self) -> bool {
        !matches!(self, TaskStatus::Done | TaskStatus::Cancelled)
    }

    /// Whether this status is considered "actionable" (todo or in progress)
    pub fn is_actionable(&self) -> bool {
        matches!(self, TaskStatus::Todo | TaskStatus::InProgress)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Todo => write!(f, "To Do"),
            TaskStatus::InProgress => write!(f, "In Progress"),
            TaskStatus::Blocked => write!(f, "Blocked"),
            TaskStatus::Waiting => write!(f, "Waiting"),
            TaskStatus::OnHold => write!(f, "On Hold"),
            TaskStatus::Done => write!(f, "Done"),
            TaskStatus::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Priority level for task emoji metadata
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskPriority {
    Lowest,
    Low,
    Medium,
    High,
    Highest,
}

/// A parsed task from a note
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub status: TaskStatus,
    pub content: String,
    pub position: SourcePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<TaskPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<String>,
    /// Content hash for anchoring (blake3 of the task line)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seven_statuses_have_markers() {
        let statuses = [
            (TaskStatus::Todo, ' '),
            (TaskStatus::InProgress, '/'),
            (TaskStatus::Blocked, 'b'),
            (TaskStatus::Waiting, 'w'),
            (TaskStatus::OnHold, 'h'),
            (TaskStatus::Done, 'x'),
            (TaskStatus::Cancelled, '-'),
        ];
        for (status, expected_marker) in statuses {
            assert_eq!(status.marker(), expected_marker, "marker for {status}");
        }
    }

    #[test]
    fn from_marker_roundtrips() {
        for c in [' ', '/', 'b', 'w', 'h', 'x', '-'] {
            let status = TaskStatus::from_marker(c).unwrap_or_else(|| panic!("should parse '{c}'"));
            assert_eq!(status.marker(), c);
        }
    }

    #[test]
    fn from_marker_uppercase_x() {
        assert_eq!(TaskStatus::from_marker('X'), Some(TaskStatus::Done));
    }

    #[test]
    fn from_marker_unknown_returns_none() {
        assert_eq!(TaskStatus::from_marker('?'), None);
        assert_eq!(TaskStatus::from_marker('z'), None);
    }

    #[test]
    fn is_open_for_active_statuses() {
        assert!(TaskStatus::Todo.is_open());
        assert!(TaskStatus::InProgress.is_open());
        assert!(TaskStatus::Blocked.is_open());
        assert!(TaskStatus::Waiting.is_open());
        assert!(TaskStatus::OnHold.is_open());
        assert!(!TaskStatus::Done.is_open());
        assert!(!TaskStatus::Cancelled.is_open());
    }

    #[test]
    fn is_actionable_only_for_todo_and_in_progress() {
        assert!(TaskStatus::Todo.is_actionable());
        assert!(TaskStatus::InProgress.is_actionable());
        assert!(!TaskStatus::Blocked.is_actionable());
        assert!(!TaskStatus::Waiting.is_actionable());
        assert!(!TaskStatus::OnHold.is_actionable());
        assert!(!TaskStatus::Done.is_actionable());
        assert!(!TaskStatus::Cancelled.is_actionable());
    }

    #[test]
    fn display_names() {
        assert_eq!(TaskStatus::Todo.to_string(), "To Do");
        assert_eq!(TaskStatus::InProgress.to_string(), "In Progress");
        assert_eq!(TaskStatus::Blocked.to_string(), "Blocked");
        assert_eq!(TaskStatus::Waiting.to_string(), "Waiting");
        assert_eq!(TaskStatus::OnHold.to_string(), "On Hold");
        assert_eq!(TaskStatus::Done.to_string(), "Done");
        assert_eq!(TaskStatus::Cancelled.to_string(), "Cancelled");
    }
}
