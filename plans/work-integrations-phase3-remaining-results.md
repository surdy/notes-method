---
title: Work integrations phase 3 remaining results
date: 2026-09-03
tags:
  - notesmith
  - verification
  - workiq
  - mcp
  - handoff
status: needs-follow-up
---

# Work integrations phase 3 remaining results

Related:

- [[work-integrations-phase3-remaining-handoff]]
- [[work-integrations-phase3-verification-handoff]]
- [[work-integrations-phase3-functional-f-results]]
- [[work-integrations-phase3-auth-fixture-results]]

## Environment

- Repository: `surdy/notes-method`, `main` at `496300b`
- Managed-section implementation: `706ef0b`
- Notesmith binary: release build from this checkout
- External agent: GitHub Copilot CLI `1.0.83-3`
- Work IQ CLI: `@microsoft/workiq` `1.0.0`
- Scratch vault:
  `/Users/surdy/vaults/verify-work-phase3-2026-09-02`
- Verification date: September 3, 2026, matching the daemon's
  `date('now', 'localtime')`

The workspace baseline passed before the run:

```text
cargo test --workspace
cargo build --release
```

## Section 1: A-C and D/E regression results

| Check | Result | Evidence |
|---|---|---|
| Six exact context queries | Pass | Every expected row appeared. The old event, far-off and completed tasks, customer-matched external meeting, and recently touched active stream controls were excluded. |
| A: absent-note read-only preview | Pass | Exit 0; all four briefing sections printed; the daily note remained absent; complete SHA-256 manifests before and after were identical. |
| A: present-note read-only preview | Pass | Exit 0; all four briefing sections printed; complete SHA-256 manifests before and after were identical. |
| B: deterministic write path | Pass | Copilot made exactly four `update_managed_section` calls, one for each section. No `update_note` function call occurred. |
| B: briefing content grounding | Fail | The context query supplies only meeting start times, but the prompt requires start-end ranges. Copilot invented end times, and they changed between runs. |
| C1: outside bytes and `updated:` | Pass | Replaced every managed interior with the same fixed placeholder and compared the remaining bytes with `cmp`. They were identical; `updated: 2026-09-03 09:52` did not change. Trailing spaces, a tab-indented line, and an unclosed human HTML comment survived. |
| C2: idempotent reruns | Pass | Two full agent reruns produced the same SHA-256: `9872a6155e172c7826506c90a0733584c718f290d91dc1db14418c585f596f5f`. Four begin and four end markers remained. |
| C3: missing pair append | Flaky/fail | First run exited 0 but falsely reported the available write tool was unavailable and wrote nothing. Immediate retry appended the block correctly at EOF with exactly one blank-line separator, unchanged prefix bytes, and unchanged `updated:`. |
| C4: data propagation | Pass | After first adding a September 3 meeting reference and then changing Payments Migration to active, it disappeared entirely from Attention. Vendor Review, Data Platform, and Globex remained. Both fixture edits were restored. |
| C5: malformed marker HTTP refusal | Pass | HTTP 422 with `reason: duplicate_begin_marker`, `section_id: briefing/tasks`, and line details. SHA-256 was unchanged. |
| C5: malformed marker MCP refusal | Pass | The MCP tool surfaced `managed section [duplicate_begin_marker]` as a tool error. The note remained byte-identical. |
| D: config hot reload | Pass | Switching the job off and on was reflected by `job list` without restarting the daemon. |
| D: manual and concurrent run | Pass | First run started and completed successfully; the immediate second command exited 1 with `job daily-briefing is already running`. |
| D: scheduled fire | Pass | Setting `at = "10:04"` fired successfully at `2026-09-03T17:04:05.300691+00:00`. |
| D: wake catch-up | Pass | The daemon was stopped across `at = "10:06"` and restarted at 10:06:15 PDT. Catch-up completed at `2026-09-03T17:06:24.450083+00:00`. |
| D: weekdays-only | Partial | `job list` rendered `weekdays`; an actual weekend skip was not exercised. |
| E: missing prompt | Pass | The job recorded `failed` and the daemon stayed healthy. The prompt was restored byte-identically. |
| E: malformed context SQL | Pass | Exit 1 with `could not render prompt "daily-note": Only SELECT statements are allowed`. The prompt was restored byte-identically to the kit copy. |

### Finding: meeting end times are not grounded

The `todays_meetings` query returns:

```text
start, title, audience, path
```

It does not return an end time or duration, but the prompt requires:

```text
HH:MM-HH:MM [[title]] (audience)
```

Observed outputs for the same 14:00 Acme meeting included:

```text
14:00-15:00
14:00-14:30
```

Neither end time exists in the source note. The query should include an
authoritative end field, or the prompt should render start-only meetings when
no end is available.

### Finding: missing-pair refresh can falsely succeed

With `briefing/attention` removed, the first C3 run exited 0 and printed:

```text
The Notesmith managed-section write tool was unavailable, so nothing was
written to [[Daily/2026-09-03.md]].
```

