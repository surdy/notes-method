# ADR 0016 — Customization Discovery, MCP Management & `@agent` Routing

## Status

Accepted (2026-06). Implements the model-free **P4** slice of the AI roadmap
([ADR 0015](0015-ai-agent-integration-roadmap.md)): issues **#210** (customization
discovery), **#211** (MCP server management UI), and **#212** (`@agent` routing).
Builds on [ADR 0013](0013-agent-discovery-and-diagnostics.md) (agent registry +
`[agents]` config) and [ADR 0012](0012-agent-transport-acp-mcp.md) (ACP + MCP
transport). Resilience per [ADR 0009](0009-resilience-to-malformed-content.md).

All three issues are **implemented**: #210 via the `notesmith-customization`
crate + `GET /api/v/{vault}/customizations`; #212 via persona session-switch
routing (chat-panel picker + leading `@persona` mention); #211 via the global
`[mcp]` config section (`notesmith-config`), the `mcp_servers_get`/`set` Tauri
commands, per-session `with_extra_mcp` wiring in `agent_bridge.rs`, and the
**Settings → MCP Servers** surface.

## Context

The roadmap's P4 "scale & customization" slice adds three control surfaces that
exploit Notesmith's structural advantages (filesystem-native config, MCP, ACP):

1. **Discover** user-authored *custom agents (personas)*, *skills*, and
   *instructions* and surface them in the chat UI.
2. **Manage** which MCP servers the agent sees (built-in vault tools + external).
3. **Route** a chat session to a discovered persona via `@agent-name`.

Notesmith already uses `.notesmith/` per-vault (skill.md, routing.yaml,
templates/) and `~/.config/notesmith/` globally (config.toml). Custom *prompts*
(#193) live in a vault `_prompts/` folder. Discovery must work whether the daemon
is local or remote, so it is **daemon-side** (served over HTTP), not a desktop-only
filesystem scan.

## Decision

### 1. Discovery directories (#210)

Customizations are markdown files under two scopes, each with three subdirs:

| Scope   | Base                                   | Subdirs                                  |
|---------|----------------------------------------|------------------------------------------|
| Project | `<vault>/.notesmith/`                  | `agents/` · `skills/` · `instructions/`  |
| Global  | `~/.config/notesmith/` (XDG-aware)     | `agents/` · `skills/` · `instructions/`  |

Each item is a single `*.md` file with YAML frontmatter + a markdown body. The
file **stem** is the item `id`; frontmatter `name`/`description` are optional
(name falls back to the stem). Discovery is resilient (ADR 0009): a missing
directory is empty, a malformed/unreadable file is logged (`WARN`) and skipped,
never a panic or a failed request.

**Frontmatter by type:**

- **Agent (persona)** — `name`, `description`, optional `backend` (a discovered
  ACP agent id: `copilot`/`claude`/`codex`/…), optional `model`. Body = the
  system/preamble prompt. A persona is **not** a separate CLI; it runs *on top of*
  one of the already-discovered ACP agent backends (ADR 0013). When `backend` is
  omitted the user's currently-selected agent is used.
- **Skill** — `name`, `description`. Body = reusable instructions the agent can
  load. Complements the single `.notesmith/skill.md` (ADR 0012 P5 preamble).
- **Instruction** — `name`, `description`. Body = always-applied guidance.

### 2. Precedence (#210)

Project overrides global **by id** (filename stem), per type. When an id exists in
only one scope, both are shown; on collision the project file wins and the global
one is hidden. (Mirrors `notesmith-prompts` merge semantics, vault winning.)

### 3. MCP server config scope (#211)

External MCP servers live in **global** config (`~/.config/notesmith/config.toml`)
under an `[[mcp.servers]]` array (id, transport command/url, args, env, enabled),
reusable across vaults. The **built-in daemon vault tools** (`/mcp/<vault>`) are
always shown per-vault and are **not removable** (they are the product). Optional
**per-vault overrides** are a later extension; v1 is global + the implicit
built-in. Config is persisted by the daemon and handed to each agent session as
additional `mcpServers` entries alongside the built-in vault binding.

### 4. `@agent` routing behaviour (#212)

Typing `@agent-name` in the composer **switches the session's active persona**
going forward (a thread = one active agent), rather than per-message routing.
This keeps transcripts coherent and the implementation simple. An autocomplete
picker lists discovered personas as the user types `@`. Switching applies the
persona's `backend`/`model`/preamble to subsequent turns; it does not rewrite
prior turns. (True per-message routing is a possible later extension.)

## Consequences

- **Pros.** Zero-config by default (no dirs required); filesystem-native and
  versionable; personas benefit any ACP backend; one global MCP list avoids
  per-vault duplication; session-switch routing is unambiguous.
- **Cons / risks.** Session-switch (not per-message) means mixed-persona threads
  need an explicit switch each time. Global-only MCP config defers the per-vault
  override request. Personas layer a preamble rather than swapping CLIs, so a
  persona cannot select a backend the user has not installed.
- **Resilience.** All three parsers (agents/skills/instructions, MCP config)
  follow ADR 0009: per-item isolation, warn-and-skip, no panics on file-derived
  content, no `?`-propagation of YAML above the per-item boundary.

## Alternatives considered

- **Custom agents as separate CLI definitions.** Rejected for v1: duplicates the
  `[agents]` custom-command surface (ADR 0013) and is heavier than a persona.
- **Per-message `@agent` routing.** Deferred: complicates transcript modelling and
  the ACP session lifecycle for little near-term value.
- **Per-vault MCP config as the primary store.** Deferred: global is the common
  case; per-vault overrides can layer on later without a breaking change.
- **Desktop-only filesystem discovery.** Rejected: would not work against a remote
  daemon; discovery is daemon-side and served over HTTP.

## References

- [ADR 0015 — AI Agent Integration Roadmap](0015-ai-agent-integration-roadmap.md)
- [ADR 0013 — Agent Discovery & Diagnostics](0013-agent-discovery-and-diagnostics.md)
- [ADR 0012 — Agent Transport: ACP + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- Issues: #210 (discovery), #211 (MCP management), #212 (`@agent` routing).
