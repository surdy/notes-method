use std::fs;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use notesmith_config::GlobalConfig;
use notesmith_core::VaultEngine;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;
use crate::write_guard::WriteGuard;

use super::helpers::internal_error;

#[derive(Debug, Deserialize)]
pub struct AddVaultRequest {
    pub name: String,
    pub path: String,
    /// When true and `path` does not exist, the daemon will create the directory
    /// (recursively) before registering the vault. When false (default), a
    /// missing path returns 422.
    #[serde(default)]
    pub create: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVaultRequest {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RemoveVaultQuery {
    #[serde(default)]
    pub delete_files: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetDefaultRequest {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReindexVaultQuery {
    #[serde(default)]
    pub cache_only: bool,
    #[serde(default)]
    pub search_only: bool,
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
        if body.create {
            fs::create_dir_all(&vault_path).map_err(|error| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "error": "path_create_failed",
                        "message": format!("Failed to create '{}': {}", body.path, error)
                    })),
                )
            })?;
        } else {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(
                    json!({ "error": "path_not_found", "message": format!("Path '{}' does not exist", body.path) }),
                ),
            ));
        }
    } else if !vault_path.is_dir() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(
                json!({ "error": "path_not_directory", "message": format!("Path '{}' is not a directory", body.path) }),
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

    emit_vaults_changed(&state, &body.name).await;

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

    emit_vaults_changed(&state, &new_name).await;

    Ok(Json(json!({ "name": new_name, "status": "updated" })))
}

pub async fn remove_vault(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Query(query): Query<RemoveVaultQuery>,
    _guard: WriteGuard,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
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
    let vault_path = registration.path;
    if query.delete_files && vault_path.exists() && !vault_path.is_dir() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "path_not_directory",
                "message": format!("Path '{}' is not a directory", vault_path.display())
            })),
        ));
    }

    // If the removed vault was the default, pick a new one (first remaining
    // by sorted name for determinism), or clear it entirely if no vaults
    // remain. This lets the user delete the last vault without first having
    // to promote another to default.
    if config.default_vault.as_deref() == Some(vault_name.as_str()) {
        let next_default = {
            let mut names: Vec<&String> = config.vaults.keys().collect();
            names.sort();
            names.first().map(|name| (*name).clone())
        };
        config.default_vault = next_default;
    }

    config.save_to(&config_path).map_err(internal_error)?;

    if query.delete_files && vault_path.exists() {
        fs::remove_dir_all(&vault_path).map_err(|error| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "error": "path_delete_failed",
                    "message": format!("Failed to delete '{}': {}", vault_path.display(), error)
                })),
            )
        })?;
    }

    emit_vaults_changed(&state, &vault_name).await;

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

    emit_vaults_changed(&state, &body.name).await;

    Ok(Json(json!({ "default_vault": body.name })))
}

/// Emit a `vaults.changed` SSE event so other windows (Settings UI, vault
/// switcher, etc.) refresh without requiring a restart.
async fn emit_vaults_changed(state: &SharedAppState, vault_name: &str) {
    let state = state.read().await;
    events::emit(
        &state.event_tx,
        &state.event_buffer,
        VaultEvent::new(vault_name, EventType::VaultsChanged, ""),
    );
}

pub async fn reindex_vault(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
    Query(query): Query<ReindexVaultQuery>,
    _guard: WriteGuard,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if query.cache_only && query.search_only {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": "invalid_reindex_mode",
                "message": "cache_only and search_only cannot both be true"
            })),
        ));
    }

    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "vault_not_found" })),
        )
    })?;

    vault
        .rebuilding
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let _rebuild_guard = RebuildGuard(&vault.rebuilding);
    let notes = vault.engine.scan(&vault.root).map_err(internal_error)?;

    if !query.search_only {
        vault
            .cache
            .reindex(&vault_name, &notes)
            .map_err(internal_error)?;
    }
    if !query.cache_only {
        vault
            .search_index
            .reindex(&vault_name, &notes)
            .map_err(internal_error)?;
    }

    Ok(Json(
        json!({ "vault": vault_name, "status": "reindexed", "notes": notes.len() }),
    ))
}

