use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use notesmith_core::{VaultEngine, VaultPath};
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
    pub task_hash: Option<String>,
    pub note_path: String,
    pub line_number: i64,
    pub status: String,
    pub status_char: String,
    pub status_group: String,
    pub text: String,
    pub note_title: Option<String>,
    pub customer: Option<String>,
    pub stream: Option<String>,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub priority: Option<String>,
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
        let status_char = parse_status_str(status).map_err(|err| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": err })),
            )
        })?;
        conditions.push(format!(
            "t.status_char = '{}'",
            escape_sql_char(status_char)
        ));
    }
    if let Some(ref customer) = filters.customer {
        conditions.push(format!(
            "customer.value = '{}'",
            customer.replace('\'', "''")
        ));
    }
    if let Some(ref due_before) = filters.due_before {
        conditions.push(format!("due.value < '{}'", due_before.replace('\'', "''")));
    }

    let sql = format!(
        "SELECT t.content_hash, t.note_path, t.line_number, t.status_char, t.status_group, t.text, \
                n.title, customer.value, stream.value, owner.value, due.value, priority.value \
         FROM tasks t \
         JOIN notes n ON n.vault_name = t.vault_name AND n.path = t.note_path \
         LEFT JOIN task_fields customer ON customer.vault_name = t.vault_name AND customer.task_id = t.id AND customer.key = 'customer' \
         LEFT JOIN task_fields stream ON stream.vault_name = t.vault_name AND stream.task_id = t.id AND stream.key = 'stream' \
         LEFT JOIN task_fields owner ON owner.vault_name = t.vault_name AND owner.task_id = t.id AND owner.key = 'owner' \
         LEFT JOIN task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due' \
         LEFT JOIN task_fields priority ON priority.vault_name = t.vault_name AND priority.task_id = t.id AND priority.key = 'priority' \
         WHERE {} ORDER BY due.value IS NULL, due.value ASC, t.line_number ASC LIMIT {}",
        conditions.join(" AND "),
        filters.limit
    );

    let conn = vault.cache.connection();
    let mut stmt = conn.prepare(&sql).map_err(internal_error)?;
    let tasks = stmt
        .query_map([], |row| {
            let status_char: String = row.get(3)?;
            Ok(TaskSummary {
                task_hash: row.get(0)?,
                note_path: row.get(1)?,
                line_number: row.get(2)?,
                status: status_name_for_char(&status_char),
                status_char,
                status_group: row.get(4)?,
                text: row.get(5)?,
                note_title: row.get(6)?,
                customer: row.get(7)?,
                stream: row.get(8)?,
                owner: row.get(9)?,
                due: row.get(10)?,
                priority: row.get(11)?,
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
        .map(validate_due_string)
        .transpose()
        .map_err(|err| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": err })),
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
        status_char: Some(' '),
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
        &state.event_buffer,
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
        };
        (status, Json(json!({ "error": msg })))
    })?;

    let content = apply_save_pipeline(&updated);
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::TaskUpdated, note_path.as_str()),
    );

    Ok(Json(response))
}

fn parse_status_str(s: &str) -> Result<char, String> {
    match s {
        "todo" => Ok(' '),
        "in_progress" => Ok('/'),
        "blocked" => Ok('b'),
        "waiting" => Ok('w'),
        "on_hold" => Ok('h'),
        "done" => Ok('x'),
        "cancelled" => Ok('-'),
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Ok(ch),
                _ => Err(format!(
                    "unknown status '{other}'; expected todo, in_progress, blocked, waiting, on_hold, done, cancelled, or a single status character"
                )),
            }
        }
    }
}

fn parse_priority_str(s: &str) -> Result<String, String> {
    match s {
        "highest" | "high" | "medium" | "low" | "lowest" => Ok(s.to_string()),
        other if !other.trim().is_empty() => Ok(other.trim().to_string()),
        _ => Err("priority must not be empty".to_string()),
    }
}

fn validate_due_string(s: &str) -> Result<String, String> {
    s.parse::<chrono::NaiveDate>()
        .map(|date| date.to_string())
        .map_err(|err| format!("invalid due date: {err}"))
}

fn escape_sql_char(ch: char) -> String {
    ch.to_string().replace('\'', "''")
}

fn status_name_for_char(status_char: &str) -> String {
    match status_char.chars().next().unwrap_or(' ') {
        ' ' => "todo",
        '/' => "in_progress",
        'b' => "blocked",
        'w' => "waiting",
        'h' => "on_hold",
        'x' | 'X' => "done",
        '-' => "cancelled",
        other => return other.to_string(),
    }
    .to_string()
}
