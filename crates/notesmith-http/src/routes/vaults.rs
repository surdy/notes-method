use std::fs;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_config::GlobalConfig;
use notesmith_core::VaultEngine;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;
use crate::write_guard::WriteGuard;

use super::helpers::internal_error;

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
