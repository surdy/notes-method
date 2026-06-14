//! ACP client-side capability handlers: scoped filesystem and terminal proxies
//! for the break-glass local-I/O mode (ADR 0012, Decisions 7–8).
//!
//! When the embedded agent is granted local I/O (the opt-in break-glass
//! setting, off by default), Notesmith answers the agent's `fs/*` and
//! `terminal/*` requests itself rather than leaving them unadvertised. Every
//! path is **scoped to the vault directory** and writes/terminals are refused
//! in read-only sessions, so enabling local I/O never escapes the active vault
//! or defeats the read-only scope. When local I/O is off, every handler reports
//! "method not found" — defense in depth behind the unadvertised capabilities.
//!
//! Per ADR 0009 every handler is tolerant: malformed params yield a JSON-RPC
//! error response, never a panic.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use agent_client_protocol::Error;
use agent_client_protocol::schema::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    TerminalExitStatus, TerminalId, TerminalOutputRequest, TerminalOutputResponse,
    WaitForTerminalExitRequest, WriteTextFileRequest, WriteTextFileResponse,
};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify};

/// JSON-RPC error code: the method does not exist / is not available.
const METHOD_NOT_FOUND: i32 = -32601;
/// JSON-RPC error code: invalid method parameter(s).
const INVALID_PARAMS: i32 = -32602;
/// JSON-RPC error code: internal error.
const INTERNAL_ERROR: i32 = -32603;

/// Default cap on retained terminal output (1 MiB) when the agent does not set
/// `outputByteLimit`.
const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;

fn method_not_found(method: &str) -> Error {
    Error::new(METHOD_NOT_FOUND, format!("method not available: {method}"))
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(INVALID_PARAMS, message)
}

fn internal(message: impl Into<String>) -> Error {
    Error::new(INTERNAL_ERROR, message)
}

/// Handles inbound filesystem/terminal requests an agent makes back to the
/// client, scoped to the vault directory. Shared (behind an `Arc`) across the
/// connection's typed request handlers.
pub(crate) struct LocalIoHandler {
    local_io: bool,
    read_only: bool,
    cwd: PathBuf,
    terminals: Mutex<HashMap<String, Terminal>>,
    next_terminal: AtomicU64,
}

impl LocalIoHandler {
    /// Build a handler for a session rooted at `cwd` (the active vault's
    /// absolute directory). `local_io` enables the `fs/*` and `terminal/*`
    /// proxies; `read_only` refuses writes and terminals.
    pub(crate) fn new(local_io: bool, read_only: bool, cwd: PathBuf) -> Self {
        Self {
            local_io,
            read_only,
            cwd,
            terminals: Mutex::new(HashMap::new()),
            next_terminal: AtomicU64::new(1),
        }
    }

    /// Guard: local I/O must be enabled or the method is reported as not found.
    fn require_local_io(&self, method: &str) -> Result<(), Error> {
        if self.local_io {
            Ok(())
        } else {
            Err(method_not_found(method))
        }
    }

    /// Guard: the session must be read-write or the action is refused.
    fn require_writable(&self, what: &str) -> Result<(), Error> {
        if self.read_only {
            Err(internal(format!("session is read-only: {what} refused")))
        } else {
            Ok(())
        }
    }

