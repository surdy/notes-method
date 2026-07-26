use std::{
    fs,
    path::{Component, Path as StdPath, PathBuf},
};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
};
use notesmith_core::{Note, NotesmithError, VaultEngine, VaultName, VaultPath};
use notesmith_index::SearchResult;
use notesmith_query::execute_sql;
use notesmith_vault::{apply_save_pipeline, parse_note};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{
    WriteNoteResponse, build_note_document, build_note_document_from_yaml, internal_error,
    merge_frontmatter, note_error, query_error, raw_frontmatter_to_mapping, write_note,
};

#[derive(Debug, Serialize)]
pub struct NoteSummary {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub mtime_unix: i64,
    pub frontmatter: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FolderSort {
    Modified,
    Created,
    Name,
}

pub(crate) fn default_sort() -> FolderSort {
    FolderSort::Modified
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SortDir {
    Asc,
    Desc,
}

pub(crate) fn default_sort_dir() -> SortDir {
    SortDir::Desc
}

#[derive(Debug, Deserialize)]
pub struct FolderNotesQuery {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default = "default_folder_notes_limit")]
    pub limit: Option<usize>,
    #[serde(default = "default_sort")]
    pub sort: FolderSort,
    #[serde(default = "default_sort_dir")]
    pub sort_dir: SortDir,
}

fn default_folder_notes_limit() -> Option<usize> {
    Some(50)
}

#[derive(Debug, Serialize)]
pub struct FolderNoteItem {
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub modified_at: Option<String>,
    pub created_at: Option<String>,
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
pub struct MoveNoteResponse {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameNoteRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RenameNoteResponse {
    pub from: String,
    pub to: String,
    pub references_rewritten: usize,
}

#[derive(Debug, Deserialize)]
pub struct RenameFolderRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RenameFolderResponse {
    pub from: String,
    pub to: String,
    pub folder_note_from: Option<String>,
    pub folder_note_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct HtmlRenderQuery {
    #[serde(default)]
    pub inline_styles: bool,
}

fn default_limit() -> usize {
    20
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
            "SELECT path, title, created_at, updated_at, mtime_unix
             FROM notes
             ORDER BY path",
        )
        .map_err(internal_error)?;
    let base_rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(internal_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_error)?;
    drop(statement);

    let mut notes = Vec::with_capacity(base_rows.len());
    for (path, title, created_at, updated_at, mtime_unix) in base_rows {
        let frontmatter =
            load_note_frontmatter(&conn, &vault_name, &path).map_err(internal_error)?;
        let tags = extract_tags(&frontmatter);
        notes.push(NoteSummary {
            path,
            title,
            tags,
            created_at,
            updated_at,
            mtime_unix,
            frontmatter,
        });
    }

    Ok(Json(notes))
}

fn load_note_frontmatter(
    conn: &Connection,
    vault_name: &str,
    path: &str,
) -> rusqlite::Result<Value> {
    let mut fields_stmt = conn.prepare(
        "SELECT key, value, value_type FROM fields WHERE vault_name = ?1 AND note_path = ?2 ORDER BY key",
    )?;
    let mut field_rows = fields_stmt.query(params![vault_name, path])?;
    let mut frontmatter = Map::new();
    while let Some(row) = field_rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        let value_type: String = row.get(2)?;
        frontmatter.insert(key, parse_field_json_value(&value, &value_type));
    }
    drop(field_rows);
    drop(fields_stmt);

    let mut tags_stmt =
        conn.prepare("SELECT tag FROM tags WHERE vault_name = ?1 AND note_path = ?2 ORDER BY tag")?;
    let mut tag_rows = tags_stmt.query(params![vault_name, path])?;
    let mut tags = Vec::new();
    while let Some(row) = tag_rows.next()? {
        tags.push(row.get::<_, String>(0)?);
    }
    if !tags.is_empty() {
        frontmatter.insert(
            "tags".to_string(),
            Value::Array(tags.into_iter().map(Value::String).collect()),
        );
    }

