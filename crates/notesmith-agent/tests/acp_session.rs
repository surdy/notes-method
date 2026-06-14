//! Integration tests for [`AcpSession`] against a fake ACP agent.
//!
//! ACP (Agent Client Protocol) is JSON-RPC 2.0 over newline-delimited JSON on a
//! child process's stdio. These tests drive a small Python "agent" that speaks
//! just enough of the protocol — `initialize`, `session/new`, `session/prompt`
//! with streamed `session/update` notifications, and a `session/request_permission`
//! callback — so the spawn → handshake → prompt → stream → terminal pipeline is
//! exercised end-to-end without depending on a real agent binary.

use std::sync::Arc;

use futures::future::BoxFuture;
use notesmith_agent::{
    AcpSession, AgentEvent, AgentSession, EditorContext, PermissionDecider, PermissionDecision,
    PermissionRequest, VaultSummary,
};

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
                    "toolCall": {"toolCallId": "perm-1", "title": "create_note"},
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

/// A decider that always returns a fixed decision, standing in for the desktop
/// chat UI's permission prompt (Phase 8).
struct FixedDecider(PermissionDecision);
impl PermissionDecider for FixedDecider {
    fn decide(&self, _request: PermissionRequest) -> BoxFuture<'static, PermissionDecision> {
        let decision = self.0;
        Box::pin(async move { decision })
    }
}

fn fake_agent_session_with_decider(read_only: bool, decision: PermissionDecision) -> AcpSession {
    fake_agent_session(read_only).with_permission_decider(Arc::new(FixedDecider(decision)))
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
async fn read_write_with_allow_decider_approves_a_write() {
    // A read-write session prompts per write; an "allow once" decision lets the
    // write through (CHOICE:yes).
    let mut session = fake_agent_session_with_decider(false, PermissionDecision::AllowOnce);
    session.send("please PERMISSION").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(events.contains(&AgentEvent::AgentMessageDelta {
        text: "CHOICE:yes".to_string()
    }));
}

#[tokio::test]
async fn read_write_denies_writes_without_an_explicit_grant() {
    // The default decider denies, so a read-write write is refused until the
    // user explicitly approves it (CHOICE:no).
    let mut session = fake_agent_session(false);
    session.send("please PERMISSION").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(events.contains(&AgentEvent::AgentMessageDelta {
        text: "CHOICE:no".to_string()
    }));
}

#[tokio::test]
async fn read_only_scope_rejects_permission_requests() {
    // A read-only session hard-denies writes even when a decider would allow.
    let mut session = fake_agent_session_with_decider(true, PermissionDecision::AllowAlways);
    session.send("please PERMISSION").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(events.contains(&AgentEvent::AgentMessageDelta {
        text: "CHOICE:no".to_string()
    }));
}

/// A fake agent that echoes the *full* prompt back — the joined text of every
/// content block — as an assistant chunk, so a test can assert exactly what
/// context the client injected into each turn.
const ECHO_PROMPT_AGENT: &str = r#"
import sys, json

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

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
        joined = "\u0000".join(b.get("text", "") for b in msg["params"]["prompt"])
        send({
            "jsonrpc": "2.0", "method": "session/update",
            "params": {"sessionId": "fake-session", "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": joined},
            }},
        })
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
"#;

fn echo_prompt_session() -> AcpSession {
    AcpSession::new(
        "python3",
        vec![
            "-u".to_string(),
            "-c".to_string(),
            ECHO_PROMPT_AGENT.to_string(),
        ],
    )
}

/// Drain to `Done` and return the single echoed prompt (joined block texts).
async fn echoed_prompt(session: &mut AcpSession) -> String {
    drain_until_done(session)
        .await
        .into_iter()
        .find_map(|event| match event {
            AgentEvent::AgentMessageDelta { text } => Some(text),
            _ => None,
        })
        .expect("the echo agent emits one delta per turn")
}

