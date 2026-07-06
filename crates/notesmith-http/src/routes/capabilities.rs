use axum::Json;
use serde_json::{Value, json};

pub async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn get_capabilities() -> Json<Value> {
    Json(json!({
        "deployment_mode": "desktop",
        "can_edit_global_config": true,
        "can_edit_vault_config": true,
        "can_open_local_paths": true,
        "restart_required_fields": ["daemon.bind"],
        "folder_picker": false,
        "vaults_root": null,
        "embeddings": {
            "compiled_in": cfg!(feature = "local-embed"),
            "model": notesmith_embed::CANONICAL_MODEL_ID,
            "dim": notesmith_embed::CANONICAL_DIM,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capabilities_advertise_embeddings_block() {
        let Json(caps) = get_capabilities().await;
        let embeddings = &caps["embeddings"];

        // Process-global facts only — per-vault `enabled` lives in vault config,
        // not here (ADR 0018 §9.3).
        assert!(embeddings.get("enabled").is_none());
        assert_eq!(
            embeddings["compiled_in"],
            Value::Bool(cfg!(feature = "local-embed"))
        );
        assert_eq!(embeddings["model"], notesmith_embed::CANONICAL_MODEL_ID);
        assert_eq!(embeddings["dim"], notesmith_embed::CANONICAL_DIM);
    }
}