    Ok(Value::Object(frontmatter))
}

fn parse_field_json_value(value: &str, value_type: &str) -> Value {
    match value_type {
        "boolean" => Value::Bool(value == "true"),
        "number" => value
            .parse::<i64>()
            .map(|number| Value::Number(number.into()))
            .or_else(|_| {
                value
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .ok_or(())
            })
            .unwrap_or_else(|_| Value::String(value.to_string())),
        "list" => serde_yaml::from_str::<Value>(value)
            .unwrap_or_else(|_| Value::String(value.to_string())),
        _ => Value::String(value.to_string()),
    }
}

fn extract_tags(frontmatter: &Value) -> Vec<String> {
    match frontmatter.get("tags") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

pub async fn get_folders(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    let mut folders = Vec::new();
    collect_visible_directories(&vault.root, &vault.root, &mut folders).map_err(internal_error)?;
    folders.sort();

    Ok(Json(folders))
}

pub async fn get_folder_notes(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Query(params): Query<FolderNotesQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let folder = params.path.trim_end_matches('/');
    let path_filter = format!("{folder}/%");

    let sort_column = match params.sort {
        FolderSort::Modified => "updated_at",
        FolderSort::Created => "created_at",
        FolderSort::Name => "path",
    };
    let sort_direction = match params.sort_dir {
        SortDir::Asc => "ASC",
        SortDir::Desc => "DESC",
    };

    let non_recursive_filter = if params.recursive {
        String::new()
    } else {
        let depth_prefix = format!("{folder}/%/%");
        format!(" AND path NOT LIKE '{depth_prefix}'")
    };

    let limit_clause = match params.limit {
        Some(n) => format!(" LIMIT {n}"),
        None => String::new(),
    };

    let sql = format!(
        "SELECT path, title, body_excerpt, updated_at, created_at \
         FROM notes \
         WHERE vault_name = '{vault_name}' AND path LIKE '{path_filter}'{non_recursive_filter} \
         ORDER BY {sort_column} {sort_direction}{limit_clause}"
    );

    let result = execute_sql(&vault.cache, &sql).map_err(query_error)?;

    let notes: Vec<FolderNoteItem> = result
        .rows
        .into_iter()
        .map(|row| {
            let path = row[0].as_str().unwrap_or("").to_string();
            let title = row[1].as_str().unwrap_or("").to_string();
            let body_excerpt = row[2].as_str().unwrap_or("");
            let snippet = extract_snippet(body_excerpt);
            let modified_at = row[3].as_str().map(|s| s.to_string());
            let created_at = row[4].as_str().map(|s| s.to_string());
            FolderNoteItem {
                path,
                title,
                snippet,
                modified_at,
                created_at,
            }
        })
        .collect();

    Ok(Json(json!({ "notes": notes })))
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

    // `key:value` tokens in the query (tag:x, path:x, customer:x, any
    // field:value) become metadata filters; the rest is the text query.
    let (text, filters) = notesmith_ops::parse_search_query(&params.q);
    if filters.is_empty() {
        return vault
            .search_index
            .search(&params.q, params.limit)
            .map(Json)
            .map_err(internal_error);
    }

    let allowed =
        notesmith_ops::resolve_filter_paths(&vault.cache, &filters).map_err(internal_error)?;
    if allowed.is_empty() {
        return Ok(Json(Vec::new()));
    }
    // A token-only query (no text) lists the filter matches directly.
    if text.trim().is_empty() {
        let mut results: Vec<SearchResult> = Vec::new();
        let conn = vault.cache.connection();
        let mut stmt = conn
            .prepare(
                "SELECT vault_name, path, title FROM notes WHERE path IN (SELECT value FROM json_each(?1)) ORDER BY path",
            )
            .map_err(internal_error)?;
        let paths_json =
            serde_json::to_string(&allowed.iter().collect::<Vec<_>>()).map_err(internal_error)?;
        let rows = stmt
            .query_map([paths_json], |row| {
                Ok(SearchResult {
                    vault_name: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    note_type: String::new(),
                    score: 0.0,
                    snippet: String::new(),
                })
            })
            .map_err(internal_error)?;
        for row in rows {
            let row = row.map_err(internal_error)?;
            results.push(row);
            if results.len() >= params.limit {
                break;
            }
        }
        return Ok(Json(results));
    }

    vault
        .search_index
        .search_in_paths(&text, params.limit, &allowed)
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

    let config = vault.vault_config.load();
    let default_folder = &config.capture.folder;
    let folder = request.folder.as_deref().unwrap_or(default_folder);
    let note_path = if folder.is_empty() {
        VaultPath::new(format!("{}.md", request.title))
    } else {
        VaultPath::new(format!("{folder}/{}.md", request.title))
    };
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

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteCreated, note_path.as_str())
            .with_hash(response.hash.clone()),
    );

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
    Ok(Json(parse_note(&vault_id, &vault_path, &content)))
}

