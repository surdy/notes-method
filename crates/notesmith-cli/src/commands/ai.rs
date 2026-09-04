//! `notesmith ai` — headless, non-interactive ACP agent commands.
//!
//! These subcommands drive the user's external ACP agent (Copilot/Claude/Codex/
//! Gemini/OpenCode) for scripting and cron, with no human at the keyboard. The
//! agent reaches vault content through Notesmith's MCP tools — over the
//! daemon's Streamable HTTP endpoint when the agent supports HTTP MCP, with the
//! local `notesmith mcp start` stdio bridge as the fallback (ADR 0012, ADR 0015
//! Option A). Notesmith never runs its own chat LLM.
//!
//! HTTP is preferred rather than merely available: GitHub Copilot's ACP mode
//! **rejects** stdio MCP servers supplied by the ACP client (`Rejecting
//! non-http/sse MCP server "<id>" from client`, Copilot CLI 1.0.83-1), so a
//! stdio-only binding leaves a Copilot session with no Notesmith tools at all.
//! The same rejection applies to external stdio `[[mcp.servers]]` entries in a
//! Copilot session; other agents accept them. See the 2026-09-02 amendment to
//! ADR 0012.
//!
//! ## Headless permission safety
//!
//! There is no human to answer ACP `session/request_permission` prompts, so the
//! run is **read-only by default**: the agent binds the daemon's read-only MCP
//! scope and a deny-by-default [`HeadlessDecider`] refuses every write or
//! permission request. Writes are only possible when the operator passes the
//! explicit `--allow-writes` opt-in, which flips the bridge to the read-write
//! scope and lets the decider auto-approve actions. Granting writes to an
//! unattended agent is dangerous — every requested edit is approved without
//! review — so `--allow-writes` should be used sparingly and only against
//! trusted prompts.

use std::path::Path;
use std::sync::Arc;

use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::{Args, Subcommand};
use futures::future::BoxFuture;
use notesmith_agent::{
    AgentEvent, AgentSession, McpBinding, PermissionDecider, PermissionDecision, PermissionRequest,
};
use notesmith_config::{GlobalConfig, detect_vault};

use crate::commands::vault::OutputFormat;

/// Headless ACP agent commands for scripting and cron.
#[derive(Debug, Subcommand)]
pub enum AiCommand {
    /// Summarize a note (vault-relative path) or today's daily note.
    Summarize(SummarizeArgs),
    /// Produce a digest from the current week's notes.
    WeeklyDigest(WeeklyDigestArgs),
    /// Run a named vault prompt (`.notesmith/prompts/<name>.md`) headlessly.
    /// The daemon renders the prompt (executing its `context_queries`); the
    /// agent is driven with the result. Agent-kind `[[jobs]]` run this.
    Prompt(PromptArgs),
}

/// Options shared by every `notes ai` command for selecting and scoping the
/// agent run.
#[derive(Debug, Clone, Args)]
pub struct AgentOpts {
    /// Built-in agent to drive (copilot, claude, codex, gemini, opencode).
    #[arg(long, default_value = "copilot")]
    pub agent: String,
    /// Override the agent binary path (otherwise resolved from PATH).
    #[arg(long)]
    pub agent_bin: Option<String>,
    /// Allow the agent to perform writes. DANGEROUS in headless mode: every
    /// requested edit is auto-approved without human review. Off by default
    /// (the run is read-only and denies all writes).
    #[arg(long)]
    pub allow_writes: bool,
}

/// Arguments for `notes ai summarize`.
#[derive(Debug, Args)]
pub struct SummarizeArgs {
    /// Note path (vault-relative) or the literal `today` for the daily note.
    pub target: String,
    #[command(flatten)]
    pub agent: AgentOpts,
}

/// Arguments for `notes ai weekly-digest`.
#[derive(Debug, Args)]
pub struct WeeklyDigestArgs {
    #[command(flatten)]
    pub agent: AgentOpts,
}