struct RebuildGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for RebuildGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, sync::Arc};

    use axum::{
        extract::{Path, Query, State},
        http::StatusCode,
    };
    use chrono::Utc;
    use notesmith_config::{GlobalConfig, VaultConfig, VaultRegistration, migration};
    use notesmith_core::VaultEngine;
    use notesmith_index::{SearchIndex, VaultCache};
    use notesmith_vault::NativeVaultEngine;
    use tokio::sync::RwLock;

    use crate::{
        events::{EventBuffer, create_event_channel},
        server::{AppState, SharedAppState, VaultState},
        watcher::WatcherState,
        write_guard::WriteGuard,
    };

    use super::{ReindexVaultQuery, RemoveVaultQuery, reindex_vault, remove_vault};

    fn minimal_state(config_path: std::path::PathBuf) -> SharedAppState {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        Arc::new(RwLock::new(AppState {
            vaults: HashMap::new(),
            event_tx: create_event_channel().0,
            event_buffer: Arc::new(EventBuffer::new(crate::events::EVENT_BUFFER_CAPACITY)),
            global_config_path: config_path,
            started_at: Utc::now(),
            sse_connection_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
        }))
    }

    fn write_global_config(
        config_path: &std::path::Path,
        vault_name: &str,
        vault_root: std::path::PathBuf,
    ) {
        let mut config = GlobalConfig::default();
        config.default_vault = Some(vault_name.to_string());
        config.vaults.insert(
            vault_name.to_string(),
            VaultRegistration { path: vault_root },
        );
        config.save_to(config_path).unwrap();
    }

    #[tokio::test]
    async fn remove_vault_keeps_files_by_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Notes")).unwrap();
        fs::write(vault_root.join("Notes/Keep.md"), "# Keep\n").unwrap();
        write_global_config(&config_path, "work", vault_root.clone());
        let state = minimal_state(config_path.clone());

        let status = remove_vault(
            State(state),
            Path("work".to_string()),
            Query(RemoveVaultQuery::default()),
            WriteGuard,
        )
        .await
        .unwrap();

        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(vault_root.join("Notes/Keep.md").exists());
        let config = GlobalConfig::load_from(&config_path).unwrap();
        assert!(!config.vaults.contains_key("work"));
    }

    #[tokio::test]
    async fn remove_vault_deletes_files_when_requested() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Notes")).unwrap();
        fs::write(vault_root.join("Notes/Delete.md"), "# Delete\n").unwrap();
        write_global_config(&config_path, "work", vault_root.clone());
        let state = minimal_state(config_path.clone());

        let status = remove_vault(
            State(state),
            Path("work".to_string()),
            Query(RemoveVaultQuery { delete_files: true }),
            WriteGuard,
        )
        .await
        .unwrap();

        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(!vault_root.exists());
        let config = GlobalConfig::load_from(&config_path).unwrap();
        assert!(!config.vaults.contains_key("work"));
    }

    #[tokio::test]
    async fn reindex_vault_honors_cache_only_and_search_only_flags() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(
            vault_root.join("Inbox/Existing.md"),
            "# Existing\n\nready\n",
        )
        .unwrap();

        let engine = NativeVaultEngine;
        let notes = engine.scan(&vault_root).unwrap();
        let vault_config =
            migration::load_and_migrate(&vault_root).unwrap_or_else(|_| VaultConfig {
                name: "work".to_string(),
                ..Default::default()
            });
        let cache = VaultCache::open_in_memory().unwrap();
        cache
            .reindex_with_periodic("work", &notes, &vault_config.periodic)
            .unwrap();
        let search_index = SearchIndex::open_in_memory().unwrap();
        search_index.reindex("work", &notes).unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let state: SharedAppState = Arc::new(RwLock::new(AppState {
            vaults: HashMap::from([(
                "work".to_string(),
                VaultState {
                    cache,
                    search_index,
                    engine,
                    root: vault_root.clone(),
                    vault_config: arc_swap::ArcSwap::from_pointee(VaultConfig {
                        name: "work".to_string(),
                        ..Default::default()
                    }),
                    watcher_state: WatcherState::new(),
                    template_engine: notesmith_templates::TemplateEngine::new(
                        vault_root.clone(),
                        None,
                    ),
                    rebuilding: std::sync::atomic::AtomicBool::new(false),
                },
            )]),
            event_tx: create_event_channel().0,
            event_buffer: Arc::new(EventBuffer::new(crate::events::EVENT_BUFFER_CAPACITY)),
            global_config_path: temp_dir.path().join("config.toml"),
            started_at: Utc::now(),
            sse_connection_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
        }));

        fs::write(
            vault_root.join("Inbox/Cache Only.md"),
            "# Cache Only\n\ncache-only reindex\n",
        )
        .unwrap();

        let _ = reindex_vault(
            State(state.clone()),
            Path("work".to_string()),
            Query(ReindexVaultQuery {
                cache_only: true,
                search_only: false,
            }),
            WriteGuard,
        )
        .await
        .unwrap();

        {
            let state = state.read().await;
            let vault = state.vaults.get("work").unwrap();
            let cache_count: i64 = vault
                .cache
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM notes WHERE path = 'Inbox/Cache Only.md'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap();
            assert_eq!(cache_count, 1);
            assert!(
                vault
                    .search_index
                    .search("cache-only reindex", 10)
                    .unwrap()
                    .is_empty()
            );
        }

        fs::write(
            vault_root.join("Inbox/Search Only.md"),
            "# Search Only\n\nsearch-only reindex\n",
        )
        .unwrap();

        let _ = reindex_vault(
            State(state.clone()),
            Path("work".to_string()),
            Query(ReindexVaultQuery {
                cache_only: false,
                search_only: true,
            }),
            WriteGuard,
        )
        .await
        .unwrap();

        let state = state.read().await;
        let vault = state.vaults.get("work").unwrap();
        let cache_count: i64 = vault
            .cache
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM notes WHERE path = 'Inbox/Search Only.md'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(cache_count, 0);
        let search_results = vault
            .search_index
            .search("search-only reindex", 10)
            .unwrap();
        assert!(
            search_results
                .iter()
                .any(|result| result.path == "Inbox/Search Only.md")
        );
    }
}
