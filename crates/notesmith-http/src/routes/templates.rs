use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::internal_error;

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
