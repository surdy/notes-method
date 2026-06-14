# ADR 0013 — Agent Discovery, Manual Configuration & Diagnostics

## Status

Accepted (2026-06-14). Builds on [ADR 0012](0012-agent-transport-acp-mcp.md)
(ACP transport + the desktop IPC bridge). This ADR does not change the transport;
it specifies **how the desktop app discovers, launches, and configures** the
external agent CLIs, and how a user debugs discovery when it fails.

## Context

ADR 0012 shipped the agent chat panel with a hardcoded three-entry catalog
(Copilot / Claude / Codex). Availability is decided by `binary_on_path(program)`
in `crates/notesmith-tauri/src/agent_bridge.rs`, which reads
`std::env::var_os("PATH")` and checks each directory for the program.

This fails in the field for a cluster of reasons:

- **macOS GUI launches get a minimal `launchd` PATH** (`/usr/bin:/bin:…`) that
  excludes Homebrew (`/opt/homebrew/bin`), nvm, asdf, volta, bun, and
  `~/.cargo/bin`. A bundled `.app` launched from Finder/Dock therefore cannot
  find `copilot`, `npx`, or `codex-acp` even when they are installed and on the
  user's shell PATH. The agent picker then renders **all options disabled**, and
  WebKit draws a `<select>` whose selected option is disabled as an **empty
  box** — the user sees a blank, unexplained dropdown.
- **No manual override.** If detection is wrong, or the user installed an agent
  in a non-standard location, or wants a custom ACP agent, there is no escape
  hatch.
- **No diagnosability.** When an agent does not appear there is no way to see
  *why* — which PATH was searched, which candidates were probed, what failed.
- **Gemini is missing** from the catalog despite being a popular ACP agent.

The widely-used reference integration is **Zed** (authors of ACP). Zed offers a
curated **ACP Registry** of common agents (Claude, Codex, Copilot, Gemini CLI,
OpenCode, Cursor, Pi), an **Add Custom Agent** path that writes an
`agent_servers.<id>` block (`{ type, command, args, env }`), and a **wire-log
viewer** (`dev: open acp logs`) for debugging the JSON-RPC stream. Agents own
their own auth/billing/model — which Notesmith already assumes.

## Decision

The goal is **idiot-proof discovery**: a user who has installed any popular agent
CLI (Copilot, Claude, Codex, Gemini) should open the panel and just use it, with
a clear manual fallback and opt-in diagnostics when auto-detection misses.

### Discovery & launch

1. **Resolve the real PATH at startup (root-cause fix).** The desktop app
   computes an augmented PATH once, by (a) querying the user's login shell
   (`$SHELL -lic 'printf %s "$PATH"'`, bounded timeout, best-effort) and (b)
   merging a curated set of common locations (`/opt/homebrew/bin`,
   `/opt/homebrew/sbin`, `/usr/local/bin`, `/usr/bin`, `~/.cargo/bin`,
   `~/.local/bin`, `~/.bun/bin`, `~/.deno/bin`, `~/.volta/bin`, the npm global
   prefix, and asdf shims). This resolved PATH is used **both** for availability
   detection **and** when spawning the agent process, so detection and launch
   never disagree. Failure to query the shell degrades to the curated set; it
   never panics (ADR 0009 spirit).

2. **A declarative agent registry replaces the hardcoded tuple.** A single
   source of truth in `notesmith-agent` describes each known agent: `id`,
   display name, an **ordered list of launch candidates** (native-ACP binary /
   `npx` package adapter / separate adapter binary, each with `program` + base
   `args`), an optional **probe** (e.g. `--version`) used only for diagnostics,
   a setup hint, a docs URL, and an auth hint. Shipped registry: **Copilot**
   (`copilot --acp`), **Claude** (`npx --yes @zed-industries/claude-code-acp`),
   **Codex** (`codex-acp`), **Gemini** (`gemini --experimental-acp`), and
   **OpenCode**. The registry is the only place a new built-in agent is added.

3. **Detection is fast by default, deep on demand.** The availability check is a
   cheap PATH-existence test against the resolved PATH (no process spawn) so the
   picker populates instantly. Version **probes** (which spawn the CLI) run only
   when the user invokes diagnostics, keeping the happy path snappy.

### Manual configuration

