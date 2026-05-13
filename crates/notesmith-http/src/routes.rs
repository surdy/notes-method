use std::{convert::Infallible, fs, path::Path as StdPath};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use chrono;
use futures::stream::Stream;
use notesmith_config::{GlobalConfig, VaultConfig};
use notesmith_core::{
    Note, NotesmithError, TaskStatus, VaultEngine, VaultName, VaultPath, WriteResult,
};
use notesmith_index::SearchResult;
use notesmith_query::{QueryError, QueryResult, execute_sql};
use notesmith_tasks::{AddTaskOptions, ToggleError, add_task, toggle_task};
use notesmith_vault::{apply_save_pipeline, extract_frontmatter, parse_note};
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use serde_yaml::{Mapping, Value as YamlValue};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::config_io::{
    compute_config_hash, compute_sidebar_config_hash, load_sidebar_config_with_hash,
    load_vault_config_with_hash, validate_sidebar_config, validate_vault_config,
};
use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;
use crate::write_guard::WriteGuard;

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

#[derive(Debug, Deserialize)]
pub struct AddVaultRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVaultRequest {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetDefaultRequest {
    pub name: String,
}

pub async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn list_vaults(
    State(state): State<SharedAppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_path = {
        let state = state.read().await;
        state.global_config_path.clone()
    };
    let config = GlobalConfig::load_from(&config_path).map_err(internal_error)?;
    let default_vault = config.effective_default().map(str::to_string);
    let vaults = config
        .vaults
        .iter()
        .map(|(name, registration)| {
            json!({
                "name": name,
                "path": registration.path,
                "is_default": default_vault.as_deref() == Some(name.as_str()),
            })
        })
        .collect();
    Ok(Json(Value::Array(vaults)))
}

pub async fn add_vault(
    State(state): State<SharedAppState>,
    _guard: WriteGuard,
    Json(body): Json<AddVaultRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let config_path = {
        let state = state.read().await;
        state.global_config_path.clone()
    };
    let mut config = GlobalConfig::load_from(&config_path).map_err(internal_error)?;

    if config.vaults.contains_key(&body.name) {
        return Err((
            StatusCode::CONFLICT,
            Json(
                json!({ "error": "vault_exists", "message": format!("Vault '{}' already registered", body.name) }),
            ),
        ));
    }

    let vault_path = std::path::PathBuf::from(&body.path);
    if !vault_path.exists() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(
                json!({ "error": "path_not_found", "message": format!("Path '{}' does not exist", body.path) }),
            ),
        ));
    }

    // Create .notesmith/ dir if needed
    let notesmith_dir = vault_path.join(".notesmith");
    fs::create_dir_all(&notesmith_dir).map_err(internal_error)?;

    config.vaults.insert(
        body.name.clone(),
        notesmith_config::VaultRegistration { path: vault_path },
    );
    config.save_to(&config_path).map_err(internal_error)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "name": body.name, "status": "registered" })),
    ))
}

pub async fn update_vault(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    _guard: WriteGuard,
    Json(body): Json<UpdateVaultRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_path = {
        let state = state.read().await;
        state.global_config_path.clone()
    };
    let mut config = GlobalConfig::load_from(&config_path).map_err(internal_error)?;

    let registration = config.vaults.remove(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    let new_name = body.name.unwrap_or_else(|| vault_name.clone());

    if new_name != vault_name && config.vaults.contains_key(&new_name) {
        // Put the original back before returning error
        config.vaults.insert(vault_name, registration);
        return Err((
            StatusCode::CONFLICT,
            Json(
                json!({ "error": "vault_exists", "message": format!("Vault '{}' already exists", new_name) }),
            ),
        ));
    }

    // Update default_vault if it was the renamed vault
    if config.default_vault.as_deref() == Some(&vault_name) {
        config.default_vault = Some(new_name.clone());
    }

    config.vaults.insert(new_name.clone(), registration);
    config.save_to(&config_path).map_err(internal_error)?;

    Ok(Json(json!({ "name": new_name, "status": "updated" })))
}

pub async fn remove_vault(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    _guard: WriteGuard,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let config_path = {
        let state = state.read().await;
        state.global_config_path.clone()
    };
    let mut config = GlobalConfig::load_from(&config_path).map_err(internal_error)?;

    if !config.vaults.contains_key(&vault_name) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        ));
    }

    if config.default_vault.as_deref() == Some(vault_name.as_str()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "cannot_remove_default",
                "message": "Cannot remove the default vault. Set a different default first."
            })),
        ));
    }

    config.vaults.remove(&vault_name);
    config.save_to(&config_path).map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_default_vault(
    State(state): State<SharedAppState>,
    _guard: WriteGuard,
    Json(body): Json<SetDefaultRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config_path = {
        let state = state.read().await;
        state.global_config_path.clone()
    };
    let mut config = GlobalConfig::load_from(&config_path).map_err(internal_error)?;

    if !config.vaults.contains_key(&body.name) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                json!({ "error": "vault_not_found", "message": format!("Vault '{}' not registered", body.name) }),
            ),
        ));
    }

    config.default_vault = Some(body.name.clone());
    config.save_to(&config_path).map_err(internal_error)?;

    Ok(Json(json!({ "default_vault": body.name })))
}

