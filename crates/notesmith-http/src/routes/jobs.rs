//! Jobs endpoints (issue #280, ADR 0025).
//!
//! - `GET  /api/v/{vault}/jobs` — list configured jobs with schedule,
//!   validity, running flag, and last-run status (debuggability).
//! - `POST /api/v/{vault}/jobs/{name}/run` — manual trigger. Returns `202`
//!   and runs in the background (watch `job.*` SSE events or the list for
//!   the outcome); `409` when a run of that job is already in flight; `404`
//!   for unknown vault/job; `400` when the job has no runnable `command`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::json;

use crate::jobs::{TriggerOutcome, is_running, schedule, state::JobStateStore, trigger_job};
use crate::server::SharedAppState;

#[derive(Debug, Serialize)]
pub struct JobLastRun {
    pub at: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct JobListEntry {
    pub name: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    pub weekdays_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Whether the scheduler will run this entry as configured.
    pub valid: bool,
    /// Why the scheduler skips this entry, when `valid` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_error: Option<String>,
    /// Whether a run (scheduled or manual) is executing right now.
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<JobLastRun>,
}

/// `GET /api/v/{vault}/jobs`
pub async fn list_jobs(
    State(state): State<SharedAppState>,
    Path(vault_name): Path<String>,
) -> Response {
    let jobs = {
        let app = state.read().await;
        let Some(vault) = app.vaults.get(&vault_name) else {
            return not_found(format!("vault not found: {vault_name}"));
        };
        vault.vault_config.load().jobs.clone()
    };

    // Last-run records come from the durable per-vault state store; a missing
    // or corrupt file degrades to "no recorded runs".
    let records = JobStateStore::for_vault(&vault_name)
        .map(|store| store.all())
        .unwrap_or_default();

    let entries: Vec<JobListEntry> = jobs
        .iter()
        .map(|job| {
            let config_error = schedule::validate_job(job).err();
            let last_run = records.get(&job.name).map(|record| JobLastRun {
                at: record.last_run.to_rfc3339(),
                status: record.status.as_str().to_string(),
                exit_code: record.exit_code,
                duration_ms: record.duration_ms,
            });
            JobListEntry {
                name: job.name.clone(),
                enabled: job.enabled,
                every: job.every.clone(),
                at: job.at.clone(),
                weekdays_only: job.weekdays_only,
                timezone: job.timezone.clone(),
                command: job.command.clone(),
                timeout: job.timeout.clone(),
                valid: config_error.is_none(),
                config_error,
                running: is_running(&vault_name, &job.name),
                last_run,
            }
        })
        .collect();

    Json(json!({ "vault": vault_name, "jobs": entries })).into_response()
}

/// `POST /api/v/{vault}/jobs/{name}/run`
pub async fn run_job(
    State(state): State<SharedAppState>,
    Path((vault_name, job_name)): Path<(String, String)>,
) -> Response {
    match trigger_job(state, &vault_name, &job_name).await {
        TriggerOutcome::Started => (
            StatusCode::ACCEPTED,
            Json(json!({
                "vault": vault_name,
                "job": job_name,
                "status": "started"
            })),
        )
            .into_response(),
        TriggerOutcome::AlreadyRunning => (
            StatusCode::CONFLICT,
            Json(json!({ "error": format!("job already running: {job_name}") })),
        )
            .into_response(),
        TriggerOutcome::UnknownVault => not_found(format!("vault not found: {vault_name}")),
        TriggerOutcome::UnknownJob => not_found(format!("job not found: {job_name}")),
        TriggerOutcome::NotRunnable(reason) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("job {job_name} cannot run: {reason}") })),
        )
            .into_response(),
    }
}

