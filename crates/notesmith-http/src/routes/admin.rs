use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    events::{self, EventType, VaultEvent},
    server::SharedAppState,
};

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    #[serde(default = "default_tail")]
    pub tail: usize,
}

fn default_tail() -> usize {
    200
}

pub async fn shutdown(State(state): State<SharedAppState>) -> StatusCode {
    let (vault_names, event_tx, event_buffer, shutdown_tx) = {
        let state = state.read().await;
        (
            state.vaults.keys().cloned().collect::<Vec<_>>(),
            state.event_tx.clone(),
            state.event_buffer.clone(),
            state.shutdown_tx.clone(),
        )
    };

    for vault_name in vault_names {
        events::emit(
            &event_tx,
            &event_buffer,
            VaultEvent::new(vault_name, EventType::ShuttingDown, ""),
        );
    }

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = shutdown_tx.send(true);
    });

    StatusCode::OK
}

pub async fn restart(State(state): State<SharedAppState>) -> StatusCode {
    shutdown(State(state)).await
}

pub async fn get_logs(Query(query): Query<LogsQuery>) -> Response {
    let tail = query.tail.min(1000);
    let Some(path) = crate::logging::current_log_path() else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let body = if tail == 0 {
        String::new()
    } else {
        let lines = contents.lines().collect::<Vec<_>>();
        let start = lines.len().saturating_sub(tail);
        let mut body = lines[start..].join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        body
    };

    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn get_logs_returns_recent_lines_from_daemon_log() {
        let _guard = crate::logging::test_log_lock().lock().unwrap();
        let test_dir = TestLogDir::new();
        let logs_dir = test_dir.path().to_path_buf();
        fs::create_dir_all(&logs_dir).unwrap();
        fs::write(
            logs_dir.join(format!(
                "daemon.log.{}",
                chrono::Utc::now().date_naive().format("%Y-%m-%d")
            )),
            "one\ntwo\nthree\n",
        )
        .unwrap();

        let response = crate::server::build_router(crate::server::AppState::default())
            .oneshot(
                Request::builder()
                    .uri("/admin/logs?tail=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "text/plain; charset=utf-8"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(String::from_utf8(body.to_vec()).unwrap(), "two\nthree\n");
    }

    #[tokio::test]
    async fn get_logs_returns_not_found_when_no_log_file_exists() {
        let _guard = crate::logging::test_log_lock().lock().unwrap();
        let _test_dir = TestLogDir::new();

        let response = crate::server::build_router(crate::server::AppState::default())
            .oneshot(
                Request::builder()
                    .uri("/admin/logs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    struct TestLogDir {
        path: PathBuf,
    }

    impl TestLogDir {
        fn new() -> Self {
            let path = test_log_dir();
            fs::create_dir_all(&path).unwrap();
            crate::logging::set_test_log_dir_override(Some(path.clone()));

            Self { path }
        }

        fn path(&self) -> &PathBuf {
            &self.path
        }
    }

    impl Drop for TestLogDir {
        fn drop(&mut self) {
            crate::logging::set_test_log_dir_override(None);
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_log_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-artifacts")
            .join(format!("admin-logs-{unique}-{}", std::process::id()))
    }
}
