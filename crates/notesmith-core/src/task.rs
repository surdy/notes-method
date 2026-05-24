use crate::link::SourcePosition;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parsed task from a note.
/// Status is stored as raw character + resolved group for maximum flexibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// The raw checkbox character (e.g., ' ', 'x', '/', 'b', 'w', 'h', '-')
    pub status_char: char,
    /// Resolved status group: "open" or "done"
    pub status_group: StatusGroup,
    /// The task text content (without the checkbox)
    pub content: String,
    /// Source position in the note
    pub position: SourcePosition,
    /// Inline fields on the task line (e.g., [due:: 2026-06-01])
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub inline_fields: HashMap<String, String>,
    /// Content hash for anchoring (blake3 of the task line)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// The two status groups. Users can add any character and map it to a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusGroup {
    Open,
    Done,
}

impl StatusGroup {
    pub fn is_open(&self) -> bool {
        matches!(self, StatusGroup::Open)
    }
}

/// Configuration for a single task status character
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusConfig {
    pub label: String,
    pub group: StatusGroup,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// The full task status configuration mapping characters to their meanings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskStatusMap {
    #[serde(flatten)]
    pub statuses: HashMap<char, TaskStatusConfig>,
}

impl Default for TaskStatusMap {
    fn default() -> Self {
        let mut statuses = HashMap::new();
        statuses.insert(
            ' ',
            TaskStatusConfig {
                label: "Todo".into(),
                group: StatusGroup::Open,
                icon: Some("circle".into()),
            },
        );
        statuses.insert(
            'x',
            TaskStatusConfig {
                label: "Done".into(),
                group: StatusGroup::Done,
                icon: Some("check".into()),
            },
        );
        statuses.insert(
            '/',
            TaskStatusConfig {
                label: "In Progress".into(),
                group: StatusGroup::Open,
                icon: Some("half-circle".into()),
            },
        );
        statuses.insert(
            'b',
            TaskStatusConfig {
                label: "Blocked".into(),
                group: StatusGroup::Open,
                icon: Some("stop".into()),
            },
        );
        statuses.insert(
            'w',
            TaskStatusConfig {
                label: "Waiting".into(),
                group: StatusGroup::Open,
                icon: Some("clock".into()),
            },
        );
        statuses.insert(
            'h',
            TaskStatusConfig {
                label: "On Hold".into(),
                group: StatusGroup::Open,
                icon: Some("pause".into()),
            },
        );
        statuses.insert(
            '-',
            TaskStatusConfig {
                label: "Cancelled".into(),
                group: StatusGroup::Done,
                icon: Some("dash".into()),
            },
        );
        Self { statuses }
    }
}

impl TaskStatusMap {
    /// Resolve the status group for a given character.
    /// Unknown characters default to "open".
    pub fn resolve_group(&self, c: char) -> StatusGroup {
        self.statuses
            .get(&c)
            .map(|cfg| cfg.group)
            .unwrap_or(StatusGroup::Open)
    }

    /// Get the label for a status character
    pub fn label(&self, c: char) -> Option<&str> {
        self.statuses.get(&c).map(|cfg| cfg.label.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_status_map_covers_standard_markers() {
        let statuses = TaskStatusMap::default();

        assert_eq!(statuses.label(' '), Some("Todo"));
        assert_eq!(statuses.label('/'), Some("In Progress"));
        assert_eq!(statuses.label('x'), Some("Done"));
        assert_eq!(statuses.label('-'), Some("Cancelled"));
        assert_eq!(statuses.resolve_group('x'), StatusGroup::Done);
        assert_eq!(statuses.resolve_group(' '), StatusGroup::Open);
    }

    #[test]
    fn unknown_status_char_defaults_to_open_group() {
        let statuses = TaskStatusMap::default();

        assert_eq!(statuses.resolve_group('!'), StatusGroup::Open);
        assert_eq!(statuses.label('!'), None);
    }

    #[test]
    fn custom_status_map_deserializes_with_flattened_keys() {
        let yaml = r#"
"!":
  label: Needs Review
  group: open
  icon: alert
"c":
  label: Closed
  group: done
"#;
        let statuses: TaskStatusMap = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(statuses.label('!'), Some("Needs Review"));
        assert_eq!(statuses.resolve_group('c'), StatusGroup::Done);
    }

    #[test]
    fn status_group_reports_open_state() {
        assert!(StatusGroup::Open.is_open());
        assert!(!StatusGroup::Done.is_open());
    }
}
