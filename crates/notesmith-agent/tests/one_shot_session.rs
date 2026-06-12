//! Integration tests for [`OneShotProcessSession`] against real child processes.
//!
//! Single-shot agents (Codex `exec`, Copilot CLI) spawn one process per prompt.
//! These tests use fake agents (`sh -c`) to exercise both prompt-delivery modes
//! — argument and stdin — through the spawn → read → parse → EOF pipeline
//! without depending on a real agent binary.

use notesmith_agent::{
    AgentEvent, AgentSession, Launch, LineAdapter, OneShotProcessSession, PromptDelivery,
};

/// Fake one-shot agent. Each output line is treated as plain assistant text;
/// the launch command and prompt delivery are configurable.
#[derive(Clone)]
struct FakeOneShot {
    program: String,
    args: Vec<String>,
    prompt_args: Vec<String>,
    delivery: PromptDelivery,
}

impl LineAdapter for FakeOneShot {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
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
        (self.program.clone(), self.args.clone())
    }

    fn command_for_prompt(&self, prompt: &str) -> (String, Vec<String>) {
        let mut args = self.prompt_args.clone();
        args.push(prompt.to_string());
        (self.program.clone(), args)
    }

    fn launch(&self) -> Launch {
        Launch::OneShot(self.delivery)
    }
}

#[tokio::test]
async fn one_shot_arg_delivery_echoes_prompt_argument() {
    // The fake agent echoes its last argument (the prompt) back as a line.
    let adapter = FakeOneShot {
        program: "sh".to_string(),
        args: Vec::new(),
        prompt_args: vec![
            "-c".to_string(),
            r#"printf '%s\n' "$1""#.to_string(),
            "sh".to_string(),
        ],
        delivery: PromptDelivery::Arg,
    };

    let mut session = OneShotProcessSession::new(adapter, None);
    session.send("hello from arg").await.expect("send prompt");

    let mut texts = Vec::new();
    while let Some(event) = session.next_event().await {
        if let AgentEvent::AgentMessageDelta { text } = event {
            texts.push(text);
        }
    }
    assert_eq!(texts, vec!["hello from arg".to_string()]);
}

#[tokio::test]
async fn one_shot_stdin_delivery_reads_prompt_from_stdin_then_eofs() {
    // The fake agent echoes everything it reads from stdin; it only completes
    // once stdin is closed, proving the session closes the pipe.
    let adapter = FakeOneShot {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "cat".to_string()],
        prompt_args: Vec::new(),
        delivery: PromptDelivery::Stdin,
    };

    let mut session = OneShotProcessSession::new(adapter, None);
    session.send("hello from stdin").await.expect("send prompt");

    let mut texts = Vec::new();
    while let Some(event) = session.next_event().await {
        if let AgentEvent::AgentMessageDelta { text } = event {
            texts.push(text);
        }
    }
    assert_eq!(texts, vec!["hello from stdin".to_string()]);
}

#[tokio::test]
async fn one_shot_session_ends_after_a_single_turn() {
    let adapter = FakeOneShot {
        program: "sh".to_string(),
        args: Vec::new(),
        prompt_args: vec![
            "-c".to_string(),
            r#"printf 'done\n'"#.to_string(),
            "sh".to_string(),
        ],
        delivery: PromptDelivery::Arg,
    };

    let mut session = OneShotProcessSession::new(adapter, None);
    session.send("go").await.expect("send prompt");
    // Drain the turn.
    while session.next_event().await.is_some() {}
    // A second send is a no-op and produces no further events.
    session.send("again").await.expect("second send is a no-op");
    assert!(session.next_event().await.is_none());
}

#[tokio::test]
async fn one_shot_missing_binary_is_a_clean_error_not_a_panic() {
    let adapter = FakeOneShot {
        program: "notesmith-agent-does-not-exist".to_string(),
        args: Vec::new(),
        prompt_args: vec!["x".to_string()],
        delivery: PromptDelivery::Arg,
    };

    let mut session = OneShotProcessSession::new(adapter, None);
    assert!(session.send("hello").await.is_err());
}
