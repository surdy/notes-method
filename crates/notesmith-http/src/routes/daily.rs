use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use notesmith_core::{NotesmithError, VaultEngine, VaultName, VaultPath, WriteResult};
use notesmith_query::{execute_sql, format_query_as_markdown_table};
use notesmith_vault::{extract_frontmatter, parse_note};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::events::{self, EventType, VaultEvent};
use crate::server::SharedAppState;

use super::helpers::{internal_error, note_error, query_error};

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
    let parsed = parse_note(&vault_id, &note_path, &content);

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
