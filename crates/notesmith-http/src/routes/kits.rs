//! Vault kits — the blessed configurations a vault can be created from.
//!
//! Clients list these to offer a choice at vault-creation time; applying one is
//! part of `POST /api/app/vaults` (see `routes::vaults::add_vault`), not a
//! separate call.

use axum::Json;
use serde_json::{Value, json};

/// `GET /api/app/kits` — the built-in kits, in registry order.
///
/// Deliberately not vault-scoped: kits ship with the binary, so this is
/// answerable before any vault exists.
pub async fn list_kits() -> Json<Value> {
    let kits: Vec<Value> = notesmith_kit::Kit::all()
        .iter()
        .map(|kit| {
            json!({
                "id": kit.id(),
                "description": kit.description(),
                "files": kit.files().len(),
                "folders": kit.folders(),
            })
        })
        .collect();

    Json(json!(kits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lists_the_builtin_kits() {
        let Json(body) = list_kits().await;
        let kits = body.as_array().unwrap();

        assert!(!kits.is_empty(), "at least the work-notes kit should ship");

        let work_notes = kits
            .iter()
            .find(|kit| kit["id"] == "work-notes")
            .expect("work-notes kit should be listed");

        assert!(work_notes["description"].as_str().unwrap().len() > 20);
        assert!(work_notes["files"].as_u64().unwrap() >= 15);
        assert!(
            work_notes["folders"]
                .as_array()
                .unwrap()
                .contains(&json!("Meetings"))
        );
    }
}
