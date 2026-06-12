//! ACP client-side capability handlers: scoped filesystem and terminal proxies.
//!
//! When the embedded agent is granted local I/O (the opt-in
//! `agent.local_file_access` setting, ADR 0012), Notesmith answers the agent's
//! `fs/*` and `terminal/*` requests itself rather than rejecting them. Every
//! path is **scoped to the vault directory** and writes/terminals are refused in
//! read-only sessions, so enabling local I/O never escapes the active vault or
//! defeats the read-only scope.
//!
//! Per ADR 0009 every handler is tolerant: malformed params yield a JSON-RPC
//! error response, never a panic.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify};

use crate::acp::permission_result;

/// Default cap on retained terminal output (1 MiB) when the agent does not set
/// `outputByteLimit`.
const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;

/// JSON-RPC error code for "method not found".
const METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC error code for "invalid params".
const INVALID_PARAMS: i64 = -32602;
/// JSON-RPC error code for "internal error".
const INTERNAL_ERROR: i64 = -32603;

/// Handles inbound JSON-RPC requests an agent makes back to the client
/// (permission prompts plus, when local I/O is enabled, filesystem and
/// terminal proxying scoped to the vault directory).
pub(crate) struct ClientHandler {
    read_only: bool,
    local_io: bool,
    cwd: PathBuf,
    terminals: Mutex<HashMap<String, Terminal>>,
    next_terminal: AtomicU64,
}

impl ClientHandler {
    /// Build a handler for a session rooted at `cwd` (the active vault's
    /// absolute directory). `local_io` enables the `fs/*` and `terminal/*`
    /// proxies; `read_only` refuses writes and terminals.
    pub(crate) fn new(read_only: bool, local_io: bool, cwd: PathBuf) -> Self {
        Self {
            read_only,
            local_io,
            cwd,
            terminals: Mutex::new(HashMap::new()),
            next_terminal: AtomicU64::new(1),
        }
    }

    /// Route an inbound agent request to a JSON-RPC response.
    pub(crate) async fn handle(&self, method: &str, params: Option<&Value>, id: Value) -> Value {
        match method {
            "session/request_permission" => ok(
                id,
                permission_result(params.unwrap_or(&Value::Null), self.read_only),
            ),
            "fs/read_text_file" if self.local_io => self.fs_read(params, id),
            "fs/write_text_file" if self.local_io => self.fs_write(params, id),
            "terminal/create" if self.local_io => self.terminal_create(params, id).await,
            "terminal/output" if self.local_io => self.terminal_output(params, id).await,
            "terminal/wait_for_exit" if self.local_io => self.terminal_wait(params, id).await,
            "terminal/kill" if self.local_io => self.terminal_kill(params, id).await,
            "terminal/release" if self.local_io => self.terminal_release(params, id).await,
            // No capability advertised for this method: report "method not
            // found" rather than hanging the agent (ADR 0011).
            _ => err(
                id,
                METHOD_NOT_FOUND,
                format!("method not handled: {method}"),
            ),
        }
    }

