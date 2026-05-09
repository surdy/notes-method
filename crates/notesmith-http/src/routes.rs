use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use notesmith_core::{Note, NotesmithError, VaultEngine, VaultName, VaultPath, WriteResult};
use notesmith_index::SearchResult;
use notesmith_query::{QueryError, QueryResult, execute_sql};
use notesmith_vault::{apply_save_pipeline, parse_note};
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use serde_yaml::{Mapping, Value as YamlValue};

use crate::server::SharedAppState;

#[derive(Debug, Serialize)]
pub struct NoteSummary {
    pub path: String,
    pub title: String,
    #[serde(rename = "type")]
    pub note_type: String,
    pub customer: Option<String>,
    pub stream: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
    pub date: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived: bool,
    pub mtime_unix: i64,
    pub frontmatter: Value,
}

pub async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn list_notes(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Vec<NoteSummary>>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let conn = vault.cache.connection();
    let mut statement = conn
        .prepare(
            "SELECT path, title, type, customer, stream, state, status, date, created_at, updated_at, archived, mtime_unix, frontmatter_json
             FROM v_notes
             ORDER BY path",
        )
        .map_err(internal_error)?;
    let rows = statement
        .query_map([], |row| {
            let frontmatter_json: String = row.get(12)?;
            Ok(NoteSummary {
                path: row.get(0)?,
                title: row.get(1)?,
                note_type: row.get(2)?,
                customer: row.get(3)?,
                stream: row.get(4)?,
                state: row.get(5)?,
                status: row.get(6)?,
                date: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
                archived: row.get::<_, i64>(10)? != 0,
                mtime_unix: row.get(11)?,
                frontmatter: serde_json::from_str(&frontmatter_json).unwrap_or(Value::Null),
            })
        })
        .map_err(internal_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_error)?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct SqlQueryRequest {
    pub sql: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub title: String,
    pub folder: Option<String>,
    pub content: Option<String>,
    pub frontmatter: Option<serde_json::Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
pub struct PutNoteRequest {
    pub content: String,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchNoteRequest {
    pub frontmatter: serde_json::Map<String, Value>,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AppendNoteRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct MoveNoteRequest {
    pub destination: String,
}

#[derive(Debug, Serialize)]
pub struct WriteNoteResponse {
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Serialize)]
pub struct MoveNoteResponse {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

pub async fn execute_sql_query(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<SqlQueryRequest>,
) -> Result<Json<QueryResult>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    execute_sql(&vault.cache, &request.sql)
        .map(Json)
        .map_err(query_error)
}

pub async fn search_notes(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    vault
        .search_index
        .search(&params.q, params.limit)
        .map(Json)
        .map_err(internal_error)
}

pub async fn create_note(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<WriteNoteResponse>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let folder = request.folder.as_deref().unwrap_or("Inbox");
    let note_path = VaultPath::new(format!("{folder}/{}.md", request.title));
    match vault.engine.read(&vault.root, &note_path) {
        Ok(_) => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "note already exists",
                    "path": note_path.as_str(),
                })),
            ));
        }
        Err(NotesmithError::NoteNotFound { .. }) => {}
        Err(error) => return Err(note_error(error)),
    }

    let initial_content =
        build_note_document(request.frontmatter.as_ref(), request.content.as_deref())
            .map_err(internal_error)?;
    let content = apply_save_pipeline(&initial_content);
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn get_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
) -> Result<Json<Note>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let vault_path = VaultPath::new(note_path);
    let content = vault
        .engine
        .read(&vault.root, &vault_path)
        .map_err(note_error)?;
    let vault_id = VaultName::new(vault_name.clone());
    let parsed = parse_note(&content, &vault_id, &vault_path);

    Ok(Json(Note {
        vault: vault_id,
        path: vault_path,
        frontmatter: parsed.frontmatter,
        raw_frontmatter: parsed.raw_frontmatter,
        body: parsed.body,
        tasks: parsed.tasks,
        links: parsed.links,
        inline_fields: parsed.inline_fields,
        blocks: parsed.blocks,
        hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
    }))
}

pub async fn put_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Json(request): Json<PutNoteRequest>,
) -> Result<Json<WriteNoteResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let note_path = VaultPath::new(note_path);
    let content = apply_save_pipeline(&request.content);
    let response = write_note(
        &vault.engine,
        &vault.root,
        &note_path,
        request.expected_hash.as_deref(),
        &content,
    )?;

    Ok(Json(response))
}

pub async fn patch_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Json(request): Json<PatchNoteRequest>,
) -> Result<Json<WriteNoteResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let note_path = VaultPath::new(note_path);
    let vault_id = VaultName::new(vault_name.clone());
    let current_content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;
    let parsed = parse_note(&current_content, &vault_id, &note_path);
    let mut merged_frontmatter =
        raw_frontmatter_to_mapping(parsed.raw_frontmatter.as_deref()).map_err(internal_error)?;
    merge_frontmatter(&mut merged_frontmatter, &request.frontmatter).map_err(internal_error)?;
    let merged_content =
        build_note_document_from_yaml(&merged_frontmatter, &parsed.body).map_err(internal_error)?;
    let content = apply_save_pipeline(&merged_content);
    let response = write_note(
        &vault.engine,
        &vault.root,
        &note_path,
        request.expected_hash.as_deref(),
        &content,
    )?;

    Ok(Json(response))
}

