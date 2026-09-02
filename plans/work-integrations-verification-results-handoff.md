---
title: Work integrations verification results handoff
date: 2026-09-02
tags:
  - notesmith
  - verification
  - handoff
status: blocked
---

# Work integrations verification results handoff

## Executive summary

The real-machine verification is blocked in the headless Copilot path before
the daily-briefing agent can use any Notesmith vault tools.

Copilot launches successfully, is authenticated, receives the fully rendered
prompt, and returns exit code 0. However, its ACP session does not contain the
Notesmith MCP server. Both read-only and read-write runs respond that
`get_note`, `create_daily_note`, and note-update tools are unavailable.
Consequently:

- no daily note is created;
- managed-section behavior cannot be exercised;
- manual, scheduled, and wake catch-up jobs report `succeeded` even though
  they did not perform the requested write.

This does not appear to be an authentication or permission problem. The same
result occurred with `--allow-writes`, and Copilot was able to use its own
Work IQ tools. The failure is specific to wiring the Notesmith vault MCP
binding into the headless Copilot ACP session.

> [!bug] Primary finding
> `notesmith ai` supplies a stdio-only Notesmith MCP binding. Copilot 1.0.83-1
> is HTTP/SSE-only and silently ignores that binding.

## Environment

- Repository: `surdy/notes-method`, `main`
- Required merge: `b00c5bf` present in `HEAD`
- Notesmith binary: release build from the checked-out source
- External agent: GitHub Copilot CLI `1.0.83-1`
- Scratch vault:
  `/Users/surdy/vaults/verify-work-2026-09-01-91e53b0a`
- Verification target date: September 1, 2026, matching the daemon's PDT
  `date('now', 'localtime')`
- Notesmith global config and job state were isolated under the scratch vault
- The normal Notesmith.app daemon was restored after verification

## Minimal reproducer

With a daemon serving the scratch vault on `http://127.0.0.1:27183`:

```bash
notesmith ai prompt daily-note \
  --date 2026-09-01 \
  --vault verify-work-2026-09-01-91e53b0a \
  --url http://127.0.0.1:27183 \
  --agent copilot
```

Observed exit code: `0`

Observed stdout:

```text
Blocked: the Notesmith MCP tools (`get_note`, `create_daily_note`, and note
update tools) are not available in this session. I could read today's inbox
through Work IQ, but could not safely fetch or modify
[[Daily/2026-09-01.md]] while preserving all unmanaged content. No vault
changes were made.
```

The write-enabled command behaves the same:

```bash
notesmith ai prompt daily-note \
  --date 2026-09-01 \
  --vault verify-work-2026-09-01-91e53b0a \
  --url http://127.0.0.1:27183 \
  --agent copilot \
  --allow-writes
```

Observed stdout:

```text
Blocked: the Notesmith vault tools (`get_note`, `create_daily_note`, and note
update tools) are not attached to this session, so `Daily/2026-09-01.md` was
not read or modified. Work IQ was available, but changing the note without
the authoritative Notesmith tools would risk overwriting unmanaged content.
```

`Daily/2026-09-01.md` remained absent after both commands. A SHA-256 manifest
before and after the read-only command was byte-identical.

## Likely code path

The headless implementation in
`crates/notesmith-cli/src/commands/ai.rs:283-311`:

1. calls `ensure_daemon(...)` but discards the returned daemon base URL;
2. creates only
   `McpBinding::local_bridge(notesmith_bin, &detected.name, read_only)`;
3. installs that stdio binding with `.with_mcp(binding)`;
4. does not install an HTTP primary binding or call
   `.with_mcp_stdio_fallback(...)`.

The shared ACP driver already contains the intended capability-aware
selection:

- `crates/notesmith-agent/src/acp.rs:204-216` selects the primary binding when
  `mcpCapabilities.http` is true and the stdio fallback otherwise;
- `crates/notesmith-agent/src/acp.rs:1111-1117` reads the agent's advertised
  HTTP capability;
- unit coverage exists at
  `crates/notesmith-agent/src/acp.rs:1543-1568`.

The headless CLI does not provide the pair of bindings that selection expects.

## Suggested implementation direction

Capture the daemon URL returned by `ensure_daemon(...)`, construct the
read-only or read-write HTTP endpoint for the selected vault, and configure
the session with:

```text
HTTP Notesmith binding as the preferred MCP binding
stdio local bridge as with_mcp_stdio_fallback(...)
```

