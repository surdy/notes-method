---
title: Work integrations post-fix rerun handoff
date: 2026-09-02
tags:
  - notesmith
  - verification
  - workiq
  - handoff
status: needs-follow-up
---

# Work integrations post-fix rerun handoff

Related:

- [[work-integrations-verification-handoff]]
- [[work-integrations-verification-results-handoff]]

## Executive summary

Commit `73fc0b9` fixed the original headless Copilot transport blocker.
Copilot now receives the Notesmith vault over HTTP MCP and can read and write
the scratch vault. The write-enabled daily briefing, manual job execution,
scheduled fire, and wake catch-up all work end to end.

The rerun exposed three separate follow-up areas:

1. The managed-section contract is not reliably enforceable through
   prompt instructions plus whole-note replacement. Copilot changed bytes
   outside managed sections.
2. Two verification expectations need clarification or corrected fixtures:
   read-only behavior when the daily note is absent, and the blocked-to-active
   stream transition.
3. The proposed Work IQ HTTP configuration requires a raw bearer token, while
   the authentication paths that work on this laptop intentionally keep their
   tokens inside protected caches.

Scratch vault:

`/Users/surdy/vaults/verify-work-2026-09-01-91e53b0a`

The test targeted September 1, 2026 because that was the daemon's PDT
`date('now', 'localtime')`. Notesmith.app was restored after the rerun.

## What `73fc0b9` fixed

Before the fix, `notesmith ai` passed only a stdio vault MCP binding. Copilot
is HTTP/SSE-only and silently ignored it.

After the fix:

- the headless path preserves the daemon URL returned by `ensure_daemon`;
- the active vault is advertised through `/mcp/<vault>` or
  `/mcp-ro/<vault>`;
- the stdio bridge is retained as a fallback for non-HTTP agents;
- Copilot can call Notesmith vault tools in direct and job-driven sessions.

### Post-fix proof

The write-enabled command:

```bash
notesmith ai prompt daily-note \
  --date 2026-09-01 \
  --vault verify-work-2026-09-01-91e53b0a \
  --url http://127.0.0.1:27183 \
  --agent copilot \
  --allow-writes
```

created `Daily/2026-09-01.md` and populated:

- `briefing/meetings`;
- `briefing/email` with the exact disconnected fallback;
- `briefing/tasks`;
- `briefing/attention`.

The job runner subsequently completed all of these with real note updates:

- manual `job run`;
- concurrent-run refusal;
- scheduled fire;
- catch-up after the daemon was stopped across the scheduled time.

## Rerun results

| Phase / check | Result | Key evidence |
|---|---|---|
| Build and workspace tests at `73fc0b9` | Pass | `cargo test --workspace` and `cargo build --release` passed. |
| A: read-only vault safety | Partial/fail | Copilot received Notesmith tools and `create_daily_note` was correctly denied. The vault checksum was byte-identical, but stdout was not briefing-shaped. |
| B: write-enabled briefing | Pass | Daily note created and all four managed sections populated. Work IQ fallback was exact. |
| C1: outside content byte identity | Fail | Trailing spaces in human-owned Focus and Notes text were removed. |
| C2: idempotent reruns | Pass | Two successive runs were byte-identical; four begin and four end markers remained. |
| C3: missing pair appends at EOF | Partial/fail | Attention pair was appended at EOF, but `updated:` changed and an extra blank line was inserted outside the new markers. |
| C4: data-change propagation | Test inconsistency | Changing Payments Migration from blocked to active removed it from the blocked group but made it a stale active stream, so it correctly remained in Attention. |
| D: config hot reload | Pass | Job enablement and schedule edits appeared without daemon restart. |
| D: manual run | Pass | Job updated the note and recorded success. |
| D: duplicate run | Pass | Immediate second invocation returned `job daily-briefing is already running`. |
| D: scheduled fire | Pass | The scheduled run advanced `last_run` and refreshed the note. |
| D: wake catch-up | Pass | Restart after the missed time triggered a successful catch-up and refreshed the note. |
| D: weekdays-only | Partial | Rendering was verified; a real weekend skip was not exercised. |
| E: missing prompt | Pass | Job recorded failed while the daemon remained healthy. |
| E: malformed SQL | Pass | Prompt command exited 1 with `Only SELECT statements are allowed`. |
| F: Work IQ through Notesmith HTTP MCP | Blocked | Notesmith requires a raw bearer token; the working Work IQ clients keep it in protected authentication storage. |

## Finding 1: managed sections need a server-side operation

### Observed C1 failure

Human content was added outside the markers with deliberate trailing spaces:

```text
Human focus text with odd spacing.··

  Indented human note with trailing spaces.···
```