/// Arguments for `notes ai prompt`.
#[derive(Debug, Args)]
pub struct PromptArgs {
    /// Prompt name — a file stem under the vault's `.notesmith/prompts/`
    /// (e.g. `daily-note` for `.notesmith/prompts/daily-note.md`).
    pub name: String,
    /// Target date (`YYYY-MM-DD`) substituted for `{{ today }}` in the
    /// template. Defaults to today.
    #[arg(long)]
    pub date: Option<String>,
    #[command(flatten)]
    pub agent: AgentOpts,
}

/// What a `summarize` run should target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummarizeTarget {
    /// Today's daily note (resolved by the agent via MCP).
    Today,
    /// A specific vault-relative note path.
    Note(String),
}

impl SummarizeTarget {
    /// Interpret the raw positional argument: the literal `today` (any case)
    /// selects the daily note, anything else is treated as a note path.
    pub fn parse(raw: &str) -> Self {
        if raw.trim().eq_ignore_ascii_case("today") {
            Self::Today
        } else {
            Self::Note(raw.trim().to_string())
        }
    }
}

/// A non-interactive [`PermissionDecider`] for headless runs.
///
/// With `allow_writes == false` (the default) every permission request is
/// denied, so an unattended agent can never mutate the vault. With
/// `allow_writes == true` the operator has explicitly opted in and requests are
/// approved for this run only (nothing is persisted).
struct HeadlessDecider {
    allow_writes: bool,
}

impl PermissionDecider for HeadlessDecider {
    fn decide(&self, _request: PermissionRequest) -> BoxFuture<'static, PermissionDecision> {
        let allow = self.allow_writes;
        Box::pin(async move {
            if allow {
                PermissionDecision::AllowOnce
            } else {
                PermissionDecision::Deny
            }
        })
    }
}

impl AiCommand {
    pub async fn run(
        &self,
        global_config: &GlobalConfig,
        explicit_vault: Option<&str>,
        cwd: &Path,
        format: OutputFormat,
    ) -> anyhow::Result<()> {
        match self {
            AiCommand::Summarize(args) => {
                let target = SummarizeTarget::parse(&args.target);
                let prompt = summarize_prompt(&target);
                let output =
                    drive_agent_turn(global_config, explicit_vault, cwd, &args.agent, &prompt)
                        .await?;
                emit(&output, "summary", format)
            }
            AiCommand::WeeklyDigest(args) => {
                let (start, end) = current_week(Local::now().date_naive());
                let prompt = weekly_digest_prompt(start, end);
                let output =
                    drive_agent_turn(global_config, explicit_vault, cwd, &args.agent, &prompt)
                        .await?;
                emit(&output, "digest", format)
            }
            AiCommand::Prompt(args) => {
                let prompt = fetch_rendered_prompt(
                    global_config,
                    explicit_vault,
                    cwd,
                    &args.name,
                    args.date.as_deref(),
                )
                .await?;
                let output =
                    drive_agent_turn(global_config, explicit_vault, cwd, &args.agent, &prompt)
                        .await?;
                emit(&output, "output", format)
            }
        }
    }
}

/// Fetch the daemon-rendered prompt for `name` (the daemon executes the
/// template's `context_queries` against the vault index). Any failure —
/// unknown prompt, bad SQL, unreachable daemon — is an error, so a job
/// running this command records a failed run rather than driving the agent
/// with a half-rendered instruction.
async fn fetch_rendered_prompt(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    name: &str,
    date: Option<&str>,
) -> anyhow::Result<String> {
    crate::daemon_client::ensure_daemon(global_config).await?;
    let detected = detect_vault(cwd, explicit_vault, global_config)?;

    let mut url = crate::daemon_client::daemon_url(global_config)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("daemon URL cannot be a base"))?
        .push("api")
        .push("v")
        .push(&detected.name)
        .push("agent-prompts")
        .push(name);
    if let Some(date) = date {
        url.query_pairs_mut().append_pair("date", date);
    }

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                anyhow::anyhow!(
                    "could not reach the Notesmith daemon at {}",
                    global_config.daemon.bind
                )
            } else {
                anyhow::anyhow!("prompt render request failed: {error}")
            }
        })?;

    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let reason = body["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("could not render prompt {name:?}: {reason}");
    }
    match body["prompt"].as_str() {
        Some(prompt) if !prompt.trim().is_empty() => Ok(prompt.to_string()),
        _ => anyhow::bail!("prompt {name:?} rendered empty; refusing to drive the agent with it"),
    }
}

