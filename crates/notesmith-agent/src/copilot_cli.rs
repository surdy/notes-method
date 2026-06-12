//! [`LineAdapter`] for the GitHub Copilot CLI.
//!
//! Copilot CLI's programmatic mode (`copilot -p "<prompt>"`) runs a single
//! prompt to completion and prints **human-readable text** to stdout, then
//! exits — there is no documented stable structured (JSONL) streaming schema.
//! This adapter therefore treats stdout as a plain-text stream: each non-empty
//! line becomes an [`AgentEvent::AgentMessageDelta`], and end-of-stream (process
//! EOF) ends the session. A Notesmith session maps to one Copilot turn.
//!
//! Because the output is unstructured, tool calls are not surfaced as
//! structured [`AgentEvent::ToolCall`]s; they appear inline in the assistant
//! text exactly as Copilot CLI prints them.
//!
//! Per ADR 0009 the parser is tolerant by construction: any byte sequence is a
//! valid line and never panics.

use crate::adapter::{Launch, LineAdapter, PromptDelivery};
use crate::event::AgentEvent;
use crate::mcp::McpBinding;

/// Default binary name for the GitHub Copilot CLI.
pub const DEFAULT_BIN: &str = "copilot";

/// [`LineAdapter`] for the GitHub Copilot CLI (`copilot -p`).
#[derive(Debug, Clone)]
pub struct CopilotCliAdapter {
    bin: String,
    mcp: Option<McpBinding>,
}

impl Default for CopilotCliAdapter {
    fn default() -> Self {
        Self::new(DEFAULT_BIN)
    }
}

impl CopilotCliAdapter {
    /// Build an adapter that launches the given binary (path or name).
    pub fn new(bin: impl Into<String>) -> Self {
        Self {
            bin: bin.into(),
            mcp: None,
        }
    }

    /// Auto-wire the agent to a Notesmith MCP endpoint (ADR 0011 Phase C).
    ///
    /// Copilot CLI augments its MCP servers per session via
    /// `--additional-mcp-config`, so the binding is added to the launch command.
    pub fn with_mcp(mut self, binding: McpBinding) -> Self {
        self.mcp = Some(binding);
        self
    }

    /// Flags shared by every non-interactive Copilot invocation.
    fn base_args(&self) -> Vec<String> {
        let mut args = vec![
            "--allow-all-tools".to_string(),
            "--no-color".to_string(),
            "--log-level".to_string(),
            "none".to_string(),
        ];
        if let Some(binding) = &self.mcp {
            args.push("--additional-mcp-config".to_string());
            args.push(binding.claude_config_json());
        }
        args
    }
}

impl LineAdapter for CopilotCliAdapter {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.trim().is_empty() {
            return Vec::new();
        }
        vec![AgentEvent::AgentMessageDelta {
            text: trimmed.to_string(),
        }]
    }

    fn encode_user_message(&self, text: &str) -> Vec<u8> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        bytes
    }

    fn command(&self) -> (String, Vec<String>) {
        (self.bin.clone(), self.base_args())
    }

    fn launch(&self) -> Launch {
        // `copilot -p <text>` runs one prompt to completion and exits; the
        // prompt is a command argument, not stdin.
        Launch::OneShot(PromptDelivery::Arg)
    }

    fn command_for_prompt(&self, prompt: &str) -> (String, Vec<String>) {
        let mut args = self.base_args();
        args.push("--prompt".to_string());
        args.push(prompt.to_string());
        (self.bin.clone(), args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter() -> CopilotCliAdapter {
        CopilotCliAdapter::default()
    }

    #[test]
    fn each_text_line_becomes_a_delta() {
        assert_eq!(
            adapter().parse_line("Here is a summary."),
            vec![AgentEvent::AgentMessageDelta {
                text: "Here is a summary.".to_string()
            }]
        );
    }

    #[test]
    fn blank_lines_are_ignored() {
        assert!(adapter().parse_line("").is_empty());
        assert!(adapter().parse_line("   ").is_empty());
        assert!(adapter().parse_line("\r\n").is_empty());
    }

    #[test]
    fn trailing_newlines_are_trimmed() {
        assert_eq!(
            adapter().parse_line("line\r\n"),
            vec![AgentEvent::AgentMessageDelta {
                text: "line".to_string()
            }]
        );
    }

    #[test]
    fn pathological_lines_never_panic() {
        let mut a = adapter();
        for line in ["", "\0\0\0", "{not json", "𝔘𝔫𝔦𝔠𝔬𝔡𝔢", &"x".repeat(100_000)]
        {
            let _ = a.parse_line(line);
        }
    }

    #[test]
    fn command_uses_default_binary_and_noninteractive_flags() {
        let (program, args) = adapter().command();
        assert_eq!(program, DEFAULT_BIN);
        assert!(args.iter().any(|a| a == "--allow-all-tools"));
        assert!(args.iter().any(|a| a == "--no-color"));
    }

    #[test]
    fn command_honors_binary_override() {
        let (program, _) = CopilotCliAdapter::new("/opt/copilot").command();
        assert_eq!(program, "/opt/copilot");
    }

    #[test]
    fn launch_is_one_shot_with_prompt_arg() {
        use crate::adapter::{Launch, PromptDelivery};
        assert_eq!(adapter().launch(), Launch::OneShot(PromptDelivery::Arg));
    }

    #[test]
    fn command_for_prompt_passes_prompt_as_argument() {
        let (program, args) = adapter().command_for_prompt("summarize my notes");
        assert_eq!(program, DEFAULT_BIN);
        let idx = args
            .iter()
            .position(|a| a == "--prompt")
            .expect("--prompt present");
        assert_eq!(args[idx + 1], "summarize my notes");
        assert!(args.iter().any(|a| a == "--allow-all-tools"));
    }

    #[test]
    fn encode_user_message_appends_newline() {
        assert_eq!(adapter().encode_user_message("hi"), b"hi\n");
    }

    #[test]
    fn with_mcp_adds_additional_mcp_config() {
        let binding = McpBinding::new("notesmith", "http://h/mcp/x");
        let (_, args) = adapter().with_mcp(binding.clone()).command_for_prompt("go");
        let idx = args
            .iter()
            .position(|a| a == "--additional-mcp-config")
            .expect("--additional-mcp-config present");
        assert_eq!(args[idx + 1], binding.claude_config_json());
    }
}
