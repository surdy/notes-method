//! [`LineAdapter`] for OpenAI Codex's `codex exec --json` transport.
//!
//! `codex exec --json` emits one JSON object per line (JSONL). The schema is the
//! Codex thread-event model: a `thread.started` preamble, `turn.started` /
//! `turn.completed` / `turn.failed` bookends, and a stream of `item.started` /
//! `item.updated` / `item.completed` events wrapping typed items
//! (`agent_message`, `reasoning`, `command_execution`, `mcp_tool_call`,
//! `file_change`, `web_search`, `todo_list`, `error`).
//!
//! `codex exec` is single-turn: it runs one prompt to completion and exits.
//! A Notesmith session therefore maps to one turn; `turn.completed` is surfaced
//! as [`AgentEvent::Done`] and process EOF ends the session.
//!
//! Per ADR 0009 the parser is tolerant: malformed lines yield a single
//! non-fatal [`AgentEvent::Error`] and unrecognized shapes are ignored.

use serde_json::{Value, json};

use crate::adapter::{Launch, LineAdapter, PromptDelivery};
use crate::event::{AgentEvent, ToolCall, ToolResult};
use crate::mcp::McpBinding;

/// Default binary name for Codex.
pub const DEFAULT_BIN: &str = "codex";

/// [`LineAdapter`] for `codex exec --json`.
#[derive(Debug, Clone)]
pub struct CodexAdapter {
    bin: String,
    mcp: Option<McpBinding>,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new(DEFAULT_BIN)
    }
}

impl CodexAdapter {
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

impl LineAdapter for CodexAdapter {
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
            Some("thread.started") => vec![AgentEvent::Status {
                message: "Codex session started".to_string(),
            }],
            Some("turn.completed") => vec![AgentEvent::Done { result: None }],
            Some("turn.failed") => vec![AgentEvent::Error {
                message: error_message(value.get("error"), "the turn failed"),
            }],
            Some("error") => vec![AgentEvent::Error {
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("agent reported an error")
                    .to_string(),
            }],
            Some(kind @ ("item.started" | "item.completed")) => parse_item(kind, value.get("item")),
            // `turn.started`, `item.updated` and unknown events carry no
            // renderable content; ignore them and keep the session running.
            _ => Vec::new(),
        }
    }

    fn encode_user_message(&self, text: &str) -> Vec<u8> {
        // `codex exec` reads the prompt from stdin (the `-` positional). Send the
        // raw text followed by a newline.
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        bytes
    }

    fn command(&self) -> (String, Vec<String>) {
        let mut args = vec![
            "exec".to_string(),
            "--json".to_string(),
            "--skip-git-repo-check".to_string(),
        ];
        if let Some(binding) = &self.mcp {
            for override_ in binding.codex_config_overrides() {
                args.push("-c".to_string());
                args.push(override_);
            }
        }
        // Read the prompt from stdin.
        args.push("-".to_string());
        (self.bin.clone(), args)
    }

    fn launch(&self) -> Launch {
        // `codex exec -` reads the whole prompt from stdin and exits after one
        // turn; the runner writes the prompt and closes stdin.
        Launch::OneShot(PromptDelivery::Stdin)
    }
}

fn error_message(error: Option<&Value>, fallback: &str) -> String {
    error
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .or_else(|| error.and_then(Value::as_str))
        .unwrap_or(fallback)
        .to_string()
}