pub async fn reindex_vault(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    _guard: WriteGuard,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    let notes = vault.engine.scan(&vault.root).map_err(internal_error)?;
    vault
        .cache
        .reindex(&vault_name, &notes)
        .map_err(internal_error)?;
    vault
        .search_index
        .reindex(&vault_name, &notes)
        .map_err(internal_error)?;

    Ok(Json(
        json!({ "vault": vault_name, "status": "reindexed", "notes": notes.len() }),
    ))
}

// ── SSE event stream ─────────────────────────────────────────────────────────

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

// ── Sidebar config types ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidebarConfig {
    #[serde(default)]
    pub views: Vec<SidebarView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidebarView {
    pub id: String,
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub sections: Vec<SidebarSection>,
    pub badge_query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SidebarSection {
    RecentlyViewed {
        label: String,
        #[serde(default = "default_recently_viewed_mode")]
        mode: RecentlyViewedMode,
        #[serde(default = "default_section_limit")]
        limit: usize,
    },
    CustomFolders {
        label: String,
        folders: Vec<String>,
    },
    CustomItems {
        label: String,
        items: Vec<CustomItem>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecentlyViewedMode {
    Viewed,
    Edited,
    Both,
}

fn default_recently_viewed_mode() -> RecentlyViewedMode {
    RecentlyViewedMode::Both
}

fn default_section_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomItem {
    pub name: String,
    pub icon: String,
    pub source: ItemSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ItemSource {
    Folder(FolderSource),
    Query(QuerySource),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderSource {
    pub folder: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default = "default_sort")]
    pub sort: FolderSort,
    #[serde(default = "default_sort_dir")]
    pub sort_dir: SortDir,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FolderSort {
    Modified,
    Created,
    Name,
}

fn default_sort() -> FolderSort {
    FolderSort::Modified
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SortDir {
    Asc,
    Desc,
}

fn default_sort_dir() -> SortDir {
    SortDir::Desc
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuerySource {
    pub query: String,
    pub title_column: Option<String>,
    pub subtitle_column: Option<String>,
    #[serde(default)]
    pub badge_columns: Vec<String>,
}

// ── Folder notes types ───────────────────────────────────────

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
pub struct AgentCreateRequest {
    pub date: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ContextQuery {
    name: String,
    sql: String,
}

#[derive(Debug, Default, Deserialize)]
struct PromptTemplateFrontmatter {
    #[serde(default)]
    context_queries: Vec<ContextQuery>,
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

pub async fn get_sidebar_config(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<axum::response::Response, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    let (config, hash) = load_sidebar_config_with_hash(&vault.root).map_err(internal_error)?;
    let (_, warnings) = validate_sidebar_config(&config, &vault.root);

    let body = json!({
        "config": config,
        "hash": hash,
        "path": ".notesmith/sidebar.yaml",
        "warnings": warnings
    });

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("etag", format!("\"{hash}\""))
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap())
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
        VaultEvent::new(&vault_name, EventType::NoteCreated, note_path.as_str()),
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
        VaultEvent::new(&vault_name, EventType::NoteUpdated, note_path.as_str()),
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

    events::emit(
        &state.event_tx,
        VaultEvent::new(&vault_name, EventType::NoteUpdated, note_path.as_str()),
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
        VaultEvent::new(&vault_name, EventType::NoteUpdated, note_path.as_str()),
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
        VaultEvent::new(&vault_name, EventType::NoteMoved, to.as_str()),
    );

    Ok(Json(MoveNoteResponse {
        from: from.to_string(),
        to: to.to_string(),
    }))
}

// ── Inbox routes ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InboxCaptureRequest {
    pub text: String,
    pub title: Option<String>,
}

pub async fn inbox_capture(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Json(request): Json<InboxCaptureRequest>,
) -> Result<(StatusCode, Json<WriteNoteResponse>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let config = vault.vault_config.load();
    let capture_folder = &config.capture.folder;
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string();

    let slug = match &request.title {
        Some(title) => sanitize_slug(title),
        None => sanitize_slug(&request.text.chars().take(40).collect::<String>()),
    };

    let filename = if slug.is_empty() {
        format!("{timestamp}.md")
    } else {
        format!("{timestamp} - {slug}.md")
    };

    let note_path = if capture_folder.is_empty() {
        VaultPath::new(filename)
    } else {
        VaultPath::new(format!("{capture_folder}/{filename}"))
    };
    let content = request.text.clone();
    let response = write_note(&vault.engine, &vault.root, &note_path, None, &content)?;

    events::emit(
        &state.event_tx,
        VaultEvent::new(&vault_name, EventType::InboxAdded, note_path.as_str()),
    );

    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn list_inbox(
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

    let inbox_folder = &vault.vault_config.load().capture.folder;
    let like_pattern = format!("{inbox_folder}/%");

    let conn = vault.cache.connection();
    let mut statement = conn
        .prepare(
            "SELECT path, title, type, customer, stream, state, status, date, created_at, updated_at, archived, mtime_unix, frontmatter_json
             FROM v_notes
             WHERE path LIKE ?1 AND archived = 0
             ORDER BY path DESC
             LIMIT 100",
        )
        .map_err(internal_error)?;
    let rows = statement
        .query_map([&like_pattern], |row| {
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

fn sanitize_slug(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == ' ' || ch == '-' {
                ch
            } else {
                ' '
            }
        })
        .collect();
    // Collapse multiple spaces and trim
    sanitized.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── Task routes ───────────────────────────────────────────────────────────────

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

// ── Template routes ──────────────────────────────────────────────────────────

pub async fn list_templates(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let templates = vault
        .template_engine
        .list_templates()
        .map_err(internal_error)?;

    let result: Vec<Value> = templates
        .iter()
        .map(|m| {
            json!({
                "name": m.spec.name,
                "description": m.spec.description,
                "output_path": m.spec.output_path,
                "prompts": m.spec.prompts.iter().map(|p| json!({
                    "name": p.name,
                    "type": p.prompt_type,
                    "required": p.required,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(Json(json!(result)))
}

#[derive(Debug, Deserialize)]
pub struct TemplateRenderRequest {
    pub prompts: Option<std::collections::HashMap<String, String>>,
}

pub async fn render_template(
    State(state): State<SharedAppState>,
    Path((vault_name, template_name)): Path<(String, String)>,
    Json(request): Json<TemplateRenderRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let prompts = request.prompts.unwrap_or_default();
    match vault.template_engine.render(&template_name, &prompts) {
        Ok(rendered) => Ok(Json(json!({
            "path": rendered.path,
            "content": rendered.content,
        }))),
        Err(notesmith_templates::TemplateError::NotFound { name }) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("template not found: {name}") })),
        )),
        Err(notesmith_templates::TemplateError::MissingPrompts { prompts }) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "missing required prompts", "missing": prompts })),
        )),
        Err(e) => Err(internal_error(e)),
    }
}

pub async fn instantiate_template(
    State(state): State<SharedAppState>,
    Path((vault_name, template_name)): Path<(String, String)>,
    Json(request): Json<TemplateRenderRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let prompts = request.prompts.unwrap_or_default();
    match vault
        .template_engine
        .instantiate(&template_name, &prompts, &vault.engine)
    {
        Ok(rendered) => {
            events::emit(
                &state.event_tx,
                VaultEvent::new(&vault_name, EventType::NoteCreated, &rendered.path),
            );
            Ok((StatusCode::CREATED, Json(json!({ "path": rendered.path }))))
        }
        Err(notesmith_templates::TemplateError::NotFound { name }) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("template not found: {name}") })),
        )),
        Err(notesmith_templates::TemplateError::MissingPrompts { prompts }) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "missing required prompts", "missing": prompts })),
        )),
        Err(e) => Err(internal_error(e)),
    }
}

