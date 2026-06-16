//! HTTP endpoint for per-vault customization discovery (issue #210, ADR 0016).
//!
//! Serves the **merged** customization set for a vault — custom agents
//! (personas), skills, and instructions — discovered from the vault's
//! `.notesmith/{agents,skills,instructions}/` folders overridden by the global
//! `~/.config/notesmith/{agents,skills,instructions}/` folders (project wins by
//! id). Discovery is daemon-side so it works against a remote daemon too.
//!
//! Customization `.md` files are untrusted (ADR 0009): malformed files are
//! logged and skipped inside [`notesmith_customization::discover`], so this
//! handler always returns 200 with a (possibly empty) set and never a 500 from a
//! bad file.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

use crate::server::SharedAppState;

/// `GET /api/v/{vault}/customizations` — return the merged customization set.
///
/// Response shape:
/// ```json
/// {
///   "agents":       [ { "id": "...", "name": "...", "description": "...",
///                       "backend": "copilot" | null, "model": "..." | null,
///                       "body": "...", "source": "project" | "global" } ],
///   "skills":       [ { "id", "name", "description", "body", "source" } ],
///   "instructions": [ { "id", "name", "description", "body", "source" } ]
/// }
/// ```
pub async fn list_customizations(
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

    let found = notesmith_customization::discover(&vault.root);
    Ok(Json(json!(found)))
}