4. **`config.toml` `[agents]` is the manual escape hatch** (chosen over frontend
   `localStorage` so it is Rust-readable, consistent with `[daemon]`/`[vaults]`,
   and survives reinstalls). Schema, mirroring Zed's `agent_servers`:

   ```toml
   [agents]
   debug = false              # opt-in diagnostics (see below)

   [agents.copilot]           # override a built-in: point at a custom binary
   command = "/opt/copilot/bin/copilot"
   args = ["--acp"]

   [agents.my-agent]          # add a brand-new custom ACP agent
   display_name = "My Agent"
   command = "node"
   args = ["~/projects/agent/index.js", "--acp"]
   enabled = true
   [agents.my-agent.env]
   FOO = "bar"
   ```

   A user entry **always wins** over auto-detection for the same `id`. A custom
   `id` not in the registry is launched verbatim. `enabled = false` hides a
   built-in. Tilde and `$VAR` in `command`/`args` are expanded against the
   resolved environment.

### Diagnostics

5. **Opt-in, structured diagnostics (`agents.debug`, default off).** When on, the
   discovery pipeline records a **step-by-step trace**: how PATH was resolved
   (shell query result vs. curated fallback, final entries), and per agent — each
   candidate program, the directories searched, found/not-found with a reason,
   the probe command and a bounded stdout/exit snippet, and the final verdict
   (available / not-found / overridden / disabled). The trace is surfaced two
   ways: a **"Run diagnostics" button** in Settings that renders the trace inline
   (copyable for bug reports), and — for spawn/handshake failures — an opt-in
   **ACP wire log** that tees the JSON-RPC stream to a file (Zed's
   `open acp logs` analogue). Default-off means zero overhead and no log noise
   for the common case.

### UI

6. **Never show a blank picker.** The agent `<select>` always renders the
   selected agent's name even when its option is disabled (the WebKit fix); when
   **zero** agents are detected it shows an inline empty-state — *"No agent CLI
   found — install Copilot, Claude, Codex, or Gemini, or configure one in
   Settings"* — linking to the AI-agent settings. Available agents are listed
   first; unavailable ones are shown disabled and labelled "(not found)".

7. **Settings gains an "AI Agent" discovery surface:** a list of registry agents
   with availability (✓ / ✗ + reason), a "Set path…" override for a
   detected-but-missing agent, an "Add custom agent" form (id, command, args,
   env), the **debug toggle**, and the **"Run diagnostics"** action. The
   existing break-glass toggle stays in this section.

## Consequences

- **Pros.** Popular agents work out-of-the-box from a GUI launch; the empty/blank
  picker is eliminated and explained; users have a first-class manual fallback
  and custom-agent support; failures are self-serviceable via copyable
  diagnostics; the registry centralises agent knowledge for future additions.
- **Cons / risks.** Querying the login shell spawns a short-lived process at
  startup (bounded, best-effort, cached). The augmented PATH widens what the app
  can launch — mitigated because we only ever launch a registry/user-configured
  agent command, never arbitrary input. Custom agents run user-specified
  binaries; this is an explicit, user-authored escape hatch (same trust model as
  Zed's `agent_servers`).
- **Resilience.** PATH resolution, registry parsing, and `[agents]` config
  parsing all degrade rather than panic (missing shell, unreadable config, bad
  entry → skip with a WARN). Diagnostics probes are bounded in time and output.

## Suggested phasing

1. **PATH resolution** — login-shell + curated dirs; spawn agents with it; used
   by detection. (Rust, TDD.) Root-cause fix; unblocks GUI launches immediately.
2. **Declarative registry** incl. **Gemini** + OpenCode. (Rust, TDD.)
3. **Detection pipeline + structured diagnostics trace.** (Rust, TDD.)
4. **`config.toml` `[agents]`** manual overrides + custom agents + `debug`.
   (Rust, TDD.)
5. **Settings UI**: agent list + availability/reasons, custom-agent form, debug
   toggle, "Run diagnostics". (Svelte + Playwright.)
6. **Picker empty-state / blank-select fix.** (Svelte + Playwright.)
7. **Docs sync** (CONTEXT, README, `docs/cli.md`/config docs, this ADR).

## Alternatives considered

- **Frontend `localStorage` for config** (like break-glass). Rejected: not
  Rust-readable (detection/launch live in Rust), desktop-only, lost on reinstall.
- **Bundling/auto-installing the agent CLIs.** Rejected: large binaries, each has
  its own auth/update cadence; matches neither Zed's model nor the user's
  "install via their own package manager" preference.
- **Always probing `--version` during detection.** Rejected for the fast path
  (per-agent process spawns add startup latency); kept as an on-demand
  diagnostics step.
- **A full ACP Registry with remote install** (Zed-style). Deferred: our curated
  built-in registry + manual custom agents covers the requirement without a
  network-backed registry service.

## References

- [ADR 0012 — Agent Transport: ACP Client + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- Zed external agents & `agent_servers`: https://zed.dev/docs/ai/external-agents
- Agent Client Protocol: https://agentclientprotocol.com
