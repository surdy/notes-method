use std::time::Duration;

use axum::{extract::State, http::StatusCode};

use crate::{
    events::{self, EventType, VaultEvent},
    server::SharedAppState,
};

pub async fn shutdown(State(state): State<SharedAppState>) -> StatusCode {
    let (vault_names, event_tx, shutdown_tx) = {
        let state = state.read().await;
        (
            state.vaults.keys().cloned().collect::<Vec<_>>(),
            state.event_tx.clone(),
            state.shutdown_tx.clone(),
        )
    };

    for vault_name in vault_names {
        events::emit(
            &event_tx,
            VaultEvent::new(vault_name, EventType::ShuttingDown, ""),
        );
    }

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = shutdown_tx.send(true);
    });

    StatusCode::OK
}

pub async fn restart(State(state): State<SharedAppState>) -> StatusCode {
    shutdown(State(state)).await
}
