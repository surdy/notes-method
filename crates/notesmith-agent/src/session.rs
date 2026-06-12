//! Agent session abstraction and a process-backed implementation.
//!
//! [`AgentSession`] is the transport-agnostic contract the desktop runner and
//! the headless CLI consume: push user messages in, pull normalized
//! [`AgentEvent`]s out. [`ProcessAgentSession`] implements it by spawning an
//! agent CLI and streaming its stdout through a [`LineAdapter`].
//!
//! The line-driving loop ([`drive_lines`]) is factored out and generic over the
//! reader so it can be unit-tested against an in-memory transcript without
//! spawning a real process.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;

use crate::adapter::{Launch, LineAdapter, PromptDelivery};
use crate::error::AgentError;
use crate::event::AgentEvent;

/// A live conversation with an agent.
///
/// Implementations stream [`AgentEvent`]s; [`next_event`](AgentSession::next_event)
/// returns `None` once the agent process has ended.
pub trait AgentSession: Send {
    /// Send a user message to the agent.
    fn send(
        &mut self,
        message: &str,
    ) -> impl std::future::Future<Output = Result<(), AgentError>> + Send;

    /// Await the next normalized event, or `None` when the session has ended.
    fn next_event(&mut self) -> impl std::future::Future<Output = Option<AgentEvent>> + Send;
}

/// Read `reader` line by line, push each line through `adapter`, and forward the
/// resulting events on `tx`. Returns when the reader reaches EOF or `tx` closes.
///
/// A read error is reported as a single [`AgentEvent::Error`] and ends the loop;
/// it never panics.
pub async fn drive_lines<R, A>(reader: R, adapter: &mut A, tx: mpsc::UnboundedSender<AgentEvent>)
where
    R: tokio::io::AsyncBufRead + Unpin,
    A: LineAdapter,
{
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                for event in adapter.parse_line(&line) {
                    if tx.send(event).is_err() {
                        return;
                    }
                }
            }
            Ok(None) => return,
            Err(err) => {
                let _ = tx.send(AgentEvent::Error {
                    message: format!("could not read agent output: {err}"),
                });
                return;
            }
        }
    }
}

/// An [`AgentSession`] backed by a spawned agent CLI process.
pub struct ProcessAgentSession<A: LineAdapter> {
    adapter: A,
    child: Child,
    stdin: ChildStdin,
    events: mpsc::UnboundedReceiver<AgentEvent>,
}

impl<A: LineAdapter + Clone + 'static> ProcessAgentSession<A> {
    /// Spawn the agent described by `adapter` and begin streaming its output.
    pub fn spawn(adapter: A) -> Result<Self, AgentError> {
        Self::spawn_in(adapter, None)
    }

    /// Spawn the agent in `working_dir` (when given) and begin streaming output.
    ///
    /// The desktop runner uses this to launch the agent inside the active
    /// vault's directory so relative paths and the agent's own config
    /// resolution match the vault the user is viewing.
    pub fn spawn_in(adapter: A, working_dir: Option<PathBuf>) -> Result<Self, AgentError> {
        let (program, args) = adapter.command();
        let mut command = tokio::process::Command::new(&program);
        command
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(dir) = working_dir {
            command.current_dir(dir);
        }
        let mut child = command.spawn().map_err(|source| AgentError::Spawn {
            program: program.clone(),
            source,
        })?;

        let stdin = child.stdin.take().ok_or(AgentError::MissingPipe("stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(AgentError::MissingPipe("stdout"))?;

        let (tx, events) = mpsc::unbounded_channel();
        let mut reader_adapter = adapter.clone();
        tokio::spawn(async move {
            drive_lines(BufReader::new(stdout), &mut reader_adapter, tx).await;
        });

        Ok(Self {
            adapter,
            child,
            stdin,
            events,
        })
    }
}

impl<A: LineAdapter> AgentSession for ProcessAgentSession<A> {
    async fn send(&mut self, message: &str) -> Result<(), AgentError> {
        let bytes = self.adapter.encode_user_message(message);
        self.stdin
            .write_all(&bytes)
            .await
            .map_err(AgentError::Send)?;
        self.stdin.flush().await.map_err(AgentError::Send)?;
        Ok(())
    }

    async fn next_event(&mut self) -> Option<AgentEvent> {
        self.events.recv().await
    }
}

impl<A: LineAdapter> Drop for ProcessAgentSession<A> {
    fn drop(&mut self) {
        // Best-effort: don't leave an orphaned agent process behind.
        let _ = self.child.start_kill();
    }
}

/// An [`AgentSession`] for single-shot agents (Codex `exec`, Copilot CLI).
///
/// Unlike [`ProcessAgentSession`], the process is spawned on the **first**
/// [`send`](AgentSession::send): the prompt is delivered either as a command
/// argument ([`PromptDelivery::Arg`]) or written to stdin which is then closed
/// ([`PromptDelivery::Stdin`]). The process runs one turn and exits; when its
/// stdout reaches EOF, [`next_event`](AgentSession::next_event) returns `None`
/// and the session ends. A new turn requires a new session.
pub struct OneShotProcessSession<A: LineAdapter + Clone + 'static> {
    adapter: A,
    working_dir: Option<PathBuf>,
    delivery: PromptDelivery,
    child: Option<Child>,
    events: Option<mpsc::UnboundedReceiver<AgentEvent>>,
    started: bool,
}

