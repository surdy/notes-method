use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_core::{NotesmithError, PeriodKind, VaultEngine, VaultName, VaultPath, WriteResult};
use notesmith_vault::parse_note;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{internal_error, note_error, render_prompt_error};

#[derive(Debug, Deserialize)]
pub struct AgentCreateRequest {
    pub date: Option<String>,
    pub content: Option<String>,
}

pub async fn get_daily_note(
    State(state): State<SharedAppState>,
    Path((vault_name, date)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let parsed_date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid date: {e}") })),
        )
    })?;
    let config = vault.vault_config.load();
    let resolved = crate::scheduler::resolve_periodic_note(
        &config.periodic,
        PeriodKind::Daily,
        parsed_date,
        &vault.template_engine,
    )
    .map_err(internal_error)?;
    let note_path = VaultPath::new(resolved.path);

    let content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;
    let vault_id = VaultName::new(vault_name.clone());
    let parsed = parse_note(&vault_id, &note_path, &content);

    Ok(Json(json!({
        "path": note_path.as_str(),
        "content": content,
        "frontmatter": parsed.frontmatter,
    })))
}

pub async fn create_daily_note(
    State(state): State<SharedAppState>,
    Path((vault_name, date)): Path<(String, String)>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let parsed_date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid date: {e}") })),
        )
    })?;

    let config = vault.vault_config.load();
    let result = crate::scheduler::ensure_periodic_note(
        &vault.root,
        &config.periodic,
        PeriodKind::Daily,
        parsed_date,
        &vault.template_engine,
        &vault.engine,
    )
    .map_err(internal_error)?;

    if let Some(path) = result.created_path.as_deref() {
        refresh_daily_indexes(vault, &vault_name, path).map_err(internal_error)?;
        events::emit(
            &state.event_tx,
            &state.event_buffer,
            VaultEvent::new(&vault_name, EventType::DailyCreated, path),
        );
        Ok((
            StatusCode::CREATED,
            Json(json!({ "path": path, "created": true })),
        ))
    } else {
        Ok((
            StatusCode::OK,
            Json(json!({
                "path": result.note.path,
                "created": false,
            })),
        ))
    }
}

pub async fn agent_create_daily(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<AgentCreateRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;
    let (parsed_date, date_str) = parse_daily_date(request.date.as_deref())?;
    let config = vault.vault_config.load();
    let resolved = crate::scheduler::resolve_periodic_note(
        &config.periodic,
        PeriodKind::Daily,
        parsed_date,
        &vault.template_engine,
    )
    .map_err(internal_error)?;
    let note_path = VaultPath::new(resolved.path);

    if let Some(content) = request.content {
        match vault.engine.read(&vault.root, &note_path) {
            Ok(_) => {
                return Err((
                    StatusCode::CONFLICT,
                    Json(json!({
                        "error": "daily note already exists",
                        "path": note_path.as_str(),
                    })),
                ));
            }
            Err(NotesmithError::NoteNotFound { .. }) => {}
            Err(error) => return Err(note_error(error)),
        }

        match vault
            .engine
            .write(&vault.root, &note_path, None, &content)
            .map_err(note_error)?
        {
            WriteResult::Written { .. } => {
                refresh_daily_indexes(vault, &vault_name, note_path.as_str())
                    .map_err(internal_error)?;
                events::emit(
                    &state.event_tx,
                    &state.event_buffer,
                    VaultEvent::new(&vault_name, EventType::DailyCreated, note_path.as_str()),
                );
                Ok((
                    StatusCode::CREATED,
                    Json(json!({
                        "path": note_path.as_str(),
                        "created": true,
                    })),
                ))
            }
            WriteResult::Conflict { expected, actual } => Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "write conflict",
                    "path": note_path.as_str(),
                    "expected": expected,
                    "actual": actual,
                })),
            )),
        }
    } else {
        // Shared with `GET /api/v/{vault}/agent-prompts/{name}` (issue #282):
        // the daily agent prompt is just the `daily-note` template.
        let prompt = crate::prompt_render::render_prompt(vault, "daily-note", &date_str)
            .map_err(render_prompt_error)?;

        Ok((
            StatusCode::OK,
            Json(json!({
                "prompt": prompt,
                "date": date_str,
            })),
        ))
    }
}

fn parse_daily_date(
    date: Option<&str>,
) -> Result<(chrono::NaiveDate, String), (StatusCode, Json<Value>)> {
    let date_str = date
        .map(|value| value.to_string())
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let parsed = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid date: {e}") })),
        )
    })?;
    Ok((parsed, date_str))
}

fn refresh_daily_indexes(
    vault: &crate::server::VaultState,
    vault_name: &str,
    path: &str,
) -> anyhow::Result<()> {
    let note_path = VaultPath::new(path.to_string());
    let content = vault.engine.read(&vault.root, &note_path)?;
    let note = parse_note(
        &VaultName::new(vault_name.to_string()),
        &note_path,
        &content,
    );
    let config = vault.vault_config.load();
    vault
        .cache
        .update_note_with_periodic(vault_name, &note, &config.periodic)?;
    vault.search_index.update_note(vault_name, &note)?;
    Ok(())
}
