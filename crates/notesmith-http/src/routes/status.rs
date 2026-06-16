use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use axum::{Json, extract::State};
use chrono::{DateTime, SecondsFormat, Utc};
use notesmith_index::VaultCache;
use serde::Serialize;

use crate::{
    API_SCHEMA_VERSION,
    server::{SharedAppState, VaultState},
};

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    status: &'static str,
    version: &'static str,
    api_schema: u32,
    pid: u32,
    started_at: DateTime<Utc>,
    binary_path: String,
    vaults: Vec<VaultStatus>,
    watchers: Vec<WatcherStatus>,
    indexes: Vec<IndexStatus>,
    resources: ResourceStatus,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct VaultStatus {
    name: String,
    state: &'static str,
    notes: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct WatcherStatus {
    vault: String,
    state: String,
    message: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct IndexStatus {
    vault: String,
    state: &'static str,
    last_reindex: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ResourceStatus {
    rss_bytes: u64,
    open_fds: u64,
    sse_connections: usize,
    cache_size_bytes: u64,
}

pub async fn get_status(State(state): State<SharedAppState>) -> Json<StatusResponse> {
    let state = state.read().await;
    let started_at = state.started_at;
    let sse_connections = state.sse_connection_count.load(Ordering::Relaxed);

    let mut vault_names: Vec<_> = state.vaults.keys().cloned().collect();
    vault_names.sort();

    let vaults = vault_names
        .iter()
        .map(|vault_name| {
            let vault = &state.vaults[vault_name];
            let rebuilding = vault.rebuilding.load(Ordering::Relaxed);
            VaultStatus {
                name: vault_name.clone(),
                state: if rebuilding { "rebuilding" } else { "ready" },
                notes: if rebuilding {
                    0
                } else {
                    note_count(&vault.cache, vault_name)
                },
            }
        })
        .collect();

    let watchers = vault_names
        .iter()
        .map(|vault_name| {
            let vault = &state.vaults[vault_name];
            WatcherStatus {
                vault: vault_name.clone(),
                state: vault.watcher_state.health().as_str().to_string(),
                message: vault.watcher_state.message(),
            }
        })
        .collect();

    let indexes = vault_names
        .iter()
        .map(|vault_name| {
            let vault = &state.vaults[vault_name];
            IndexStatus {
                vault: vault_name.clone(),
                state: "healthy",
                last_reindex: last_reindex_timestamp(vault, started_at),
            }
        })
        .collect();

    let cache_size_bytes = vault_names
        .iter()
        .map(|vault_name| cache_size_for_vault(&state.vaults[vault_name]))
        .sum();

    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        api_schema: API_SCHEMA_VERSION,
        pid: std::process::id(),
        started_at,
        binary_path: current_binary_path(),
        vaults,
        watchers,
        indexes,
        resources: ResourceStatus {
            rss_bytes: rss_bytes(),
            open_fds: open_fd_count(),
            sse_connections,
            cache_size_bytes,
        },
    })
}

fn note_count(cache: &VaultCache, vault_name: &str) -> u64 {
    cache
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM notes WHERE vault_name = ?1",
            [vault_name],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .and_then(|count| u64::try_from(count).ok())
        .unwrap_or(0)
}

fn last_reindex_timestamp(vault: &VaultState, started_at: DateTime<Utc>) -> String {
    fs::metadata(vault.cache.cache_path())
        .and_then(|metadata| metadata.modified())
        .ok()
        .map(DateTime::<Utc>::from)
        .unwrap_or(started_at)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn cache_size_for_vault(vault: &VaultState) -> u64 {
    sqlite_artifact_paths(vault.cache.cache_path())
        .into_iter()
        .filter_map(file_size)
        .sum()
}

fn sqlite_artifact_paths(cache_path: &Path) -> Vec<PathBuf> {
    if cache_path == Path::new(":memory:") {
        return Vec::new();
    }

    let base = cache_path.to_string_lossy();
    vec![
        cache_path.to_path_buf(),
        PathBuf::from(format!("{base}-wal")),
        PathBuf::from(format!("{base}-shm")),
    ]
}

fn file_size(path: PathBuf) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

fn current_binary_path() -> String {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| std::env::args().next().unwrap_or_default())
}

fn rss_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage writes the rusage struct on success and does not retain the pointer.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return 0;
    }

    // SAFETY: getrusage succeeded, so the struct is initialized.
    let usage = unsafe { usage.assume_init() };
    let max_rss = usage.ru_maxrss;
    if max_rss <= 0 {
        0
    } else if cfg!(target_os = "linux") {
        (max_rss as u64) * 1024
    } else {
        max_rss as u64
    }
}