pub async fn render_note_html(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Query(query): Query<HtmlRenderQuery>,
) -> Result<Html<String>, (StatusCode, Json<Value>)> {
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
    let hardbreaks = !vault.vault_config.load().editor.strict_line_breaks;
    let html = if query.inline_styles {
        notesmith_html::render_to_html_with_inline_styles_opts(&content, hardbreaks)
    } else {
        notesmith_html::render_to_html_opts(notesmith_html::strip_frontmatter(&content), hardbreaks)
    };
    Ok(Html(html))
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

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteUpdated, note_path.as_str())
            .with_hash(response.hash.clone()),
    );

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
    let parsed = parse_note(&vault_id, &note_path, &current_content);
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

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteUpdated, note_path.as_str())
            .with_hash(response.hash.clone()),
    );

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

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteDeleted, note_path.as_str()),
    );

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

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteUpdated, note_path.as_str())
            .with_hash(response.hash.clone()),
    );

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

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteMoved, to.as_str()),
    );

    Ok(Json(MoveNoteResponse {
        from: from.to_string(),
        to: to.to_string(),
    }))
}

pub async fn rename_note(
    State(state): State<SharedAppState>,
    Path((vault_name, note_path)): Path<(String, String)>,
    Json(request): Json<RenameNoteRequest>,
) -> Result<Json<RenameNoteResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let new_name = validate_note_name(&request.name)?;

    let from = note_path.trim_matches('/').to_string();
    if from.is_empty() {
        return Err(bad_request("note path must not be empty"));
    }
    let from_path = StdPath::new(&from);
    let parent = from_path.parent().and_then(|p| p.to_str()).unwrap_or("");
    let target_filename = format!("{new_name}.md");
    let to = if parent.is_empty() {
        target_filename.clone()
    } else {
        format!("{parent}/{target_filename}")
    };

    let from_abs = vault.root.join(&from);
    if !from_abs.is_file() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "note not found", "path": from })),
        ));
    }
    let to_abs = vault.root.join(&to);

    if from == to {
        return Ok(Json(RenameNoteResponse {
            from,
            to,
            references_rewritten: 0,
        }));
    }

    if to_abs.exists() && !paths_reference_same_entry(&from_abs, &to_abs).unwrap_or(false) {
        return Err(conflict("a note with that name already exists", to));
    }

    rename_path_with_case_support(&from_abs, &to_abs).map_err(internal_error)?;

    // Rewrite wikilinks vault-wide. Best-effort: if rewrite fails we log and
    // continue — the rename has already succeeded and is not rolled back.
    let old_stem = from_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let references_rewritten = if old_stem.is_empty() || old_stem == new_name {
        0
    } else {
        match notesmith_vault::rewrite_wikilinks(&vault.root, old_stem, &new_name) {
            Ok(result) => result.references_rewritten,
            Err(error) => {
                tracing::warn!("wikilink rewrite after rename failed for {from} -> {to}: {error}");
                0
            }
        }
    };

    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(&vault_name, EventType::NoteMoved, &to),
    );

    Ok(Json(RenameNoteResponse {
        from,
        to,
        references_rewritten,
    }))
}