fn parse_item(envelope: &str, item: Option<&Value>) -> Vec<AgentEvent> {
    let Some(item) = item else {
        return Vec::new();
    };
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    let id = item.get("id").and_then(Value::as_str).map(str::to_string);
    let completed = envelope == "item.completed";

    match item_type {
        "agent_message" => {
            if completed {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    return vec![AgentEvent::AgentMessageDelta {
                        text: text.to_string(),
                    }];
                }
            }
            Vec::new()
        }
        "command_execution" => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if completed {
                let exit_code = item.get("exit_code").and_then(Value::as_i64);
                let status = item.get("status").and_then(Value::as_str);
                let is_error = exit_code.is_some_and(|c| c != 0) || status == Some("failed");
                let content = item
                    .get("aggregated_output")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                vec![AgentEvent::ToolResult(ToolResult {
                    id,
                    content,
                    is_error,
                })]
            } else {
                vec![AgentEvent::ToolCall(ToolCall {
                    id,
                    name: "shell".to_string(),
                    args: json!({ "command": command }),
                })]
            }
        }
        "mcp_tool_call" => {
            let server = item.get("server").and_then(Value::as_str).unwrap_or("");
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("");
            let name = if server.is_empty() {
                tool.to_string()
            } else {
                format!("{server}.{tool}")
            };
            if completed {
                let error = item.get("error").and_then(Value::as_str);
                let content = error
                    .map(str::to_string)
                    .or_else(|| render_value(item.get("result")))
                    .unwrap_or_default();
                vec![AgentEvent::ToolResult(ToolResult {
                    id,
                    content,
                    is_error: error.is_some()
                        || item.get("status").and_then(Value::as_str) == Some("failed"),
                })]
            } else {
                vec![AgentEvent::ToolCall(ToolCall {
                    id,
                    name,
                    args: item.get("arguments").cloned().unwrap_or(Value::Null),
                })]
            }
        }
        "file_change" => {
            if completed {
                let is_error = item.get("status").and_then(Value::as_str) == Some("failed");
                vec![AgentEvent::ToolResult(ToolResult {
                    id,
                    content: render_value(item.get("changes")).unwrap_or_default(),
                    is_error,
                })]
            } else {
                vec![AgentEvent::ToolCall(ToolCall {
                    id,
                    name: "file_change".to_string(),
                    args: item.get("changes").cloned().unwrap_or(Value::Null),
                })]
            }
        }
        "web_search" => {
            if completed {
                let query = item.get("query").and_then(Value::as_str).unwrap_or("");
                vec![AgentEvent::Status {
                    message: format!("searched the web: {query}"),
                }]
            } else {
                Vec::new()
            }
        }
        "error" => vec![AgentEvent::Error {
            message: item
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("agent reported an error")
                .to_string(),
        }],
        // `reasoning`, `todo_list`, and unknown item types carry no
        // chat-renderable content here.
        _ => Vec::new(),
    }
}

