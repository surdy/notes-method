//! notesmith-hooks: Subprocess hook runner for note lifecycle events

use std::{path::Path, process::Stdio, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

/// The six lifecycle events that can trigger hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    OnNoteCreate,
    OnNoteUpdate,
    OnNoteRoute,
    OnPeriodicCreate,
    OnTaskChange,
    OnFieldChange,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::OnNoteCreate => "on_note_create",
            HookEvent::OnNoteUpdate => "on_note_update",
            HookEvent::OnNoteRoute => "on_note_route",
            HookEvent::OnPeriodicCreate => "on_periodic_create",
            HookEvent::OnTaskChange => "on_task_change",
            HookEvent::OnFieldChange => "on_field_change",
        }
    }
}

impl std::str::FromStr for HookEvent {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "on_note_create" => Ok(Self::OnNoteCreate),
            "on_note_update" => Ok(Self::OnNoteUpdate),
            "on_note_route" => Ok(Self::OnNoteRoute),
            "on_periodic_create" => Ok(Self::OnPeriodicCreate),
            "on_task_change" => Ok(Self::OnTaskChange),
            "on_field_change" => Ok(Self::OnFieldChange),
            _ => Err("unknown hook event"),
        }
    }
}

/// Base payload sent to all hooks via stdin as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    pub event: String,
    pub vault: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// For on_note_route: the routing rule that matched
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// For on_note_route: original path before routing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_path: Option<String>,
    /// For on_note_route: destination path after routing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_path: Option<String>,
    /// For on_note_route: mutations applied
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutations: Option<serde_json::Value>,
    /// For on_periodic_create: the period kind (daily, weekly, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_kind: Option<String>,
    /// For on_periodic_create: the period key (2026-05-23, 2026-W21, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_key: Option<String>,
    /// For on_task_change: old status character
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_status: Option<String>,
    /// For on_task_change: new status character
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_status: Option<String>,
    /// For on_task_change: task text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_text: Option<String>,
    /// For on_field_change: batched field changes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<FieldChange>>,
}

/// A single field change within an on_field_change batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub key: String,
    pub action: FieldChangeAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
}

/// Discriminator for field change actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldChangeAction {
    Add,
    Change,
    Remove,
}

/// Configuration for a hook registration in vault.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub command: String,
    /// For on_field_change: only fire when these fields change
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_fields: Option<Vec<String>>,
}

