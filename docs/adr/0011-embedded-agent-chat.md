# ADR 0011 — Embedded Agent Chat (Desktop-Only Runner, MCP for Server)

## Status

Accepted (2026-06-11). Builds on [ADR 0010](0010-agent-access-architecture.md).
Implemented across all phases. The original four phases delivered the
`notesmith-agent` crate + headless `notesmith agent run` (Phase A), the desktop
Tauri runner + Svelte chat panel (Phase B), active-vault MCP auto-wiring with a
read-only/read-write toggle (Phase C, #155), and per-agent line adapters for
Claude Code, Codex, and Copilot CLI (Phase D, #156). **Phase E** then converged
all three agents onto a **single ACP transport** and is **fully implemented**:
E1 (Copilot native ACP, #157), E2 (Claude Code + Codex via adapter binaries,
#158), and E3 (#159) — which **retired the per-agent line adapters** so ACP is
the only transport. See the
[ACP amendment](#amendment-2026--acp-single-transport-convergence-phase-e).

### Transport (single, ACP)

Every agent is driven over the **Agent Client Protocol** (newline-delimited
JSON-RPC 2.0) by one `AcpSession`. A chat session is **multi-turn**: the agent
process is spawned on the first user message and reused across turns
(`session/prompt` against the same `sessionId`). Copilot speaks ACP natively
(`copilot --acp`); Claude Code and Codex run over the same protocol via small
adapter binaries (`npx @zed-industries/claude-code-acp` and `codex-acp`). MCP is
wired via the ACP `session/new` `mcpServers` param and the read-only/read-write
scope maps onto ACP `session/request_permission`. End-to-end behavior depends on
the relevant CLI/adapter being installed and authenticated locally; a missing
binary surfaces as a clean, actionable error (naming the expected binary and
install command) rather than a crash.

> **History:** Phase D originally implemented three distinct line adapters with
> two launch strategies (`Launch::Streaming` for Claude Code's persistent
> `stream-json`; `Launch::OneShot` for Codex `exec --json` and Copilot CLI
> `--prompt`). Phase E3 removed that layer — `claude_code.rs`/`codex.rs`/
> `copilot_cli.rs`, `ProcessAgentSession`/`OneShotProcessSession`, the
> `LineAdapter` trait, and the `Launch`/`PromptDelivery` enums — once all three
> agents were proven on ACP.

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
   `ToolCall`, `ToolResult`, `Status`, `Done`/`Error`). A single `AcpSession`
   drives every agent over the **Agent Client Protocol (ACP)** and maps its
   `session/update` stream onto the common events (see the Phase E amendment;
   earlier phases used one line adapter per CLI, since retired). The frontend is
   a single generic chat renderer regardless of which agent is selected.
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
  no Tauri dependency. **(Implemented.)**
- **B — Desktop runner + chat panel.** Tauri spawns the agent (PTY/subprocess),
  bridges the normalized event stream over IPC, and a generic Svelte chat panel
  renders it in the right-rail "Context" area (message stream, collapsible
  tool-call cards, agent picker). **(Implemented — desktop-only "Agent" tab in
  the right rail; sessions stream over `notesmith://agent-event` /
  `notesmith://agent-ended`.)**
- **C — MCP auto-wiring + scope toggle.** Launch the agent pre-configured with
  Notesmith MCP for the active vault; add the read-only (default) / read-write
  toggle and an "operating on `<vault>` · read-only" badge.
- **D — Additional adapters.** Add the remaining CLI adapters (the other two of
  Copilot CLI / Claude Code / Codex). **(Implemented, #156.)**
- **E — ACP single-transport convergence.** Replace the three per-agent line
  adapters with one **Agent Client Protocol** client (see the amendment below).
  Staged: **E1** Copilot (native ACP) — **implemented (#157)**; **E2** Claude
  Code + Codex via adapter binaries — **implemented (#158)**; **E3** retire the
  line adapters — **implemented (#159)**. **(Phase E complete.)**

## Amendment (2026) — ACP single-transport convergence (Phase E)

The Phase A–D design used **three per-agent line adapters** (Claude stream-json,
Codex `exec --json`, Copilot plain-text) over **two session drivers**
(`ProcessAgentSession`, `OneShotProcessSession`). This carried two costs: each
new agent needed a bespoke adapter, and only Claude Code was multi-turn — Codex
and Copilot were one-shot (one turn per chat session). The
[Agent Client Protocol (ACP)](https://agentclientprotocol.com) — JSON-RPC 2.0
over stdio — has since matured enough to be a single convergence point.

**Decision (amends Decision 3).** Converge all agents onto **one ACP transport**:
a single JSON-RPC 2.0 client (`acp.rs`) plus one persistent `AcpSession` driver
behind the existing `AgentSession` contract, fed by a small **per-agent launch
table**. The normalized `AgentEvent` model, the generic Svelte renderer,
`agent-chat.ts`, the Tauri IPC envelope, and the read-only/read-write toggle all
stay unchanged — only the per-agent transport layer is replaced. We accept that
Claude Code and Codex require an **extra adapter binary** install; Copilot CLI
speaks ACP natively.

**Empirically confirmed against `copilot --acp` v1.0.61 (spike, 2026):**

- **Framing is newline-delimited JSON** (one JSON-RPC message per line), *not*
  LSP `Content-Length` headers.
- **`initialize`** → `{ protocolVersion: 1, clientCapabilities: { fs:
  { readTextFile, writeTextFile }, terminal } }`. The agent replies with
  `agentCapabilities` including **`mcpCapabilities: { http: true, sse: true }`** —
  so Notesmith's HTTP MCP endpoints can be passed directly — plus `authMethods`
  and `agentInfo { name, version }`.
- **`session/new`** → `{ cwd, mcpServers: [...] }` returns `{ sessionId, models,
  modes, configOptions }`. Session modes use the canonical
  `agentclientprotocol.com/protocol/session-modes#{agent,plan,autopilot}` IDs;
  `configOptions` includes an `allow_all` (permissions) select. **`cwd` must be
  an absolute path** — a relative value (e.g. `.`) is rejected with a
  `-32603 Internal error` (`Directory path must be absolute`), so `AcpSession`
  resolves the working directory to an absolute path before `session/new`.
- **`session/prompt`** → `{ sessionId, prompt: [{ type: "text", text }] }`.
  Streaming arrives as `session/update` notifications whose `update.sessionUpdate`
  is e.g. `agent_message_chunk` (carrying `content: { type: "text", text }`),
  `config_option_update`, `available_commands_update`. The prompt request's final
  response carries `{ stopReason: "end_turn" }`.

**Per-agent launch table:**

| Agent | ACP launch | Install | Kind |
| --- | --- | --- | --- |
| **Copilot** | `copilot --acp` (native) | already installed; **E1 — implemented (#157)**, verified end-to-end. | `copilot` |
| **Claude Code** | `npx @zed-industries/claude-code-acp` (adapter binary) | **E2 — implemented (#158)**; graceful "adapter not installed" error + setup docs. Real end-to-end pending the adapter being installed locally. | `claude-code` |
| **Codex** | `codex-acp` (adapter binary; native Codex is app-server/proto) | **E2 — implemented (#158)**; same graceful-error treatment. Real end-to-end pending the adapter. | `codex` |

**MCP wiring moves into `session/new`.** Instead of per-CLI flags
(`--mcp-config` / `-c mcp_servers...` / `--additional-mcp-config`), the active
vault's endpoint is passed once as an ACP `mcpServers` entry pointing at
`/mcp/<vault>` (read-write) or `/mcp-ro/<vault>` (read-only) — one code path for
all agents, leaning on the confirmed `mcpCapabilities.http`.

**Permission model maps onto the RO/RW toggle.** ACP `session/request_permission`
round-trips are answered by the runner from the toggle state: read-only
auto-denies write/destructive tool calls, read-write auto-approves. This replaces
the per-CLI "allow all tools" flags with a uniform, scope-aware gate.

**Multi-turn for free.** A persistent ACP session accepts repeated
`session/prompt` calls against the same `sessionId`, so the single-turn caveat
that affected Codex/Copilot under Phase D disappears.

**Staging — completed.** E1 **(#157)** landed the ACP client + `AcpSession` for
Copilot, coexisting with the line adapters; E2 **(#158)** brought Claude/Codex
onto the same ACP transport via adapter binaries, attaching a setup hint so a
missing adapter yields a clear, actionable error instead of a hang; E3 **(#159)**
retired the now-redundant line-adapter layer — `claude_code.rs` / `codex.rs` /
`copilot_cli.rs`, `ProcessAgentSession` / `OneShotProcessSession`, the
`LineAdapter` trait, the `Launch` / `PromptDelivery` enums, and the
`NotesmithAdapter` / `DriverSession` dispatch enums — collapsing onto the single
`AcpSession`. The agent kinds collapsed accordingly to `claude-code`, `codex`,
and `copilot` (the now-meaningless `-acp` / `-cli` suffixes were dropped, since
ACP is the only transport). The normalized `AgentEvent` model, the generic Svelte
renderer, the Tauri IPC envelope, and the RO/RW toggle are unchanged. The
Claude/Codex adapters are exercised only on the graceful-error path locally; full
end-to-end verification follows once the adapter binaries are installed. Tracked
as issues E1/E2/E3 (label `agent-chat`).

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
