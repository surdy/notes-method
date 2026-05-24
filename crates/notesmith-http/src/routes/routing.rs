use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_core::{VaultEngine, VaultPath};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{internal_error, note_error};

fn map_routing_error(error: notesmith_routing::RoutingError) -> (StatusCode, Json<Value>) {
    match error {
        notesmith_routing::RoutingError::ConfigNotFound { .. }
        | notesmith_routing::RoutingError::NoMatch { .. } => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": error.to_string() })),
        ),
        notesmith_routing::RoutingError::NoFrontmatter { .. }
        | notesmith_routing::RoutingError::InvalidFrontmatter { .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": error.to_string() })),
        ),
        notesmith_routing::RoutingError::AlreadyArchived { .. }
        | notesmith_routing::RoutingError::DestinationExists { .. } => (
            StatusCode::CONFLICT,
            Json(json!({ "error": error.to_string() })),
        ),
        other => internal_error(other),
    }
}

#[derive(Debug, Deserialize)]
pub struct RoutePreviewRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct RouteApplyRequest {
    pub paths: Option<Vec<String>>,
}

pub async fn route_preview(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<RoutePreviewRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let routing_engine =
        notesmith_routing::RoutingEngine::load(&vault.root).map_err(map_routing_error)?;

    let note_path = VaultPath::new(request.path.clone());
    let content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;

    let route_match = routing_engine
        .preview(&request.path, &content)
        .map_err(map_routing_error)?;

    Ok(Json(json!({
        "path": request.path,
        "destination": route_match.destination,
        "rule_id": route_match.rule_id,
    })))
}

pub async fn route_apply(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<RouteApplyRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let routing_engine =
        notesmith_routing::RoutingEngine::load(&vault.root).map_err(map_routing_error)?;

    let paths = request.paths.unwrap_or_default();
    let mut results = Vec::new();
    for path in &paths {
        let result = routing_engine
            .apply(&vault.root, path, &vault.engine)
            .map_err(map_routing_error)?;
        events::emit(
            &state.event_tx,
            &state.event_buffer,
            VaultEvent::new(&vault_name, EventType::NoteMoved, &result.to),
        );
        results.push(result);
    }

    Ok(Json(json!({ "routed": results.len(), "results": results })))
}