// ── Daily routes ─────────────────────────────────────────────────────────────

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

    let daily_folder = &vault.vault_config.load().daily.folder;
    let note_path = VaultPath::new(format!("{daily_folder}/{date}.md"));

    let content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;
    let vault_id = VaultName::new(vault_name.clone());
    let parsed = parse_note(&content, &vault_id, &note_path);

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
    let result = crate::scheduler::ensure_daily_note(
        &vault.root,
        &config.daily.folder,
        &config.daily.template,
        parsed_date,
        &vault.template_engine,
        &vault.engine,
    )
    .map_err(internal_error)?;

    match result {
        Some(path) => {
            events::emit(
                &state.event_tx,
                VaultEvent::new(&vault_name, EventType::DailyCreated, &path),
            );
            Ok((
                StatusCode::CREATED,
                Json(json!({ "path": path, "created": true })),
            ))
        }
        None => Ok((
            StatusCode::OK,
            Json(json!({
                "path": format!("{}/{}.md", config.daily.folder, date),
                "created": false,
            })),
        )),
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
    let (_, date_str) = parse_daily_date(request.date.as_deref())?;
    let note_path = daily_note_path(&vault.vault_config.load().daily.folder, &date_str);

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
                events::emit(
                    &state.event_tx,
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
        let prompt_path = vault
            .root
            .join(".notesmith")
            .join("prompts")
            .join("daily-note.md");
        let template = std::fs::read_to_string(&prompt_path).map_err(|error| {
            let status = if error.kind() == std::io::ErrorKind::NotFound {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(json!({
                    "error": format!("failed to read prompt template {}: {error}", prompt_path.display())
                })),
            )
        })?;
        let (queries, body_template) = parse_prompt_template(&template).map_err(internal_error)?;
        let mut prompt = body_template;
        for query in queries {
            let result = execute_sql(&vault.cache, &query.sql).map_err(query_error)?;
            let table = format_query_as_markdown_table(&result);
            prompt = prompt
                .replace(&format!("{{{{ {} }}}}", query.name), &table)
                .replace(&format!("{{{{{}}}}}", query.name), &table);
        }
        prompt = prompt
            .replace("{{ today }}", &date_str)
            .replace("{{today}}", &date_str);

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

fn daily_note_path(daily_folder: &str, date: &str) -> VaultPath {
    VaultPath::new(format!("{daily_folder}/{date}.md"))
}

fn parse_prompt_template(content: &str) -> anyhow::Result<(Vec<ContextQuery>, String)> {
    let (raw_frontmatter, body) = extract_frontmatter(content);
    let frontmatter = match raw_frontmatter {
        Some(raw) => serde_yaml::from_str::<PromptTemplateFrontmatter>(&raw)?,
        None => PromptTemplateFrontmatter::default(),
    };

    Ok((
        frontmatter.context_queries,
        body.trim_start_matches(['\r', '\n']).to_string(),
    ))
}

fn format_query_as_markdown_table(result: &QueryResult) -> String {
    if result.row_count == 0 || result.rows.is_empty() {
        return "(no results)".to_string();
    }

    let header = format!("| {} |", result.columns.join(" | "));
    let separator = format!(
        "| {} |",
        result
            .columns
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let rows = result
        .rows
        .iter()
        .map(|row| {
            format!(
                "| {} |",
                row.iter()
                    .map(format_markdown_cell)
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("{header}\n{separator}\n{rows}")
}

fn format_markdown_cell(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    text.replace(['\n', '\r'], " ").replace('|', "\\|")
}

// ── Route routes ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RoutePreviewRequest {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct RouteApplyRequest {
    pub paths: Option<Vec<String>>,
    #[serde(default)]
    pub inbox: bool,
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
        notesmith_routing::RoutingEngine::load(&vault.root).map_err(|e| match &e {
            notesmith_routing::RoutingError::ConfigNotFound { .. } => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": e.to_string() })),
            ),
            _ => internal_error(e),
        })?;

    let note_path = VaultPath::new(request.path.clone());
    let content = vault
        .engine
        .read(&vault.root, &note_path)
        .map_err(note_error)?;

    let route_match = routing_engine
        .preview(&request.path, &content)
        .map_err(|e| match &e {
            notesmith_routing::RoutingError::NoMatch { .. } => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": e.to_string() })),
            ),
            notesmith_routing::RoutingError::NoFrontmatter { .. } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": e.to_string() })),
            ),
            notesmith_routing::RoutingError::AlreadyArchived { .. } => (
                StatusCode::CONFLICT,
                Json(json!({ "error": e.to_string() })),
            ),
            _ => internal_error(e),
        })?;

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
        notesmith_routing::RoutingEngine::load(&vault.root).map_err(internal_error)?;

    if request.inbox {
        let inbox_folder = &vault.vault_config.load().capture.folder;
        let results = routing_engine
            .apply_inbox(&vault.root, inbox_folder, &vault.engine)
            .map_err(internal_error)?;
        for r in &results {
            events::emit(
                &state.event_tx,
                VaultEvent::new(&vault_name, EventType::NoteMoved, &r.to),
            );
        }
        return Ok(Json(json!({ "routed": results.len(), "results": results })));
    }

    let paths = request.paths.unwrap_or_default();
    let mut results = Vec::new();
    for path in &paths {
        let result = routing_engine
            .apply(&vault.root, path, &vault.engine)
            .map_err(internal_error)?;
        events::emit(
            &state.event_tx,
            VaultEvent::new(&vault_name, EventType::NoteMoved, &result.to),
        );
        results.push(result);
    }

    Ok(Json(json!({ "routed": results.len(), "results": results })))
}

// ---------------------------------------------------------------------------
// Git endpoints
// ---------------------------------------------------------------------------

pub async fn git_status(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    if !notesmith_git::ops::is_git_repo(&vault.root) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "vault is not a git repository" })),
        ));
    }

    let status = notesmith_git::ops::status(&vault.root).map_err(internal_error)?;
    Ok(Json(serde_json::to_value(status).map_err(internal_error)?))
}