fn not_found(message: String) -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::server::{build_app_state, build_router, create_vault_state};
    use notesmith_config::GlobalConfig;

    fn write_executable(path: &Path, content: &str) {
        std::fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).unwrap();
        }
    }

    fn setup_vault(vault_toml: &str, scripts: &[(&str, &str)]) -> tempfile::TempDir {
        let vault = tempfile::TempDir::new().unwrap();
        let dir = vault.path().join(".notesmith");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vault.toml"), vault_toml).unwrap();
        for (name, content) in scripts {
            write_executable(&vault.path().join(name), content);
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

    async fn request(
        router: &axum::Router,
        method: &str,
        uri: &str,
    ) -> (StatusCode, serde_json::Value) {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    async fn wait_for<F: Fn() -> bool>(what: &str, predicate: F) {
        for _ in 0..100 {
            if predicate() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("timed out waiting for {what}");
    }

    #[tokio::test]
    async fn list_jobs_reports_schedule_validity_and_unknown_vault_404s() {
        let vault = setup_vault(
            r#"
name = "jobs-routes-list"

[[jobs]]
name = "calendar-sync"
every = "15m"
command = "sync.sh"

[[jobs]]
name = "broken"
every = "15m"
at = "07:30"
command = "sync.sh"

[[jobs]]
name = "agent-only"
at = "07:30"
agent = { prompt = "daily-note" }
"#,
            &[("sync.sh", "#!/bin/sh\nexit 0\n")],
        );
        let router = router_for("jobs-routes-list", vault.path());

        let (status, body) = request(&router, "GET", "/api/v/jobs-routes-list/jobs").await;
        assert_eq!(status, StatusCode::OK);
        let jobs = body["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 3);

        assert_eq!(jobs[0]["name"], "calendar-sync");
        assert_eq!(jobs[0]["valid"], true);
        assert_eq!(jobs[0]["running"], false);
        assert!(jobs[0].get("config_error").is_none());
        assert!(jobs[0].get("last_run").is_none());

        assert_eq!(jobs[1]["valid"], false);
        assert!(
            jobs[1]["config_error"]
                .as_str()
                .unwrap()
                .contains("mutually exclusive")
        );

        assert_eq!(jobs[2]["valid"], false);
        assert!(jobs[2]["config_error"].as_str().unwrap().contains("#282"));

        let (status, _) = request(&router, "GET", "/api/v/no-such-vault/jobs").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn run_job_triggers_manually_and_records_last_run() {
        let vault = setup_vault(
            r#"
name = "jobs-routes-run"

[[jobs]]
name = "toucher"
every = "1h"
command = "touch.sh"

[[jobs]]
name = "no-command"
at = "07:30"
"#,
            &[("touch.sh", "#!/bin/sh\ntouch manual-marker\n")],
        );
        let router = router_for("jobs-routes-run", vault.path());

        // Unknown job: 404.
        let (status, _) = request(&router, "POST", "/api/v/jobs-routes-run/jobs/nope/run").await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // No command: 400 with the reason.
        let (status, body) = request(
            &router,
            "POST",
            "/api/v/jobs-routes-run/jobs/no-command/run",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("command"));

        // Manual trigger: 202 and the job runs in the background.
        let (status, body) =
            request(&router, "POST", "/api/v/jobs-routes-run/jobs/toucher/run").await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body["status"], "started");

        let marker = vault.path().join("manual-marker");
        wait_for("manual job to run", || marker.exists()).await;

        // The run lands in the list's last_run once recorded.
        let mut last_status = String::new();
        for _ in 0..100 {
            let (_, body) = request(&router, "GET", "/api/v/jobs-routes-run/jobs").await;
            if let Some(last_run) = body["jobs"][0].get("last_run") {
                last_status = last_run["status"].as_str().unwrap_or("").to_string();
                if !last_status.is_empty() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(last_status, "succeeded");
    }

    #[tokio::test]
    async fn run_job_conflicts_while_already_running() {
        let vault = setup_vault(
            r#"
name = "jobs-routes-busy"

[[jobs]]
name = "sleeper"
every = "1h"
command = "sleep.sh"
timeout = "30s"
"#,
            &[("sleep.sh", "#!/bin/sh\nsleep 3\n")],
        );
        let router = router_for("jobs-routes-busy", vault.path());

        let (status, _) =
            request(&router, "POST", "/api/v/jobs-routes-busy/jobs/sleeper/run").await;
        assert_eq!(status, StatusCode::ACCEPTED);

        // The reservation is taken synchronously before 202 returns, so an
        // immediate second trigger deterministically conflicts.
        let (status, body) =
            request(&router, "POST", "/api/v/jobs-routes-busy/jobs/sleeper/run").await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("already running"));

        // And the list shows it running.
        let (_, body) = request(&router, "GET", "/api/v/jobs-routes-busy/jobs").await;
        assert_eq!(body["jobs"][0]["running"], true);
    }
}
