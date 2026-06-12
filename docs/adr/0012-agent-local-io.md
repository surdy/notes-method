# ADR 0012 — Agent Vault Access: MCP-First, Opt-In Local Filesystem/Terminal

## Status

Accepted (2026-06-12). Refines [ADR 0010](0010-agent-access-architecture.md)
(agent access architecture) and [ADR 0011](0011-embedded-agent-chat.md)
(embedded agent chat over a single ACP transport).

## Context

The embedded agent is driven over ACP (ADR 0011). A subtlety of ACP is that an
agent's **filesystem and terminal tools are proxied by the client**, not run
directly against the OS: the agent issues `fs/read_text_file`,
`fs/write_text_file`, and `terminal/*` requests back to Notesmith, and Notesmith
decides whether to service them. Whether those capabilities exist is advertised
in the `initialize` response's `clientCapabilities`.

Notesmith originally advertised `fs` and `terminal` as **false**, on the
assumption that a local agent "does its own file I/O." That assumption is wrong
for ACP: a local agent (Copilot natively, Claude Code/Codex via adapters) has
**no** independent file or shell access — it relies entirely on the client. With
both capabilities off, any attempt by the agent to list a directory, read a
note, or run a command was answered with JSON-RPC `-32601 method not found`, so
the agent could not inspect the vault even though its MCP tools were wired. In
practice a user asking "tell me about this note" saw the agent try a shell
command (`cd … && ls …`) and fail.

This forced a decision about **how** the embedded agent should reach the vault.
ADR 0010 already establishes that vault access should flow through the
index-aware ops layer (MCP), which respects routing, field conventions, SQL view
contracts, and the read-only scope — rather than raw files that bypass the
index. But a general-purpose coding agent that cannot read a file at all is
broken UX.

## Decision

Adopt a **two-part** approach: MCP-first by default, with opt-in local I/O.

### 1. MCP-steering preamble (always on)

Every session opens with a one-time **context preamble** prepended to the first
`session/prompt` (as a leading text block, so the user's message is not
mangled). It tells the agent it is operating inside a Notesmith vault and steers
it to the vault-aware MCP tools (`search_notes`, `get_note`, `list_notes`,
`query_sql`, `list_tasks`). The wording adapts to the session: whether the vault
MCP endpoint is wired, and whether local I/O (below) is enabled. When local I/O
is off, the preamble explicitly states the agent has no shell or filesystem
access and must use the MCP tools.

This keeps the read-only/read-write MCP scope (ADR 0011 Phase C) meaningful and
aligns with ADR 0010's "ops layer is the single source of truth" stance.

### 2. Opt-in scoped local filesystem/terminal access (default off)

A new global setting, `agent.local_file_access` (off by default), advertises the
ACP `fs.{readTextFile,writeTextFile}` and `terminal` client capabilities and
implements them in a `ClientHandler`:

- **`fs/read_text_file`** — reads a file (honours `line`/`limit`), scoped to the
  vault directory.
- **`fs/write_text_file`** — writes/creates a file, scoped to the vault
  directory; **refused in read-only sessions**.
- **`terminal/create|output|wait_for_exit|kill|release`** — runs a command in
  the vault directory with captured, byte-capped output; **refused in read-only
  sessions** (a shell can mutate the vault, so it follows the write gate).

Every path is **scoped to the vault directory** via symlink-free lexical
normalization, so traversal (`..`) cannot escape the vault even for files that do
not yet exist. Inbound agent requests are handled on detached tasks so a blocking
`terminal/wait_for_exit` never stalls the protocol reader loop.

When the setting is off, `fs/*` and `terminal/*` are not advertised and any such
request is answered with `-32601`, per the ACP contract that an agent **must
not** call an unadvertised capability.

## Consequences

- **Out of the box**, the agent reads the vault through MCP — vault-aware,
  index-backed, and bounded by the read-only scope. "Ask about a note" works as
  long as the MCP endpoint is wired (always, in the desktop runner).
- **Power users** can opt into `agent.local_file_access` to give the agent a
  real, vault-scoped filesystem and shell (e.g. for bulk edits or running
  scripts) without abandoning the MCP tools, which remain available alongside.
- Enabling local I/O **widens the trust surface**: a read-write session can run
  arbitrary commands in the vault directory. It is therefore off by default and
  the write/terminal handlers are disabled in read-only sessions, so a read-only
  session cannot mutate the vault even with local I/O enabled.
- The CLI exposes the same capability via `notesmith agent run
  --local-file-access` for headless testing. The desktop runner reads the global
  config as the default and surfaces a per-session **"Local file access"** toggle
  in the chat panel (alongside the read-only toggle); the toggle is initialized
  from `agent.local_file_access` via the `agent_local_file_access_default`
  command and passed to `agent_start` to override the default for that session.
- Server/remote vaults are unaffected: there is no server-side chat runner, and
  hosted access stays MCP-only over HTTP (ADR 0010/0011).

## Alternatives considered

- **Proxy fs/terminal unconditionally** — simplest for the agent, but it makes
  the read-only scope meaningless (the agent could write/delete files directly)
  and bypasses the index. Rejected as the default; offered as an opt-in instead.
- **Steer to MCP only, never proxy fs/terminal** — safest and most aligned with
  ADR 0010, but it permanently denies the agent raw file/shell access that a
  general-purpose coding agent is built around. Kept as the default behaviour,
  but paired with the opt-in escape hatch.
