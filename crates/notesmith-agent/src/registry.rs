//! Declarative agent registry (ADR 0013, decision 2).
//!
//! A single source of truth describing every built-in agent: its `id`, display
//! name, an ordered list of launch candidates (native-ACP binary / `npx`
//! package adapter / separate adapter binary), an optional diagnostics-only
//! probe, a setup hint, a docs URL, and an auth hint. The registry is the only
//! place a new built-in agent is added; the desktop bridge consumes it for both
//! availability detection and launch so the two never disagree.
//!
//! Per ADR 0009 the registry is fully static data, so there is nothing here that
//! can panic on malformed input.

use crate::acp::{AcpSession, CLAUDE_ACP_PACKAGE, DEFAULT_CODEX_ACP_BIN, DEFAULT_COPILOT_BIN};

/// One way to launch an agent: a `program` looked up on PATH (or an absolute
/// path) plus the base `args` it is spawned with. The first candidate whose
/// `program` is found on the resolved PATH wins.
pub struct LaunchCandidate {
    /// Program to look up on PATH (or an absolute path).
    pub program: &'static str,
    /// Base arguments passed when launching the candidate.
    pub args: &'static [&'static str],
    /// Optional version-probe arguments used only for on-demand diagnostics
    /// (never spawned on the fast availability path).
    pub probe_args: Option<&'static [&'static str]>,
}

/// A built-in agent description: identity plus its ordered launch candidates and
/// human-facing hints.
pub struct AgentDescriptor {
    /// Stable identifier used by the UI and `build_session` (e.g. `"copilot"`).
    pub id: &'static str,
    /// Human-readable name shown in the picker (e.g. `"GitHub Copilot"`).
    pub display_name: &'static str,
    /// Ordered launch candidates; the first whose `program` resolves wins.
    pub candidates: &'static [LaunchCandidate],
    /// Actionable setup guidance appended to start/handshake failures.
    pub setup_hint: &'static str,
    /// Documentation URL for installing / configuring the agent.
    pub docs_url: &'static str,
    /// Where the agent gets its own auth / billing / model from.
    pub auth_hint: &'static str,
    /// The lowest agent CLI version Notesmith is known to work with, as an
    /// `"x.y.z"`/`"x.y"` string. `None` means no known floor — the safe
    /// default, since inventing version floors we cannot justify would only
    /// produce false "outdated" warnings (issue #192). Diagnostics parse the
    /// `--version` probe and warn only when a detected version is strictly
    /// below this.
    pub min_version: Option<&'static str>,
}

impl AgentDescriptor {
    /// The program used as the cheap availability signal: the first candidate's
    /// `program` (PATH existence test, no process spawn).
    pub fn availability_program(&self) -> &'static str {
        self.candidates
            .first()
            .map(|candidate| candidate.program)
            .unwrap_or(self.id)
    }

    /// Build (but do not start) an [`AcpSession`] from this descriptor.
    ///
    /// Uses the first launch candidate, overriding its `program` with
    /// `bin_override` when a non-empty value is supplied, and applies the
    /// descriptor's setup hint when present.
    pub fn session(&self, bin_override: Option<&str>) -> AcpSession {
        let (program, args): (&str, Vec<String>) = match self.candidates.first() {
            Some(candidate) => (
                bin_override
                    .filter(|bin| !bin.is_empty())
                    .unwrap_or(candidate.program),
                candidate.args.iter().map(|arg| arg.to_string()).collect(),
            ),
            None => (
                bin_override
                    .filter(|bin| !bin.is_empty())
                    .unwrap_or(self.id),
                Vec::new(),
            ),
        };
        let session = AcpSession::new(program, args);
        if self.setup_hint.is_empty() {
            session
        } else {
            session.with_setup_hint(self.setup_hint)
        }
    }
}

static COPILOT_CANDIDATES: &[LaunchCandidate] = &[LaunchCandidate {
    program: DEFAULT_COPILOT_BIN,
    args: &["--acp"],
    probe_args: Some(&["--version"]),
}];

static CLAUDE_CANDIDATES: &[LaunchCandidate] = &[LaunchCandidate {
    program: "npx",
    args: &["--yes", CLAUDE_ACP_PACKAGE],
    probe_args: Some(&["--version"]),
}];

static CODEX_CANDIDATES: &[LaunchCandidate] = &[LaunchCandidate {
    program: DEFAULT_CODEX_ACP_BIN,
    args: &[],
    probe_args: Some(&["--version"]),
}];

static GEMINI_CANDIDATES: &[LaunchCandidate] = &[LaunchCandidate {
    program: "gemini",
    args: &["--experimental-acp"],
    probe_args: Some(&["--version"]),
}];

// The OpenCode CLI exposes its ACP server via the `acp` subcommand
// (`opencode acp`). This is a best-effort default; a user override via the
// `[agents]` config (ADR 0013, decision 4) takes precedence if it is wrong.
static OPENCODE_CANDIDATES: &[LaunchCandidate] = &[LaunchCandidate {
    program: "opencode",
    args: &["acp"],
    probe_args: Some(&["--version"]),
}];

