//! Integration tests for [`AcpSession`] against a fake ACP agent.
//!
//! ACP (Agent Client Protocol) is JSON-RPC 2.0 over newline-delimited JSON on a
//! child process's stdio. These tests drive a small Python "agent" that speaks
//! just enough of the protocol — `initialize`, `session/new`, `session/prompt`
//! with streamed `session/update` notifications, and a `session/request_permission`
//! callback — so the spawn → handshake → prompt → stream → terminal pipeline is
//! exercised end-to-end without depending on a real agent binary.

use notesmith_agent::{AcpSession, AgentEvent, AgentSession};

/// A fake ACP agent. Reads newline-delimited JSON-RPC requests on stdin and:
/// - answers `initialize` and `session/new` (returning `sessionId`),
/// - on `session/prompt`, streams an `agent_message_chunk` then answers with a
///   `stopReason`. When the prompt text contains `PERMISSION`, it first issues a
///   `session/request_permission` request and echoes the chosen optionId back as
///   an assistant chunk so the test can assert the scope decision.
const FAKE_AGENT: &str = r#"
import sys, json

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

pending = {}
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "fake-session"}})
    elif method == "session/prompt":
        text = msg["params"]["prompt"][-1]["text"]
        if "PERMISSION" in text:
            # Ask the client to approve; reuse a fixed request id (100). The
            # shape mirrors a real ACP `session/request_permission` request
            # (sessionId + toolCall + named options), which the typed client
            # must deserialize before it can answer.
            send({
                "jsonrpc": "2.0", "id": 100, "method": "session/request_permission",
                "params": {
                    "sessionId": "fake-session",
                    "toolCall": {"toolCallId": "perm-1"},
                    "options": [
                        {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                        {"optionId": "no", "name": "Reject", "kind": "reject_once"},
                    ],
                },
            })
            pending[100] = mid
            continue
        send({
            "jsonrpc": "2.0", "method": "session/update",
            "params": {"sessionId": "fake-session", "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "ACK:" + text},
            }},
        })
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
    elif mid in pending and "result" in msg:
        # Response to our permission request: echo the chosen option, then finish.
        prompt_id = pending.pop(mid)
        choice = msg["result"]["outcome"].get("optionId", "cancelled")
        send({
            "jsonrpc": "2.0", "method": "session/update",
            "params": {"sessionId": "fake-session", "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "CHOICE:" + choice},
            }},
        })
        send({"jsonrpc": "2.0", "id": prompt_id, "result": {"stopReason": "end_turn"}})
"#;

fn fake_agent_session(read_only: bool) -> AcpSession {
    let session = AcpSession::new(
        "python3",
        vec!["-u".to_string(), "-c".to_string(), FAKE_AGENT.to_string()],
    );
    session.read_only(read_only)
}

async fn drain_until_done(session: &mut AcpSession) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = session.next_event().await {
        let done = matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. });
        events.push(event);
        if done {
            break;
        }
    }
    events
}

#[tokio::test]
async fn handshake_then_prompt_streams_a_delta_and_done() {
    let mut session = fake_agent_session(true);
    session.send("hello").await.expect("send prompt");

    let events = drain_until_done(&mut session).await;
    assert_eq!(
        events,
        vec![
            AgentEvent::AgentMessageDelta {
                text: "ACK:hello".to_string()
            },
            AgentEvent::Done { result: None },
        ]
    );
}

#[tokio::test]
async fn acp_session_is_multi_turn_on_one_session_id() {
    let mut session = fake_agent_session(true);

    session.send("first").await.expect("first send");
    let first = drain_until_done(&mut session).await;
    assert!(first.contains(&AgentEvent::AgentMessageDelta {
        text: "ACK:first".to_string()
    }));

    // A second prompt reuses the same session (no re-handshake) and completes.
    session.send("second").await.expect("second send");
    let second = drain_until_done(&mut session).await;
    assert!(second.contains(&AgentEvent::AgentMessageDelta {
        text: "ACK:second".to_string()
    }));
    assert!(second.contains(&AgentEvent::Done { result: None }));
}

#[tokio::test]
async fn read_write_scope_approves_permission_requests() {
    let mut session = fake_agent_session(false);
    session.send("please PERMISSION").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(events.contains(&AgentEvent::AgentMessageDelta {
        text: "CHOICE:yes".to_string()
    }));
}

#[tokio::test]
async fn read_only_scope_rejects_permission_requests() {
    let mut session = fake_agent_session(true);
    session.send("please PERMISSION").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(events.contains(&AgentEvent::AgentMessageDelta {
        text: "CHOICE:no".to_string()
    }));
}

#[tokio::test]
async fn missing_binary_is_a_clean_error_not_a_panic() {
    let mut session = AcpSession::new("notesmith-acp-does-not-exist", vec!["--acp".to_string()]);
    assert!(session.send("hi").await.is_err());
}

#[tokio::test]
async fn missing_codex_adapter_error_includes_setup_hint() {
    // The default `codex-acp` adapter is not installed in CI; the failure must
    // be a clean error carrying actionable setup guidance, not a panic.
    let mut session = AcpSession::codex(Some("notesmith-codex-acp-missing"));
    let error = session
        .send("hi")
        .await
        .expect_err("missing adapter errors");
    assert!(
        error.to_string().contains("codex-acp"),
        "error should mention the adapter: {error}"
    );
}

/// A fake agent that interleaves malformed output with a well-formed turn:
/// a non-JSON line, an unknown `sessionUpdate` kind, and an
/// `agent_message_chunk` carrying non-text content — none of which should panic
/// the client (ADR 0009). After the noise it streams a real text delta and ends
/// the turn normally, so the session must still degrade to a valid result.
const MALFORMED_AGENT: &str = r#"
import sys, json

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def raw(line):
    sys.stdout.write(line + "\n")
    sys.stdout.flush()

def update(u):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": "fake-session", "update": u}})

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method = msg.get("method")
    mid = msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "fake-session"}})
    elif method == "session/prompt":
        raw("this is not json at all <<garbage>>")
        update({"sessionUpdate": "totally_unknown_kind", "blah": 1})
        update({"sessionUpdate": "agent_message_chunk",
                "content": {"type": "image", "data": "AAAA", "mimeType": "image/png"}})
        update({"sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "RECOVERED"}})
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
"#;

#[tokio::test]
async fn malformed_agent_output_degrades_without_panicking() {
    let mut session = AcpSession::new(
        "python3",
        vec![
            "-u".to_string(),
            "-c".to_string(),
            MALFORMED_AGENT.to_string(),
        ],
    );
    session.send("hello").await.expect("send prompt");

    // The garbage line, the unknown update kind, and the non-text chunk are all
    // ignored; the session still recovers to the real text delta and a clean
    // turn end without panicking or hanging.
    let events = drain_until_done(&mut session).await;
    assert!(
        events.contains(&AgentEvent::AgentMessageDelta {
            text: "RECOVERED".to_string()
        }),
        "expected the recovered delta, got {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. })),
        "session should reach a terminal event, got {events:?}"
    );
}
