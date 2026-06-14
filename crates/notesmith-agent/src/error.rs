//! Error type for agent session management.

/// Errors raised while spawning or communicating with an agent process.
///
/// Note that *content* problems (a malformed line in the agent's output) do
/// **not** surface as an [`AgentError`]; they become an
/// [`AgentEvent::Error`](crate::AgentEvent::Error) on the stream so the session
/// keeps running. `AgentError` is reserved for failures that prevent the
/// session from operating at all (spawn failure, broken stdin pipe).
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// The agent binary could not be spawned.
    #[error("could not start agent `{program}`: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    /// The child process did not expose the pipe we need (stdin/stdout).
    #[error("agent process is missing its {0} pipe")]
    MissingPipe(&'static str),

    /// Writing a message to the agent's stdin failed.
    #[error("could not send message to agent: {0}")]
    Send(#[source] std::io::Error),

    /// A protocol-level failure while driving an ACP (Agent Client Protocol)
    /// session — e.g. the agent closed the connection before answering a
    /// request, or returned a JSON-RPC error during the handshake.
    #[error("agent protocol error: {0}")]
    Protocol(String),
}
