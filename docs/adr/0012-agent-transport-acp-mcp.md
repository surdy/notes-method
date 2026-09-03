# ADR 0012 — Agent Transport: ACP Client + stdio/HTTP MCP

## Status

Accepted (2026-06-13). Builds on [ADR 0010](0010-agent-access-architecture.md)
and **supersedes the transport and MCP-wiring decisions of
[ADR 0011](0011-embedded-agent-chat.md)** — specifically Decision 3 (transport
abstraction), Decision 4 (active-vault MCP auto-wiring), and the entire Phase E
amendment. ADR 0011 Decisions 1, 2, 5, and 6 (desktop-only runner,
hosted = MCP-only, Notesmith does not manage model credentials, Tauri IPC) still
stand, refined where noted below.

The ADR 0011 Phase A–E implementation was **reverted during the 2026-06 agent
reset**; this ADR is the basis for the rebuild, not an incremental change to
existing code.

## Context

Multi-agent support — **Copilot CLI, Claude Code, and Codex** — is a
**non-negotiable** product requirement. The Agent Client Protocol (ACP) is the
only transport all three speak (Copilot natively; Claude/Codex via Zed adapter
binaries), so ACP is locked in.

The first agent build (ADR 0011) failed in the field for a cluster of reasons we
must not reintroduce:

- **HTTP MCP fragility.** Wiring the agent to a daemon **URL** meant: the URL had
  to be discovered and embedded in agent config; per-vault endpoints were only
  mounted at **daemon startup** (vaults added later had no endpoint until a
  restart); and Copilot's strict client rejects MCP `structuredContent` that is
  not a JSON **object** (`"expected record, received array"`), which our lenient
  test client accepted — so tests passed while the real agent failed.
- **Stale sidecar binaries** masked fixes.
- **Hand-rolled JSON-RPC** for ACP was a maintenance burden.

Two facts reshape the design:

1. The **CLI is already a remote-capable daemon HTTP client** (`--url` /
   `NOTESMITH_URL` → `daemon_client.rs`). "Remote vault" simply means "a daemon
   hosting that vault, reachable over HTTP." So both CLI and any MCP server are
   thin clients of the same daemon → same [`Ops`](0010-agent-access-architecture.md).
2. The official **Zed `agent-client-protocol` crate (v0.14)** provides the ACP
   `Client` trait (`request_permission`, `read_text_file`/`write_text_file`,
   `session_notification`, `create_terminal`/`terminal_output`, `ext_*`),
   removing the need to hand-roll the protocol.

The lesson from `obsidian-copilot` (in-process tools over `app.vault`, context
injected into the prompt) is portable: route **everything through `Ops`**, and
**inject context** rather than expose the editor as a tool.

## Decision

### Transport & topology

1. **ACP via the Zed `agent-client-protocol` crate (v0.14).** We implement the
   ACP **`Client`** trait; external binaries are the agents. The hand-rolled
   JSON-RPC transport is retired. (Supersedes ADR 0011 Decision 3 + Phase E
   transport.)
2. **MCP transport is chosen per agent from its declared `mcpCapabilities`.**
   The **HTTP** Streamable endpoint is preferred whenever the agent advertises
   HTTP MCP support — it works for both the local and remote daemon, and some
   agents (notably **GitHub Copilot**, whose ACP client supports *only* HTTP/SSE
   and silently ignores stdio servers) can use **nothing else**. A **stdio**
   bridge to the local daemon is supplied only as a **fallback** for a local
   daemon when the agent does *not* advertise HTTP MCP. In every case the
   **daemon remains the single owner** of the SQLite + Tantivy index (no second
   in-process index writer). (Supersedes ADR 0011 Decision 4; refines the
   original "stdio-local / HTTP-remote" split, which broke Copilot.)
3. **The local stdio bridge is a thin stdio↔HTTP forwarder** to the daemon's one
   MCP server — the *same* server remote uses — so there is a single MCP
   code/test surface. This bridge **already exists** from
   [ADR 0010](0010-agent-access-architecture.md) Phase 3: `notesmith mcp start
   [--read-only]` resolves the vault + daemon endpoint (local or remote via
   `NOTESMITH_URL`) and forwards over `notesmith_mcp::run_stdio_bridge`. It is
   reused as-is; the ACP `mcpServers` stdio entry launches it. The
   **startup-only endpoint-mounting bug is fixed**:
   per-vault `/mcp/<vault>` and `/mcp-ro/<vault>` routes are mounted
   **dynamically** when a vault is added, not only at daemon startup.