pub async fn delete_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let note_path = VaultPath::new(note_path);
    vault
        .engine
        .delete(&vault.root, &note_path)
        .map_err(note_error)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn append_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Json(request): Json<AppendNoteRequest>,
) -> Result<Json<WriteNoteResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let note_path = VaultPath::new(note_path);
    let current_content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;
    let separator = if current_content.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let appended_content = format!("{current_content}{separator}{}", request.content);
    let content = apply_save_pipeline(&appended_content);
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    Ok(Json(response))
}

pub async fn move_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Json(request): Json<MoveNoteRequest>,
) -> Result<Json<MoveNoteResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let from = VaultPath::new(note_path);
    let to = VaultPath::new(request.destination);
    vault
        .engine
        .move_path(&vault.root, &from, &to)
        .map_err(note_error)?;

    Ok(Json(MoveNoteResponse {
        from: from.to_string(),
        to: to.to_string(),
    }))
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
}

fn query_error(error: QueryError) -> (StatusCode, Json<Value>) {
    let status = match error {
        QueryError::NotReadOnly => StatusCode::BAD_REQUEST,
        QueryError::ExecutionError(_) => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (status, Json(json!({ "error": error.to_string() })))
}

fn note_error(error: NotesmithError) -> (StatusCode, Json<Value>) {
    let status = match error {
        NotesmithError::NoteNotFound { .. } => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": error.to_string() })))
}

fn build_note_document(
    frontmatter: Option<&serde_json::Map<String, Value>>,
    body: Option<&str>,
) -> anyhow::Result<String> {
    let frontmatter = match frontmatter {
        Some(frontmatter) => json_frontmatter_to_mapping(frontmatter)?,
        None => Mapping::new(),
    };
    build_note_document_from_yaml(&frontmatter, body.unwrap_or_default())
}

fn build_note_document_from_yaml(frontmatter: &Mapping, body: &str) -> anyhow::Result<String> {
    let yaml = serialize_yaml_mapping(frontmatter)?;
    Ok(if yaml.is_empty() {
        format!("---\n---\n{body}")
    } else {
        format!("---\n{yaml}\n---\n{body}")
    })
}

fn serialize_yaml_mapping(frontmatter: &Mapping) -> anyhow::Result<String> {
    let serialized = serde_yaml::to_string(&YamlValue::Mapping(frontmatter.clone()))?;
    Ok(serialized
        .strip_prefix("---\n")
        .unwrap_or(&serialized)
        .trim_end_matches('\n')
        .to_string())
}

fn json_frontmatter_to_mapping(
    frontmatter: &serde_json::Map<String, Value>,
) -> anyhow::Result<Mapping> {
    if frontmatter.is_empty() {
        return Ok(Mapping::new());
    }

    let yaml_value = serde_yaml::to_value(Value::Object(frontmatter.clone()))?;
    match yaml_value {
        YamlValue::Mapping(mapping) => Ok(mapping),
        other => Err(anyhow::anyhow!(
            "expected frontmatter object, got {other:?}"
        )),
    }
}

fn raw_frontmatter_to_mapping(raw_frontmatter: Option<&str>) -> anyhow::Result<Mapping> {
    let Some(raw_frontmatter) = raw_frontmatter else {
        return Ok(Mapping::new());
    };

    if raw_frontmatter.trim().is_empty() {
        return Ok(Mapping::new());
    }

    match serde_yaml::from_str::<YamlValue>(raw_frontmatter)? {
        YamlValue::Mapping(mapping) => Ok(mapping),
        YamlValue::Null => Ok(Mapping::new()),
        other => Err(anyhow::anyhow!(
            "frontmatter must be a YAML mapping, got {other:?}"
        )),
    }
}

fn merge_frontmatter(
    target: &mut Mapping,
    updates: &serde_json::Map<String, Value>,
) -> anyhow::Result<()> {
    for (key, value) in updates {
        target.insert(
            YamlValue::String(key.clone()),
            serde_yaml::to_value(value.clone())?,
        );
    }
    Ok(())
}

fn write_note(
    engine: &impl VaultEngine,
    root: &std::path::Path,
    path: &VaultPath,
    expected_hash: Option<&str>,
    content: &str,
) -> Result<WriteNoteResponse, (StatusCode, Json<Value>)> {
    match engine
        .write(root, path, expected_hash, content)
        .map_err(note_error)?
    {
        WriteResult::Written { hash } => Ok(WriteNoteResponse {
            path: path.to_string(),
            hash,
        }),
        WriteResult::Conflict { expected, actual } => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "write conflict",
                "path": path.as_str(),
                "expected": expected,
                "actual": actual,
            })),
        )),
    }
}