fn render_value(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CodexAdapter {
        CodexAdapter::default()
    }

    #[test]
    fn thread_started_becomes_status() {
        let events = adapter().parse_line(r#"{"type":"thread.started","thread_id":"t1"}"#);
        assert_eq!(
            events,
            vec![AgentEvent::Status {
                message: "Codex session started".to_string()
            }]
        );
    }

    #[test]
    fn agent_message_completed_becomes_delta() {
        let line = r#"{"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"Hello!"}}"#;
        assert_eq!(
            adapter().parse_line(line),
            vec![AgentEvent::AgentMessageDelta {
                text: "Hello!".to_string()
            }]
        );
    }

    #[test]
    fn agent_message_only_emits_on_completed() {
        let started = r#"{"type":"item.started","item":{"type":"agent_message","text":"partial"}}"#;
        assert!(adapter().parse_line(started).is_empty());
    }

    #[test]
    fn command_execution_round_trip() {
        let mut a = adapter();
        let started = r#"{"type":"item.started","item":{"id":"c1","type":"command_execution","command":"ls"}}"#;
        assert_eq!(
            a.parse_line(started),
            vec![AgentEvent::ToolCall(ToolCall {
                id: Some("c1".to_string()),
                name: "shell".to_string(),
                args: json!({ "command": "ls" }),
            })]
        );

        let completed = r#"{"type":"item.completed","item":{"id":"c1","type":"command_execution","command":"ls","aggregated_output":"a.md\n","exit_code":0,"status":"completed"}}"#;
        assert_eq!(
            a.parse_line(completed),
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("c1".to_string()),
                content: "a.md\n".to_string(),
                is_error: false,
            })]
        );
    }

    #[test]
    fn command_execution_nonzero_exit_is_error() {
        let line = r#"{"type":"item.completed","item":{"id":"c2","type":"command_execution","command":"false","aggregated_output":"","exit_code":1,"status":"completed"}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("c2".to_string()),
                content: String::new(),
                is_error: true,
            })]
        );
    }

    #[test]
    fn mcp_tool_call_round_trip() {
        let mut a = adapter();
        let started = r#"{"type":"item.started","item":{"id":"m1","type":"mcp_tool_call","server":"notesmith","tool":"search_notes","arguments":{"q":"todo"}}}"#;
        assert_eq!(
            a.parse_line(started),
            vec![AgentEvent::ToolCall(ToolCall {
                id: Some("m1".to_string()),
                name: "notesmith.search_notes".to_string(),
                args: json!({ "q": "todo" }),
            })]
        );

        let completed = r#"{"type":"item.completed","item":{"id":"m1","type":"mcp_tool_call","server":"notesmith","tool":"search_notes","result":{"hits":2},"status":"completed"}}"#;
        assert_eq!(
            a.parse_line(completed),
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("m1".to_string()),
                content: "{\"hits\":2}".to_string(),
                is_error: false,
            })]
        );
    }

    #[test]
    fn mcp_tool_call_error_marks_result() {
        let line = r#"{"type":"item.completed","item":{"id":"m2","type":"mcp_tool_call","server":"notesmith","tool":"x","error":"boom","status":"failed"}}"#;
        let events = adapter().parse_line(line);
        assert_eq!(
            events,
            vec![AgentEvent::ToolResult(ToolResult {
                id: Some("m2".to_string()),
                content: "boom".to_string(),
                is_error: true,
            })]
        );
    }

    #[test]
    fn turn_completed_becomes_done() {
        let line = r#"{"type":"turn.completed","usage":{"input_tokens":10}}"#;
        assert_eq!(
            adapter().parse_line(line),
            vec![AgentEvent::Done { result: None }]
        );
    }

    #[test]
    fn turn_failed_becomes_error() {
        let line = r#"{"type":"turn.failed","error":{"message":"rate limited"}}"#;
        assert_eq!(
            adapter().parse_line(line),
            vec![AgentEvent::Error {
                message: "rate limited".to_string()
            }]
        );
    }

    #[test]
    fn top_level_error_becomes_error() {
        let line = r#"{"type":"error","message":"fatal"}"#;
        assert_eq!(
            adapter().parse_line(line),
            vec![AgentEvent::Error {
                message: "fatal".to_string()
            }]
        );
    }

    #[test]
    fn reasoning_and_unknown_items_are_ignored() {
        assert!(
            adapter()
                .parse_line(
                    r#"{"type":"item.completed","item":{"type":"reasoning","text":"thinking"}}"#
                )
                .is_empty()
        );
        assert!(
            adapter()
                .parse_line(r#"{"type":"turn.started"}"#)
                .is_empty()
        );
        assert!(
            adapter()
                .parse_line(r#"{"type":"item.updated","item":{"type":"agent_message","text":"x"}}"#)
                .is_empty()
        );
    }

    #[test]
    fn malformed_json_yields_error_without_panicking() {
        let events = adapter().parse_line("{not json");
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], AgentEvent::Error { .. }));
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
            r#"{"type":"item.completed"}"#,
            r#"{"type":"item.completed","item":{}}"#,
            r#"{"type":"item.completed","item":{"type":"command_execution"}}"#,
            r#"{"type":"turn.failed"}"#,
            &"x".repeat(100_000),
        ] {
            let _ = a.parse_line(line);
        }
    }

    #[test]
    fn command_reads_prompt_from_stdin_with_json_flag() {
        let (program, args) = CodexAdapter::new("/usr/bin/codex").command();
        assert_eq!(program, "/usr/bin/codex");
        assert_eq!(args.first().map(String::as_str), Some("exec"));
        assert!(args.iter().any(|a| a == "--json"));
        assert!(args.iter().any(|a| a == "--skip-git-repo-check"));
        assert_eq!(args.last().map(String::as_str), Some("-"));
        assert!(!args.iter().any(|a| a == "-c"));
    }

    #[test]
    fn command_with_mcp_registers_http_server_via_config_override() {
        let binding = McpBinding::new("notesmith", "http://127.0.0.1:27183/mcp/work");
        let (_, args) = CodexAdapter::new("codex").with_mcp(binding).command();
        let idx = args.iter().position(|a| a == "-c").expect("-c present");
        assert_eq!(
            args[idx + 1],
            "mcp_servers.notesmith.url=\"http://127.0.0.1:27183/mcp/work\""
        );
        // Prompt still read from stdin after the override.
        assert_eq!(args.last().map(String::as_str), Some("-"));
    }

    #[test]
    fn encode_user_message_appends_newline() {
        let bytes = adapter().encode_user_message("summarize my notes");
        assert_eq!(bytes, b"summarize my notes\n");
    }

    #[test]
    fn launch_is_one_shot_over_stdin() {
        use crate::adapter::{Launch, PromptDelivery};
        assert_eq!(adapter().launch(), Launch::OneShot(PromptDelivery::Stdin));
    }
}