4. **The runner stays desktop-local.** The Tauri shell hosts the ACP `Client`
   impl, spawns the agent process, and streams the normalized event stream to
   the Svelte chat panel over **Tauri IPC**. (Keeps ADR 0011 Decisions 1 & 6.)

### Scope, permissions & capabilities

5. **Read-write by default, gated by per-write permission prompts.** The default
   session uses the read-write MCP server, but **every write tool is gated by an
   ACP `session/request_permission`** prompt offering three allow tiers plus
   deny: **Allow Once** (allow this single call, remember nothing), **Allow This
   Session** (allow + suppress re-prompts for this tool for the rest of the chat
   session, nothing persisted), and **Always Allow** (allow + remember in the
   session **and** persist the grant so a future session — even after a
   daemon/app restart — is pre-seeded and never re-prompts). The prompt also
   carries a **diff/preview** of the proposed change (extracted from the ACP
   `ToolCallContent::Diff`, bounded to ~8 KB per side per ADR 0009) so the user
   reviews the change before any write is applied (issue #189).

   **Persisted grants are daemon-owned and frontend-orchestrated.** They live in
   a single durable SQLite store (`agent-permissions.sqlite`) in the daemon data
   dir, scoped per-vault and keyed by `(vault, tool)`, exposed over the per-vault
   REST surface (`GET/POST /api/v/{vault}/agent/permissions`,
   `DELETE …/{tool}`). The Rust agent/ACP layer stays **HTTP-free**: it only
   (a) accepts a list of already-granted tool names to pre-seed the session
   permission state (`AcpSession::with_granted_tools`), and (b) carries the
   proposed diff in the permission request. The Svelte chat store fetches the
   persisted grants at session start (pre-seeding the session) and POSTs a new
   grant when the user picks "Always Allow"; persistence is best-effort, so a
   store failure never blocks the agent (the in-session grant still applies).
6. **A hard read-only mode** swaps the session to the **`/mcp-ro/<vault>`**
   (`ReadOnlyOps`) server, which exposes no write tools at all — a guarantee, not
   a prompt. Because the read-only scope can only ever surface safe reads (the
   daemon rejects writes server-side and no fs-write/terminal capability is
   advertised), the session **allows read-only permission requests silently**
   rather than prompting or denying — otherwise the agent could not read the
   vault at all.
7. **Filesystem and terminal capabilities are OFF by default.** The client
   advertises **neither** `fs/read_text_file`/`fs/write_text_file` **nor**
   `terminal`, so all vault I/O flows through MCP tools over `Ops` (single access
   path, guaranteed indexing/validation/provenance).
8. **An app-level "break-glass" setting** re-enables raw access when explicitly
   turned on. With break-glass ON, the client advertises **fs read/write**
   (**path-scoped to the vault root**; writes still trigger the permission prompt
   and are **blocked in read-only mode**) **and** the **terminal** capability
   (**cwd = vault root**, permission-gated, blocked in read-only mode). Because
   break-glass fs writes bypass `Ops` indexing, the daemon's `notify` file
   watcher reindexes the touched files afterward, so the index self-heals.

### Agents, context & configuration

9. **All three agents are wired**; **Copilot gates milestone 1.** The ACP
   transport is agent-agnostic; only the launch command + availability detection
   differ (`copilot --acp`; `npx @zed-industries/claude-code-acp`; `codex-acp`).
   Claude and Codex run through the identical code path and are verified
   opportunistically once their adapters are installed; a missing adapter yields
   a clean, actionable error.
10. **Editor/UI context is injected, not a tool.** The Tauri app (which owns
    editor state) injects a compact context block — active note path/title,
    current selection, open tabs — into the **prompt at the start of each turn**.
    The MCP server, being a separate process, exposes no `get_active_note` tool.
11. **A session preamble is injected at session start:** the vault's
    `.notesmith/skill.md` (if present) **plus** a compact auto-generated summary
    (vault name, note count, top tags/folders), kept small to control tokens.
12. **In-app model selection via ACP Session Config Options.** The agent
    advertises its models in the `session/new` result `configOptions`
    (`category: "model"`); Notesmith renders a picker from whatever the agent
    advertises (it **hardcodes no model lists**) and sets the choice. Prefer
    `configOptions`; fall back to the deprecated `modes` field; **degrade
    gracefully** (no picker) if an agent advertises neither. Notesmith still does
    **not** manage agent auth — each CLI handles its own login. (Refines ADR 0011
    Decision 5.)
13. **Transcripts persist per-vault in the daemon's local DB** (not as files
    inside the vault, so they neither clutter nor sync). Each vault has its own
    revisitable chat history that survives restarts; ACP child sessions are
    re-established lazily.