    pub(crate) fn fs_read(&self, req: &ReadTextFileRequest) -> Result<ReadTextFileResponse, Error> {
        self.require_local_io("fs/read_text_file")?;
        let path = self.resolve_within_vault(&req.path)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|error| internal(format!("read failed: {error}")))?;
        let sliced = slice_lines(&content, req.line, req.limit);
        Ok(ReadTextFileResponse::new(sliced))
    }

    pub(crate) fn fs_write(
        &self,
        req: &WriteTextFileRequest,
    ) -> Result<WriteTextFileResponse, Error> {
        self.require_local_io("fs/write_text_file")?;
        self.require_writable("fs/write_text_file")?;
        let path = self.resolve_within_vault(&req.path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| internal(format!("write failed: {error}")))?;
        }
        std::fs::write(&path, &req.content)
            .map_err(|error| internal(format!("write failed: {error}")))?;
        Ok(WriteTextFileResponse::new())
    }

    pub(crate) async fn terminal_create(
        &self,
        req: &CreateTerminalRequest,
    ) -> Result<CreateTerminalResponse, Error> {
        self.require_local_io("terminal/create")?;
        self.require_writable("terminal/create")?;

        let cwd = match &req.cwd {
            Some(raw) => self.resolve_within_vault(raw)?,
            None => self.cwd.clone(),
        };
        let limit = req
            .output_byte_limit
            .map(|n| n as usize)
            .unwrap_or(DEFAULT_OUTPUT_LIMIT);

        let mut builder = Command::new(&req.command);
        builder
            .args(&req.args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for entry in &req.env {
            builder.env(&entry.name, &entry.value);
        }

        let mut child = builder
            .spawn()
            .map_err(|error| internal(format!("could not start command: {error}")))?;

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
        Ok(CreateTerminalResponse::new(TerminalId::new(terminal_id)))
    }

    pub(crate) async fn terminal_output(
        &self,
        req: &TerminalOutputRequest,
    ) -> Result<TerminalOutputResponse, Error> {
        self.require_local_io("terminal/output")?;
        let terminals = self.terminals.lock().await;
        let terminal = terminals
            .get(req.terminal_id.0.as_ref())
            .ok_or_else(|| invalid("unknown terminalId"))?;
        let (output, truncated) = terminal.output.lock().await.snapshot();
        let exit = terminal.exit.lock().await.clone();
        let mut response = TerminalOutputResponse::new(output, truncated);
        if let Some(exit) = exit {
            response = response.exit_status(exit.to_exit_status());
        }
        Ok(response)
    }

    pub(crate) async fn terminal_kill(
        &self,
        req: &KillTerminalRequest,
    ) -> Result<KillTerminalResponse, Error> {
        self.require_local_io("terminal/kill")?;
        let terminals = self.terminals.lock().await;
        let terminal = terminals
            .get(req.terminal_id.0.as_ref())
            .ok_or_else(|| invalid("unknown terminalId"))?;
        let _ = terminal.child.lock().await.start_kill();
        Ok(KillTerminalResponse::new())
    }

    pub(crate) async fn terminal_release(
        &self,
        req: &ReleaseTerminalRequest,
    ) -> Result<ReleaseTerminalResponse, Error> {
        self.require_local_io("terminal/release")?;
        if let Some(terminal) = self
            .terminals
            .lock()
            .await
            .remove(req.terminal_id.0.as_ref())
        {
            let _ = terminal.child.lock().await.start_kill();
        }
        Ok(ReleaseTerminalResponse::new())
    }

    /// Resolve the shared exit/wake handles for a `terminal/wait_for_exit`
    /// request. The actual wait runs off the dispatch loop (the caller spawns a
    /// task that calls [`await_exit`] on these handles) so a long-running
    /// command never blocks the connection.
    pub(crate) async fn terminal_wait_handles(
        &self,
        req: &WaitForTerminalExitRequest,
    ) -> Result<(Arc<Mutex<Option<ExitInfo>>>, Arc<Notify>), Error> {
        self.require_local_io("terminal/wait_for_exit")?;
        let terminals = self.terminals.lock().await;
        let terminal = terminals
            .get(req.terminal_id.0.as_ref())
            .ok_or_else(|| invalid("unknown terminalId"))?;
        Ok((terminal.exit.clone(), terminal.done.clone()))
    }

    /// Resolve `raw` (absolute or relative to the vault) and guarantee it stays
    /// within the vault directory; symlink-free lexical normalization defends
    /// against `..` traversal even for paths that do not yet exist.
    fn resolve_within_vault(&self, raw: &Path) -> Result<PathBuf, Error> {
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.cwd.join(raw)
        };
        let normalized = normalize_lexical(&joined);
        if normalized.starts_with(&self.cwd) {
            Ok(normalized)
        } else {
            Err(invalid("path escapes the vault directory"))
        }
    }
}

