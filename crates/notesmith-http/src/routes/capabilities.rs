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
        "vaults_root": null
    }))
}
