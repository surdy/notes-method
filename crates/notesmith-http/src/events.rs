use std::{
    collections::VecDeque,
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use chrono::Local;
use serde::Serialize;
use tokio::sync::broadcast;

pub const EVENT_CHANNEL_CAPACITY: usize = 256;
pub const EVENT_BUFFER_CAPACITY: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDetail {
    pub key: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct EventBuffer {
    events: RwLock<VecDeque<VaultEvent>>,
    next_id: AtomicU64,
    capacity: usize,
}

impl EventBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: RwLock::new(VecDeque::with_capacity(capacity)),
            next_id: AtomicU64::new(1),
            capacity,
        }
    }

    pub fn push(&self, mut event: VaultEvent) -> VaultEvent {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        event.id = Some(id);

        let mut events = self
            .events
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if events.len() >= self.capacity {
            events.pop_front();
        }
        events.push_back(event.clone());
        event
    }

    pub fn events_since(&self, last_id: u64, vault: &str) -> Vec<VaultEvent> {
        let events = self
            .events
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        events
            .iter()
            .filter(|event| {
                event.id.is_some_and(|id| id > last_id)
                    && (event.vault == vault || event.vault == "_system")
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub vault: String,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub path: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigDetail>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum EventType {
    #[serde(rename = "note.created")]
    NoteCreated,
    #[serde(rename = "note.updated")]
    NoteUpdated,
    #[serde(rename = "note.moved")]
    NoteMoved,
    #[serde(rename = "note.deleted")]
    NoteDeleted,
    #[serde(rename = "task.updated")]
    TaskUpdated,
    #[serde(rename = "note.captured")]
    NoteCaptured,
    #[serde(rename = "daily.created")]
    DailyCreated,
    #[serde(rename = "cache.rebuilt")]
    CacheRebuilt,
    #[serde(rename = "search.reindexed")]
    SearchReindexed,
    #[serde(rename = "config.changed")]
    ConfigChanged,
    #[serde(rename = "config.removed")]
    ConfigRemoved,
    #[serde(rename = "config.error")]
    ConfigError,
    #[serde(rename = "vaults.changed")]
    VaultsChanged,
    #[serde(rename = "shutting_down")]
    ShuttingDown,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::NoteCreated => "note.created",
            EventType::NoteUpdated => "note.updated",
            EventType::NoteMoved => "note.moved",
            EventType::NoteDeleted => "note.deleted",
            EventType::TaskUpdated => "task.updated",
            EventType::NoteCaptured => "note.captured",
            EventType::DailyCreated => "daily.created",
            EventType::CacheRebuilt => "cache.rebuilt",
            EventType::SearchReindexed => "search.reindexed",
            EventType::ConfigChanged => "config.changed",
            EventType::ConfigRemoved => "config.removed",
            EventType::ConfigError => "config.error",
            EventType::VaultsChanged => "vaults.changed",
            EventType::ShuttingDown => "shutting_down",
        }
    }
}

impl VaultEvent {
    pub fn new(vault: impl Into<String>, event_type: EventType, path: impl Into<String>) -> Self {
        Self {
            id: None,
            vault: vault.into(),
            event_type,
            path: path.into(),
            timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string(),
            config: None,
        }
    }

    pub fn config_event(
        vault: impl Into<String>,
        event_type: EventType,
        path: impl Into<String>,
        detail: ConfigDetail,
    ) -> Self {
        Self {
            id: None,
            vault: vault.into(),
            event_type,
            path: path.into(),
            timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string(),
            config: Some(detail),
        }
    }
}

pub type EventSender = broadcast::Sender<VaultEvent>;
pub type EventReceiver = broadcast::Receiver<VaultEvent>;

pub fn create_event_channel() -> (EventSender, EventReceiver) {
    broadcast::channel(EVENT_CHANNEL_CAPACITY)
}

/// Emit an event, ignoring send errors (no subscribers = ok).
pub fn emit(sender: &EventSender, buffer: &EventBuffer, event: VaultEvent) {
    let event = buffer.push(event);
    let _ = sender.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_as_str_matches_serde() {
        let event = VaultEvent::new("v", EventType::NoteCreated, "p");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"note.created\""));
        assert_eq!(EventType::NoteCreated.as_str(), "note.created");
        assert!(!json.contains("\"id\""));
    }

    #[test]
    fn event_serializes_all_fields() {
        let event = VaultEvent::new("my-vault", EventType::NoteDeleted, "Inbox/foo.md");
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert!(json.get("id").is_none());
        assert_eq!(json["vault"], "my-vault");
        assert_eq!(json["type"], "note.deleted");
        assert_eq!(json["path"], "Inbox/foo.md");
        assert!(json["timestamp"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn note_event_omits_config_field() {
        let event = VaultEvent::new("v", EventType::NoteUpdated, "x.md");
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert!(json.get("config").is_none());
    }

    #[test]
    fn broadcast_channel_delivers_events() {
        let (tx, mut rx) = create_event_channel();
        let buffer = EventBuffer::new(EVENT_BUFFER_CAPACITY);
        let event = VaultEvent::new("v", EventType::NoteCaptured, "Inbox/test.md");
        emit(&tx, &buffer, event);
        let received = rx.try_recv().unwrap();
        assert_eq!(received.id, Some(1));
        assert_eq!(received.event_type, EventType::NoteCaptured);
        assert_eq!(received.path, "Inbox/test.md");
    }

    #[test]
    fn emit_without_subscribers_does_not_panic() {
        let (tx, _) = create_event_channel();
        let buffer = EventBuffer::new(EVENT_BUFFER_CAPACITY);
        // Drop the receiver — emit should still succeed silently
        emit(
            &tx,
            &buffer,
            VaultEvent::new("v", EventType::NoteUpdated, "x.md"),
        );
    }

    #[test]
    fn event_buffer_assigns_sequential_ids() {
        let buffer = EventBuffer::new(EVENT_BUFFER_CAPACITY);

        let first = buffer.push(VaultEvent::new(
            "work",
            EventType::NoteCreated,
            "Inbox/one.md",
        ));
        let second = buffer.push(VaultEvent::new(
            "work",
            EventType::NoteUpdated,
            "Inbox/two.md",
        ));

        assert_eq!(first.id, Some(1));
        assert_eq!(second.id, Some(2));
    }

    #[test]
    fn event_buffer_returns_only_newer_matching_events() {
        let buffer = EventBuffer::new(EVENT_BUFFER_CAPACITY);

        buffer.push(VaultEvent::new(
            "work",
            EventType::NoteCreated,
            "Inbox/one.md",
        ));
        let wanted = buffer.push(VaultEvent::new(
            "work",
            EventType::NoteUpdated,
            "Inbox/two.md",
        ));
        let system = buffer.push(VaultEvent::new("_system", EventType::VaultsChanged, ""));
        buffer.push(VaultEvent::new(
            "home",
            EventType::NoteDeleted,
            "Inbox/three.md",
        ));

        let events = buffer.events_since(1, "work");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, wanted.id);
        assert_eq!(events[1].id, system.id);
    }

    #[test]
    fn event_buffer_evicts_oldest_events_when_full() {
        let buffer = EventBuffer::new(2);

        buffer.push(VaultEvent::new("work", EventType::NoteCreated, "one.md"));
        buffer.push(VaultEvent::new("work", EventType::NoteUpdated, "two.md"));
        let newest = buffer.push(VaultEvent::new("work", EventType::NoteDeleted, "three.md"));

        let events = buffer.events_since(0, "work");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, Some(2));
        assert_eq!(events[1].id, newest.id);
    }

    #[test]
    fn config_changed_serde_round_trip() {
        let detail = ConfigDetail {
            key: "sidebar".to_string(),
            status: "changed".to_string(),
            error: None,
        };
        let event = VaultEvent::config_event(
            "v",
            EventType::ConfigChanged,
            ".notesmith/sidebar.yaml",
            detail,
        );
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "config.changed");
        assert_eq!(json["config"]["key"], "sidebar");
        assert_eq!(json["config"]["status"], "changed");
        assert!(json["config"].get("error").is_none());
        assert_eq!(EventType::ConfigChanged.as_str(), "config.changed");
    }

    #[test]
    fn config_removed_serde_round_trip() {
        let detail = ConfigDetail {
            key: "vault".to_string(),
            status: "removed".to_string(),
            error: None,
        };
        let event = VaultEvent::config_event(
            "v",
            EventType::ConfigRemoved,
            ".notesmith/vault.toml",
            detail,
        );
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "config.removed");
        assert_eq!(json["config"]["key"], "vault");
        assert_eq!(json["config"]["status"], "removed");
        assert!(json["config"].get("error").is_none());
        assert_eq!(EventType::ConfigRemoved.as_str(), "config.removed");
    }

    #[test]
    fn config_error_includes_error_message() {
        let detail = ConfigDetail {
            key: "sidebar".to_string(),
            status: "error".to_string(),
            error: Some("invalid YAML at line 3".to_string()),
        };
        let event = VaultEvent::config_event(
            "v",
            EventType::ConfigError,
            ".notesmith/sidebar.yaml",
            detail,
        );
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "config.error");
        assert_eq!(json["config"]["key"], "sidebar");
        assert_eq!(json["config"]["status"], "error");
        assert_eq!(json["config"]["error"], "invalid YAML at line 3");
        assert_eq!(EventType::ConfigError.as_str(), "config.error");
    }

    #[test]
    fn config_detail_without_error_omits_field() {
        let detail = ConfigDetail {
            key: "sidebar".to_string(),
            status: "changed".to_string(),
            error: None,
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(!json.contains("error"));
    }

    #[test]
    fn config_detail_with_error_includes_field() {
        let detail = ConfigDetail {
            key: "sidebar".to_string(),
            status: "error".to_string(),
            error: Some("parse failed".to_string()),
        };
        let json: serde_json::Value = serde_json::to_value(&detail).unwrap();
        assert_eq!(json["error"], "parse failed");
    }

    #[test]
    fn shutting_down_event_type_as_str_matches_serde() {
        let event = VaultEvent::new("v", EventType::ShuttingDown, "");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"shutting_down\""));
        assert_eq!(EventType::ShuttingDown.as_str(), "shutting_down");
    }

    #[test]
    fn vaults_changed_serializes_with_named_event() {
        let event = VaultEvent::new("work", EventType::VaultsChanged, "");
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "vaults.changed");
        assert_eq!(EventType::VaultsChanged.as_str(), "vaults.changed");
    }
}
