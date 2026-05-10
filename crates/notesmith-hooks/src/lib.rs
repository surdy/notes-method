//! notesmith-hooks: Subprocess hook runner for note lifecycle events

use std::{path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    OnNoteCreate,
    OnDailyCreate,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::OnNoteCreate => "on_note_create",
            HookEvent::OnDailyCreate => "on_daily_create",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    pub event: String,
    pub vault: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug)]
pub struct HookResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("hook script not found: {path}")]
    ScriptNotFound { path: String },
    #[error("hook timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },
    #[error("hook execution failed: {source}")]
    ExecutionFailed { source: std::io::Error },
    #[error("failed to write payload to hook stdin: {source}")]
    StdinWriteFailed { source: std::io::Error },
}

#[derive(Debug, Clone)]
pub struct HookRunner {
    pub timeout: Duration,
}

impl Default for HookRunner {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }
}

impl HookRunner {
    pub async fn run_hook(
        &self,
        script_path: &Path,
        working_dir: &Path,
        payload: &HookPayload,
    ) -> Result<HookResult, HookError> {
        if !script_path.exists() {
            return Err(HookError::ScriptNotFound {
                path: script_path.display().to_string(),
            });
        }

        let payload_json =
            serde_json::to_string(payload).expect("payload serialization cannot fail");
        let mut child = build_command(script_path, working_dir)
            .spawn()
            .map_err(|source| HookError::ExecutionFailed { source })?;

        let mut stdin = child
            .stdin
            .take()
            .expect("hook command stdin should be piped");
        stdin
            .write_all(payload_json.as_bytes())
            .await
            .map_err(|source| HookError::StdinWriteFailed { source })?;
        drop(stdin);

        let stdout_task = read_stream(
            child
                .stdout
                .take()
                .expect("hook command stdout should be piped"),
        );
        let stderr_task = read_stream(
            child
                .stderr
                .take()
                .expect("hook command stderr should be piped"),
        );

        let status = match tokio::time::timeout(self.timeout, child.wait()).await {
            Ok(wait_result) => {
                wait_result.map_err(|source| HookError::ExecutionFailed { source })?
            }
            Err(_) => {
                kill_child_processes(&mut child).await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(HookError::Timeout {
                    timeout_secs: self.timeout.as_secs(),
                });
            }
        };

        let stdout = stdout_task
            .await
            .map_err(|join_error| HookError::ExecutionFailed {
                source: std::io::Error::other(join_error),
            })?
            .map_err(|source| HookError::ExecutionFailed { source })?;
        let stderr = stderr_task
            .await
            .map_err(|join_error| HookError::ExecutionFailed {
                source: std::io::Error::other(join_error),
            })?
            .map_err(|source| HookError::ExecutionFailed { source })?;

        if !stderr.trim().is_empty() {
            tracing::debug!(stderr = %stderr.trim(), "hook stderr");
        }

        Ok(HookResult {
            exit_code: status.code(),
            stdout,
            stderr,
        })
    }
}

fn read_stream<R>(mut stream: R) -> tokio::task::JoinHandle<std::io::Result<String>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).await?;
        Ok(String::from_utf8_lossy(&buffer).into_owned())
    })
}

async fn kill_child_processes(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(id) = child.id() {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{id}")])
            .status();
    }

    let _ = child.start_kill();
}