The tool was present in the session and had worked in the immediately
preceding runs. No `update_managed_section` call was attempted. The note
remained without the required pair, so this is a functional false success.

An immediate retry used the same command and unchanged input. It called the
tool and passed the complete append contract:

```text
appended marker found: true
prefix is original bytes plus one newline: true
exactly one blank-line separator: true
end marker is at EOF: true
updated unchanged: true
```

The deterministic operation itself is correct; the remaining weakness is the
agent/job success criterion when the requested managed writes never happen.

## Section 6: `--additional-mcp-config` experiment

| Check | Result | Evidence |
|---|---|---|
| Flag syntax | Pass | Copilot `1.0.83-3` accepts `--additional-mcp-config=@/absolute/path/config.json`. |
| Trivial stdio fixture | Pass | A scratch Python newline-delimited JSON-RPC MCP server exposed `stdio_ping`; the agent returned exactly `STDIO_OK NOTESMITH_OK`. |
| ACP-supplied Notesmith server remains available | Pass | The same session called the spawn-configured stdio tool and the ACP-supplied HTTP `get_note` tool successfully. |
| Preinstalled `workiq mcp` | Pass | Installed `@microsoft/workiq@1.0.0` once outside the vault and configured its absolute `workiq` binary with `args: ["mcp"]`; no `npx` cold start was involved. |
| Work IQ functional briefing | Pass | Copilot's plugin-provided `workiq` server was disabled. The spawn-configured `notesmith-workiq` stdio server exposed its complete tool list, and the briefing wrote a live three-bullet email summary instead of the fallback. |
| Work IQ persistence boundary | Pass | After moving the CLI installation outside the vault, scanned 67 Notesmith-side files. Raw-header, message-ID, quoted-reply, bearer-token, and over-500-character-line match counts were all zero. |

Exact working config:

```json
{
  "mcpServers": {
    "notesmith-workiq": {
      "type": "local",
      "command": "/absolute/path/to/node_modules/.bin/workiq",
      "args": ["mcp"],
      "tools": ["*"],
      "deferTools": "never",
      "disableToolCache": true,
      "timeout": 55000
    }
  }
}
```

Exact wrapper shape:

```sh
exec copilot \
  --disable-mcp-server workiq \
  --additional-mcp-config=@/absolute/path/copilot-additional-mcp.json \
  "$@"
```

No `Rejecting non-http/sse MCP server` message appeared. This validates the
handoff's proposed small product change: inject a per-process Copilot MCP
config at spawn time while retaining the normal ACP-supplied bindings.

## Section 4: Work IQ CLI capture

### Command surface

`workiq fetch --help` exposes one required option:

```text
-u, --urls <urls> (REQUIRED)
    WorkIQ entity URLs to fetch (e.g., /me, /me/messages)
```

Shared options are `--account`, `--log-level`, and `--help`. There are no
fetch subcommands and no `--json`, `--format`, `--since`, `--top`, `--limit`,
`--select`, `--fields`, or `--no-body` CLI flags.

Filtering, projection, and limiting are expressed in the entity URL itself.
This request succeeded:

```text
/me/mailFolders/inbox/messages?
  $filter=receivedDateTime ge 2026-09-03T00:00:00Z&
  $select=id,subject,from,receivedDateTime&
  $top=2
```

### Output shape

Stdout is a Graph-shaped JSON object:

```json
{
  "@odata.context": "<metadata URL>",
  "value": [
    {
      "@odata.etag": "<redacted>",
      "id": "<redacted>",
      "receivedDateTime": "2026-09-03T11:41:03Z",
      "subject": "<redacted>",
      "from": {
        "emailAddress": {
          "name": "<redacted>",
          "address": "<redacted>"
        }
      }
    }
  ],
  "@odata.nextLink": "<continuation URL>"
}
```

The two returned entities contained only:

```text
@odata.etag, from, id, receivedDateTime, subject
```

No `body`, `bodyPreview`, or `uniqueBody` field was present. Therefore the
CLI can receive sender and subject metadata without receiving message bodies,
provided the connector supplies a narrow `$select`.

### Authentication and performance

- With no `--account`, a fresh shell reused the existing authentication cache
  non-interactively.
- The two-message, one-day request exited 0 in approximately 4.2 seconds and
  emitted 1,507 bytes.
- Supplying a deliberately nonexistent `--account` value still exited 0 and
  returned the current user. No diagnostic explained whether the value was
  ignored or resolved through a fallback, so no stronger claim is made.
- The CLI exposes `workiq auth login`, `logout`, and `consent`.
- Expired-cache behavior was not exercised because doing so would require
  disrupting the working sign-in.
- Rate limiting was not induced.

## Cleanup and remaining manual check

- The schedule was restored to `07:30`.
- The kit prompt was restored byte-identically.
- The C4 data edits and C5 malformed marker were removed.
- The isolated daemon was stopped.
- The normal Notesmith.app daemon was restored.
- The scratch vault and stdio fixtures remain at the path above.

The only remaining handoff item is the real desktop Settings redaction check
from [[work-integrations-phase3-auth-fixture-results]].