/// Compute field changes between old and new field sets.
/// Returns only changes for keys in `watch_fields` (if Some), or all changes (if None).
pub fn diff_fields(
    old_fields: &std::collections::HashMap<String, String>,
    new_fields: &std::collections::HashMap<String, String>,
    watch_fields: Option<&[String]>,
) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    // Check for additions and changes
    for (key, new_value) in new_fields {
        if let Some(watch) = watch_fields {
            if !watch.iter().any(|w| w == key) {
                continue;
            }
        }
        match old_fields.get(key) {
            None => changes.push(FieldChange {
                key: key.clone(),
                action: FieldChangeAction::Add,
                old: None,
                new: Some(new_value.clone()),
            }),
            Some(old_value) if old_value != new_value => changes.push(FieldChange {
                key: key.clone(),
                action: FieldChangeAction::Change,
                old: Some(old_value.clone()),
                new: Some(new_value.clone()),
            }),
            _ => {}
        }
    }

    // Check for removals
    for (key, old_value) in old_fields {
        if let Some(watch) = watch_fields {
            if !watch.iter().any(|w| w == key) {
                continue;
            }
        }
        if !new_fields.contains_key(key) {
            changes.push(FieldChange {
                key: key.clone(),
                action: FieldChangeAction::Remove,
                old: Some(old_value.clone()),
                new: None,
            });
        }
    }

    changes
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
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
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
            event: HookEvent::OnPeriodicCreate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Daily/2026-05-10.md".to_string(),
            frontmatter: Some(json!({ "date": "2026-05-10" })),
            source: None,
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: Some("daily".to_string()),
            period_key: Some("2026-05-10".to_string()),
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["frontmatter"], json!({ "date": "2026-05-10" }));
        assert_eq!(json["period_kind"], json!("daily"));
        assert_eq!(json["period_key"], json!("2026-05-10"));
        assert!(json.get("source").is_none());
    }

    #[test]
    fn route_payload_includes_routing_fields() {
        let payload = HookPayload {
            event: HookEvent::OnNoteRoute.as_str().to_string(),
            vault: "work".to_string(),
            path: "Customers/Acme/meeting.md".to_string(),
            frontmatter: None,
            source: None,
            rule_id: Some("route-meetings".to_string()),
            from_path: Some("Inbox/meeting.md".to_string()),
            to_path: Some("Customers/Acme/meeting.md".to_string()),
            mutations: Some(json!({"set_fields": {"status": "filed"}})),
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["rule_id"], json!("route-meetings"));
        assert_eq!(json["from_path"], json!("Inbox/meeting.md"));
        assert_eq!(json["to_path"], json!("Customers/Acme/meeting.md"));
    }

    #[test]
    fn task_change_payload() {
        let payload = HookPayload {
            event: HookEvent::OnTaskChange.as_str().to_string(),
            vault: "work".to_string(),
            path: "Tasks/todo.md".to_string(),
            frontmatter: None,
            source: None,
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: Some(" ".to_string()),
            new_status: Some("x".to_string()),
            task_text: Some("Complete the report".to_string()),
            changes: None,
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["old_status"], json!(" "));
        assert_eq!(json["new_status"], json!("x"));
        assert_eq!(json["task_text"], json!("Complete the report"));
    }

    #[test]
    fn field_change_payload() {
        let payload = HookPayload {
            event: HookEvent::OnFieldChange.as_str().to_string(),
            vault: "work".to_string(),
            path: "Streams/foo.md".to_string(),
            frontmatter: None,
            source: None,
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: Some(vec![
                FieldChange {
                    key: "status".to_string(),
                    action: FieldChangeAction::Change,
                    old: Some("active".to_string()),
                    new: Some("blocked".to_string()),
                },
                FieldChange {
                    key: "priority".to_string(),
                    action: FieldChangeAction::Add,
                    old: None,
                    new: Some("P0".to_string()),
                },
            ]),
        };

        let json = serde_json::to_value(&payload).unwrap();
        let changes = json["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0]["key"], json!("status"));
        assert_eq!(changes[0]["action"], json!("change"));
        assert_eq!(changes[0]["old"], json!("active"));
        assert_eq!(changes[0]["new"], json!("blocked"));
        assert_eq!(changes[1]["action"], json!("add"));
    }

    #[test]
    fn diff_fields_detects_add_change_remove() {
        use std::collections::HashMap;
        let mut old: HashMap<String, String> = HashMap::new();
        old.insert("status".into(), "active".into());
        old.insert("owner".into(), "me".into());

        let mut new: HashMap<String, String> = HashMap::new();
        new.insert("status".into(), "blocked".into());
        new.insert("priority".into(), "P0".into());

        let changes = diff_fields(&old, &new, None);
        assert_eq!(changes.len(), 3);

        let status_change = changes.iter().find(|c| c.key == "status").unwrap();
        assert_eq!(status_change.action, FieldChangeAction::Change);
        assert_eq!(status_change.old.as_deref(), Some("active"));
        assert_eq!(status_change.new.as_deref(), Some("blocked"));

        let priority_add = changes.iter().find(|c| c.key == "priority").unwrap();
        assert_eq!(priority_add.action, FieldChangeAction::Add);

        let owner_remove = changes.iter().find(|c| c.key == "owner").unwrap();
        assert_eq!(owner_remove.action, FieldChangeAction::Remove);
    }

    #[test]
    fn diff_fields_respects_watch_fields() {
        use std::collections::HashMap;
        let mut old: HashMap<String, String> = HashMap::new();
        old.insert("status".into(), "active".into());
        old.insert("owner".into(), "me".into());

        let mut new: HashMap<String, String> = HashMap::new();
        new.insert("status".into(), "blocked".into());
        new.insert("priority".into(), "P0".into());

        let watch = vec!["status".to_string()];
        let changes = diff_fields(&old, &new, Some(&watch));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key, "status");
        assert_eq!(changes[0].action, FieldChangeAction::Change);
    }

    #[test]
    fn hook_event_from_str_roundtrips() {
        let events = [
            HookEvent::OnNoteCreate,
            HookEvent::OnNoteUpdate,
            HookEvent::OnNoteRoute,
            HookEvent::OnPeriodicCreate,
            HookEvent::OnTaskChange,
            HookEvent::OnFieldChange,
        ];
        for event in events {
            let s = event.as_str();
            let parsed = s.parse::<HookEvent>().unwrap();
            assert_eq!(parsed, event);
        }
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
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
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
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
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
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
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
            event: HookEvent::OnNoteUpdate.as_str().to_string(),
            vault: "work".to_string(),
            path: "Notes/test.md".to_string(),
            frontmatter: None,
            source: None,
            rule_id: None,
            from_path: None,
            to_path: None,
            mutations: None,
            period_kind: None,
            period_key: None,
            old_status: None,
            new_status: None,
            task_text: None,
            changes: None,
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
