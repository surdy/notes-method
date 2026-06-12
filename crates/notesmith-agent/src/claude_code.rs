//! Adapter for [Claude Code](https://code.claude.com)'s `stream-json` mode.
//!
//! Claude Code, when run headless with
//! `--print --output-format stream-json --input-format stream-json --verbose`,
//! emits one JSON object per line. Each object has a `type`:
//!
//! - `system` (`subtype: "init"`) — session start / model info.
//! - `assistant` — wraps an Anthropic message whose `content` is an array of
//!   blocks: `text` blocks become [`AgentEvent::AgentMessageDelta`] and
//!   `tool_use` blocks become [`AgentEvent::ToolCall`].
//! - `user` — carries `tool_result` blocks, which become
//!   [`AgentEvent::ToolResult`].
//! - `result` — the final outcome; success becomes [`AgentEvent::Done`], an
//!   error becomes [`AgentEvent::Error`].
//!
//! Input (stdin) is a single JSON line per user turn:
//! `{"type":"user","message":{"role":"user","content":[{"type":"text","text":"…"}]}}`.
//!
//! The parser is deliberately tolerant: unknown `type`s are ignored and a line
//! that is not valid JSON yields a single [`AgentEvent::Error`] rather than
//! aborting the session (ADR 0009).

use serde_json::{Value, json};

use crate::adapter::LineAdapter;
use crate::event::{AgentEvent, ToolCall, ToolResult};
use crate::mcp::McpBinding;

/// Default binary name for Claude Code.
pub const DEFAULT_BIN: &str = "claude";

/// [`LineAdapter`] for Claude Code's `stream-json` transport.
#[derive(Debug, Clone)]
pub struct ClaudeCodeAdapter {
    bin: String,
    mcp: Option<McpBinding>,
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new(DEFAULT_BIN)
    }
}

impl ClaudeCodeAdapter {
    /// Build an adapter that launches the given binary (path or name).
    pub fn new(bin: impl Into<String>) -> Self {
        Self {
            bin: bin.into(),
            mcp: None,
        }
    }

    /// Auto-wire the agent to a Notesmith MCP endpoint (ADR 0011 Phase C).
    pub fn with_mcp(mut self, binding: McpBinding) -> Self {
        self.mcp = Some(binding);
        self
    }
}

impl LineAdapter for ClaudeCodeAdapter {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let value: Value = match serde_json::from_str(trimmed) {
            Ok(value) => value,
            Err(err) => {
                return vec![AgentEvent::Error {
                    message: format!("could not parse agent output: {err}"),
                }];
            }
        };

        match value.get("type").and_then(Value::as_str) {
            Some("system") => parse_system(&value),
            Some("assistant") => parse_assistant(&value),
            Some("user") => parse_user(&value),
            Some("result") => parse_result(&value),
            // Unknown or partial-stream events are ignored; the session
            // continues until EOF / a `result` line.
            _ => Vec::new(),
        }
    }

    fn encode_user_message(&self, text: &str) -> Vec<u8> {
        let line = json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": text }]
            }
        });
        let mut bytes = serde_json::to_vec(&line).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    fn command(&self) -> (String, Vec<String>) {
        let mut args = vec![
            "--print".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
        ];
        if let Some(binding) = &self.mcp {
            args.push("--mcp-config".to_string());
            args.push(binding.claude_config_json());
            args.push("--strict-mcp-config".to_string());
        }
        (self.bin.clone(), args)
    }
}

fn parse_system(value: &Value) -> Vec<AgentEvent> {
    if value.get("subtype").and_then(Value::as_str) == Some("init") {
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return vec![AgentEvent::Status {
            message: format!("session initialized (model={model})"),
        }];
    }
    Vec::new()
}

fn parse_assistant(value: &Value) -> Vec<AgentEvent> {
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut events = Vec::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        events.push(AgentEvent::AgentMessageDelta {
                            text: text.to_string(),
                        });
                    }
                }
            }
            Some("tool_use") => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                events.push(AgentEvent::ToolCall(ToolCall {
                    id: block.get("id").and_then(Value::as_str).map(str::to_string),
                    name,
                    args: block.get("input").cloned().unwrap_or(Value::Null),
                }));
            }
            _ => {}
        }
    }
    events
}

fn parse_user(value: &Value) -> Vec<AgentEvent> {
    let Some(content) = value.pointer("/message/content").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut events = Vec::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) == Some("tool_result") {
            events.push(AgentEvent::ToolResult(ToolResult {
                id: block
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content: tool_result_content(block.get("content")),
                is_error: block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }));
        }
    }
    events
}