After the agent refreshed the briefing, both lines had their trailing spaces
removed. A comparison was performed by replacing each managed-section
interior with a fixed placeholder and running `diff` on the remaining bytes.

The agent reported that it preserved all human-authored content, but the
outside-region diff was non-empty.

### Observed C3 failure

After deleting the complete `briefing/attention` pair, the agent correctly
appended a replacement block at the end of the note. However, the prefix was
not byte-identical:

```diff
-updated: 2026-09-01 23:45
+updated: 2026-09-01 23:46
```

An additional blank line was also inserted before the appended begin marker.

### Root cause

The current implementation asks a language model to:

1. read the complete note;
2. splice managed content into a reconstructed string;
3. replace the complete note through the generic note-update operation.

Prompt instructions cannot guarantee byte preservation. Model formatting,
serialization, newline handling, or update-side metadata can change content
outside the markers even when the model intends to comply.

### Recommendation

Add a deterministic Notesmith operation such as:

```text
update_managed_section(
    note_path,
    section_id,
    content,
    append_if_missing = true
)
```

The server should:

- locate an exact begin/end pair;
- replace only the interior byte range;
- preserve all bytes before the begin marker and after the end marker;
- append a complete marked block when the pair is absent;
- reject duplicate, inverted, or malformed marker pairs with a structured
  error;
- define explicitly whether automatic `updated` metadata is suppressed for
  managed-section writes;
- perform the operation atomically and detect stale-write conflicts.

The agent prompt should compose section content and call this operation once
per section. It should not reconstruct or replace the complete note.

Recommended tests:

1. trailing spaces, tabs, mixed line endings, and incomplete HTML comments
   outside markers remain byte-identical;
2. malformed or duplicate marker pairs return a structured error;
3. a missing pair appends exactly one separator and one complete block;
4. successive writes with identical content are byte-identical;
5. updating one section cannot change another managed section;
6. concurrent human edits produce a conflict rather than silent overwrite.

## Finding 2: phase A expectation conflicts with the prompt

The read-only run now behaves safely:

- Copilot receives the read-only Notesmith HTTP MCP endpoint;
- it fetches or attempts to fetch the daily note;
- `create_daily_note` is denied;
- no vault bytes change.

When the daily note is absent, Copilot responds:

```text
Unable to refresh [[Daily/2026-09-01.md]]: the note does not exist, and this
Notesmith vault is exposed as read-only, so create_daily_note was denied. No
vault content was changed.
```

The verification plan also expects stdout to contain a briefing-shaped result.
The prompt, however, tells the agent to ensure the note exists and then update
its managed sections. It does not explicitly tell the agent to render a
standalone briefing after creation is denied.

### Recommendation

Choose and document one intended read-only behavior:

1. **Safety-only check:** treat the current response as passing because the
   write was denied and the vault was unchanged.
2. **Preview behavior:** update the prompt so that when note creation or update
   is denied, the agent renders the proposed four-section briefing to stdout
   without writing it.
3. **Fixture behavior:** create the daily note before phase A, then verify that
   the agent can read it and produce a preview while all writes remain denied.

Option 2 provides the most useful CLI behavior, but it should be an explicit
product requirement rather than an inference left to the model.

## Finding 3: the C4 fixture does not produce the expected state

`Payments Migration` starts as:

```yaml
kind: stream
status: blocked
```

Changing it to `status: active` removes it from `blocked_streams`. However, it
has no meeting reference within the last 30 days, so the unchanged
`stale_streams` query immediately returns it. The agent therefore keeps it in
Attention as a stale active stream.

### Recommendation

Before changing the stream to active, create or update a recent meeting with:

```yaml
date: 2026-09-01
streams: ["[[Payments Migration]]"]
```

Alternatively, change the assertion to:

> Payments Migration leaves the blocked/waiting group and appears in the
> stale-active group.

The first option better tests that a source-data change can make an item leave
Attention entirely.

## Work IQ integration paths

Three distinct paths were evaluated.

### 1. Notesmith-configured HTTP MCP

```text
Notesmith job
  -> notesmith ai
    -> ACP session/new
      -> Notesmith vault HTTP MCP
      -> Work IQ HTTP MCP + Authorization bearer token
        -> Copilot calls both servers
```

Notesmith passes the Work IQ URL and resolved headers to the agent. The agent,
not Notesmith, opens the HTTP connection.

Advantages:

- centrally configured in Notesmith;
- available to desktop and headless sessions;
- potentially portable across HTTP-capable ACP agents.

Current problem:

- Notesmith supports static request headers but does not perform OAuth;
- the operator must obtain and refresh a raw bearer token;
- the authentication methods that work on this laptop intentionally store
  tokens in protected caches instead of exposing them.

### 2. Copilot-owned Work IQ plugin