    fn fs_read(&self, params: Option<&Value>, id: Value) -> Value {
        let params = params.unwrap_or(&Value::Null);
        let raw = match params.get("path").and_then(Value::as_str) {
            Some(p) => p,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "fs/read_text_file requires `path`".into(),
                );
            }
        };
        let path = match self.resolve_within_vault(raw) {
            Ok(path) => path,
            Err(message) => return err(id, INVALID_PARAMS, message),
        };
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let line = params.get("line").and_then(Value::as_u64);
                let limit = params.get("limit").and_then(Value::as_u64);
                ok(id, json!({ "content": slice_lines(&content, line, limit) }))
            }
            Err(error) => err(id, INTERNAL_ERROR, format!("read failed: {error}")),
        }
    }

    fn fs_write(&self, params: Option<&Value>, id: Value) -> Value {
        if self.read_only {
            return err(id, INTERNAL_ERROR, "session is read-only".into());
        }
        let params = params.unwrap_or(&Value::Null);
        let raw = match params.get("path").and_then(Value::as_str) {
            Some(p) => p,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "fs/write_text_file requires `path`".into(),
                );
            }
        };
        let content = params
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let path = match self.resolve_within_vault(raw) {
            Ok(path) => path,
            Err(message) => return err(id, INVALID_PARAMS, message),
        };
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return err(id, INTERNAL_ERROR, format!("write failed: {error}"));
            }
        }
        match std::fs::write(&path, content) {
            Ok(()) => ok(id, Value::Null),
            Err(error) => err(id, INTERNAL_ERROR, format!("write failed: {error}")),
        }
    }

    async fn terminal_create(&self, params: Option<&Value>, id: Value) -> Value {
        if self.read_only {
            return err(
                id,
                INTERNAL_ERROR,
                "session is read-only: terminal disabled".into(),
            );
        }
        let params = params.unwrap_or(&Value::Null);
        let command = match params.get("command").and_then(Value::as_str) {
            Some(c) => c,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "terminal/create requires `command`".into(),
                );
            }
        };
        let args: Vec<String> = params
            .get("args")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let cwd = match params.get("cwd").and_then(Value::as_str) {
            Some(raw) => match self.resolve_within_vault(raw) {
                Ok(path) => path,
                Err(message) => return err(id, INVALID_PARAMS, message),
            },
            None => self.cwd.clone(),
        };
        let limit = params
            .get("outputByteLimit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_OUTPUT_LIMIT);

        let mut builder = Command::new(command);
        builder
            .args(&args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(env) = params.get("env").and_then(Value::as_array) {
            for entry in env {
                if let (Some(name), Some(value)) = (
                    entry.get("name").and_then(Value::as_str),
                    entry.get("value").and_then(Value::as_str),
                ) {
                    builder.env(name, value);
                }
            }
        }

        let mut child = match builder.spawn() {
            Ok(child) => child,
            Err(error) => {
                return err(
                    id,
                    INTERNAL_ERROR,
                    format!("could not start command: {error}"),
                );
            }
        };

        let output = Arc::new(Mutex::new(OutputBuffer::new(limit)));
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, output.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, output.clone());
        }

        let exit = Arc::new(Mutex::new(None::<ExitInfo>));
        let done = Arc::new(Notify::new());
        let child = Arc::new(Mutex::new(child));
        spawn_supervisor(child.clone(), exit.clone(), done.clone());

        let terminal_id = format!(
            "term-{}",
            self.next_terminal.fetch_add(1, Ordering::Relaxed)
        );
        self.terminals.lock().await.insert(
            terminal_id.clone(),
            Terminal {
                child,
                output,
                exit,
                done,
            },
        );
        ok(id, json!({ "terminalId": terminal_id }))
    }

    async fn terminal_output(&self, params: Option<&Value>, id: Value) -> Value {
        let terminal_id = match terminal_id(params) {
            Some(t) => t,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "terminal/output requires `terminalId`".into(),
                );
            }
        };
        let terminals = self.terminals.lock().await;
        let Some(terminal) = terminals.get(&terminal_id) else {
            return err(id, INVALID_PARAMS, "unknown terminalId".into());
        };
        let (output, truncated) = terminal.output.lock().await.snapshot();
        let exit = terminal.exit.lock().await.clone();
        let mut result = json!({ "output": output, "truncated": truncated });
        if let Some(exit) = exit {
            result["exitStatus"] = exit.to_json();
        }
        ok(id, result)
    }

    async fn terminal_wait(&self, params: Option<&Value>, id: Value) -> Value {
        let terminal_id = match terminal_id(params) {
            Some(t) => t,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "terminal/wait_for_exit requires `terminalId`".into(),
                );
            }
        };
        let (exit, done) = {
            let terminals = self.terminals.lock().await;
            let Some(terminal) = terminals.get(&terminal_id) else {
                return err(id, INVALID_PARAMS, "unknown terminalId".into());
            };
            (terminal.exit.clone(), terminal.done.clone())
        };
        loop {
            if let Some(exit) = exit.lock().await.clone() {
                return ok(id, exit.to_json());
            }
            done.notified().await;
        }
    }

    async fn terminal_kill(&self, params: Option<&Value>, id: Value) -> Value {
        let terminal_id = match terminal_id(params) {
            Some(t) => t,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "terminal/kill requires `terminalId`".into(),
                );
            }
        };
        let terminals = self.terminals.lock().await;
        let Some(terminal) = terminals.get(&terminal_id) else {
            return err(id, INVALID_PARAMS, "unknown terminalId".into());
        };
        let _ = terminal.child.lock().await.start_kill();
        ok(id, Value::Null)
    }

    async fn terminal_release(&self, params: Option<&Value>, id: Value) -> Value {
        let terminal_id = match terminal_id(params) {
            Some(t) => t,
            None => {
                return err(
                    id,
                    INVALID_PARAMS,
                    "terminal/release requires `terminalId`".into(),
                );
            }
        };
        if let Some(terminal) = self.terminals.lock().await.remove(&terminal_id) {
            let _ = terminal.child.lock().await.start_kill();
        }
        ok(id, Value::Null)
    }

    /// Resolve `raw` (absolute or relative to the vault) and guarantee it stays
    /// within the vault directory; symlink-free lexical normalization defends
    /// against `..` traversal.
    fn resolve_within_vault(&self, raw: &str) -> Result<PathBuf, String> {
        let candidate = Path::new(raw);
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.cwd.join(candidate)
        };
        let normalized = normalize_lexical(&joined);
        if normalized.starts_with(&self.cwd) {
            Ok(normalized)
        } else {
            Err("path escapes the vault directory".into())
        }
    }
}

