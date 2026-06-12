//! Adapter abstraction: turn an agent CLI's raw output lines into
//! [`AgentEvent`]s.
//!
//! Each supported agent gets a [`LineAdapter`] that parses one line of the
//! tool's streaming output into zero or more normalized events. Keeping the
//! parser separate from process I/O makes it a pure, exhaustively testable
//! function.

use crate::event::AgentEvent;

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
}
