use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Top-level frontmatter discriminated by `type` field.
/// Closed for known note kinds, open for unknown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Frontmatter {
    Daily(DailyMeta),
    Meeting(MeetingMeta),
    Stream(StreamMeta),
    Customer(CustomerMeta),
    AccountInfo(AccountInfoMeta),
    Glossary(GlossaryMeta),
    Milestones(MilestonesMeta),
    Note(NoteMeta),
    Dashboard(DashboardMeta),
    Contact(ContactMeta),
    #[serde(other)]
    Other,
}

/// Common metadata fields shared across note types
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CommonMeta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "archived-at")]
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyMeta {
    pub date: NaiveDate,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingMeta {
    #[serde(rename = "meeting-kind")]
    pub meeting_kind: MeetingKind,
    pub customer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    pub date: NaiveDate,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingKind {
    Internal,
    External,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamMeta {
    pub customer: String,
    pub stream: String,
    pub status: StreamStatus,
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NaiveDate>,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamStatus {
    #[serde(rename = "In Progress")]
    InProgress,
    Blocked,
    Done,
    #[serde(rename = "Awaiting Customer")]
    AwaitingCustomer,
    #[serde(rename = "On Hold")]
    OnHold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomerMeta {
    pub customer: String,
    pub state: CustomerState,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CustomerState {
    Active,
    #[serde(rename = "On Hold")]
    OnHold,
    Temp,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountInfoMeta {
    pub customer: String,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossaryMeta {
    pub customer: String,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MilestonesMeta {
    pub customer: String,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMeta {
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardMeta {
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactMeta {
    pub customer: String,
    #[serde(flatten)]
    pub common: CommonMeta,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_daily_frontmatter() {
        let yaml = r#"
type: daily
date: 2025-01-15
tags:
  - daily
  - wednesday
created: \"2025-01-15 08:00\"
updated: \"2025-01-15 17:30\"
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        match fm {
            Frontmatter::Daily(daily) => {
                assert_eq!(daily.date, NaiveDate::from_ymd_opt(2025, 1, 15).unwrap());
                assert_eq!(daily.common.tags, vec!["daily", "wednesday"]);
            }
            other => panic!("expected Daily, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_meeting_frontmatter() {
        let yaml = r#"
type: meeting
meeting-kind: internal
customer: "[[Acme Corp]]"
date: 2025-01-15
tags:
  - meeting
created: \"2025-01-15 10:00\"
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        match fm {
            Frontmatter::Meeting(m) => {
                assert_eq!(m.meeting_kind, MeetingKind::Internal);
                assert_eq!(m.customer, "[[Acme Corp]]");
            }
            other => panic!("expected Meeting, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_stream_frontmatter() {
        let yaml = r#"
type: stream
customer: "[[Acme Corp]]"
stream: "[[Migration to v2]]"
status: In Progress
priority: P1
owner: me
started: 2024-11-01
target: 2025-03-31
tags:
  - migration
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        match fm {
            Frontmatter::Stream(s) => {
                assert_eq!(s.status, StreamStatus::InProgress);
                assert_eq!(s.priority, Priority::P1);
                assert_eq!(s.owner, Some("me".to_string()));
            }
            other => panic!("expected Stream, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_customer_frontmatter() {
        let yaml = r#"
type: customer
customer: "[[Acme Corp]]"
state: Active
tags:
  - customer
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        match fm {
            Frontmatter::Customer(c) => {
                assert_eq!(c.state, CustomerState::Active);
            }
            other => panic!("expected Customer, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_dashboard_frontmatter() {
        let yaml = r#"
type: dashboard
tags:
  - dashboard
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(fm, Frontmatter::Dashboard(_)));
    }

    #[test]
    fn unknown_type_deserializes_as_other() {
        let yaml = r#"
type: custom-unknown
foo: bar
"#;
        let fm: Frontmatter = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(fm, Frontmatter::Other));
    }
}
