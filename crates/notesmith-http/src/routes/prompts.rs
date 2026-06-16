//! HTTP endpoint for per-vault static custom prompts (issue #193).
//!
//! Serves the **merged** prompt set for a vault: built-in defaults seeded in the
//! daemon config dir (`<config>/notesmith/prompts/*.md`) overridden by the
//! vault's own `_prompts/` folder, keyed by `name` (vault wins). The body of
//! each prompt is sent verbatim to the user's agent; variable substitution is
//! intentionally out of scope here (see [`notesmith_prompts`]).
//!
//! Prompt `.md` files are untrusted (ADR 0009): malformed files are logged and
//! skipped inside [`notesmith_prompts::load_merged_prompts`], so this handler
//! always returns a 200 with a (possibly empty) list and never a 500 from a bad
//! file.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use crate::server::SharedAppState;

/// `GET /api/v/{vault}/prompts` — return the merged static-prompt list.
///
/// Response shape:
/// ```json
/// { "prompts": [ { "name": "...", "description": "...", "body": "...", "source": "default" | "vault" } ] }
/// ```
pub async fn list_prompts(
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

    let defaults_dir = notesmith_prompts::default_prompts_dir().unwrap_or_default();
    let prompts = notesmith_prompts::load_merged_prompts(&defaults_dir, &vault.root);

    Ok(Json(json!({ "prompts": prompts })))
}
