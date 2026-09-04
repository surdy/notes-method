# How Notesmith gets stdio MCP servers into Copilot

A reference for future-us: what Copilot's ACP stdio limitation actually is,
how it bit this project twice, and how the spawn-time injection workaround
gets around it. The normative docs are the ADR 0012 amendments and
`docs/ai-mcp-servers.md`; this is the narrative that connects them.

## The limitation, precisely

GitHub Copilot CLI's ACP mode (`copilot --acp`) **rejects stdio MCP servers
supplied by the ACP client** in `session/new`. It logs

```text
Rejecting non-http/sse MCP server "<id>" from client
```

and never spawns the server — while `session/new` still returns *success*,
so the client sees a healthy session with silently missing tools.

- **Root cause:** Copilot's ACP `initialize` advertises
  `mcpCapabilities: {http: true, sse: true}` and omits `stdio` — the one
  transport the ACP spec marks as mandatory ("All Agents MUST support
  connecting to MCP servers via stdio").
- **It is ACP-specific.** Copilot happily runs stdio MCP servers from its own
  config file and SDK (`createSession({mcpServers: {type: "stdio", ...}})`).
  Only the ACP client-supplied door is bolted.
- **The docs disagree with the runtime.** Copilot CLI v1.0.25 release notes
  claim "ACP clients can now provide MCP servers (stdio, HTTP, SSE)…". That
  has been false continuously from at least 1.0.52 (third-party verification,
  achieveai/LmDotnetTools#59) through 1.0.83-3 (our verification).
- **Upstream tracker:** [github/copilot-cli#3889](https://github.com/github/copilot-cli/issues/3889)
  (open, no maintainer response). Ancestors #1040 and #1255 are also open.
  **We do not file or comment upstream** — watch #3889 and re-test on new
  releases.

## It bit us twice

**1. The vault binding.** Notesmith's own vault tools were being offered to the
agent over a stdio bridge (`notesmith mcp start`) installed as the *primary*
MCP binding. Copilot took nothing, so a headless briefing run had no vault
tools at all and quietly did nothing while still reporting success. The fix
(commit `73fc0b9`) was to make the **HTTP** endpoint the preferred binding —
`/mcp/<vault>` read-write, `/mcp-ro/<vault>` read-only — with the stdio bridge
kept only as a fallback for agents that do not advertise HTTP MCP, and omitted
entirely for a remote daemon. This is the correct answer on its own merits (no
subprocess, works unchanged against a remote daemon, native daemon transport),
and it is what ADR 0012 Decision 2 already intended; the stdio-primary wiring
was the bug. See `crates/notesmith-cli/src/commands/ai.rs`.

**2. External stdio servers.** The HTTP-first vault binding does not help the
*external* `[[mcp.servers]]` surface: a stdio-only tool such as Microsoft Work
IQ's `workiq mcp` has no HTTP endpoint to fall back to. Configured through
Notesmith, it reached Claude / Codex / Gemini / OpenCode sessions but was
silently absent from every Copilot session. That is what the workaround below
solves.

## The workaround: hand Copilot its own config file

Copilot accepts stdio servers on its **own command line**:
`--additional-mcp-config=@<absolute path>` loads an extra MCP config document
at startup, and the servers it names are spawned normally — no ACP rejection,
and they coexist with the ACP-supplied HTTP bindings in the same session.

The flag is **process-scoped**, which is normally its weakness (one Copilot
process could serve many sessions with one config). But a headless
`notesmith ai` run is **one session in one process**, so for our use it is
effectively session-scoped and the weakness does not apply. Field-validated on
Copilot CLI `1.0.83-3`, 2026-09-03
(`plans/work-integrations-phase3-remaining-results.md`, Section 6): the
injected `workiq mcp` server delivered a live email briefing while the
ACP-supplied HTTP vault tools worked in the same turn, and no `Rejecting`
line appeared.

### What Notesmith does

When a session is built for an agent whose descriptor sets
`rejects_client_stdio_mcp` (today only `copilot`) and whose extra MCP bindings
include stdio entries, `crates/notesmith-agent/src/spawn_mcp.rs`:

1. **Writes a per-session config file** describing those stdio servers, in the
   shape Copilot's own config accepts:

   ```json
   {
     "mcpServers": {
       "notesmith-workiq": {
         "type": "local",
         "command": "/absolute/path/to/workiq",
         "args": ["mcp"],
         "tools": ["*"],
         "deferTools": "never",
         "disableToolCache": true,
         "timeout": 55000
       }
     }
   }
   ```

   A binding that carries environment variables additionally gets an `"env"`
   object (`{"NAME": "value"}`); it is omitted when the binding has no env.

2. **Appends `--additional-mcp-config=@<absolute path>`** to that session's
   spawn arguments (the base ACP args are unchanged).

3. **Omits those stdio bindings from the `session/new` / `session/load`**
   `mcpServers` array, where they would only be refused with a warning.
   HTTP externals and the vault binding still travel over ACP as before.

Both the headless CLI (`notesmith ai`) and the desktop bridge inherit this
through the agent registry — no caller changes — and every agent without the
flag behaves exactly as it did before. Implemented in commit `00ead65`.

### Why the details are the way they are

- **`timeout` is 55000 ms** deliberately: just under Copilot's hard,
  non-configurable 60 s MCP initialization budget
  ([copilot-cli#4421](https://github.com/github/copilot-cli/issues/4421)). A
  slow server then times out as one dead server rather than tripping Copilot's
  global budget and failing the whole session start. Prefer a pre-installed
  binary over an `npx`-launched one for this reason.
- **The file is `0600` and never logged.** A server's `env` may hold
  credentials (a Work IQ token). It is created owner-read/write only, its
  contents and any resolved env value are never written to a log, and it is
  **deleted when the session is dropped** — its lifetime is the process it
  configures.
- **A write failure degrades, it does not fail the run.** If the config file
  cannot be written, only the I/O `ErrorKind` is recorded (never the document
  or its env values) and the session falls back to the old behavior: the stdio
  servers stay in `session/new` and Copilot refuses them, exactly as before
  this mechanism existed.
- **It is a per-agent capability flag, not a hardcoded name check.** The flag
  lives on the agent descriptor (`rejects_client_stdio_mcp`), so if Copilot
  later accepts client-supplied stdio servers (i.e. #3889 is fixed), turning
  this off is a one-line change in `crates/notesmith-agent/src/registry.rs`.

## Net effect

One `[[mcp.servers]]` configuration now reaches **every** agent: Copilot
through spawn-time injection, everyone else through ACP as usual. HTTP remains
the vault binding's preferred transport regardless; this affects only the
external server surface.

## See also

- ADR [`0012-agent-transport-acp-mcp.md`](adr/0012-agent-transport-acp-mcp.md)
  — the 2026-09-02 and 2026-09-03 amendments (normative).
- [`ai-mcp-servers.md`](ai-mcp-servers.md) — the user-facing stdio-server
  caveat and the Copilot injection note.
- `crates/notesmith-agent/src/spawn_mcp.rs`,
  `crates/notesmith-agent/src/registry.rs` — the implementation.
- `plans/work-integrations-phase3-remaining-results.md` — the field
  verification (Section 6).