### Tool exposure

- **All MCP tools are eager-loaded** (the ~13 `Ops`-backed tools). We do **not**
  design for ACP "tool search" / lazy tool discovery: this is a dedicated notes
  agent where the vault tools are used in nearly every session, the token saving
  is negligible (~1–2% of context), and lazy discovery reintroduces the
  "I don't have any Notesmith tools" false-negative we already hit.

## Consequences

- ✅ **Kills the bug classes that broke the first build.** Routing every agent
  through the daemon's MCP server (HTTP, with the local stdio bridge as a
  fallback) removes the URL-in-config and (with dynamic mounting) the
  startup-only endpoint problem. The strict `structuredContent`-must-be-an-object
  fix is **still required** at the MCP server and is re-applied in Phase 1.
- ✅ **One MCP server, one ACP client.** A single MCP implementation serves local
  and remote; the Zed crate owns the protocol; behavior is uniform across agents.
- ✅ **Remote works** via direct HTTPS MCP, reusing the same `Ops`; agent and
  model selection are agent-advertised, not hardcoded.
- ⚠️ **Remote write-capable MCP is unauthenticated** — it relies entirely on the
  reverse-proxy / VPN / network perimeter (consistent with ADR 0010, auth
  deferred). Misconfiguring the perimeter exposes vault writes.
- ⚠️ **Break-glass widens the blast radius** (raw fs + shell). It is contained by
  being an explicit app-level opt-in, vault-root path scoping, the permission
  prompt, and the read-only-mode block — but it is real and should be off by
  default.
- ⚠️ **Claude/Codex are not verified end-to-end** at milestone 1; only the
  graceful-error path is exercised until their adapters are installed.
- **Resilience policy ([ADR 0009](0009-resilience-to-malformed-content.md))
  applies** to the agent stream, tool I/O, and any vault content the agent
  surfaces: malformed input must degrade, never panic the desktop shell or the
  daemon.

## Suggested phasing

- **1 — MCP backend foundation.** Re-apply the `structuredContent`
  object-wrapping fix; mount per-vault `/mcp/<vault>` + `/mcp-ro/<vault>` routes
  **dynamically** (fix the startup-only bug); integration tests that run the
  exact MCP tool calls against a real indexed vault (including the strict-client
  contract).
- **2 — stdio bridge (mostly done).** The `notesmith mcp start [--read-only]`
  stdio↔HTTP forwarder already exists (ADR 0010 Phase 3,
  `notesmith_mcp::run_stdio_bridge`). Phase 2 reduces to **reusing** it as the
  ACP `mcpServers` stdio command and confirming it passes the strict-client
  contract end-to-end after the Phase 1 fix — no new bridge to build.
- **3 — ACP client on the crate.** Rebuild `notesmith-agent` on
  `agent-client-protocol` v0.14: implement the `Client` trait (permission
  prompts, fs read/write gated by break-glass, `session_notification` →
  normalized events), session lifecycle, the three-agent launch table +
  availability detection. **Copilot verified end-to-end.**
- **4 — MCP wiring + scope.** Pass the active vault as a `session/new`
  `mcpServers` entry (HTTP preferred per agent `mcpCapabilities.http`, local
  stdio bridge as fallback — Decision 2); read-only vs read-write via
  endpoint choice; per-write permission prompts (allow once / allow always
  session-scoped / deny); the app-level break-glass setting (fs + terminal).
- **5 — Context injection.** Per-turn editor context block; session-start
  preamble (`skill.md` + compact vault summary).
- **6 — Model selection.** Render the `configOptions` model picker (fallback to
  `modes`); wire the setter; graceful degrade.
- **7 — Transcript persistence.** Per-vault transcript store in the daemon DB;
  reopen/revisit UI; lazy ACP session re-establishment.
- **8 — Chat UI.** Svelte panel: message stream, tool-call cards, agent picker,
  model picker, read-only/read-write toggle, permission prompt UI, and the
  break-glass toggle in Settings.
- **9 — Claude/Codex end-to-end** (opportunistic, as adapters are installed).

## Alternatives considered