pub async fn rename_folder(
    State(state): State<SharedAppState>,
    Path((vault_name, folder_path)): Path<(String, String)>,
    Json(request): Json<RenameFolderRequest>,
) -> Result<Json<RenameFolderResponse>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let from = validate_folder_path(&folder_path)?;
    let new_name = validate_folder_name(&request.name)?;
    let from_abs = vault.root.join(&from);
    if !from_abs.is_dir() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "folder not found", "path": from })),
        ));
    }

    let from_path = StdPath::new(&from);
    let from_name = from_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| bad_request("folder path must include a folder name"))?;
    let parent = from_path
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    let to = if parent.is_empty() {
        new_name.clone()
    } else {
        format!("{parent}/{new_name}")
    };
    let to_abs = vault.root.join(&to);

    preflight_folder_destination(&from_abs, &to_abs, &to)?;

    let source_folder_note_name = format!("{from_name}.md");
    let target_folder_note_name = format!("{new_name}.md");
    let source_folder_note_abs = from_abs.join(&source_folder_note_name);
    let source_folder_note_rel = format!("{from}/{source_folder_note_name}");
    let folder_note_exists = source_folder_note_abs.is_file();
    if folder_note_exists {
        preflight_folder_note_destination(
            &source_folder_note_abs,
            &from_abs.join(&target_folder_note_name),
            &format!("{from}/{target_folder_note_name}"),
        )?;
    }

    rename_path_with_case_support(&from_abs, &to_abs).map_err(internal_error)?;

    let mut folder_note_from = None;
    let mut folder_note_to = None;
    if folder_note_exists {
        let moved_folder_note_abs = to_abs.join(&source_folder_note_name);
        let target_folder_note_abs = to_abs.join(&target_folder_note_name);
        let target_folder_note_rel = format!("{to}/{target_folder_note_name}");
        if let Err(error) =
            rename_path_with_case_support(&moved_folder_note_abs, &target_folder_note_abs)
        {
            match fs::rename(&to_abs, &from_abs) {
                Ok(()) => {}
                Err(rollback_error) => {
                    tracing::error!(
                        "failed to roll back folder rename from {} to {} after folder-note rename error: {}; rollback error: {}",
                        from,
                        to,
                        error,
                        rollback_error
                    );
                }
            }
            return Err(internal_error(format!(
                "folder renamed but folder note rename failed: {error}"
            )));
        }

        folder_note_from = Some(source_folder_note_rel);
        folder_note_to = Some(target_folder_note_rel.clone());
        events::emit(
            &state.event_tx,
            &state.event_buffer,
            VaultEvent::new(&vault_name, EventType::NoteMoved, &target_folder_note_rel),
        );
    }

    Ok(Json(RenameFolderResponse {
        from,
        to,
        folder_note_from,
        folder_note_to,
    }))
}

fn bad_request(message: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message.into() })),
    )
}

fn conflict(message: impl Into<String>, path: impl Into<String>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::CONFLICT,
        Json(json!({ "error": message.into(), "path": path.into() })),
    )
}

fn validate_folder_path(path: &str) -> Result<String, (StatusCode, Json<Value>)> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Err(bad_request("folder path must not be empty"));
    }
    if trimmed.contains('\\') {
        return Err(bad_request("folder path must use '/' separators"));
    }

    let candidate = StdPath::new(trimmed);
    if candidate.is_absolute() {
        return Err(bad_request("folder path must be vault-relative"));
    }
    if candidate.components().any(|component| {
        !matches!(component, Component::Normal(_))
            || component.as_os_str().to_string_lossy().is_empty()
    }) {
        return Err(bad_request("folder path contains unsafe segments"));
    }

    Ok(trimmed.to_string())
}