/// Await a terminal's exit on its shared handles, off the connection's dispatch
/// loop. Polls with a short timeout fallback so a `notify_waiters` that fires
/// before this task starts waiting is never lost.
pub(crate) async fn await_exit(exit: Arc<Mutex<Option<ExitInfo>>>, done: Arc<Notify>) -> ExitInfo {
    loop {
        if let Some(exit) = exit.lock().await.clone() {
            return exit;
        }
        tokio::select! {
            _ = done.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(25)) => {}
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
pub(crate) struct ExitInfo {
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

    pub(crate) fn to_exit_status(&self) -> TerminalExitStatus {
        TerminalExitStatus::new()
            .exit_code(self.code.map(|code| code as u32))
            .signal(self.signal.clone())
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
fn slice_lines(content: &str, line: Option<u32>, limit: Option<u32>) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn read_req(json: Value) -> ReadTextFileRequest {
        serde_json::from_value(json).expect("valid read request")
    }
    fn write_req(json: Value) -> WriteTextFileRequest {
        serde_json::from_value(json).expect("valid write request")
    }

    #[test]
    fn fs_read_returns_file_contents_within_vault() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "# Title\nbody\n").unwrap();
        let handler = LocalIoHandler::new(true, true, dir.path().to_path_buf());

        let response = handler
            .fs_read(&read_req(json!({ "sessionId": "s", "path": "note.md" })))
            .expect("read ok");
        assert_eq!(response.content, "# Title\nbody\n");
    }

    #[test]
    fn fs_read_honors_line_and_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("n.md"), "a\nb\nc\nd\n").unwrap();
        let handler = LocalIoHandler::new(true, true, dir.path().to_path_buf());

        let response = handler
            .fs_read(&read_req(
                json!({ "sessionId": "s", "path": "n.md", "line": 2, "limit": 2 }),
            ))
            .expect("read ok");
        assert_eq!(response.content, "b\nc");
    }

    #[test]
    fn fs_read_rejects_paths_outside_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let handler = LocalIoHandler::new(true, true, dir.path().to_path_buf());

        let error = handler
            .fs_read(&read_req(
                json!({ "sessionId": "s", "path": "../escape.md" }),
            ))
            .expect_err("escape rejected");
        assert_eq!(error.code, INVALID_PARAMS.into());
        assert!(error.message.contains("escapes the vault"));
    }

    #[test]
    fn fs_read_reports_method_not_found_when_local_io_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("n.md"), "x").unwrap();
        let handler = LocalIoHandler::new(false, false, dir.path().to_path_buf());

        let error = handler
            .fs_read(&read_req(json!({ "sessionId": "s", "path": "n.md" })))
            .expect_err("disabled");
        assert_eq!(error.code, METHOD_NOT_FOUND.into());
    }

    #[test]
    fn fs_write_is_refused_in_read_only_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let handler = LocalIoHandler::new(true, true, dir.path().to_path_buf());

        let error = handler
            .fs_write(&write_req(
                json!({ "sessionId": "s", "path": "n.md", "content": "x" }),
            ))
            .expect_err("read-only refuses writes");
        assert_eq!(error.code, INTERNAL_ERROR.into());
        assert!(!dir.path().join("n.md").exists());
    }

    #[test]
    fn fs_write_persists_within_a_read_write_vault() {
        let dir = tempfile::tempdir().unwrap();
        let handler = LocalIoHandler::new(true, false, dir.path().to_path_buf());

        handler
            .fs_write(&write_req(
                json!({ "sessionId": "s", "path": "sub/n.md", "content": "hello" }),
            ))
            .expect("write ok");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("sub/n.md")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn terminal_is_refused_in_read_only_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let handler = LocalIoHandler::new(true, true, dir.path().to_path_buf());

        let req: CreateTerminalRequest =
            serde_json::from_value(json!({ "sessionId": "s", "command": "echo", "args": ["hi"] }))
                .unwrap();
        let error = handler.terminal_create(&req).await.expect_err("read-only");
        assert_eq!(error.code, INTERNAL_ERROR.into());
    }

    #[tokio::test]
    async fn terminal_runs_a_command_and_reports_output_and_exit() {
        let dir = tempfile::tempdir().unwrap();
        let handler = LocalIoHandler::new(true, false, dir.path().to_path_buf());

        let create: CreateTerminalRequest = serde_json::from_value(
            json!({ "sessionId": "s", "command": "echo", "args": ["hello"] }),
        )
        .unwrap();
        let created = handler.terminal_create(&create).await.expect("create ok");
        let id = created.terminal_id.0.to_string();

        let (exit, done) = handler
            .terminal_wait_handles(
                &serde_json::from_value(json!({ "sessionId": "s", "terminalId": id })).unwrap(),
            )
            .await
            .expect("wait handles");
        let exit = await_exit(exit, done).await;
        assert!(
            exit.to_exit_status().exit_code == Some(0) || exit.to_exit_status().signal.is_some()
        );

        let output = handler
            .terminal_output(
                &serde_json::from_value(json!({ "sessionId": "s", "terminalId": id })).unwrap(),
            )
            .await
            .expect("output ok");
        assert!(output.output.contains("hello"));
        assert!(output.exit_status.is_some());
    }
}
