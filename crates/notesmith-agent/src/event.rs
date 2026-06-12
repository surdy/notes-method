//! The normalized agent event model.
//!
//! Every adapter maps its agent CLI's streaming output onto this single event
//! stream, so the desktop chat panel renders one shape regardless of which
//! agent (Copilot CLI, Claude Code, Codex) is driving the session.

use serde::Serialize;

/// A single tool invocation requested by the agent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCall {
    /// Provider-assigned identifier, correlating the call with its result.
    pub id: Option<String>,
    /// Tool name (e.g. `Read`, `Bash`, or an MCP tool).
    pub name: String,
    /// Arguments passed to the tool, as provided by the agent.
    pub args: serde_json::Value,
}

/// The result of a previously requested [`ToolCall`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolResult {
    /// Identifier of the [`ToolCall`] this result belongs to, when known.
    pub id: Option<String>,
    /// Rendered textual content of the result.
    pub content: String,
    /// Whether the tool reported a failure.
    pub is_error: bool,
}

/// A normalized event emitted while driving an agent session.
///
/// The variants are serialized with an internal `type` tag so the desktop
/// frontend can match on a single discriminator.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Echo of a user message entering the conversation.
    UserMessage { text: String },
    /// A chunk of assistant text. Adapters that only emit whole messages send
    /// one delta per message; streaming adapters send many.
    AgentMessageDelta { text: String },
    /// The agent requested a tool invocation.
    ToolCall(ToolCall),
    /// A tool returned a result.
    ToolResult(ToolResult),
    /// A non-content status update (session initialized, model info, etc.).
    Status { message: String },
    /// The turn completed successfully; `result` is the final answer when the
    /// agent provided one.
    Done { result: Option<String> },
    /// A recoverable error while parsing or running the session. Emitting an
    /// `Error` event never terminates the session on its own — the stream ends
    /// only at EOF.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_message_delta_serializes_with_type_tag() {
        let event = AgentEvent::AgentMessageDelta {
            text: "hello".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({ "type": "agent_message_delta", "text": "hello" })
        );
    }

    #[test]
    fn tool_call_flattens_fields_under_the_type_tag() {
        let event = AgentEvent::ToolCall(ToolCall {
            id: Some("toolu_1".to_string()),
            name: "Read".to_string(),
            args: json!({ "path": "note.md" }),
        });
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({
                "type": "tool_call",
                "id": "toolu_1",
                "name": "Read",
                "args": { "path": "note.md" }
            })
        );
    }

    #[test]
    fn tool_result_carries_error_flag() {
        let event = AgentEvent::ToolResult(ToolResult {
            id: None,
            content: "boom".to_string(),
            is_error: true,
        });
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({ "type": "tool_result", "id": null, "content": "boom", "is_error": true })
        );
    }

    #[test]
    fn done_with_no_result_serializes_null() {
        let event = AgentEvent::Done { result: None };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({ "type": "done", "result": null })
        );
    }
}