/// Build the instruction sent to the agent for a `summarize` run. The agent
/// fetches the note content itself through the read-only MCP tools.
fn summarize_prompt(target: &SummarizeTarget) -> String {
    match target {
        SummarizeTarget::Today => "Use the Notesmith MCP tools to fetch today's daily note from \
             the vault. Produce a concise summary of its content: the key points, decisions, and \
             any open tasks. Respond with the summary text only — no preamble."
            .to_string(),
        SummarizeTarget::Note(path) => format!(
            "Use the Notesmith MCP tools to read the note at the vault-relative path `{path}`. \
             Produce a concise summary of its content: the key points, decisions, and any open \
             tasks. Respond with the summary text only — no preamble."
        ),
    }
}

/// Build the instruction sent to the agent for a `weekly-digest` run covering
/// the inclusive `start..=end` date range.
fn weekly_digest_prompt(start: NaiveDate, end: NaiveDate) -> String {
    format!(
        "Use the Notesmith MCP tools (search and periodic-note tools) to gather all notes from \
         the week of {start} to {end} inclusive. Produce a digest that highlights what happened: \
         notable activity, decisions made, and tasks still open or completed. Group related items \
         and keep it concise. Respond with the digest text only — no preamble.",
        start = start.format("%Y-%m-%d"),
        end = end.format("%Y-%m-%d"),
    )
}

/// The inclusive Monday..=Sunday range of the ISO week containing `today`.
fn current_week(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let from_monday = today.weekday().num_days_from_monday() as i64;
    let start = today - Duration::days(from_monday);
    let end = start + Duration::days(6);
    (start, end)
}

/// The vault's MCP bindings for a headless session: the daemon's HTTP endpoint
/// as the primary, and the stdio `notesmith mcp start` bridge as the fallback
/// for agents that do not advertise HTTP MCP support. The ACP driver picks by
/// the agent's `mcpCapabilities` — GitHub Copilot's ACP mode rejects stdio MCP
/// servers supplied by the client, so the HTTP endpoint must be the preferred
/// binding, exactly as in the desktop app's `agent_bridge` (ADR 0012,
/// 2026-09-02 amendment). The
/// bridge subprocess can only reach the local daemon, so it is omitted when
/// `local_daemon` is false (a remote `--url` / `NOTESMITH_URL` target).
fn vault_mcp_bindings(
    daemon_url: &str,
    vault: &str,
    read_only: bool,
    local_daemon: bool,
    notesmith_bin: String,
    run_id: Option<&str>,
) -> (McpBinding, Option<McpBinding>) {
    // A write-tracked agent-job run (`NOTESMITH_RUN_ID` set) stamps its run id
    // on the daemon HTTP vault binding so the daemon can attribute this run's
    // writes and record `no_writes` when it wrote nothing (job success
    // criteria, ADR 0025 amendment 2026-09-04). Scope A to the HTTP-bound
    // session: the briefing uses HTTP on every supported agent, and the stdio
    // bridge is a separate process that would not carry the header, so the
    // fallback deliberately goes untagged (see the plan's stdio-fallback risk).
    let http = match run_id {
        Some(id) => McpBinding::daemon_http_with_headers(
            daemon_url,
            vault,
            read_only,
            vec![(
                notesmith_agent::RUN_ID_HEADER.to_string(),
                id.to_string(),
            )],
        ),
        None => McpBinding::daemon_http(daemon_url, vault, read_only),
    };
    let fallback = local_daemon.then(|| McpBinding::local_bridge(notesmith_bin, vault, read_only));
    (http, fallback)
}

