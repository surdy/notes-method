use std::convert::Infallible;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::{events::VaultEvent, server::SharedAppState};

#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    pub last_event_id: Option<u64>,
}

pub async fn vault_events(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, (StatusCode, Json<Value>)> {
    let last_event_id = resolve_last_event_id(&headers, &query);
    let (replay_events, rx, shutdown_rx, sse_connection_count) = {
        let state = state.read().await;

        if !state.vaults.contains_key(&vault_name) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("vault not found: {vault_name}") })),
            ));
        }

        (
            replay_events(&state, &vault_name, last_event_id),
            state.event_tx.subscribe(),
            state.shutdown_rx.clone(),
            state.sse_connection_count.clone(),
        )
    };

    sse_connection_count.fetch_add(1, Ordering::Relaxed);
    let connection_guard = SseConnectionGuard::new(sse_connection_count);
    let live_stream = stream::unfold(
        (rx, shutdown_rx, vault_name, connection_guard),
        |(mut rx, mut shutdown_rx, vault_name, connection_guard): (
            broadcast::Receiver<crate::events::VaultEvent>,
            tokio::sync::watch::Receiver<bool>,
            String,
            SseConnectionGuard,
        )| async move {
            loop {
                tokio::select! {
                    result = rx.recv() => match result {
                        Ok(event) if matches_vault_stream(&vault_name, &event) => {
                            return Some((Ok(sse_event_for(event)), (rx, shutdown_rx, vault_name, connection_guard)));
                        }
                        Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                    result = shutdown_rx.changed() => {
                        if result.is_err() || *shutdown_rx.borrow() {
                            return None;
                        }
                    }
                }
            }
        },
    );
    let stream = stream::iter(
        replay_events
            .into_iter()
            .map(|event| Ok(sse_event_for(event))),
    )
    .chain(live_stream);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn resolve_last_event_id(headers: &HeaderMap, query: &EventsQuery) -> Option<u64> {
    query.last_event_id.or_else(|| {
        headers
            .get("Last-Event-ID")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
    })
}

fn replay_events(
    state: &crate::server::AppState,
    vault_name: &str,
    last_event_id: Option<u64>,
) -> Vec<VaultEvent> {
    last_event_id
        .map(|last_id| state.event_buffer.events_since(last_id, vault_name))
        .unwrap_or_default()
}

fn matches_vault_stream(vault_name: &str, event: &VaultEvent) -> bool {
    event.vault == vault_name || event.vault == "_system"
}

fn sse_event_for(event: VaultEvent) -> SseEvent {
    let data = serde_json::to_string(&event).unwrap_or_default();
    let mut sse = SseEvent::default()
        .event(event.event_type.as_str())
        .data(data);
    if let Some(id) = event.id {
        sse = sse.id(id.to_string());
    }
    sse
}

struct SseConnectionGuard {
    counter: Arc<AtomicUsize>,
}

impl SseConnectionGuard {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, atomic::AtomicUsize},
    };

    use axum::{body::to_bytes, response::IntoResponse};
    use chrono::{TimeZone, Utc};
    use futures::stream;
    use notesmith_config::VaultConfig;
    use tokio::sync::RwLock;

    use crate::{
        events::{EventBuffer, EventType, VaultEvent, create_event_channel},
        server::{AppState, VaultState},
        watcher::WatcherState,
    };

    use super::*;

    #[tokio::test]
    async fn vault_events_tracks_sse_connection_count() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let state = Arc::new(RwLock::new(AppState {
            vaults: HashMap::from([(
                "work".to_string(),
                VaultState {
                    cache: Arc::new(notesmith_index::VaultCache::open_in_memory().unwrap()),
                    search_index: Arc::new(notesmith_index::SearchIndex::open_in_memory().unwrap()),
                    engine: notesmith_vault::NativeVaultEngine,
                    root: root.clone(),
                    vault_config: arc_swap::ArcSwap::from_pointee(VaultConfig {
                        name: "work".to_string(),
                        ..Default::default()
                    }),
                    watcher_state: WatcherState::new(),
                    rebuilding: std::sync::atomic::AtomicBool::new(false),
                    template_engine: Arc::new(notesmith_templates::TemplateEngine::new(root, None)),
                },
            )]),
            event_tx: create_event_channel().0,
            event_buffer: Arc::new(EventBuffer::new(crate::events::EVENT_BUFFER_CAPACITY)),
            global_config_path: temp_dir.path().join("config.toml"),
            started_at: Utc.with_ymd_and_hms(2026, 5, 14, 19, 0, 0).unwrap(),
            sse_connection_count: Arc::new(AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
            mcp_services: Default::default(),
            transcripts: Default::default(),
            permissions: Default::default(),
            vault_watchers: Default::default(),
        }));

        let sse = vault_events(
            State(state.clone()),
            Path("work".to_string()),
            HeaderMap::new(),
            Query(EventsQuery::default()),
        )
        .await
        .unwrap();

        assert_eq!(
            state
                .read()
                .await
                .sse_connection_count
                .load(Ordering::Relaxed),
            1
        );

        drop(sse);

        assert_eq!(
            state
                .read()
                .await
                .sse_connection_count
                .load(Ordering::Relaxed),
            0
        );
    }

    #[tokio::test]
    async fn sse_event_serializes_id_field() {
        let body = to_bytes(
            Sse::new(stream::iter([Ok::<_, Infallible>(sse_event_for({
                let mut event = VaultEvent::new("work", EventType::NoteUpdated, "Inbox/note.md");
                event.id = Some(42);
                event
            }))]))
            .into_response()
            .into_body(),
            usize::MAX,
        )
        .await
        .unwrap();

        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("id: 42"));
        assert!(text.contains("event: note.updated"));
    }

    #[test]
    fn query_last_event_id_replays_buffered_events() {
        let mut state = AppState::default();
        state.vaults.insert(
            "work".to_string(),
            VaultState {
                cache: Arc::new(notesmith_index::VaultCache::open_in_memory().unwrap()),
                search_index: Arc::new(notesmith_index::SearchIndex::open_in_memory().unwrap()),
                engine: notesmith_vault::NativeVaultEngine,
                root: std::env::current_dir().unwrap(),
                vault_config: arc_swap::ArcSwap::from_pointee(VaultConfig {
                    name: "work".to_string(),
                    ..Default::default()
                }),
                watcher_state: WatcherState::new(),
                rebuilding: std::sync::atomic::AtomicBool::new(false),
                template_engine: Arc::new(notesmith_templates::TemplateEngine::new(
                    std::env::current_dir().unwrap(),
                    None,
                )),
            },
        );

        let first = state.event_buffer.push(VaultEvent::new(
            "work",
            EventType::NoteCreated,
            "Inbox/one.md",
        ));
        state
            .event_buffer
            .push(VaultEvent::new("_system", EventType::VaultsChanged, ""));
        state.event_buffer.push(VaultEvent::new(
            "home",
            EventType::NoteDeleted,
            "Inbox/two.md",
        ));

        let replayed = replay_events(
            &state,
            "work",
            resolve_last_event_id(
                &HeaderMap::new(),
                &EventsQuery {
                    last_event_id: first.id,
                },
            ),
        );

        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].event_type, EventType::VaultsChanged);
    }
}
