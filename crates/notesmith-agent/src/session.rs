//! Agent session abstraction.
//!
//! [`AgentSession`] is the transport-agnostic contract the desktop runner and
//! the headless CLI consume: push user messages in, pull normalized
//! [`AgentEvent`]s out. The single implementation is
//! [`AcpSession`](crate::AcpSession), the Agent Client Protocol transport
//! (ADR 0011 Phase E).

use crate::error::AgentError;
use crate::event::AgentEvent;

/// A live conversation with an agent.
///
/// Implementations stream [`AgentEvent`]s; [`next_event`](AgentSession::next_event)
/// returns `None` once the session has ended.
pub trait AgentSession: Send {
    /// Send a user message to the agent.
    fn send(
        &mut self,
        message: &str,
    ) -> impl std::future::Future<Output = Result<(), AgentError>> + Send;

    /// Await the next normalized event, or `None` when the session has ended.
    fn next_event(&mut self) -> impl std::future::Future<Output = Option<AgentEvent>> + Send;
}
