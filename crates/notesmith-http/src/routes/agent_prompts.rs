//! Agent-prompt rendering endpoint (issue #282).
//!
//! `GET /api/v/{vault}/agent-prompts/{name}` renders the vault's
//! `.notesmith/prompts/<name>.md` — executing its `context_queries` against
//! the vault index and substituting `{{ today }}` — and returns the assembled
//! prompt. This is the generic sibling of the daily `agent-create` prompt
//! mode; `notesmith ai prompt <name>` fetches its instruction here before
//! driving the headless agent.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use super::helpers::render_prompt_error;
use crate::prompt_render::{is_valid_prompt_name, render_prompt};
use crate::server::SharedAppState;

#[derive(Debug, Default, Deserialize)]
pub struct RenderPromptParams {
    /// Target date (`YYYY-MM-DD`) substituted for `{{ today }}`; defaults to
    /// the daemon's local date.
    pub date: Option<String>,
}

/// `GET /api/v/{vault}/agent-prompts/{name}`
pub async fn render_agent_prompt(
    State(state): State<SharedAppState>,
    Path((vault_name, prompt_name)): Path<(String, String)>,
    Query(params): Query<RenderPromptParams>,
) -> Response {
    if !is_valid_prompt_name(&prompt_name) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("invalid prompt name: {prompt_name:?}") })),
        )
            .into_response();
    }

    let date_str = match &params.date {
        Some(raw) => match chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
            Ok(_) => raw.clone(),
            Err(error) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("invalid date: {error}") })),
                )
                    .into_response();
            }
        },
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    };

    let state = state.read().await;
    let Some(vault) = state.vaults.get(&vault_name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("vault not found: {vault_name}") })),
        )
            .into_response();
    };

    match render_prompt(vault, &prompt_name, &date_str) {
        Ok(prompt) => Json(json!({
            "name": prompt_name,
            "date": date_str,
            "prompt": prompt,
        }))
        .into_response(),
        Err(error) => render_prompt_error(error).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::server::{build_app_state, build_router, create_vault_state};
    use notesmith_config::GlobalConfig;

    fn setup_vault(prompts: &[(&str, &str)]) -> tempfile::TempDir {
        let vault = tempfile::TempDir::new().unwrap();
        let prompts_dir = vault.path().join(".notesmith").join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        for (name, content) in prompts {
            std::fs::write(prompts_dir.join(format!("{name}.md")), content).unwrap();
        }
        vault
    }

    fn router_for(vault_name: &str, root: &Path) -> axum::Router {
        let mut state = build_app_state(&GlobalConfig::default()).unwrap();
        state.vaults.insert(
            vault_name.to_string(),
            create_vault_state(vault_name, root).unwrap(),
        );
        build_router(state)
    }

    async fn get(router: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn renders_a_named_prompt_with_queries_and_date() {
        let vault = setup_vault(&[(
            "briefing",
            "---\ncontext_queries:\n  - name: notes\n    sql: \"SELECT path FROM v_notes LIMIT 1\"\n---\n# Briefing for {{ today }}\n\n{{ notes }}\n",
        )]);
        let router = router_for("agent-prompts-vault", vault.path());

        let (status, body) = get(
            &router,
            "/api/v/agent-prompts-vault/agent-prompts/briefing?date=2026-08-05",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["name"], "briefing");
        assert_eq!(body["date"], "2026-08-05");
        let prompt = body["prompt"].as_str().unwrap();
        assert!(prompt.contains("# Briefing for 2026-08-05"));
        assert!(
            !prompt.contains("{{ notes }}"),
            "query placeholder replaced"
        );
    }

    #[tokio::test]
    async fn unknown_prompt_vault_and_bad_names_are_client_errors() {
        let vault = setup_vault(&[]);
        let router = router_for("agent-prompts-404", vault.path());

        let (status, body) = get(&router, "/api/v/agent-prompts-404/agent-prompts/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body["error"].as_str().unwrap().contains("not found"));

        let (status, _) = get(&router, "/api/v/no-such-vault/agent-prompts/x").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let (status, _) = get(
            &router,
            "/api/v/agent-prompts-404/agent-prompts/..%2Fsecrets",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let (status, _) = get(
            &router,
            "/api/v/agent-prompts-404/agent-prompts/x?date=not-a-date",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn bad_context_sql_is_unprocessable_not_a_crash() {
        let vault = setup_vault(&[(
            "broken",
            "---\ncontext_queries:\n  - name: q\n    sql: \"SELECT nope FROM not_a_table\"\n---\n{{ q }}\n",
        )]);
        let router = router_for("agent-prompts-sql", vault.path());

        let (status, _) = get(&router, "/api/v/agent-prompts-sql/agent-prompts/broken").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
