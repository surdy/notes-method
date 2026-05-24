use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use notesmith_index::FieldRegistry;
use notesmith_query::execute_sql_with_options;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::server::SharedAppState;

const MAX_SUGGESTIONS: usize = 50;
const MAX_QUERY_ROWS: usize = 1_000;

#[derive(Debug, Default, Deserialize)]
pub struct SuggestQuery {
    #[serde(default)]
    pub q: String,
}

pub async fn get_fields(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Result<Json<FieldRegistry>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    Ok(Json(FieldRegistry::load(&vault.root)))
}

pub async fn suggest_field_values(
    State(state): State<SharedAppState>,
    Path((vault_name, key)): Path<(String, String)>,
    Query(query): Query<SuggestQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<Value>)> {
    let state = state.read().await;
    let vault = state.vaults.get(&vault_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
    })?;

    let registry = FieldRegistry::load(&vault.root);
    let Some(definition) = registry.get(&key) else {
        return Ok(Json(Vec::new()));
    };

    if let Some(values) = &definition.values {
        return Ok(Json(filter_suggestions(values.iter().cloned(), &query.q)));
    }

    if let Some(sql) = &definition.suggest_from {
        let result = match execute_sql_with_options(&vault.cache, sql, Some(MAX_QUERY_ROWS)) {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(vault = %vault_name, field = %key, "Failed to fetch field suggestions: {error}");
                return Ok(Json(Vec::new()));
            }
        };

        let suggestions = filter_suggestions(
            result.rows.into_iter().filter_map(|row| {
                row.into_iter().next().and_then(|value| match value {
                    Value::String(text) => Some(text),
                    Value::Number(number) => Some(number.to_string()),
                    Value::Bool(flag) => Some(flag.to_string()),
                    _ => None,
                })
            }),
            &query.q,
        );
        return Ok(Json(suggestions));
    }

    Ok(Json(Vec::new()))
}

fn filter_suggestions<I>(values: I, query: &str) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut suggestions = Vec::new();
    for value in values {
        if !query.is_empty() && !value.starts_with(query) {
            continue;
        }
        if suggestions.contains(&value) {
            continue;
        }
        suggestions.push(value);
        if suggestions.len() >= MAX_SUGGESTIONS {
            break;
        }
    }
    suggestions
}
