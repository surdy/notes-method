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
use std::collections::HashMap;

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{WriteNoteResponse, internal_error, note_error, write_note};

#[derive(Debug, Deserialize)]
pub struct TaskFilters {
    pub status: Option<String>,
    pub field: Option<String>,
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
    pub fields: HashMap<String, String>,
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
    if let Some(ref field_filter) = filters.field {
        if let Some((key, value)) = field_filter.split_once('=') {
            conditions.push(format!(
                "EXISTS (SELECT 1 FROM task_fields tf WHERE tf.vault_name = t.vault_name AND tf.task_id = t.id AND tf.key = '{}' AND tf.value = '{}')",
                key.replace('\'', "''"),
                value.replace('\'', "''")
            ));
        }
    }
    if let Some(ref due_before) = filters.due_before {
        conditions.push(format!(
            "EXISTS (SELECT 1 FROM task_fields tf WHERE tf.vault_name = t.vault_name AND tf.task_id = t.id AND tf.key = 'due' AND tf.value < '{}')",
            due_before.replace('\'', "''")
        ));
    }

    let sql = format!(
        "SELECT t.id, t.content_hash, t.note_path, t.line_number, t.status_char, t.status_group, t.text, n.title \
         FROM tasks t \
         JOIN notes n ON n.vault_name = t.vault_name AND n.path = t.note_path \
         WHERE {} ORDER BY t.line_number ASC LIMIT {}",
        conditions.join(" AND "),
        filters.limit
    );

    let conn = vault.cache.connection();
    let mut stmt = conn.prepare(&sql).map_err(internal_error)?;

    struct RawTask {
        id: i64,
        task_hash: Option<String>,
        note_path: String,
        line_number: i64,
        status_char: String,
        status_group: String,
        text: String,
        note_title: Option<String>,
    }

    let raw_tasks = stmt
        .query_map([], |row| {
            Ok(RawTask {
                id: row.get(0)?,
                task_hash: row.get(1)?,
                note_path: row.get(2)?,
                line_number: row.get(3)?,
                status_char: row.get(4)?,
                status_group: row.get(5)?,
                text: row.get(6)?,
                note_title: row.get(7)?,
            })
        })
        .map_err(internal_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_error)?;

    let task_ids: Vec<i64> = raw_tasks.iter().map(|task| task.id).collect();
    let mut fields_map: HashMap<i64, HashMap<String, String>> = HashMap::new();

    if !task_ids.is_empty() {
        let placeholders = task_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fields_sql = format!(
            "SELECT task_id, key, value FROM task_fields WHERE vault_name = ?1 AND task_id IN ({placeholders})"
        );
        let mut fields_stmt = conn.prepare(&fields_sql).map_err(internal_error)?;
        let rows = fields_stmt
            .query_map([vault_name.as_str()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(internal_error)?;
        for row in rows {
            let (task_id, key, value) = row.map_err(internal_error)?;
            fields_map.entry(task_id).or_default().insert(key, value);
        }
    }

    let tasks = raw_tasks
        .into_iter()
        .map(|task| TaskSummary {
            task_hash: task.task_hash,
            note_path: task.note_path,
            line_number: task.line_number,
            status: status_name_for_char(&task.status_char),
            status_char: task.status_char,
            status_group: task.status_group,
            text: task.text,
            note_title: task.note_title,
            fields: fields_map.remove(&task.id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(tasks))
}

#[derive(Debug, Deserialize)]
pub struct AddTaskRequest {
    pub note_path: String,
    pub description: String,
    pub status_char: Option<String>,
    pub fields: Option<HashMap<String, String>>,
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

    if let Some(fields) = &request.fields {
        if let Some(due) = fields.get("due") {
            validate_due_string(due).map_err(|err| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": err })),
                )
            })?;
        }
        if let Some(priority) = fields.get("priority") {
            parse_priority_str(priority).map_err(|err| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": err })),
                )
            })?;
        }
    }

    let status_char = request
        .status_char
        .as_deref()
        .map(|status| status.chars().next().unwrap_or(' '))
        .unwrap_or(' ');

    let opts = AddTaskOptions {
        status_char: Some(status_char),
        fields: request.fields.unwrap_or_default(),
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
