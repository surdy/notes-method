//! Per-write permission policy for ACP sessions (ADR 0012, Decision 5).
//!
//! Every write an agent attempts flows through ACP's
//! `session/request_permission` callback. The session resolves each prompt from
//! three inputs, in order:
//!
//! 1. **Read-only scope** — a read-only session allows reads silently (it can
//!    only ever request safe reads) and the user is never prompted; the daemon
//!    rejects any write server-side and no fs-write/terminal capability is
//!    advertised.
//! 2. **Session-scoped "allow always" grants** — once the user picks "allow
//!    always" for a tool, later uses of that same tool are allowed silently for
//!    the rest of the chat session (and only that session — nothing is
//!    persisted).
//! 3. **The user's decision** — anything not resolved by the two rules above is
//!    delegated to a [`PermissionDecider`]; the desktop chat UI implements it
//!    as a real prompt (Phase 8), while tests and headless callers inject a
//!    fixed decision. The default decider denies, so a write can never slip
//!    through unprompted.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

/// A user's decision on a single write-permission prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Allow this one action.
    AllowOnce,
    /// Allow this action and every later use of the same tool for the rest of
    /// the current chat session.
    AllowAlways,
    /// Refuse this action.
    Deny,
}

/// A proposed file change carried in a permission request so the UI can show a
/// diff/preview before the user decides (issue #189). Mirrors the ACP
/// `ToolCallContent::Diff` payload, with both texts bounded to a sane size so a
/// pathological tool call can never hand the UI a megabyte of content (ADR
/// 0009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffPreview {
    /// The file path being modified.
    pub path: String,
    /// The original content (`None` for a new file).
    pub old_text: Option<String>,
    /// The new content after the modification.
    pub new_text: String,
}

/// Context describing the action an agent is asking permission for, handed to
/// the [`PermissionDecider`] so a UI can render a meaningful prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    /// Tool name/title — also the per-tool "allow always" key.
    pub tool: String,
    /// Tool kind (`read`/`edit`/`execute`/…), when the agent supplied one.
    pub kind: Option<String>,
    /// The proposed change to preview before deciding, when the request carries
    /// a diff (issue #189). `None` for non-file actions (e.g. command runs).
    pub diff: Option<DiffPreview>,
}

impl PermissionRequest {
    /// A request with no diff preview — the common shape before issue #189 and
    /// for actions that do not carry a proposed file change.
    pub fn new(tool: impl Into<String>, kind: Option<String>) -> Self {
        Self {
            tool: tool.into(),
            kind,
            diff: None,
        }
    }
}

/// Decides how to answer a write-permission prompt that the session cannot
/// resolve from its own state (i.e. not auto-allowed by read-only and not
/// already granted "allow always" this session).
pub trait PermissionDecider: Send + Sync + 'static {
    /// Resolve the prompt for `request` to a [`PermissionDecision`].
    fn decide(&self, request: PermissionRequest) -> BoxFuture<'static, PermissionDecision>;
}

/// The safe default decider: deny anything that would otherwise require a user
/// prompt. Used until a real prompt UI (Phase 8) is injected.
pub struct DenyAll;

impl PermissionDecider for DenyAll {
    fn decide(&self, _request: PermissionRequest) -> BoxFuture<'static, PermissionDecision> {
        Box::pin(async { PermissionDecision::Deny })
    }
}

/// Session-scoped permission state: the per-tool "allow always" grants made
/// during this chat session. Cloneable and cheap to share into the connection
/// driver; all clones observe the same grants.
#[derive(Clone, Default)]
pub struct PermissionState {
    always: Arc<Mutex<HashSet<String>>>,
}

impl PermissionState {
    /// A fresh state with no grants.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `tool` has a session "allow always" grant.
    pub fn is_always(&self, tool: &str) -> bool {
        self.always
            .lock()
            .map(|grants| grants.contains(tool))
            .unwrap_or(false)
    }

    /// Record a session "allow always" grant for `tool`.
    pub fn remember(&self, tool: &str) {
        if let Ok(mut grants) = self.always.lock() {
            grants.insert(tool.to_string());
        }
    }
}

/// The effect of resolving a permission prompt: whether to allow the action,
/// and whether to remember the grant for the rest of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionResolution {
    /// Allow the action.
    pub allow: bool,
    /// Persist a session "allow always" grant for the tool.
    pub remember: bool,
}

/// Resolve a permission prompt without performing any I/O (the pure policy).
///
/// - read-only sessions allow silently (the `decision` is ignored — the decider
///   is never consulted; a read-only session can only ever request safe reads);
/// - an already-granted "allow always" tool is allowed silently;
/// - otherwise the user's `decision` selects the outcome.
///
/// `decision` is [`None`] exactly when the decider was not consulted (the
/// read-only and already-granted cases).
pub fn resolve_permission(
    read_only: bool,
    already_always: bool,
    decision: Option<PermissionDecision>,
) -> PermissionResolution {
    if read_only {
        // A read-only session only ever exposes read tools: the agent binds the
        // `/mcp-ro/` endpoint, the daemon rejects writes server-side, and no fs
        // write / terminal capability is advertised. Any permission the agent
        // asks for is therefore a safe read — allow it silently rather than
        // denying (which would make the agent unable to read the vault at all).
        return PermissionResolution {
            allow: true,
            remember: false,
        };
    }
    if already_always {
        return PermissionResolution {
            allow: true,
            remember: false,
        };
    }
    match decision {
        Some(PermissionDecision::AllowOnce) => PermissionResolution {
            allow: true,
            remember: false,
        },
        Some(PermissionDecision::AllowAlways) => PermissionResolution {
            allow: true,
            remember: true,
        },
        // No decision (and not otherwise resolved) is treated as a denial.
        Some(PermissionDecision::Deny) | None => PermissionResolution {
            allow: false,
            remember: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_allows_silently_regardless_of_decision() {
        for decision in [
            None,
            Some(PermissionDecision::AllowOnce),
            Some(PermissionDecision::AllowAlways),
            Some(PermissionDecision::Deny),
        ] {
            let resolution = resolve_permission(true, false, decision);
            assert!(resolution.allow, "read-only must allow for {decision:?}");
            assert!(!resolution.remember);
        }
    }

    #[test]
    fn already_granted_tool_is_allowed_without_prompting() {
        let resolution = resolve_permission(false, true, None);
        assert!(resolution.allow);
        assert!(!resolution.remember);
    }

    #[test]
    fn allow_once_allows_without_remembering() {
        let resolution = resolve_permission(false, false, Some(PermissionDecision::AllowOnce));
        assert!(resolution.allow);
        assert!(!resolution.remember);
    }

    #[test]
    fn allow_always_allows_and_remembers() {
        let resolution = resolve_permission(false, false, Some(PermissionDecision::AllowAlways));
        assert!(resolution.allow);
        assert!(resolution.remember);
    }

    #[test]
    fn deny_refuses_and_does_not_remember() {
        let resolution = resolve_permission(false, false, Some(PermissionDecision::Deny));
        assert!(!resolution.allow);
        assert!(!resolution.remember);
    }

    #[test]
    fn missing_decision_in_read_write_is_treated_as_denial() {
        let resolution = resolve_permission(false, false, None);
        assert!(!resolution.allow);
        assert!(!resolution.remember);
    }

    #[test]
    fn state_remembers_grants_across_clones() {
        let state = PermissionState::new();
        assert!(!state.is_always("create_note"));
        let clone = state.clone();
        clone.remember("create_note");
        assert!(state.is_always("create_note"));
        assert!(!state.is_always("delete_note"));
    }
}