#[tokio::test]
async fn first_turn_injects_preamble_with_skill_and_summary() {
    let mut session = echo_prompt_session()
        .with_skill(Some("Always tag meeting notes with #meeting.".to_string()))
        .with_vault_summary(VaultSummary {
            name: "Research".to_string(),
            note_count: 87,
            top_tags: vec!["#idea".to_string()],
            top_folders: vec!["daily/".to_string()],
        });

    session.send("hello there").await.expect("send");
    let prompt = echoed_prompt(&mut session).await;

    // Preamble carries the bounded vault summary and the skill body, ahead of
    // the user's message (which is the final block).
    assert!(prompt.contains("Vault \"Research\": 87 notes."));
    assert!(prompt.contains("Always tag meeting notes with #meeting."));
    assert!(prompt.contains(".notesmith/skill.md"));
    assert!(prompt.ends_with("hello there"));
}

#[tokio::test]
async fn preamble_is_sent_once_and_editor_context_rides_each_turn() {
    let mut session = echo_prompt_session().with_skill(Some("SKILLDOC".to_string()));

    // Turn 1: preamble + editor context + message.
    session
        .send_with_context(
            "first",
            EditorContext {
                active_path: Some("projects/acp.md".to_string()),
                active_title: Some("ACP".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("send 1");
    let first = echoed_prompt(&mut session).await;
    assert!(first.contains("SKILLDOC"), "turn 1 carries the preamble");
    assert!(
        first.contains("Active note: ACP (projects/acp.md)"),
        "turn 1 carries editor context"
    );
    assert!(first.ends_with("first"));

    // Turn 2: no preamble (sent once), no editor context this time.
    session.send("second").await.expect("send 2");
    let second = echoed_prompt(&mut session).await;
    assert!(
        !second.contains("SKILLDOC"),
        "the preamble is injected only on the first turn"
    );
    assert!(
        !second.contains("Active note"),
        "absent editor state degrades to no context block"
    );
    assert!(second.ends_with("second"));
}

/// A fake agent that, on every prompt, issues an `fs/write_text_file` request
/// back to the client and reports whether the client answered with a result or
/// an error. It exercises the break-glass local-I/O wiring end-to-end (the
/// capability advertisement plus the registered fs handler).
const BREAK_GLASS_AGENT: &str = r#"
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
        send({
            "jsonrpc": "2.0", "id": 200, "method": "fs/write_text_file",
            "params": {
                "sessionId": "fake-session",
                "path": "bg.md",
                "content": "hi from agent",
            },
        })
        pending[200] = mid
    elif mid in pending:
        prompt_id = pending.pop(mid)
        note = "WROTE:ok" if "result" in msg else "WROTE:err"
        send({
            "jsonrpc": "2.0", "method": "session/update",
            "params": {"sessionId": "fake-session", "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": note},
            }},
        })
        send({"jsonrpc": "2.0", "id": prompt_id, "result": {"stopReason": "end_turn"}})
"#;

fn break_glass_session(dir: &std::path::Path) -> AcpSession {
    AcpSession::new(
        "python3",
        vec![
            "-u".to_string(),
            "-c".to_string(),
            BREAK_GLASS_AGENT.to_string(),
        ],
    )
    .in_dir(Some(dir.to_path_buf()))
    .read_only(false)
}

#[tokio::test]
async fn break_glass_on_lets_the_agent_write_within_the_vault() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = break_glass_session(dir.path()).with_local_io(true);
    session.send("write a note").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(
        events.contains(&AgentEvent::AgentMessageDelta {
            text: "WROTE:ok".to_string()
        }),
        "expected the write to succeed, got {events:?}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("bg.md")).unwrap(),
        "hi from agent"
    );
}

#[tokio::test]
async fn break_glass_off_refuses_agent_writes() {
    let dir = tempfile::tempdir().unwrap();
    // local_io defaults off: the fs handler reports method-not-found.
    let mut session = break_glass_session(dir.path());
    session.send("write a note").await.expect("send");

    let events = drain_until_done(&mut session).await;
    assert!(
        events.contains(&AgentEvent::AgentMessageDelta {
            text: "WROTE:err".to_string()
        }),
        "expected the write to be refused, got {events:?}"
    );
    assert!(!dir.path().join("bg.md").exists());
}

