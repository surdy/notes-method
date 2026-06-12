//! Adapter abstraction: turn an agent CLI's raw output lines into
//! [`AgentEvent`]s.
//!
//! Each supported agent gets a [`LineAdapter`] that parses one line of the
//! tool's streaming output into zero or more normalized events. Keeping the
//! parser separate from process I/O makes it a pure, exhaustively testable
//! function.

use crate::event::AgentEvent;

/// How an agent receives a user prompt and produces a turn.
///
/// Claude Code keeps a persistent bidirectional stream; Codex (`exec`) and the
/// Copilot CLI are single-shot — each prompt spawns a fresh process that runs
/// one turn and exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// Spawn once; write each user message to stdin (Claude Code).
    Streaming,
    /// Spawn a fresh process per prompt; the process runs one turn and exits.
    OneShot(PromptDelivery),
}

/// How a single-shot agent's prompt is delivered to the spawned process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDelivery {
    /// Prompt is passed as a trailing command argument (Copilot CLI `-p`).
    Arg,
    /// Prompt is written to stdin which is then closed so the agent proceeds
    /// (Codex `exec -`).
    Stdin,
}

/// Parses an agent CLI's streaming output, one line at a time.
///
/// Implementations must be **tolerant**: a malformed or unrecognized line must
/// never panic. Unparseable lines should yield an [`AgentEvent::Error`] (or be
/// ignored) so the session keeps running, in line with the resilience policy
/// (ADR 0009).
pub trait LineAdapter: Send {
    /// Parse a single line of agent output into normalized events.
    ///
    /// `line` is a raw line of the agent's stdout with the trailing newline
    /// already stripped. Returns the events the line produced (possibly empty).
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent>;

    /// Encode a user message into the bytes to write to the agent's stdin,
    /// including any trailing delimiter the agent's input protocol requires.
    fn encode_user_message(&self, text: &str) -> Vec<u8>;

    /// The program name and arguments used to launch this agent in headless,
    /// stream-oriented mode.
    fn command(&self) -> (String, Vec<String>);

    /// How this agent is launched and fed a prompt. Defaults to
    /// [`Launch::Streaming`].
    fn launch(&self) -> Launch {
        Launch::Streaming
    }

    /// The program and arguments for a single-shot launch carrying `prompt` as
    /// a command argument ([`PromptDelivery::Arg`]). Defaults to ignoring the
    /// prompt and returning [`command`](LineAdapter::command).
    fn command_for_prompt(&self, _prompt: &str) -> (String, Vec<String>) {
        self.command()
    }
}
