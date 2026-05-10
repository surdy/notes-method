use chrono::Local;
use serde::Serialize;
use tokio::sync::broadcast;

pub const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct VaultEvent {
    pub vault: String,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub path: String,
    pub timestamp: String,
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
    #[serde(rename = "inbox.added")]
    InboxAdded,
    #[serde(rename = "daily.created")]
    DailyCreated,
    #[serde(rename = "cache.rebuilt")]
    CacheRebuilt,
    #[serde(rename = "search.reindexed")]
    SearchReindexed,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::NoteCreated => "note.created",
            EventType::NoteUpdated => "note.updated",
            EventType::NoteMoved => "note.moved",
            EventType::NoteDeleted => "note.deleted",
            EventType::TaskUpdated => "task.updated",
            EventType::InboxAdded => "inbox.added",
            EventType::DailyCreated => "daily.created",
            EventType::CacheRebuilt => "cache.rebuilt",
            EventType::SearchReindexed => "search.reindexed",
        }
    }
}

impl VaultEvent {
    pub fn new(vault: impl Into<String>, event_type: EventType, path: impl Into<String>) -> Self {
        Self {
            vault: vault.into(),
            event_type,
            path: path.into(),
            timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string(),
        }
    }
}

pub type EventSender = broadcast::Sender<VaultEvent>;
pub type EventReceiver = broadcast::Receiver<VaultEvent>;

pub fn create_event_channel() -> (EventSender, EventReceiver) {
    broadcast::channel(EVENT_CHANNEL_CAPACITY)
}

/// Emit an event, ignoring send errors (no subscribers = ok).
pub fn emit(sender: &EventSender, event: VaultEvent) {
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
    }

    #[test]
    fn event_serializes_all_fields() {
        let event = VaultEvent::new("my-vault", EventType::NoteDeleted, "Inbox/foo.md");
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["vault"], "my-vault");
        assert_eq!(json["type"], "note.deleted");
        assert_eq!(json["path"], "Inbox/foo.md");
        assert!(json["timestamp"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn broadcast_channel_delivers_events() {
        let (tx, mut rx) = create_event_channel();
        let event = VaultEvent::new("v", EventType::InboxAdded, "Inbox/test.md");
        emit(&tx, event);
        let received = rx.try_recv().unwrap();
        assert_eq!(received.event_type, EventType::InboxAdded);
        assert_eq!(received.path, "Inbox/test.md");
    }

    #[test]
    fn emit_without_subscribers_does_not_panic() {
        let (tx, _) = create_event_channel();
        // Drop the receiver — emit should still succeed silently
        emit(&tx, VaultEvent::new("v", EventType::NoteUpdated, "x.md"));
    }
}
