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

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;

use crate::adapter::LineAdapter;
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
        let (program, args) = adapter.command();
        let mut child = tokio::process::Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| AgentError::Spawn {
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