static BUILTIN: &[AgentDescriptor] = &[
    AgentDescriptor {
        id: "copilot",
        display_name: "GitHub Copilot",
        candidates: COPILOT_CANDIDATES,
        setup_hint: "GitHub Copilot CLI speaks ACP natively; install it and run \
                     `copilot --acp` (see https://github.com/github/copilot-cli).",
        docs_url: "https://github.com/github/copilot-cli",
        auth_hint: "Sign in with your GitHub Copilot subscription via the Copilot CLI.",
        min_version: None,
    },
    AgentDescriptor {
        id: "claude",
        display_name: "Claude Code",
        candidates: CLAUDE_CANDIDATES,
        setup_hint: "Claude Code over ACP needs its adapter. Install Node.js and run \
                     `npx --yes @zed-industries/claude-code-acp` once, or set the agent \
                     binary to a `claude-code-acp` executable.",
        docs_url: "https://github.com/zed-industries/claude-code-acp",
        auth_hint: "Authenticate with your Anthropic / Claude account in Claude Code.",
        min_version: None,
    },
    AgentDescriptor {
        id: "codex",
        display_name: "Codex",
        candidates: CODEX_CANDIDATES,
        setup_hint: "Codex over ACP needs the `codex-acp` adapter on your PATH \
                     (see https://github.com/zed-industries/codex-acp).",
        docs_url: "https://github.com/zed-industries/codex-acp",
        auth_hint: "Authenticate with your OpenAI / Codex account in the Codex adapter.",
        min_version: None,
    },
    AgentDescriptor {
        id: "gemini",
        display_name: "Gemini",
        candidates: GEMINI_CANDIDATES,
        setup_hint: "Gemini over ACP needs the Gemini CLI on your PATH; launch it with \
                     `gemini --experimental-acp` (see https://github.com/google-gemini/gemini-cli).",
        docs_url: "https://github.com/google-gemini/gemini-cli",
        auth_hint: "Authenticate with your Google account in the Gemini CLI.",
        min_version: None,
    },
    AgentDescriptor {
        id: "opencode",
        display_name: "OpenCode",
        candidates: OPENCODE_CANDIDATES,
        setup_hint: "OpenCode over ACP needs the OpenCode CLI on your PATH; launch it with \
                     `opencode acp` (see https://github.com/sst/opencode).",
        docs_url: "https://github.com/sst/opencode",
        auth_hint: "Authenticate with your configured provider in the OpenCode CLI.",
        min_version: None,
    },
];

/// The shipped built-in agent registry (ADR 0013): Copilot, Claude, Codex,
/// Gemini, and OpenCode, in picker order.
pub fn builtin_registry() -> &'static [AgentDescriptor] {
    BUILTIN
}

/// Look up a built-in agent descriptor by its stable `id`.
pub fn descriptor(id: &str) -> Option<&'static AgentDescriptor> {
    BUILTIN.iter().find(|descriptor| descriptor.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_the_expected_ids() {
        let ids: Vec<&str> = builtin_registry().iter().map(|d| d.id).collect();
        assert_eq!(
            ids,
            vec!["copilot", "claude", "codex", "gemini", "opencode"]
        );
    }

    #[test]
    fn descriptor_gemini_first_candidate_is_experimental_acp() {
        let gemini = descriptor("gemini").expect("gemini descriptor present");
        let candidate = gemini.candidates.first().expect("gemini has a candidate");
        assert_eq!(candidate.program, "gemini");
        assert_eq!(candidate.args, &["--experimental-acp"]);
    }

    #[test]
    fn descriptor_copilot_first_candidate_is_copilot_acp() {
        let copilot = descriptor("copilot").expect("copilot descriptor present");
        let candidate = copilot.candidates.first().expect("copilot has a candidate");
        assert_eq!(candidate.program, "copilot");
        assert_eq!(candidate.args, &["--acp"]);
    }

    #[test]
    fn descriptor_claude_first_candidate_runs_adapter_via_npx() {
        let claude = descriptor("claude").expect("claude descriptor present");
        let candidate = claude.candidates.first().expect("claude has a candidate");
        assert_eq!(candidate.program, "npx");
        assert_eq!(candidate.args, &["--yes", CLAUDE_ACP_PACKAGE]);
    }

    #[test]
    fn descriptor_returns_none_for_unknown_id() {
        assert!(descriptor("nope").is_none());
    }

    #[test]
    fn availability_program_is_first_candidate_program() {
        assert_eq!(descriptor("claude").unwrap().availability_program(), "npx");
        assert_eq!(
            descriptor("codex").unwrap().availability_program(),
            DEFAULT_CODEX_ACP_BIN
        );
    }

    #[test]
    fn gemini_session_builder_applies_setup_hint() {
        let session = descriptor("gemini").unwrap().session(None);
        assert_eq!(session.program, "gemini");
        assert_eq!(session.args, vec!["--experimental-acp".to_string()]);
        assert!(session.setup_hint.is_some());
        assert!(!session.setup_hint.as_deref().unwrap_or("").is_empty());
    }

    #[test]
    fn session_builder_honors_binary_override() {
        let session = descriptor("gemini").unwrap().session(Some("/opt/gemini"));
        assert_eq!(session.program, "/opt/gemini");
        assert_eq!(session.args, vec!["--experimental-acp".to_string()]);
    }
}
