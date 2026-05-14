use std::convert::Infallible;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures::stream::{self, Stream};
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::server::SharedAppState;

pub async fn vault_events(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, (StatusCode, Json<Value>)> {
    let state = state.read().await;

    if !state.vaults.contains_key(&vault_name) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        ));
    }

    let rx = state.event_tx.subscribe();
    let shutdown_rx = state.shutdown_rx.clone();
    let sse_connection_count = state.sse_connection_count.clone();
    drop(state);

    sse_connection_count.fetch_add(1, Ordering::Relaxed);
    let connection_guard = SseConnectionGuard::new(sse_connection_count);
    let stream = stream::unfold(
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
                        Ok(event) if event.vault == vault_name => {
                            let data = serde_json::to_string(&event).unwrap_or_default();
                            return Some((
                                Ok(SseEvent::default()
                                    .event(event.event_type.as_str())
                                    .data(data)),
                                (rx, shutdown_rx, vault_name, connection_guard),
                            ));
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

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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

    use chrono::{TimeZone, Utc};
    use notesmith_config::VaultConfig;
    use tokio::sync::RwLock;

    use crate::{
        events::create_event_channel,
        server::{AppState, VaultState},
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
                    cache: notesmith_index::VaultCache::open_in_memory().unwrap(),
                    search_index: notesmith_index::SearchIndex::open_in_memory().unwrap(),
                    engine: notesmith_vault::NativeVaultEngine,
                    root: root.clone(),
                    vault_config: arc_swap::ArcSwap::from_pointee(VaultConfig {
                        name: "work".to_string(),
                        capture: Default::default(),
                        daily: Default::default(),
                        editor: Default::default(),
                        git: Default::default(),
                        hooks: Default::default(),
                        homepage: None,
                    }),
                    template_engine: notesmith_templates::TemplateEngine::new(root, None),
                },
            )]),
            event_tx: create_event_channel().0,
            global_config_path: temp_dir.path().join("config.toml"),
            started_at: Utc.with_ymd_and_hms(2026, 5, 14, 19, 0, 0).unwrap(),
            sse_connection_count: Arc::new(AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
        }));

        let sse = vault_events(State(state.clone()), Path("work".to_string()))
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
}
