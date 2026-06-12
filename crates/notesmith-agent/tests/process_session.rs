//! Integration test for [`ProcessAgentSession`] against a real child process.
//!
//! Uses a fake agent (`sh -c` emitting canned `stream-json` lines) so the
//! spawn → read → parse → EOF pipeline is exercised without depending on a real
//! `claude` binary being installed.

use notesmith_agent::{
    AgentEvent, AgentSession, ClaudeCodeAdapter, LineAdapter, ProcessAgentSession,
};

#[derive(Clone)]
struct FakeAgent {
    inner: ClaudeCodeAdapter,
    script: String,
}

impl LineAdapter for FakeAgent {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        self.inner.parse_line(line)
    }

    fn encode_user_message(&self, text: &str) -> Vec<u8> {
        self.inner.encode_user_message(text)
    }

    fn command(&self) -> (String, Vec<String>) {
        (
            "sh".to_string(),
            vec!["-c".to_string(), self.script.clone()],
        )
    }
}

#[tokio::test]
async fn process_session_streams_events_from_a_real_child() {
    let script = concat!(
        "printf '%s\\n' ",
        r#"'{"type":"assistant","message":{"content":[{"type":"text","text":"hi from child"}]}}' "#,
        r#"'{"type":"result","is_error":false,"result":"hi from child"}'"#,
    );
    let adapter = FakeAgent {
        inner: ClaudeCodeAdapter::default(),
        script: script.to_string(),
    };

    let mut session = ProcessAgentSession::spawn(adapter).expect("spawn fake agent");

    let mut events = Vec::new();
    while let Some(event) = session.next_event().await {
        events.push(event);
    }

    assert!(
        events.contains(&AgentEvent::AgentMessageDelta {
            text: "hi from child".to_string()
        }),
        "expected the child's assistant text, got {events:?}"
    );
    assert!(
        matches!(events.last(), Some(AgentEvent::Done { .. })),
        "expected the stream to end with Done, got {events:?}"
    );
}

#[tokio::test]
async fn spawning_a_missing_binary_is_a_clean_error_not_a_panic() {
    let adapter = FakeAgent {
        inner: ClaudeCodeAdapter::default(),
        script: String::new(),
    };
    // Override to a binary that does not exist.
    struct Missing(FakeAgent);
    impl LineAdapter for Missing {
        fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
            self.0.parse_line(line)
        }
        fn encode_user_message(&self, text: &str) -> Vec<u8> {
            self.0.encode_user_message(text)
        }
        fn command(&self) -> (String, Vec<String>) {
            ("notesmith-agent-does-not-exist".to_string(), Vec::new())
        }
    }
    impl Clone for Missing {
        fn clone(&self) -> Self {
            Missing(self.0.clone())
        }
    }

    let result = ProcessAgentSession::spawn(Missing(adapter));
    assert!(result.is_err());
}

#[tokio::test]
async fn spawn_in_runs_the_child_in_the_given_working_directory() {
    let dir = std::env::temp_dir().join(format!("notesmith-agent-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    // Canonicalize because macOS /tmp is a symlink to /private/tmp and the
    // child reports the resolved path.
    let canonical = std::fs::canonicalize(&dir).expect("canonicalize temp dir");

    // The fake agent prints its working directory as an assistant text block,
    // so the parsed AgentMessageDelta lets us assert the cwd was applied.
    let script = concat!(
        r#"printf '{"type":"assistant","message":{"content":[{"type":"text","text":"%s"}]}}\n' "$(pwd)"; "#,
        r#"printf '{"type":"result","is_error":false,"result":"ok"}\n'"#,
    );
    let adapter = FakeAgent {
        inner: ClaudeCodeAdapter::default(),
        script: script.to_string(),
    };

    let mut session =
        ProcessAgentSession::spawn_in(adapter, Some(canonical.clone())).expect("spawn in dir");

    let mut texts = Vec::new();
    while let Some(event) = session.next_event().await {
        if let AgentEvent::AgentMessageDelta { text } = event {
            texts.push(text);
        }
    }

    std::fs::remove_dir_all(&dir).ok();
    let reported = texts.join("");
    assert_eq!(
        reported.trim(),
        canonical.to_string_lossy(),
        "child should run in the requested working directory"
    );
}