```text
Notesmith job
  -> Copilot ACP process
    -> Copilot loads its own Work IQ plugin
      -> Copilot uses its existing OAuth session
```

This already works on the laptop. It is why the initial verification saw Work
IQ tools even though the isolated Notesmith configuration had no external MCP
servers.

Advantages:

- no token handling in Notesmith;
- OAuth, refresh, and consent are handled by Copilot;
- immediately usable for Copilot-backed jobs.

Tradeoffs:

- agent-specific rather than Notesmith-managed;
- another ACP agent does not inherit Copilot's plugins;
- it does not validate Notesmith's external-MCP configuration path.

### 3. Official Work IQ CLI

The official CLI authenticated successfully:

```bash
npx -y @microsoft/workiq@latest ask -q "Reply with OK only."
```

It also exposes:

```bash
workiq ask
workiq fetch
workiq mcp
```

The CLI uses its own approved OAuth client and protected MSAL cache.

#### Direct CLI connector

```text
Notesmith scheduled connector/job
  -> workiq ask or workiq fetch
    -> bounded result
      -> briefing flow writes only the summary
```

This avoids raw bearer-token management and would not encounter the Azure CLI
authentication block seen during verification.

#### `workiq mcp` as a stdio server

Configuring the CLI directly as a stdio MCP server is not sufficient for
Copilot because Copilot's ACP implementation ignores stdio MCP servers.

It would require:

- a different ACP agent with stdio MCP support; or
- a Notesmith-owned stdio-to-Streamable-HTTP adapter; or
- Notesmith invoking the CLI directly as a connector instead of passing it to
  the agent as an MCP server.

## Authentication evidence

The following were tested:

| Authentication route | Result |
|---|---|
| Copilot Work IQ plugin | Works |
| Official `@microsoft/workiq` CLI | Works |
| Generic `mcp-remote` OAuth | Fails because Work IQ does not support dynamic client registration |
| Azure CLI device-code login | Blocked by Conditional Access, `AADSTS53003` |
| Azure CLI browser login | Rejected with `AADSTS65002`; Azure CLI is not preauthorized for the Work IQ API |

The GitHub email being an alias for the Microsoft work account is not the core
problem. The clients use different Entra application registrations, token
caches, and authorization flows. A working Copilot or Work IQ CLI login does
not make its token available to Azure CLI or Notesmith.

## Recommended direction

### Immediate

Use Copilot's existing Work IQ plugin for Copilot-backed daily briefing jobs.
This validates the user-facing briefing flow without asking users to export
short-lived bearer tokens.

Update the verification plan so it distinguishes:

- **functional Work IQ briefing:** Work IQ may be supplied by the active agent;
- **Notesmith external-MCP propagation:** test with a controllable
  auth-protected HTTP fixture server rather than production Work IQ.

The fixture server can assert that Notesmith:

- includes enabled HTTP servers in `session/new`;
- expands environment-backed headers;
- never logs the resolved secret;
- handles authentication failures without breaking the vault job.

### Recommended product integration

Implement a Work IQ CLI-backed Notesmith connector.

The connector should:

- invoke the official CLI using its existing authentication cache;
- provide an explicit authentication-health check;
- use bounded inbox queries or a narrowly scoped `workiq ask`;
- return only the summary required by the daily briefing;
- keep raw messages out of vault files, job history, stdout capture, and error
  logs;
- surface expired authentication as an actionable reconnect error;
- support scheduled/headless execution after interactive authentication;
- apply per-run timeouts and output-size limits.

### Longer term

If external HTTP MCP remains the preferred architecture, add OAuth-aware MCP
configuration rather than requiring users to manage bearer tokens manually.
The design should cover:

- public-client OAuth metadata;
- browser/device authorization policies;
- secure token storage and refresh;
- tenant/admin consent errors;
- headless-job behavior after tokens expire;
- redaction from configuration APIs, diagnostics, and logs.

## Decisions requested from the implementing agent

1. Should managed-section byte preservation become a deterministic server
   operation?
2. Should read-only `ai prompt` produce a preview when writes are denied?
3. Should Work IQ be agent-provided, a first-class CLI-backed connector, or an
   OAuth-aware external MCP integration?
4. Should the production Work IQ dependency be removed from transport tests
   in favor of a deterministic auth-protected fixture MCP server?
5. Is automatic `updated` metadata allowed to change during a managed-section
   refresh, or does "outside is inviolable" prohibit it?

## Repository and machine state

- Repository `main` is synchronized with `origin/main`.
- No tracked repository changes were produced by the verification itself.
- The scratch vault remains available for inspection.
- Its job schedule was restored to `07:30`.
- Notesmith.app and its normal daemon were restored on port `27183`.
- No Work IQ bearer token was printed, exported, or written into the vault.