/// A live terminal: its child process plus shared output/exit state.
struct Terminal {
    child: Arc<Mutex<tokio::process::Child>>,
    output: Arc<Mutex<OutputBuffer>>,
    exit: Arc<Mutex<Option<ExitInfo>>>,
    done: Arc<Notify>,
}

/// Captured terminal output with a byte cap; oldest bytes are dropped first.
struct OutputBuffer {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl OutputBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit: limit.max(1),
            truncated: false,
        }
    }

    fn push(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data);
        if self.bytes.len() > self.limit {
            let drop = self.bytes.len() - self.limit;
            self.bytes.drain(..drop);
            self.truncated = true;
        }
    }

    fn snapshot(&self) -> (String, bool) {
        (
            String::from_utf8_lossy(&self.bytes).into_owned(),
            self.truncated,
        )
    }
}

/// A process exit outcome in ACP's `{ exitCode, signal }` shape.
#[derive(Clone)]
struct ExitInfo {
    code: Option<i32>,
    signal: Option<String>,
}

impl ExitInfo {
    fn from_status(status: std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt;
            status.signal().map(|s| s.to_string())
        };
        #[cfg(not(unix))]
        let signal = None;
        Self {
            code: status.code(),
            signal,
        }
    }

    fn to_json(&self) -> Value {
        json!({ "exitCode": self.code, "signal": self.signal })
    }
}

/// Drain a child pipe into the shared output buffer until EOF.
fn spawn_reader<R>(mut reader: R, output: Arc<Mutex<OutputBuffer>>)
where
    R: AsyncReadExt + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => output.lock().await.push(&buf[..n]),
            }
        }
    });
}

/// Poll the child for exit (without holding the lock across an await), record
/// its status, and wake any `terminal/wait_for_exit` waiters.
fn spawn_supervisor(
    child: Arc<Mutex<tokio::process::Child>>,
    exit: Arc<Mutex<Option<ExitInfo>>>,
    done: Arc<Notify>,
) {
    tokio::spawn(async move {
        loop {
            let status = child.lock().await.try_wait();
            match status {
                Ok(Some(status)) => {
                    *exit.lock().await = Some(ExitInfo::from_status(status));
                    done.notify_waiters();
                    break;
                }
                Ok(None) => tokio::time::sleep(Duration::from_millis(50)).await,
                Err(_) => {
                    done.notify_waiters();
                    break;
                }
            }
        }
    });
}