/// A `tool_result` `content` may be a plain string or an array of content
/// blocks; normalize both to a single string.
fn tool_result_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn parse_result(value: &Value) -> Vec<AgentEvent> {
    let is_error = value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = value
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_string);

    if is_error {
        return vec![AgentEvent::Error {
            message: text.unwrap_or_else(|| "agent reported an error".to_string()),
        }];
    }
    vec![AgentEvent::Done { result: text }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> ClaudeCodeAdapter {
        ClaudeCodeAdapter::default()
    }

    #[test]
    fn system_init_becomes_status() {
        let line = r#"{"type":"system","subtype":"init","model":"claude-x","session_id":"s1"}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::Status {
                message: "session initialized (model=claude-x)".to_string()
            }]
        );
    }

    #[test]
    fn assistant_text_block_becomes_delta() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello there"}]}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::AgentMessageDelta {
                text: "Hello there".to_string()
            }]
        );
    }

    #[test]
    fn assistant_tool_use_block_becomes_tool_call() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_9","name":"Read","input":{"path":"a.md"}}]}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::ToolCall(ToolCall {
                id: Some("toolu_9".to_string()),
                name: "Read".to_string(),
                args: serde_json::json!({ "path": "a.md" }),
            })]
        );
    }

    #[test]
    fn assistant_mixed_blocks_preserve_order() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"text","text":"Let me read it."},
            {"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"a.md"}}
        ]}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], AgentEvent::AgentMessageDelta { .. }));
        assert!(matches!(events[1], AgentEvent::ToolCall(_)));
    }

    #[test]
    fn user_tool_result_string_content() {
        let line = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"file body","is_error":false}]}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("toolu_1".to_string()),
                content: "file body".to_string(),
                is_error: false,
            })]
        );
    }

    #[test]
    fn user_tool_result_array_content_is_joined() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":[{"type":"text","text":"line1"},{"type":"text","text":"line2"}],"is_error":true}]}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("t".to_string()),
                content: "line1\nline2".to_string(),
                is_error: true,
            })]
        );
    }

    #[test]
    fn result_success_becomes_done() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"All set."}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::Done {
                result: Some("All set.".to_string())
            }]
        );
    }

    #[test]
    fn result_error_becomes_error_event() {
        let line = r#"{"type":"result","subtype":"error","is_error":true,"result":"it broke"}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::Error {
                message: "it broke".to_string()
            }]
        );
    }

    #[test]
    fn malformed_json_yields_error_event_without_panicking() {
        let events = adapter().parse_line("{not json");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AgentEvent::Error { .. }));
    }

    #[test]
    fn blank_and_unknown_lines_are_ignored() {
        assert!(adapter().parse_line("   ").is_empty());
        assert!(
            adapter()
                .parse_line(r#"{"type":"stream_event","event":{}}"#)
                .is_empty()
        );
        assert!(adapter().parse_line(r#"{"no_type":true}"#).is_empty());
    }

    #[test]
    fn pathological_lines_never_panic() {
        let mut a = adapter();
        for line in [
            "",
            "\0\0\0",
            "{",
            "[]",
            "null",
            "12345",
            r#"{"type":"assistant"}"#,
            r#"{"type":"assistant","message":{"content":"not-an-array"}}"#,
            r#"{"type":"user","message":{}}"#,
            r#"{"type":"result"}"#,
            &"x".repeat(100_000),
        ] {
            // Must not panic; result is allowed to be empty or an Error event.
            let _ = a.parse_line(line);
        }
    }

    #[test]
    fn encode_user_message_is_a_single_json_line() {
        let bytes = adapter().encode_user_message("hi");
        assert_eq!(*bytes.last().unwrap(), b'\n');
        let value: Value = serde_json::from_slice(&bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(value["type"], "user");
        assert_eq!(value["message"]["content"][0]["text"], "hi");
    }

    #[test]
    fn command_runs_headless_stream_json() {
        let (program, args) = ClaudeCodeAdapter::new("/usr/bin/claude").command();
        assert_eq!(program, "/usr/bin/claude");
        assert!(args.iter().any(|a| a == "--print"));
        assert!(
            args.windows(2)
                .any(|w| w == ["--output-format", "stream-json"])
        );
        assert!(
            args.windows(2)
                .any(|w| w == ["--input-format", "stream-json"])
        );
        // Without an MCP binding, no MCP flags are emitted.
        assert!(!args.iter().any(|a| a == "--mcp-config"));
    }

    #[test]
    fn command_with_mcp_appends_strict_http_config() {
        let binding = McpBinding::new("notesmith", "http://127.0.0.1:27183/mcp-ro/work");
        let (_, args) = ClaudeCodeAdapter::new("claude")
            .with_mcp(binding.clone())
            .command();

        let idx = args
            .iter()
            .position(|a| a == "--mcp-config")
            .expect("--mcp-config present");
        assert_eq!(args[idx + 1], binding.claude_config_json());
        assert!(args.iter().any(|a| a == "--strict-mcp-config"));
    }
}