- **HTTP MCP for local only (no stdio fallback).** Rejected: not every agent
  supports HTTP MCP, so a local stdio bridge is retained as a fallback. The
  bridge still forwards to the daemon over HTTP, so it adds no second in-process
  index writer.
- **Embed `LocalOps` directly in the bridge.** Rejected: a second in-process
  index writer alongside the running desktop daemon risks SQLite/Tantivy
  corruption.
- **Keep the hand-rolled ACP JSON-RPC transport.** Rejected: ongoing maintenance
  burden; the Zed crate is the canonical implementation and tracks the spec.
- **CLI-over-terminal + shipped skills as the primary agent surface.** Rejected
  as primary: weaker scoping (grants terminal access), shell-escaping hazards on
  note content, and uneven skill formats across agents. The CLI remains the
  human/scripting/remote-ops surface and a fallback transport; a thin
  `skill.md` is still shipped as the session preamble.
- **ACP "tool search" / lazy MCP tool discovery.** Rejected for our scale (see
  *Tool exposure*).
- **Per-daemon/per-vault bearer-token auth on remote MCP now.** Deferred: stays
  consistent with ADR 0010's deferred-auth posture; the perimeter is the
  boundary until ADR 0010 Phase 5.

## References

- [ADR 0010 — Agent Access Architecture](0010-agent-access-architecture.md)
  (daemon-hosted MCP, the `Ops` layer, the read-only endpoint).
- [ADR 0011 — Embedded Agent Chat](0011-embedded-agent-chat.md) (the
  desktop-only runner and chat UI this ADR rebuilds the transport for).
- [ADR 0009 — Resilience to malformed content](0009-resilience-to-malformed-content.md).
- [ADR 0006 — Crate per domain](0006-crate-per-domain.md) (the `notesmith-agent`
  crate).
- Zed `agent-client-protocol` crate v0.14 — the ACP `Client` implementation.
- [agentclientprotocol.com](https://agentclientprotocol.com) — Session Config
  Options (model selection), MCP servers, filesystem & terminal capabilities.

## Amendment (2026-09-02) — scoping the Copilot stdio claim

Decision 2 describes GitHub Copilot as an ACP client that "supports *only*
HTTP/SSE and silently ignores stdio servers". Real-machine verification
(`plans/work-integrations-post-fix-rerun-handoff.md`) shows that claim is too
broad, and wrong about the failure mode. The empirically correct statement is:

> **Copilot's ACP mode rejects stdio MCP servers supplied by the ACP client.**
> On Copilot CLI `1.0.83-1`, `copilot --acp` logs
> `Rejecting non-http/sse MCP server "<id>" from client` during `session/new`
> — an explicit refusal, not silence, and the server subprocess is never
> spawned. Copilot *does* support stdio MCP servers configured through its own
> paths (its own config file and the Copilot SDK); the Copilot CLI `1.0.25`
> changelog even states that ACP clients may supply stdio servers. Runtime
> behaviour and changelog disagree; treat it as an ACP-mode gap or Copilot bug
> until upstream confirms the intended contract.

**Nothing about the decision changes.** HTTP remains the preferred and primary
transport for the vault binding for the reasons already recorded — it removes a
subprocess, it works unchanged against a remote daemon, and it is the daemon's
native transport. The stdio bridge remains the fallback for agents that do not
advertise HTTP MCP. Only the justification is narrowed: Copilot needs the HTTP
binding because *its ACP client* will not accept a client-supplied stdio
server, not because Copilot lacks stdio MCP support altogether.

The practical consequence for users is in the *external* `[[mcp.servers]]`
surface rather than the vault binding: a stdio-only external server (e.g.
`workiq mcp`) reaches Claude/Codex/Gemini/OpenCode sessions but is rejected in
Copilot sessions. That caveat is documented in
[`docs/ai-mcp-servers.md`](../ai-mcp-servers.md).

Tracked upstream as
[copilot-cli#3889](https://github.com/github/copilot-cli/issues/3889) (open
since 2026-06-23; no maintainer response as of 2026-09-03; independently
reproduced on 1.0.52 through 1.0.83). Root cause per third-party analysis:
Copilot's ACP `initialize` advertises `mcpCapabilities` of http and sse only,
omitting stdio — the one transport the ACP spec marks mandatory — while its
own v1.0.25 changelog claims ACP clients may supply all three. We do not file
or comment upstream; re-test against new Copilot releases when it matters. A
known ecosystem workaround is `--additional-mcp-config=@<file>` at Copilot
spawn time (process-scoped — which matches one-session-per-process headless
runs).
