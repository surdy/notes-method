use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde_json::{Value, json};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

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
    drop(state);

    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(event) if event.vault == vault_name => {
            let data = serde_json::to_string(&event).unwrap_or_default();
            Some(Ok(SseEvent::default()
                .event(event.event_type.as_str())
                .data(data)))
        }
        _ => None,
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
