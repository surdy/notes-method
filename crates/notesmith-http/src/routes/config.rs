use std::fs;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_config::VaultConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config_io::{
    compute_config_hash, compute_sidebar_config_hash, load_sidebar_config_with_hash,
    load_vault_config_with_hash, validate_sidebar_config, validate_vault_config,
};
use crate::server::SharedAppState;
use crate::write_guard::WriteGuard;

use super::helpers::internal_error;
use super::notes::{FolderSort, SortDir, default_sort, default_sort_dir};

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
pub struct QuerySource {
    pub query: String,
    pub title_column: Option<String>,
    pub subtitle_column: Option<String>,
    #[serde(default)]
    pub badge_columns: Vec<String>,
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

    // Auto-initialize a git repository when git is enabled for a vault that
    // isn't a repo yet, so enabling versioning is a zero-setup action. Failures
    // are non-fatal: the config save still succeeds and the outcome is surfaced
    // in the response.
    let git_init = if saved_config.git.enabled && !notesmith_git::ops::is_git_repo(&vault.root) {
        match notesmith_git::ops::init_repo(&vault.root) {
            Ok(result) => serde_json::to_value(result).ok(),
            Err(e) => {
                tracing::warn!(vault = %vault_name, error = %e, "auto git init failed");
                Some(json!({ "error": e.to_string() }))
            }
        }
    } else {
        None
    };

    let response_body = json!({
        "config": saved_config,
        "hash": new_hash,
        "path": ".notesmith/vault.toml",
        "warnings": warnings,
        "gitInit": git_init
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
