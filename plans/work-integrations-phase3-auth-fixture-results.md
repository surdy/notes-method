---
title: Work integrations phase 3 auth fixture results
date: 2026-09-03
tags:
  - notesmith
  - verification
  - mcp
  - security
  - handoff
status: complete
---

# Work integrations phase 3 auth fixture results

Related:

- [[work-integrations-phase3-verification-handoff]]
- [[work-integrations-phase3-functional-f-results]]

## Environment

- Repository: `surdy/notes-method`, `main` at `0bc7c3b`
- External agent: GitHub Copilot CLI `1.0.83-3`
- Scratch vault:
  `/Users/surdy/vaults/verify-work-phase3-2026-09-02`
- Verification date: September 3, 2026
- Fixture endpoint: `http://127.0.0.1:8765/mcp`
- Notesmith config stored only the unexpanded
  `Authorization = "Bearer $FIXTURE_TOKEN"` form

The fixture was a scratch-only Python Streamable HTTP MCP server exposing one
tool, `fixture_ping`, and logging only whether the authorization header was
present and matched. It never logged the value.

## Results

| Auth-fixture check | Result | Evidence |
|---|---|---|
| Config propagation and environment expansion | Pass | A real `notesmith ai prompt auth-fixture --agent copilot` session exited 0 with exactly `AUTH_FIXTURE_OK`. The fixture logged `auth header present, matches=True` for initialization, tool listing, and tool invocation. |
| Agent can list and call the tool | Pass | Copilot called `fixture_ping` and received `OK`. This proves the external HTTP binding reached ACP `session/new` and was opened by the agent. |
| Streamable HTTP compatibility | Pass with observation | Copilot attempted an optional `GET /mcp` SSE stream. The minimal fixture returned 501, but the JSON Streamable HTTP path remained usable and the tool call succeeded. A production fixture should return a deliberate 405 or implement SSE rather than inheriting Python's generic 501 response. |
| Resolved secret absent from Notesmith storage | Pass | Scanned the scratch vault, vault-specific application state, and daemon log segment for both the valid and deliberately wrong token values. Both produced zero matches. |
| Wrong token / HTTP 401 | Pass | With `FIXTURE_TOKEN` changed while the server expectation remained unchanged, the fixture logged `matches=False` and returned 401. Copilot then probed OAuth protected-resource metadata, but the daily-briefing job still recorded `succeeded`, created all four balanced sections, and used the exact disconnected fallback. |
| Connection refused | Pass | With the fixture process stopped, a second daily-briefing job advanced `last` from `2026-09-03T16:09:08.862493+00:00` to `2026-09-03T16:09:48.010515+00:00`, remained `succeeded`, retained four balanced sections, and used the exact fallback. |
| Desktop Settings redaction and save preservation | Pass | Launched the real desktop app with the isolated config. The `Authorization` field rendered empty with the `value stored - leave blank to keep` hint. Saving with the field blank succeeded; the unexpanded stored value remained in `config.toml`, and the resolved value was absent. |

## Commands

Successful authenticated session:

```bash
XDG_CONFIG_HOME="$SCRATCH/.xdg-config" \
FIXTURE_TOKEN=s3cr3t-fixture-value \
  ./target/release/notesmith ai prompt auth-fixture \
  --date 2026-09-03 \
  --vault verify-work-phase3-2026-09-02 \
  --url http://127.0.0.1:27183 \
  --agent copilot
```

Observed stdout:

```text
AUTH_FIXTURE_OK
```

The wrong-token and offline checks used the scratch vault's
`daily-briefing` daemon job with Copilot's own `workiq` server disabled, so
the exact fallback was an unambiguous result of having no usable email tool:

```text
Email summary unavailable (Work IQ not connected).
```

## Secret scan

The literal valid and invalid fixture values were searched across:

```text
/Users/surdy/vaults/verify-work-phase3-2026-09-02/**
/Users/surdy/Library/Application Support/notesmith/verify-work-phase3-2026-09-02/**
~/Library/Logs/Notesmith/daemon.log
  (the segment beginning with this scratch vault's job-runner startup)
```

Match counts:

```text
valid resolved value: 0
wrong resolved value: 0
```

## Desktop acceptance and cleanup

The real desktop Settings check was completed against the isolated config:

1. `Authorization` rendered with an empty value and the
   `value stored - leave blank to keep` hint.
2. Saving without entering a replacement value preserved the unexpanded
   environment-variable reference in `config.toml`.
3. The resolved dummy secret did not appear in the config.

The fixture entry was then removed from the isolated config. The scratch
fixture and prompt remain in the scratch vault as evidence. The isolated app
and daemon were stopped, and the normal Notesmith.app desktop and daemon were
restored.