fn build_command(script_path: &Path, working_dir: &Path) -> Command {
    let ext = script_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    let mut command = match ext {
        "py" => {
            let mut command = Command::new("python3");
            command.arg(script_path);
            command
        }
        "sh" => {
            let mut command = Command::new("sh");
            command.arg(script_path);
            command
        }
        "js" => {
            let mut command = Command::new("node");
            command.arg(script_path);
            command
        }
        _ => Command::new(script_path),
    };

    #[cfg(unix)]
    command.process_group(0);

    command
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Fire a hook asynchronously. Failures are logged but never propagated.
pub async fn fire_hook(
    runner: &HookRunner,
    vault_root: &Path,
    script_relative: &str,
    payload: HookPayload,
) {
    let script_path = vault_root.join(script_relative);
    match runner.run_hook(&script_path, vault_root, &payload).await {
        Ok(result) => {
            if let Some(code) = result.exit_code {
                if code != 0 {
                    tracing::warn!(
                        hook = payload.event,
                        exit_code = code,
                        stderr = %result.stderr.trim(),
                        "hook exited with non-zero status"
                    );
                } else {
                    tracing::debug!(hook = payload.event, "hook completed successfully");
                }
            }
            if !result.stderr.is_empty() {
                tracing::debug!(hook = payload.event, stderr = %result.stderr.trim(), "hook stderr");
            }
        }
        Err(error) => {
            tracing::warn!(hook = payload.event, error = %error, "hook failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, time::Duration};

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn payload_serializes_to_expected_json() {
        let payload = HookPayload {
            event: HookEvent::OnNoteCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Inbox/Test.md".to_string(),
            frontmatter: None,
            source: Some("http".to_string()),
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(
            json,
            json!({
                "event": "on_note_create",
                "vault": "work",
                "path": "Inbox/Test.md",
                "source": "http"
            })
        );
    }

    #[test]
    fn payload_with_frontmatter_includes_it() {
        let payload = HookPayload {
            event: HookEvent::OnDailyCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Inbox/Daily/2026-05-10.md".to_string(),
            frontmatter: Some(json!({ "type": "daily", "date": "2026-05-10" })),
            source: None,
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(
            json["frontmatter"],
            json!({ "type": "daily", "date": "2026-05-10" })
        );
        assert!(json.get("source").is_none());
    }

    #[tokio::test]
    async fn run_hook_script_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let runner = HookRunner::default();
        let payload = HookPayload {
            event: HookEvent::OnNoteCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Inbox/Test.md".to_string(),
            frontmatter: None,
            source: None,
        };

        let error = runner
            .run_hook(
                &temp_dir.path().join("missing.sh"),
                temp_dir.path(),
                &payload,
            )
            .await
            .unwrap_err();

        match error {
            HookError::ScriptNotFound { path } => assert!(path.ends_with("missing.sh")),
            other => panic!("expected ScriptNotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_hook_executes_script_and_captures_output() {
        let temp_dir = TempDir::new().unwrap();
        let marker = temp_dir.path().join("hook-output.json");
        let script_path = temp_dir.path().join("hook.sh");
        write_executable(
            &script_path,
            &format!(
                "#!/bin/sh\ncat | tee '{}'\necho 'hook ok'\n",
                escape_single_quotes(&marker)
            ),
        );

        let runner = HookRunner::default();
        let payload = HookPayload {
            event: HookEvent::OnNoteCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Inbox/Test.md".to_string(),
            frontmatter: None,
            source: Some("http".to_string()),
        };

        let result = runner
            .run_hook(&script_path, temp_dir.path(), &payload)
            .await
            .unwrap();

        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(marker).unwrap()).unwrap();
        assert_eq!(written["event"], json!("on_note_create"));
        assert_eq!(written["path"], json!("Inbox/Test.md"));
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hook ok"));
    }

    #[tokio::test]
    async fn run_hook_timeout_kills_subprocess() {
        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("sleep.sh");
        write_executable(&script_path, "#!/bin/sh\nsleep 60\n");

        let runner = HookRunner {
            timeout: Duration::from_secs(1),
        };
        let payload = HookPayload {
            event: HookEvent::OnNoteCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Inbox/Test.md".to_string(),
            frontmatter: None,
            source: None,
        };

        let start = std::time::Instant::now();
        let error = runner
            .run_hook(&script_path, temp_dir.path(), &payload)
            .await
            .unwrap_err();

        assert!(matches!(error, HookError::Timeout { timeout_secs: 1 }));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn run_hook_captures_stderr() {
        let temp_dir = TempDir::new().unwrap();
        let script_path = temp_dir.path().join("stderr.sh");
        write_executable(&script_path, "#!/bin/sh\necho 'warn me' >&2\n");

        let runner = HookRunner::default();
        let payload = HookPayload {
            event: HookEvent::OnDailyCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Inbox/Daily/2026-05-10.md".to_string(),
            frontmatter: None,
            source: None,
        };

        let result = runner
            .run_hook(&script_path, temp_dir.path(), &payload)
            .await
            .unwrap();

        assert_eq!(result.exit_code, Some(0));
        assert!(result.stderr.contains("warn me"));
    }

    #[test]
    fn build_command_uses_python_for_py_files() {
        let command = build_command(Path::new("hook.py"), Path::new("."));
        let std = command.as_std();
        assert_eq!(std.get_program().to_string_lossy(), "python3");
        assert_eq!(std.get_args().next().unwrap().to_string_lossy(), "hook.py");
    }

    #[test]
    fn build_command_uses_sh_for_sh_files() {
        let command = build_command(Path::new("hook.sh"), Path::new("."));
        let std = command.as_std();
        assert_eq!(std.get_program().to_string_lossy(), "sh");
        assert_eq!(std.get_args().next().unwrap().to_string_lossy(), "hook.sh");
    }

    #[test]
    fn build_command_uses_node_for_js_files() {
        let command = build_command(Path::new("hook.js"), Path::new("."));
        let std = command.as_std();
        assert_eq!(std.get_program().to_string_lossy(), "node");
        assert_eq!(std.get_args().next().unwrap().to_string_lossy(), "hook.js");
    }

    fn write_executable(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    fn escape_single_quotes(path: &Path) -> String {
        path.display().to_string().replace('\'', "'\\''")
    }
}