fn validate_folder_name(name: &str) -> Result<String, (StatusCode, Json<Value>)> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(bad_request("folder name must not be empty"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(bad_request("folder name must not contain path separators"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(bad_request("folder name contains unsafe segments"));
    }

    Ok(trimmed.to_string())
}

fn validate_note_name(name: &str) -> Result<String, (StatusCode, Json<Value>)> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(bad_request("note name must not be empty"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(bad_request("note name must not contain path separators"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(bad_request("note name contains unsafe segments"));
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
    {
        return Err(bad_request(
            "note name contains characters not allowed in filenames",
        ));
    }
    // Strip trailing .md if user supplied it; we always append it.
    let stripped = trimmed.strip_suffix(".md").unwrap_or(trimmed).trim();
    if stripped.is_empty() {
        return Err(bad_request("note name must not be empty"));
    }
    Ok(stripped.to_string())
}

fn preflight_folder_destination(
    from_abs: &StdPath,
    to_abs: &StdPath,
    to: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    if !to_abs.exists() {
        return Ok(());
    }
    if paths_reference_same_entry(from_abs, to_abs).map_err(internal_error)? {
        return Ok(());
    }

    Err(conflict("destination folder already exists", to))
}

fn preflight_folder_note_destination(
    source_abs: &StdPath,
    target_abs: &StdPath,
    target: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    if !target_abs.exists() {
        return Ok(());
    }
    if paths_reference_same_entry(source_abs, target_abs).map_err(internal_error)? {
        return Ok(());
    }

    Err(conflict("folder note target already exists", target))
}

fn paths_reference_same_entry(left: &StdPath, right: &StdPath) -> std::io::Result<bool> {
    Ok(fs::canonicalize(left)? == fs::canonicalize(right)?)
}

fn rename_path_with_case_support(from: &StdPath, to: &StdPath) -> std::io::Result<()> {
    if from == to || paths_reference_same_entry(from, to).unwrap_or(false) {
        let temp = sibling_temp_path(from);
        fs::rename(from, &temp)?;
        if let Err(error) = fs::rename(&temp, to) {
            let _ = fs::rename(&temp, from);
            return Err(error);
        }
        return Ok(());
    }

    fs::rename(from, to)
}

fn sibling_temp_path(path: &StdPath) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| StdPath::new(""));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("folder");
    let process_id = std::process::id();
    for index in 0..1000 {
        let candidate = parent.join(format!(
            ".notesmith-rename-{process_id}-{index}-{file_name}"
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(".notesmith-rename-{process_id}-{file_name}"))
}

fn collect_visible_directories(
    vault_root: &StdPath,
    directory: &StdPath,
    folders: &mut Vec<String>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let relative = path
            .strip_prefix(vault_root)?
            .to_string_lossy()
            .replace('\\', "/");
        folders.push(relative);
        collect_visible_directories(vault_root, &path, folders)?;
    }

    Ok(())
}

fn extract_snippet(body_excerpt: &str) -> String {
    let trimmed = if body_excerpt.len() > 200 {
        &body_excerpt[..200]
    } else {
        body_excerpt
    };
    let lines: Vec<&str> = trimmed.lines().take(2).collect();
    lines.join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_extraction() {
        assert_eq!(extract_snippet(""), "");

        assert_eq!(extract_snippet("Short text"), "Short text");

        assert_eq!(
            extract_snippet("Line one\nLine two\nLine three"),
            "Line one\nLine two"
        );

        let long = "a".repeat(300);
        let snippet = extract_snippet(&long);
        assert!(snippet.len() <= 200);

        assert_eq!(
            extract_snippet("First line\nSecond line\nThird line\nFourth line"),
            "First line\nSecond line"
        );

        assert_eq!(extract_snippet("  \n  \n  "), "");
    }
}
