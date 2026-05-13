use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use notesmith_core::{TaskStatus, VaultEngine, VaultPath};
use notesmith_tasks::{AddTaskOptions, ToggleError, add_task, toggle_task};
use notesmith_vault::apply_save_pipeline;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{WriteNoteResponse, internal_error, note_error, write_note};

#[derive(Debug, Deserialize)]
pub struct TaskFilters {
    pub status: Option<String>,
    pub customer: Option<String>,
    pub due_before: Option<String>,
    #[serde(default = "default_task_limit")]
    pub limit: usize,
}

fn default_task_limit() -> usize {
    200
}

#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub task_hash: String,
    pub note_path: String,
    pub heading_path: Option<String>,
    pub ordinal: i64,
    pub status: String,
    pub text: String,
    pub customer: Option<String>,
    pub stream: Option<String>,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub scheduled: Option<String>,
    pub start_date: Option<String>,
    pub done_at: Option<String>,
    pub priority: Option<i64>,
}

pub async fn list_tasks(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Query(filters): Query<TaskFilters>,
) -> Result<Json<Vec<TaskSummary>>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let mut conditions = vec!["1=1".to_string()];
    if let Some(ref status) = filters.status {
        conditions.push(format!("status = '{}'", status.replace('\'', "''")));
    }
    if let Some(ref customer) = filters.customer {
        conditions.push(format!("customer = '{}'", customer.replace('\'', "''")));
    }
    if let Some(ref due_before) = filters.due_before {
        conditions.push(format!("due < '{}'", due_before.replace('\'', "''")));
    }

    let sql = format!(
        "SELECT task_hash, note_path, heading_path, ordinal, status, text, \
         customer, stream, owner, due, scheduled, start_date, done_at, priority \
         FROM v_tasks WHERE {} ORDER BY due ASC, ordinal ASC LIMIT {}",
        conditions.join(" AND "),
        filters.limit
    );

    let conn = vault.cache.connection();
    let mut stmt = conn.prepare(&sql).map_err(internal_error)?;
    let tasks = stmt
        .query_map([], |row| {
            Ok(TaskSummary {
                task_hash: row.get(0)?,
                note_path: row.get(1)?,
                heading_path: row.get(2)?,
                ordinal: row.get(3)?,
                status: row.get(4)?,
                text: row.get(5)?,
                customer: row.get(6)?,
                stream: row.get(7)?,
                owner: row.get(8)?,
                due: row.get(9)?,
                scheduled: row.get(10)?,
                start_date: row.get(11)?,
                done_at: row.get(12)?,
                priority: row.get(13)?,
            })
        })
        .map_err(internal_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_error)?;

    Ok(Json(tasks))
}

#[derive(Debug, Deserialize)]
pub struct AddTaskRequest {
    pub note_path: String,
    pub description: String,
    pub customer: Option<String>,
    pub stream: Option<String>,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub priority: Option<String>,
}

pub async fn create_task(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<AddTaskRequest>,
) -> Result<(StatusCode, Json<WriteNoteResponse>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let note_path = VaultPath::new(request.note_path);
    let current_content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;

    let due = request
        .due
        .as_deref()
        .map(|s| s.parse::<chrono::NaiveDate>())
        .transpose()
        .map_err(|err| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("invalid due date: {err}") })),
            )
        })?;

    let priority = request
        .priority
        .as_deref()
        .map(parse_priority_str)
        .transpose()
        .map_err(|err| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": err })),
            )
        })?;

    let opts = AddTaskOptions {
        due,
        customer: request.customer,
        stream: request.stream,
        owner: request.owner,
        priority,
    };

    let updated = add_task(&current_content, &request.description, &opts);
    let content = apply_save_pipeline(&updated);
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    events::emit(
        &state.event_tx,
        VaultEvent::new(&vault_name, EventType::TaskUpdated, note_path.as_str()),
    );

    Ok((StatusCode::CREATED, Json(response)))
}

#[derive(Debug, Deserialize)]
pub struct ToggleTaskRequest {
    pub note_path: String,
    pub task_hash: String,
    #[serde(alias = "status")]
    pub new_status: String,
}

pub async fn toggle_task_status(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<ToggleTaskRequest>,
) -> Result<Json<WriteNoteResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let new_status = parse_status_str(&request.new_status).map_err(|err| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": err })),
        )
    })?;

    let note_path = VaultPath::new(request.note_path);
    let current_content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;

    let updated = toggle_task(&current_content, &request.task_hash, new_status).map_err(|err| {
        let (status, msg) = match &err {
            ToggleError::TaskNotFound { .. } => (StatusCode::NOT_FOUND, err.to_string()),
            ToggleError::HashCollision { .. } => (StatusCode::CONFLICT, err.to_string()),
            ToggleError::InvalidTransition(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
            }
        };
        (status, Json(json!({ "error": msg })))
    })?;

    let content = apply_save_pipeline(&updated);
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    events::emit(
        &state.event_tx,
        VaultEvent::new(&vault_name, EventType::TaskUpdated, note_path.as_str()),
    );

    Ok(Json(response))
}

fn parse_status_str(s: &str) -> Result<TaskStatus, String> {
    match s {
        "todo" => Ok(TaskStatus::Todo),
        "in_progress" => Ok(TaskStatus::InProgress),
        "blocked" => Ok(TaskStatus::Blocked),
        "waiting" => Ok(TaskStatus::Waiting),
        "on_hold" => Ok(TaskStatus::OnHold),
        "done" => Ok(TaskStatus::Done),
        "cancelled" => Ok(TaskStatus::Cancelled),
        other => Err(format!(
            "unknown status '{other}'; expected one of: todo, in_progress, blocked, waiting, on_hold, done, cancelled"
        )),
    }
}

fn parse_priority_str(s: &str) -> Result<notesmith_core::TaskPriority, String> {
    match s {
        "highest" => Ok(notesmith_core::TaskPriority::Highest),
        "high" => Ok(notesmith_core::TaskPriority::High),
        "medium" => Ok(notesmith_core::TaskPriority::Medium),
        "low" => Ok(notesmith_core::TaskPriority::Low),
        "lowest" => Ok(notesmith_core::TaskPriority::Lowest),
        other => Err(format!(
            "unknown priority '{other}'; expected one of: highest, high, medium, low, lowest"
        )),
    }
}