/// Drive a single headless agent turn: ensure the daemon is up, resolve the
/// vault, wire the MCP bindings with the appropriate scope, send `prompt`, and
/// accumulate the agent's reply until the turn completes.
async fn drive_agent_turn(
    global_config: &GlobalConfig,
    explicit_vault: Option<&str>,
    cwd: &Path,
    opts: &AgentOpts,
    prompt: &str,
) -> anyhow::Result<String> {
    // Both vault MCP transports front the daemon, so it must be running; keep
    // its base URL for the HTTP binding.
    let daemon_url = crate::daemon_client::ensure_daemon(global_config).await?;
    let detected = detect_vault(cwd, explicit_vault, global_config)?;

    let descriptor = notesmith_agent::descriptor(&opts.agent).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown agent `{}` (expected one of: copilot, claude, codex, gemini, opencode)",
            opts.agent
        )
    })?;

    let read_only = !opts.allow_writes;
    let notesmith_bin = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "notesmith".to_string());
    // Set by the daemon's job runner for a write-tracked agent-job run; tags
    // this session's vault writes so the daemon can record `no_writes` when the
    // run wrote nothing (job success criteria, ADR 0025 amendment 2026-09-04).
    let run_id = std::env::var("NOTESMITH_RUN_ID")
        .ok()
        .filter(|id| !id.trim().is_empty());
    let (http, stdio_fallback) = vault_mcp_bindings(
        daemon_url.as_str(),
        &detected.name,
        read_only,
        !crate::daemon_client::has_remote_override(),
        notesmith_bin,
        run_id.as_deref(),
    );
    // Enabled external `[[mcp.servers]]` from the global config ride alongside
    // the vault bridge, as in the desktop app (#283). The vault read-only flag
    // is deliberately not applied to them: it governs vault writes, while an
    // external server is a third-party endpoint whose own credentials/scopes
    // decide what the agent may do there. Companion memory stays desktop-only.
    let extra = notesmith_agent::extra_mcp_bindings(&global_config.mcp);
    let decider: Arc<dyn PermissionDecider> = Arc::new(HeadlessDecider {
        allow_writes: opts.allow_writes,
    });

    let mut session = descriptor
        .session(opts.agent_bin.as_deref())
        .in_dir(Some(detected.root.clone()))
        .with_mcp(http)
        .with_extra_mcp(extra)
        .read_only(read_only)
        .with_permission_decider(decider);
    if let Some(bridge) = stdio_fallback {
        session = session.with_mcp_stdio_fallback(bridge);
    }

    session.start().await?;
    session.send(prompt).await?;

    let mut accumulated = String::new();
    while let Some(event) = session.next_event().await {
        match event {
            AgentEvent::AgentMessageDelta { text } => accumulated.push_str(&text),
            AgentEvent::Done { result } => {
                return Ok(result.unwrap_or(accumulated));
            }
            AgentEvent::Error { message } => {
                anyhow::bail!("agent error: {message}");
            }
            // Tool calls/results and status updates are not part of the captured
            // headless output.
            _ => {}
        }
    }

    Ok(accumulated)
}