impl<A: LineAdapter + Clone + 'static> OneShotProcessSession<A> {
    /// Build a single-shot session for `adapter`, to run in `working_dir`.
    pub fn new(adapter: A, working_dir: Option<PathBuf>) -> Self {
        let delivery = match adapter.launch() {
            Launch::OneShot(delivery) => delivery,
            // Streaming adapters should use `ProcessAgentSession`; default to
            // stdin delivery so a misconfiguration still behaves predictably.
            Launch::Streaming => PromptDelivery::Stdin,
        };
        Self {
            adapter,
            working_dir,
            delivery,
            child: None,
            events: None,
            started: false,
        }
    }

    fn spawn_turn(&mut self, prompt: &str) -> Result<(), AgentError> {
        let (program, args) = match self.delivery {
            PromptDelivery::Arg => self.adapter.command_for_prompt(prompt),
            PromptDelivery::Stdin => self.adapter.command(),
        };
        let mut command = tokio::process::Command::new(&program);
        command
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(dir) = &self.working_dir {
            command.current_dir(dir);
        }
        let mut child = command.spawn().map_err(|source| AgentError::Spawn {
            program: program.clone(),
            source,
        })?;

        // Deliver the prompt over stdin when required, then close stdin so the
        // agent stops waiting for more input and runs its turn.
        let stdin = child.stdin.take().ok_or(AgentError::MissingPipe("stdin"))?;
        if matches!(self.delivery, PromptDelivery::Stdin) {
            let bytes = self.adapter.encode_user_message(prompt);
            tokio::spawn(async move {
                let mut stdin = stdin;
                let _ = stdin.write_all(&bytes).await;
                let _ = stdin.flush().await;
                // Dropping `stdin` here closes the pipe (EOF for the agent).
            });
        }
        // For `Arg` delivery stdin is simply dropped (closed) immediately.

        let stdout = child
            .stdout
            .take()
            .ok_or(AgentError::MissingPipe("stdout"))?;
        let (tx, events) = mpsc::unbounded_channel();
        let mut reader_adapter = self.adapter.clone();
        tokio::spawn(async move {
            drive_lines(BufReader::new(stdout), &mut reader_adapter, tx).await;
        });

        self.child = Some(child);
        self.events = Some(events);
        self.started = true;
        Ok(())
    }
}

impl<A: LineAdapter + Clone + 'static> AgentSession for OneShotProcessSession<A> {
    async fn send(&mut self, message: &str) -> Result<(), AgentError> {
        if self.started {
            // Single-shot agents handle one prompt per process; ignore further
            // input (the session ends when the turn's process exits).
            return Ok(());
        }
        self.spawn_turn(message)
    }

    async fn next_event(&mut self) -> Option<AgentEvent> {
        match self.events.as_mut() {
            Some(events) => events.recv().await,
            // No turn has been started yet: never resolve, so a `select!` in the
            // runner waits for the first user message instead of ending the
            // session. Once a turn starts, the channel drives completion.
            None => std::future::pending().await,
        }
    }
}

impl<A: LineAdapter + Clone + 'static> Drop for OneShotProcessSession<A> {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_code::ClaudeCodeAdapter;
    use crate::event::{ToolCall, ToolResult};

    async fn collect(transcript: &str) -> Vec<AgentEvent> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut adapter = ClaudeCodeAdapter::default();
        let reader = BufReader::new(transcript.as_bytes());
        drive_lines(reader, &mut adapter, tx).await;
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn drives_a_full_happy_path_turn() {
        let transcript = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-x"}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hi!"}]}}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Hi!"}"#,
            "\n",
        );
        let events = collect(transcript).await;
        assert_eq!(
            events,
            vec![
                AgentEvent::Status {
                    message: "session initialized (model=claude-x)".to_string()
                },
                AgentEvent::AgentMessageDelta {
                    text: "Hi!".to_string()
                },
                AgentEvent::Done {
                    result: Some("Hi!".to_string())
                },
            ]
        );
    }

    #[tokio::test]
    async fn drives_a_tool_call_round_trip() {
        let transcript = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"a.md"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"hello","is_error":false}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}"#,
            "\n",
            r#"{"type":"result","is_error":false,"result":"Done."}"#,
            "\n",
        );
        let events = collect(transcript).await;
        assert_eq!(
            events,
            vec![
                AgentEvent::ToolCall(ToolCall {
                    id: Some("toolu_1".to_string()),
                    name: "Read".to_string(),
                    args: serde_json::json!({ "path": "a.md" }),
                }),
                AgentEvent::ToolResult(ToolResult {
                    id: Some("toolu_1".to_string()),
                    content: "hello".to_string(),
                    is_error: false,
                }),
                AgentEvent::AgentMessageDelta {
                    text: "Done.".to_string()
                },
                AgentEvent::Done {
                    result: Some("Done.".to_string())
                },
            ]
        );
    }

    #[tokio::test]
    async fn a_malformed_line_emits_an_error_but_following_lines_still_parse() {
        let transcript = concat!(
            "{ this is broken\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"still here"}]}}"#,
            "\n",
        );
        let events = collect(transcript).await;
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], AgentEvent::Error { .. }));
        assert_eq!(
            events[1],
            AgentEvent::AgentMessageDelta {
                text: "still here".to_string()
            }
        );
    }
}