pub async fn git_sync(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    if !notesmith_git::ops::is_git_repo(&vault.root) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "vault is not a git repository" })),
        ));
    }

    let root = vault.root.clone();
    drop(state);

    let pull_result = notesmith_git::ops::pull_ff(&root, "origin").map_err(internal_error)?;
    if pull_result.conflict {
        return Ok(Json(json!({
            "pull": pull_result,
            "push": null,
            "error": "pull conflict, push skipped",
        })));
    }

    let push_result = notesmith_git::ops::push(&root, "origin").map_err(internal_error)?;
    Ok(Json(json!({
        "pull": pull_result,
        "push": push_result,
    })))
}

// ── Capabilities ─────────────────────────────────────────────────────────────

pub async fn get_capabilities() -> Json<Value> {
    Json(json!({
        "deployment_mode": "desktop",
        "can_edit_global_config": true,
        "can_edit_vault_config": true,
        "can_open_local_paths": true,
        "restart_required_fields": ["daemon.bind"],
        "folder_picker": false,
        "vaults_root": null
    }))
}

// ── Sidebar config endpoints ────────────────────────────────────────────────

pub async fn put_sidebar_config(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    _guard: WriteGuard,
    headers: axum::http::HeaderMap,
    Json(body): Json<SidebarConfig>,
) -> Result<axum::response::Response, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    let if_match = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"'))
        .ok_or_else(|| {
            (
                StatusCode::PRECONDITION_REQUIRED,
                Json(json!({
                    "error": "if_match_required",
                    "message": "PUT requires If-Match header with config hash"
                })),
            )
        })?;

    let current_hash = compute_sidebar_config_hash(&vault.root).map_err(internal_error)?;
    if if_match != current_hash {
        let (config, new_hash) =
            load_sidebar_config_with_hash(&vault.root).map_err(internal_error)?;
        let (_, warnings) = validate_sidebar_config(&config, &vault.root);
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "conflict",
                "message": "Config was modified externally",
                "config": config,
                "hash": new_hash,
                "warnings": warnings
            })),
        ));
    }

    let (errors, warnings) = validate_sidebar_config(&body, &vault.root);
    if !errors.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "validation_failed",
                "errors": errors
            })),
        ));
    }

    let config_dir = vault.root.join(".notesmith");
    fs::create_dir_all(&config_dir).map_err(internal_error)?;
    let yaml = serde_yaml::to_string(&body).map_err(internal_error)?;
    fs::write(config_dir.join("sidebar.yaml"), yaml).map_err(internal_error)?;

    let (saved_config, new_hash) =
        load_sidebar_config_with_hash(&vault.root).map_err(internal_error)?;
    let response_body = json!({
        "config": saved_config,
        "hash": new_hash,
        "path": ".notesmith/sidebar.yaml",
        "warnings": warnings
    });

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("etag", format!("\"{new_hash}\""))
        .body(axum::body::Body::from(
            serde_json::to_vec(&response_body).unwrap(),
        ))
        .unwrap())
}

