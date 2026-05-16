use std::{fs, path::Path as StdPath};

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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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
    let html = if query.inline_styles {
        notesmith_html::render_to_html_with_inline_styles(&content)
    } else {
        notesmith_html::render_to_html(notesmith_html::strip_frontmatter(&content))
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