The endpoint should preserve explicit `--url` / `NOTESMITH_URL` selection and
use:

- `/mcp-ro/<vault>` for the default read-only run;
- `/mcp/<vault>` when `--allow-writes` is present.

Recommended regression coverage:

1. A headless fake ACP agent advertising `mcpCapabilities.http = true`
   receives an HTTP Notesmith server in `session/new`.
2. An agent advertising HTTP support as false receives the stdio fallback.
3. An explicit non-default daemon URL is preserved in the HTTP binding.
4. Read-only and read-write runs use the correct endpoint.
5. A headless prompt integration test proves the agent can call a vault tool,
   rather than asserting only that the ACP process exited successfully.
6. A job-level test catches the current false-success case: a daily-briefing
   run must not be considered functionally successful when no daily note or
   managed section was updated.

## Verification results

| Phase / check | Result | Evidence |
|---|---|---|
| Prerequisites and initial workspace tests | Pass | Merge present; release build succeeded; initial `cargo test --workspace` passed. |
| Kit installation | Pass | Expected vault config, prompt, daily template, and skill installed. |
| Six prompt context queries | Pass | Every expected row appeared and all controls were excluded. |
| A: read-only safety | Fail | Vault stayed byte-identical, but output was not a briefing because Notesmith tools were absent. |
| B: full run with writes | Fail | Exit 0, no write, and `Daily/2026-09-01.md` absent. |
| C: managed-section contract | Not exercised | Requires the daily note that phase B could not create. |
| D: config hot reload | Pass | Enabling the job appeared in `job list` without restart. |
| D: concurrent-run refusal | Pass | Second `job run` returned `job daily-briefing is already running`. |
| D: manual job outcome | Fail | Recorded `succeeded`, exit 0, but daily note remained absent. |
| D: scheduled fire | Partial/fail | Schedule fired and advanced `last_run`; no note was created. |
| D: wake catch-up | Partial/fail | Restart caught up the missed schedule; no note was created. |
| D: weekdays-only | Partial | `weekdays` rendered; weekend skip was not exercised. |
| E: missing prompt | Pass | Job recorded failed in 10 ms and daemon remained healthy. |
| E: malformed SQL | Pass | Exit 1: `Only SELECT statements are allowed`. |
| F: Work IQ persistence boundary | Not exercised | Missing vault tools made persistence impossible; no token was requested. |
| Final regression | Pass | Final `cargo test --workspace` passed; repository remained clean. |

The prompt and daily template were restored byte-identically to the kit
copies. The schedule was restored to `07:30`. The scratch vault remains in
place for inspection.

## Work IQ observation

The isolated Notesmith global config contained no external MCP servers, but
Copilot still exposed Work IQ from its own configuration. This contaminated
the intended "Work IQ disconnected" checks in phases A and B.

No raw email markers, message IDs, authorization headers, bearer values, or
`WORKIQ_TOKEN` strings were found in the scratch vault. `WORKIQ_TOKEN` was not
set in the verification environment.

For a clean rerun, phases A-E should disable Copilot's independently configured
Work IQ server. Phase F should then attach Work IQ through Notesmith's
`[[mcp.servers]]` configuration only.

## What Harpreet can unblock manually

> [!important] No auth or approval is currently needed for the primary failure
> Copilot authentication worked, the daemon was healthy, read-write permission
> was explicitly granted, and the prompt rendered correctly. Supplying more
> permissions or logging in again will not add the missing Notesmith MCP
> binding.

There are two useful manual paths:

1. **Validate the non-Copilot portions before the fix.** Install and
   authenticate a supported ACP agent that advertises stdio MCP support, then
   rerun with `--agent <id>`. This may unblock phases B-E and the managed
   section checks, but it does not clear the Copilot acceptance failure.
2. **Prepare for phase F after the MCP fix.** Provide the Work IQ endpoint and
   export a fresh `WORKIQ_TOKEN` in the environment used to start both the CLI
   and daemon. The token was intentionally not requested during this run
   because the summary could not be persisted without vault tools.

An optional diagnostic is to configure Copilot directly with the scratch
daemon's HTTP MCP endpoint and prove it can call `get_note`. That would confirm
the daemon endpoint independently, but it bypasses Notesmith's ACP
`session/new` wiring and therefore must not be counted as passing the product
verification.
