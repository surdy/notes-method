# ADR 0011 — Embedded Agent Chat (Desktop-Only Runner, MCP for Server)

## Status

Proposed (2026-06-11). Builds on [ADR 0010](0010-agent-access-architecture.md).
Not yet implemented; phasing below.

## Context

We want to drive coding/agent CLIs — **Copilot CLI**, **Claude Code**, and
**Codex** — as a **chat interface inside the Notesmith UI**, so a user can
converse with an agent that operates on the vault they are looking at.

There are two separable concerns:

1. **Agent → vault.** Already solved by [ADR 0010](0010-agent-access-architecture.md):
   the daemon hosts per-vault MCP endpoints (`/mcp/<vault>`, `/mcp-ro/<vault>`)
   and a `notesmith mcp start` stdio↔HTTP bridge. All three CLIs are MCP
   clients, so they can already read/write a vault, and the daemon's SSE index
   events (`/api/v/<vault>/events`) surface those edits live in the UI.
2. **Agent → chat UI inside Notesmith.** New work: spawning and managing each
   agent process, normalizing its streaming I/O into a chat panel, and a
   transport to the frontend.

These CLIs are **stdio/PTY processes** that carry their **own model
credentials** (the user's Copilot/Anthropic/OpenAI auth) and their own config.
They are not libraries to embed; they are subprocesses to manage.

The decisive fork is **where the agent process runs**:

- **Desktop (Tauri):** can spawn a local PTY/subprocess, reuses the user's
  local CLI credentials, needs no new network auth surface, but works only in
  the desktop app.
- **Server-side (daemon host):** would work in the hosted browser UI, but
  requires the CLIs and their API keys on the server and exposes an
  agent-runner over the **unauthenticated** daemon port — unacceptable under the
  current trust model (auth deferred; [ADR 0010](0010-agent-access-architecture.md)
  Phase 5).

The deployment reality (see ADR 0010) is a personal homelab reached over
LAN/VPN, with auth deferred.

## Decision

1. **Embedded agent chat is desktop-only.** The Tauri shell spawns the selected
   agent CLI as a local PTY/subprocess, using the user's existing local CLI
   credentials, and streams the conversation to the Svelte chat panel over
   **Tauri IPC**. The HTTP daemon is **not** an agent runner.
2. **Server/hosted agent access is MCP-only.** There is **no** server-side chat
   runner and **no** embedded chat in the hosted browser UI. Agents that want to
   operate on a server-hosted vault connect to its `/mcp/<vault>` (or
   `/mcp-ro/<vault>`) endpoint remotely, exactly as in ADR 0010.
3. **Agents are normalized behind an `AgentSession` abstraction.** A new
   `notesmith-agent` crate defines a transport-agnostic `AgentSession` trait and
   a normalized event stream (`UserMessage`, `AgentMessageDelta`,
   `ToolCall`, `ToolResult`, `Status`, `Done`/`Error`). One thin adapter per CLI
   maps that tool's streaming format onto the common events. Where a tool speaks
   the **Agent Client Protocol (ACP)** (e.g. Claude Code), the adapter targets
   ACP; otherwise it uses the CLI's stream-json / PTY output. The frontend is a
   single generic chat renderer regardless of which agent is selected.
4. **Spawned agents are auto-wired to Notesmith MCP for the active vault.** The
   runner launches the agent pre-configured to use the local daemon's
   `/mcp/<vault>` (read-write) or `/mcp-ro/<vault>` (read-only) endpoint for the
   **currently active vault**, with a **read-only default** and a read-only /
   read-write toggle in the chat panel. This reuses the ADR 0010 surfaces and
   closes the loop: the agent edits the vault you are viewing and SSE refreshes
   the tree/editor live.
5. **Notesmith does not manage model credentials.** Each CLI keeps its own auth
   and configuration; Notesmith only records which agent binaries are available
   and points them at the right vault/MCP/working directory.
6. **Transport is Tauri IPC — no WebSocket/network channel is introduced.**
   Because the runner is desktop-local, the existing daemon SSE channel is
   sufficient for vault updates and no bidirectional network transport is
   needed.

## Consequences

- ✅ **No new auth surface.** Agent execution and credentials stay on the user's
  machine; nothing agent-related is exposed on the unauthenticated daemon port.
- ✅ **Reuses ADR 0010 wholesale.** The agent→vault path, read-only endpoint, and
  live SSE updates already exist; this ADR only adds the desktop runner + UI.
- ⚠️ **No embedded chat in the hosted browser UI.** Hosted users converse with
  agents through their own external CLI/MCP client pointed at the server, not
  inside the Notesmith web UI. This is an accepted limitation, revisitable once
  auth (ADR 0010 Phase 5) lands and a server-side runner can be considered.
- ⚠️ **Read-only is the safe default but not a security boundary** on a shared
  machine — it guards against agent mistakes, consistent with ADR 0010.
- **Resilience policy applies.** Agent stdout, tool I/O, and any vault content
  the agent surfaces are untrusted; per [ADR 0009](0009-resilience-to-malformed-content.md)
  a malformed stream or note must degrade gracefully, never panic the desktop
  shell.

## Suggested phasing

- **A — `notesmith-agent` crate.** Define the `AgentSession` trait + normalized
  event model and the first adapter. Add a headless `notesmith agent run`
  command so the adapter is exercised under TDD without any UI. Pure logic;
  no Tauri dependency.
- **B — Desktop runner + chat panel.** Tauri spawns the agent (PTY/subprocess),
  bridges the normalized event stream over IPC, and a generic Svelte chat panel
  renders it in the right-rail "Context" area (message stream, collapsible
  tool-call cards, agent picker).
- **C — MCP auto-wiring + scope toggle.** Launch the agent pre-configured with
  Notesmith MCP for the active vault; add the read-only (default) / read-write
  toggle and an "operating on `<vault>` · read-only" badge.
- **D — Additional adapters.** Add the remaining CLI adapters (the other two of
  Copilot CLI / Claude Code / Codex).

## Alternatives considered

- **Server-side agent runner (chat in the hosted browser UI).** Rejected for
  now: it requires the agent CLIs and their API keys on the server and an
  agent-runner exposed over the unauthenticated daemon port. Deferred behind
  ADR 0010 Phase 5 auth; until then, hosted agent access is MCP-only.
- **Embed an LLM/agent loop directly in Notesmith.** Rejected: it would force
  Notesmith to manage model credentials, provider APIs, and a tool-calling loop
  that these mature CLIs already implement well. Orchestrating existing agents
  is far less surface area.
- **Bespoke per-CLI integration with no normalization.** Rejected: it would
  require N chat renderers and couple the UI to each tool's wire format. The
  `AgentSession` abstraction keeps one renderer.
- **WebSocket transport for the chat stream.** Unnecessary for a desktop-local
  runner; Tauri IPC carries the stream. A network transport would only matter
  for a server-side runner, which is out of scope here.

## References

- [ADR 0010 — Agent Access Architecture](0010-agent-access-architecture.md)
  (daemon-hosted MCP, `Ops` layer, read-only endpoint — the agent→vault half).
- [ADR 0007 — SvelteKit + Tauri](0007-sveltekit-tauri.md) (the desktop shell
  that hosts the runner).
- [ADR 0009 — Resilience to malformed content](0009-resilience-to-malformed-content.md).
- [ADR 0006 — Crate per domain](0006-crate-per-domain.md) (rationale for the new
  `notesmith-agent` crate).
- Agent Client Protocol (ACP) — the editor↔agent protocol the adapters target
  where available.