/// A fake agent that advertises a `model` config option on `session/new` and,
/// when the client applies a selection via `session/set_config_option`, echoes
/// the chosen value back so a test can assert the selection round-tripped.
const MODEL_AGENT: &str = r#"
import sys, json

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

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
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "sessionId": "fake-session",
            "configOptions": [{
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": "gpt-5",
                "options": [
                    {"value": "gpt-5", "name": "GPT-5"},
                    {"value": "sonnet", "name": "Sonnet"},
                ],
            }],
        }})
    elif method == "session/set_config_option":
        # Confirm the change and report the new configOptions current value.
        chosen = msg["params"]["value"]
        send({"jsonrpc": "2.0", "id": mid, "result": {"configOptions": [{
            "id": "model", "name": "Model", "category": "model",
            "type": "select", "currentValue": chosen,
            "options": [
                {"value": "gpt-5", "name": "GPT-5"},
                {"value": "sonnet", "name": "Sonnet"},
            ],
        }]}})
        send({"jsonrpc": "2.0", "method": "session/update", "params": {
            "sessionId": "fake-session", "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "MODEL:" + chosen},
            }}})
    elif method == "session/prompt":
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
"#;

fn model_agent_session() -> AcpSession {
    AcpSession::new(
        "python3",
        vec!["-u".to_string(), "-c".to_string(), MODEL_AGENT.to_string()],
    )
}

#[tokio::test]
async fn model_picker_is_parsed_from_config_options() {
    let mut session = model_agent_session();
    // Selecting starts the session, after which the picker is available.
    session.select_model("sonnet").await.expect("select model");

    let picker = session
        .model_picker()
        .expect("agent advertised a model picker");
    assert_eq!(picker.current(), "gpt-5");
    let ids: Vec<&str> = picker.options().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, vec!["gpt-5", "sonnet"]);
}

#[tokio::test]
async fn selecting_a_model_round_trips_to_the_agent() {
    let mut session = model_agent_session();
    session
        .select_model("sonnet")
        .await
        .expect("select succeeds");
    // A follow-up prompt gives the drain a terminating `Done`; the model
    // confirmation chunk emitted by the set arrives ahead of it.
    session.send("ping").await.expect("ping");

    let events = drain_until_done(&mut session).await;
    assert!(
        events.contains(&AgentEvent::AgentMessageDelta {
            text: "MODEL:sonnet".to_string()
        }),
        "the agent should confirm the applied model, got {events:?}"
    );
}

#[tokio::test]
async fn selecting_an_unknown_model_is_rejected_locally() {
    let mut session = model_agent_session();
    // The picker has no such option, so the client rejects without asking.
    let result = session.select_model("does-not-exist").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn no_picker_when_the_agent_advertises_no_models() {
    // The default fake agent advertises neither configOptions nor modes.
    let mut session = fake_agent_session(true);
    session.send("hello").await.expect("send");
    let _ = drain_until_done(&mut session).await;
    assert!(session.model_picker().is_none());
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

/// End-to-end smoke test against a real `copilot --acp` process.
///
/// Ignored by default: it requires the GitHub Copilot CLI to be installed and
/// authenticated, and it makes a network round-trip. Run on demand with:
///
/// ```text
/// cargo test -p notesmith-agent --test acp_session -- --ignored real_copilot
/// ```
#[tokio::test]
#[ignore = "requires an installed, authenticated `copilot` CLI and network access"]
async fn real_copilot_round_trips_a_turn() {
    let mut session = AcpSession::copilot(None);
    session
        .send("Reply with exactly the word: pong")
        .await
        .expect("copilot handshake + prompt");

    let mut saw_delta = false;
    let mut saw_terminal = false;
    while let Some(event) = session.next_event().await {
        match event {
            AgentEvent::AgentMessageDelta { .. } => saw_delta = true,
            AgentEvent::Done { .. } => {
                saw_terminal = true;
                break;
            }
            AgentEvent::Error { message } => panic!("copilot turn errored: {message}"),
            _ => {}
        }
    }
    assert!(saw_delta, "expected at least one assistant delta");
    assert!(saw_terminal, "expected a clean turn end");
}