// ── Vault config endpoints ───────────────────────────────────────────────────

pub async fn get_vault_config(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<axum::response::Response, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    let (config, hash) = load_vault_config_with_hash(&vault.root).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let (_, warnings) = validate_vault_config(&config, &vault.root);

    let body = json!({
        "config": config,
        "hash": hash,
        "path": ".notesmith/vault.toml",
        "warnings": warnings
    });

    let mut response = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("etag", format!("\"{hash}\""))
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    // Ensure the response is well-formed
    let _ = &mut response;
    Ok(response)
}

pub async fn put_vault_config(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    _guard: WriteGuard,
    headers: axum::http::HeaderMap,
    Json(body): Json<VaultConfig>,
) -> Result<axum::response::Response, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    // Require If-Match header
    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim_matches('"'))
        .ok_or_else(|| {
            (
                StatusCode::PRECONDITION_REQUIRED,
                Json(json!({
                    "error": "if_match_required",
                    "message": "PUT requires If-Match header with config hash"
                })),
            )
        })?;

    // Compute current hash for conflict detection
    let current_hash = compute_config_hash(&vault.root).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    if if_match != current_hash {
        let (config, new_hash) = load_vault_config_with_hash(&vault.root).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
        let (_, warnings) = validate_vault_config(&config, &vault.root);
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "conflict",
                "message": "Config was modified externally",
                "config": config,
                "hash": new_hash,
                "warnings": warnings
            })),
        ));
    }

    // Validate the incoming config
    let (errors, warnings) = validate_vault_config(&body, &vault.root);
    if !errors.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "validation_failed",
                "errors": errors
            })),
        ));
    }

    // Write config to disk
    let config_path = vault.root.join(".notesmith").join("vault.toml");
    body.save_to(&config_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    // Read back with new hash
    let (saved_config, new_hash) = load_vault_config_with_hash(&vault.root).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let response_body = json!({
        "config": saved_config,
        "hash": new_hash,
        "path": ".notesmith/vault.toml",
        "warnings": warnings
    });

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("etag", format!("\"{new_hash}\""))
        .body(axum::body::Body::from(
            serde_json::to_vec(&response_body).unwrap(),
        ))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn parse_prompt_template_extracts_queries() {
        let template = r#"---
context_queries:
  - name: open_tasks
    sql: "SELECT text FROM v_tasks"
  - name: inbox_count
    sql: "SELECT COUNT(*) as count FROM v_notes"
---

# Daily Note Prompt

{{ open_tasks }}
"#;

        let (queries, body) = parse_prompt_template(template).unwrap();

        assert_eq!(
            queries,
            vec![
                ContextQuery {
                    name: "open_tasks".to_string(),
                    sql: "SELECT text FROM v_tasks".to_string(),
                },
                ContextQuery {
                    name: "inbox_count".to_string(),
                    sql: "SELECT COUNT(*) as count FROM v_notes".to_string(),
                },
            ]
        );
        assert!(body.contains("# Daily Note Prompt"));
        assert!(body.contains("{{ open_tasks }}"));
    }

    #[test]
    fn format_query_as_markdown_table_basic() {
        let result = QueryResult {
            columns: vec!["text".to_string(), "due".to_string()],
            rows: vec![vec![json!("Follow up"), json!("2026-05-10")]],
            row_count: 1,
        };

        assert_eq!(
            format_query_as_markdown_table(&result),
            "| text | due |\n| --- | --- |\n| Follow up | 2026-05-10 |"
        );
    }

    #[test]
    fn format_query_as_markdown_table_empty() {
        let result = QueryResult {
            columns: vec!["text".to_string()],
            rows: vec![],
            row_count: 0,
        };

        assert_eq!(format_query_as_markdown_table(&result), "(no results)");
    }

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