/// Print captured agent output, wrapping it under `key` for JSON.
fn emit(output: &str, key: &str, format: OutputFormat) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            let value = serde_json::json!({ key: output });
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        OutputFormat::Text => println!("{output}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: AiCommand,
    }

    fn parse(args: &[&str]) -> AiCommand {
        TestCli::try_parse_from(args).expect("args parse").command
    }

    #[test]
    fn vault_bindings_prefer_the_daemon_http_endpoint_with_a_stdio_fallback() {
        let (http, fallback) = vault_mcp_bindings(
            "http://127.0.0.1:27183/",
            "work",
            false,
            true,
            "notesmith".to_string(),
            None,
        );
        match &http {
            McpBinding::Http { url, .. } => assert_eq!(url, "http://127.0.0.1:27183/mcp/work"),
            other => panic!("expected an http primary binding, got {other:?}"),
        }
        assert!(!http.read_only());
        let bridge = fallback.expect("local daemon gets a stdio fallback");
        assert!(matches!(bridge, McpBinding::Stdio { .. }));
        // Same server name on both transports — the agent sees one vault server
        // whichever binding the ACP handshake selects.
        assert_eq!(http.name(), bridge.name());
    }

    #[test]
    fn vault_bindings_use_the_read_only_scope_by_default_semantics() {
        let (http, fallback) = vault_mcp_bindings(
            "http://127.0.0.1:27183",
            "work",
            true,
            true,
            "notesmith".to_string(),
            None,
        );
        match &http {
            McpBinding::Http { url, .. } => assert_eq!(url, "http://127.0.0.1:27183/mcp-ro/work"),
            other => panic!("expected an http primary binding, got {other:?}"),
        }
        assert!(http.read_only());
        assert!(fallback.expect("stdio fallback").read_only());
    }

    #[test]
    fn vault_bindings_skip_the_stdio_fallback_for_a_remote_daemon() {
        let (http, fallback) = vault_mcp_bindings(
            "https://notes.example.com/notesmith/",
            "work",
            false,
            false,
            "notesmith".to_string(),
            None,
        );
        match &http {
            McpBinding::Http { url, .. } => {
                assert_eq!(url, "https://notes.example.com/notesmith/mcp/work");
            }
            other => panic!("expected an http primary binding, got {other:?}"),
        }
        // The stdio bridge only reaches the local daemon; a remote --url /
        // NOTESMITH_URL session must not advertise it.
        assert!(fallback.is_none());
    }

    #[test]
    fn vault_bindings_stamp_the_run_id_header_when_set() {
        let (http, fallback) = vault_mcp_bindings(
            "http://127.0.0.1:27183",
            "work",
            false,
            true,
            "notesmith".to_string(),
            Some("run-xyz"),
        );
        match &http {
            McpBinding::Http { url, headers, .. } => {
                assert_eq!(url, "http://127.0.0.1:27183/mcp/work");
                assert_eq!(
                    headers,
                    &[(
                        notesmith_agent::RUN_ID_HEADER.to_string(),
                        "run-xyz".to_string()
                    )]
                );
            }
            other => panic!("expected an http primary binding, got {other:?}"),
        }
        // Scope A to HTTP: the stdio fallback deliberately carries no header.
        match fallback.expect("local daemon gets a stdio fallback") {
            McpBinding::Stdio { .. } => {}
            other => panic!("expected a stdio fallback, got {other:?}"),
        }
    }

    #[test]
    fn vault_bindings_carry_no_header_without_a_run_id() {
        let (http, _fallback) = vault_mcp_bindings(
            "http://127.0.0.1:27183",
            "work",
            false,
            true,
            "notesmith".to_string(),
            None,
        );
        match &http {
            McpBinding::Http { headers, .. } => assert!(headers.is_empty()),
            other => panic!("expected an http primary binding, got {other:?}"),
        }
    }

    #[test]
    fn parses_summarize_with_a_note_path() {
        let command = parse(&["notes-ai", "summarize", "Projects/foo.md"]);
        match command {
            AiCommand::Summarize(args) => {
                assert_eq!(args.target, "Projects/foo.md");
                assert_eq!(args.agent.agent, "copilot");
                assert!(!args.agent.allow_writes);
                assert_eq!(
                    SummarizeTarget::parse(&args.target),
                    SummarizeTarget::Note("Projects/foo.md".to_string())
                );
            }
            other => panic!("expected summarize, got {other:?}"),
        }
    }

    #[test]
    fn parses_summarize_today() {
        let command = parse(&["notes-ai", "summarize", "today"]);
        match command {
            AiCommand::Summarize(args) => {
                assert_eq!(SummarizeTarget::parse(&args.target), SummarizeTarget::Today);
            }
            other => panic!("expected summarize, got {other:?}"),
        }
    }

    #[test]
    fn parses_weekly_digest() {
        let command = parse(&["notes-ai", "weekly-digest"]);
        assert!(matches!(command, AiCommand::WeeklyDigest(_)));
    }

    #[test]
    fn parses_prompt_with_defaults() {
        let command = parse(&["notes-ai", "prompt", "daily-note"]);
        match command {
            AiCommand::Prompt(args) => {
                assert_eq!(args.name, "daily-note");
                assert_eq!(args.date, None);
                assert_eq!(args.agent.agent, "copilot");
                assert!(!args.agent.allow_writes);
            }
            other => panic!("expected prompt, got {other:?}"),
        }
    }

    #[test]
    fn parses_prompt_with_date_agent_and_writes() {
        let command = parse(&[
            "notes-ai",
            "prompt",
            "weekly-review",
            "--date",
            "2026-08-05",
            "--agent",
            "claude",
            "--allow-writes",
        ]);
        match command {
            AiCommand::Prompt(args) => {
                assert_eq!(args.name, "weekly-review");
                assert_eq!(args.date.as_deref(), Some("2026-08-05"));
                assert_eq!(args.agent.agent, "claude");
                assert!(args.agent.allow_writes);
            }
            other => panic!("expected prompt, got {other:?}"),
        }
    }

    #[test]
    fn parses_agent_and_allow_writes_flags() {
        let command = parse(&[
            "notes-ai",
            "summarize",
            "today",
            "--agent",
            "claude",
            "--agent-bin",
            "/opt/claude",
            "--allow-writes",
        ]);
        match command {
            AiCommand::Summarize(args) => {
                assert_eq!(args.agent.agent, "claude");
                assert_eq!(args.agent.agent_bin.as_deref(), Some("/opt/claude"));
                assert!(args.agent.allow_writes);
            }
            other => panic!("expected summarize, got {other:?}"),
        }
    }

    #[test]
    fn summarize_target_parse_is_case_insensitive_and_trims() {
        assert_eq!(SummarizeTarget::parse("  Today "), SummarizeTarget::Today);
        assert_eq!(SummarizeTarget::parse("TODAY"), SummarizeTarget::Today);
        assert_eq!(
            SummarizeTarget::parse("Daily/2026-06-16.md"),
            SummarizeTarget::Note("Daily/2026-06-16.md".to_string())
        );
    }

    #[test]
    fn summarize_prompt_mentions_today() {
        let prompt = summarize_prompt(&SummarizeTarget::Today);
        assert!(prompt.contains("today's daily note"));
        assert!(prompt.contains("MCP"));
        assert!(prompt.to_lowercase().contains("summary"));
    }

    #[test]
    fn summarize_prompt_includes_the_note_path() {
        let prompt = summarize_prompt(&SummarizeTarget::Note("Area/topic.md".to_string()));
        assert!(prompt.contains("Area/topic.md"));
        assert!(prompt.contains("MCP"));
    }

    #[test]
    fn weekly_digest_prompt_includes_the_date_range() {
        let start = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
        let prompt = weekly_digest_prompt(start, end);
        assert!(prompt.contains("2026-06-15"));
        assert!(prompt.contains("2026-06-21"));
        assert!(prompt.to_lowercase().contains("digest"));
        assert!(prompt.contains("MCP"));
    }

    #[test]
    fn current_week_spans_monday_to_sunday() {
        // 2026-06-16 is a Tuesday.
        let tuesday = NaiveDate::from_ymd_opt(2026, 6, 16).unwrap();
        let (start, end) = current_week(tuesday);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
        assert_eq!(start.weekday(), chrono::Weekday::Mon);
        assert_eq!(end.weekday(), chrono::Weekday::Sun);
    }

    #[tokio::test]
    async fn headless_decider_denies_writes_by_default() {
        let decider = HeadlessDecider {
            allow_writes: false,
        };
        let decision = decider
            .decide(PermissionRequest::new(
                "create_note",
                Some("edit".to_string()),
            ))
            .await;
        assert_eq!(decision, PermissionDecision::Deny);
    }

    #[tokio::test]
    async fn headless_decider_allows_only_when_opted_in() {
        let decider = HeadlessDecider { allow_writes: true };
        let decision = decider
            .decide(PermissionRequest::new(
                "create_note",
                Some("edit".to_string()),
            ))
            .await;
        assert_eq!(decision, PermissionDecision::AllowOnce);
    }
}