fn open_fd_count() -> u64 {
    ["/proc/self/fd", "/dev/fd"]
        .into_iter()
        .find_map(|path| fs::read_dir(path).ok())
        .map(|entries| entries.count() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::{TimeZone, Utc};
    use notesmith_config::{VaultConfig, migration};
    use notesmith_core::VaultEngine;
    use notesmith_index::{SearchIndex, VaultCache};
    use notesmith_vault::NativeVaultEngine;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::{
        API_SCHEMA_VERSION,
        events::create_event_channel,
        server::{AppState, VaultState},
        watcher::{WatcherHealth, WatcherState},
    };

    #[tokio::test]
    async fn get_status_reports_runtime_and_vault_diagnostics() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(
            vault_root.join("Inbox/Status Test.md"),
            "# Status Test\n\nHealth check note.\n",
        )
        .unwrap();

        let cache_path = temp_dir.path().join("cache.sqlite");
        let response = crate::server::build_router(build_test_state(
            "work",
            &vault_root,
            &cache_path,
            Utc.with_ymd_and_hms(2026, 5, 14, 19, 0, 0).unwrap(),
        ))
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["api_schema"], API_SCHEMA_VERSION);
        assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(payload["pid"], std::process::id());
        assert_eq!(payload["started_at"], "2026-05-14T19:00:00Z");
        assert_eq!(
            payload["binary_path"],
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(
            payload["vaults"],
            json!([{ "name": "work", "state": "ready", "notes": 1 }])
        );
        assert_eq!(
            payload["watchers"],
            json!([{ "vault": "work", "state": "healthy", "message": null }])
        );
        assert_eq!(payload["indexes"][0]["vault"], "work");
        assert_eq!(payload["indexes"][0]["state"], "healthy");
        assert!(payload["indexes"][0]["last_reindex"].as_str().is_some());
        assert!(payload["resources"]["rss_bytes"].as_u64().unwrap() > 0);
        assert!(payload["resources"]["open_fds"].as_u64().unwrap() > 0);
        assert_eq!(payload["resources"]["sse_connections"], 0);
        assert!(payload["resources"]["cache_size_bytes"].as_u64().unwrap() > 0);
    }

    #[test]
    fn app_state_sse_counter_defaults_to_zero() {
        let state = AppState::default();
        assert_eq!(state.sse_connection_count.load(Ordering::Relaxed), 0);
    }

    fn build_test_state(
        vault_name: &str,
        vault_root: &Path,
        cache_path: &Path,
        started_at: chrono::DateTime<Utc>,
    ) -> AppState {
        let engine = NativeVaultEngine;
        let notes = engine.scan(vault_root).unwrap();
        let vault_config =
            migration::load_and_migrate(vault_root).unwrap_or_else(|_| VaultConfig {
                name: vault_name.to_string(),
                ..Default::default()
            });
        let cache = VaultCache::open(cache_path).unwrap();
        cache
            .reindex_with_periodic(vault_name, &notes, &vault_config.periodic)
            .unwrap();
        let search_index = SearchIndex::open_in_memory().unwrap();
        search_index.reindex(vault_name, &notes).unwrap();

        let (event_tx, _) = create_event_channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        AppState {
            vaults: HashMap::from([(
                vault_name.to_string(),
                VaultState {
                    cache: Arc::new(cache),
                    search_index: Arc::new(search_index),
                    engine,
                    root: vault_root.to_path_buf(),
                    vault_config: arc_swap::ArcSwap::from_pointee(default_vault_config(vault_name)),
                    watcher_state: WatcherState::new(),
                    rebuilding: std::sync::atomic::AtomicBool::new(false),
                    template_engine: Arc::new(notesmith_templates::TemplateEngine::new(
                        vault_root.to_path_buf(),
                        Some(PathBuf::from(cache_path)),
                    )),
                },
            )]),
            event_tx,
            event_buffer: Arc::new(crate::events::EventBuffer::new(
                crate::events::EVENT_BUFFER_CAPACITY,
            )),
            global_config_path: vault_root.join(".notesmith-http-test-config.toml"),
            started_at,
            sse_connection_count: Arc::new(AtomicUsize::new(0)),
            shutdown_tx,
            shutdown_rx,
            mcp_services: Default::default(),
            transcripts: Default::default(),
            permissions: Default::default(),
        }
    }

    fn default_vault_config(vault_name: &str) -> VaultConfig {
        VaultConfig {
            name: vault_name.to_string(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn get_status_reports_rebuilding_vault_state() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(&vault_root).unwrap();
        let cache_path = temp_dir.path().join("cache.sqlite");
        let state = build_test_state(
            "work",
            &vault_root,
            &cache_path,
            Utc.with_ymd_and_hms(2026, 5, 14, 19, 0, 0).unwrap(),
        );
        state
            .vaults
            .get("work")
            .unwrap()
            .rebuilding
            .store(true, Ordering::Relaxed);

        let response = crate::server::build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            payload["vaults"],
            json!([{ "name": "work", "state": "rebuilding", "notes": 0 }])
        );
    }

    #[tokio::test]
    async fn get_status_reports_watcher_health_and_message() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_root = temp_dir.path().join("vault");
        fs::create_dir_all(vault_root.join("Inbox")).unwrap();
        fs::write(vault_root.join("Inbox/Status Test.md"), "# Status Test\n").unwrap();

        let cache_path = temp_dir.path().join("cache.sqlite");
        let mut app_state = build_test_state(
            "work",
            &vault_root,
            &cache_path,
            Utc.with_ymd_and_hms(2026, 5, 14, 19, 0, 0).unwrap(),
        );
        app_state
            .vaults
            .get_mut("work")
            .unwrap()
            .watcher_state
            .set_health(
                WatcherHealth::Polling,
                Some("Network drive detected — updates may take up to 30s".to_string()),
            );

        let response = crate::server::build_router(app_state)
            .oneshot(
                Request::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            payload["watchers"],
            json!([{
                "vault": "work",
                "state": "polling",
                "message": "Network drive detected — updates may take up to 30s"
            }])
        );
    }
}