fn terminal_id(params: Option<&Value>) -> Option<String> {
    params?
        .get("terminalId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Lexically normalize a path (resolve `.`/`..` without touching the
/// filesystem) so traversal stays inside the vault even for paths that do not
/// yet exist.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Return at most `limit` lines starting at 1-based `line` from `content`.
fn slice_lines(content: &str, line: Option<u64>, limit: Option<u64>) -> String {
    if line.is_none() && limit.is_none() {
        return content.to_string();
    }
    let start = line.unwrap_or(1).max(1) as usize - 1;
    let lines: Vec<&str> = content.lines().collect();
    let selected: Vec<&str> = match limit {
        Some(limit) => lines.into_iter().skip(start).take(limit as usize).collect(),
        None => lines.into_iter().skip(start).collect(),
    };
    selected.join("\n")
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn err(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler(read_only: bool, local_io: bool, cwd: &Path) -> ClientHandler {
        ClientHandler::new(read_only, local_io, cwd.to_path_buf())
    }

    #[tokio::test]
    async fn fs_read_returns_file_contents_within_vault() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "# Title\nbody\n").unwrap();
        let handler = handler(true, true, dir.path());

        let response = handler
            .handle(
                "fs/read_text_file",
                Some(&json!({ "path": "note.md" })),
                json!(1),
            )
            .await;

        assert_eq!(response["result"]["content"], json!("# Title\nbody\n"));
    }

    #[tokio::test]
    async fn fs_read_honors_line_and_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("n.md"), "a\nb\nc\nd\n").unwrap();
        let handler = handler(true, true, dir.path());

        let response = handler
            .handle(
                "fs/read_text_file",
                Some(&json!({ "path": "n.md", "line": 2, "limit": 2 })),
                json!(1),
            )
            .await;

        assert_eq!(response["result"]["content"], json!("b\nc"));
    }

    #[tokio::test]
    async fn fs_read_rejects_paths_outside_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(true, true, dir.path());

        let response = handler
            .handle(
                "fs/read_text_file",
                Some(&json!({ "path": "../escape.md" })),
                json!(1),
            )
            .await;

        assert_eq!(response["error"]["code"], json!(INVALID_PARAMS));
    }

    #[tokio::test]
    async fn fs_methods_are_method_not_found_when_local_io_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(false, false, dir.path());

        let response = handler
            .handle(
                "fs/read_text_file",
                Some(&json!({ "path": "n.md" })),
                json!(1),
            )
            .await;

        assert_eq!(response["error"]["code"], json!(METHOD_NOT_FOUND));
    }

    #[tokio::test]
    async fn fs_write_creates_file_when_read_write() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(false, true, dir.path());

        let response = handler
            .handle(
                "fs/write_text_file",
                Some(&json!({ "path": "sub/new.md", "content": "hi" })),
                json!(1),
            )
            .await;

        assert!(response.get("result").is_some());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("sub/new.md")).unwrap(),
            "hi"
        );
    }

    #[tokio::test]
    async fn fs_write_is_refused_in_read_only_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(true, true, dir.path());

        let response = handler
            .handle(
                "fs/write_text_file",
                Some(&json!({ "path": "x.md", "content": "hi" })),
                json!(1),
            )
            .await;

        assert_eq!(response["error"]["code"], json!(INTERNAL_ERROR));
        assert!(!dir.path().join("x.md").exists());
    }

    #[tokio::test]
    async fn terminal_runs_a_command_and_reports_output_and_exit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "x").unwrap();
        let handler = handler(false, true, dir.path());

        let created = handler
            .handle(
                "terminal/create",
                Some(&json!({ "command": "ls" })),
                json!(1),
            )
            .await;
        let terminal_id = created["result"]["terminalId"]
            .as_str()
            .unwrap()
            .to_string();

        let waited = handler
            .handle(
                "terminal/wait_for_exit",
                Some(&json!({ "terminalId": terminal_id })),
                json!(2),
            )
            .await;
        assert_eq!(waited["result"]["exitCode"], json!(0));

        let output = handler
            .handle(
                "terminal/output",
                Some(&json!({ "terminalId": terminal_id })),
                json!(3),
            )
            .await;
        assert!(
            output["result"]["output"]
                .as_str()
                .unwrap()
                .contains("hello.txt")
        );

        let released = handler
            .handle(
                "terminal/release",
                Some(&json!({ "terminalId": terminal_id })),
                json!(4),
            )
            .await;
        assert!(released.get("result").is_some());
    }

    #[tokio::test]
    async fn terminal_is_refused_in_read_only_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(true, true, dir.path());

        let response = handler
            .handle(
                "terminal/create",
                Some(&json!({ "command": "ls" })),
                json!(1),
            )
            .await;

        assert_eq!(response["error"]["code"], json!(INTERNAL_ERROR));
    }

    #[tokio::test]
    async fn permission_request_is_handled_regardless_of_local_io() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(false, false, dir.path());

        let response = handler
            .handle(
                "session/request_permission",
                Some(&json!({
                    "options": [{ "optionId": "ok", "kind": "allow_once" }]
                })),
                json!(1),
            )
            .await;

        assert_eq!(response["result"]["outcome"]["outcome"], json!("selected"));
    }
}
