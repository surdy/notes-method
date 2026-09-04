---
title: Work integrations phase 4 verification results
date: 2026-09-04
tags:
  - notesmith
  - verification
  - workiq
  - jobs
  - handoff
status: complete
---

# Work integrations phase 4 verification results

Related:

- [[work-integrations-phase4-verification-handoff]]
- [[work-integrations-phase3-remaining-results]]
- [[work-integrations-phase3-functional-f-results]]
- [[work-integrations-phase3-auth-fixture-results]]

## Environment

- Repository: `surdy/notes-method`, `main` at `ed347de`
- Notesmith binary: release build from this checkout
- External agent: GitHub Copilot CLI `1.0.83-5`
- Work IQ CLI:
  `1.0.0.28144+10c4074955aee0affce923a5fb04d7ed22c5a09e`
- Scratch vault:
  `/Users/surdy/vaults/verify-work-phase4-2026-09-04`
- Verification date: September 4, 2026
- Daemon-local clock during the run:
  `2026-09-03 22:51:33` (`date('now', 'localtime') = 2026-09-03`)

The workspace baseline passed before the run:

```text
cargo test --workspace
cargo build --release
```

The handoff explicitly requires fixtures to follow the daemon-local date when
it differs from the requested date. The meeting fixtures and daily-note agent
runs therefore used September 3 after the September 4 fixtures correctly
produced an empty `todays_meetings` result against the host clock.

## Section 1: meeting end-time grounding

| Check | Result | Evidence |
|---|---|---|
| Exact context query | Pass | The daemon returned two rows: Standup with `start = 09:30` and authoritative `end = 10:00`; Acme sync with `start = 14:00` and `end = null`. |
| Meeting with an end | Pass | The managed section rendered `09:30–10:00 [[2026-09-03 0930 Standup]] (internal)`. |
| Meeting without an end | Pass | The managed section rendered `14:00 [[2026-09-03 1400 Acme sync]] (external)`, with no range before the wikilink. |
| Repeatability | Pass | A second unchanged agent run produced an identical `briefing/meetings` interior; Acme never acquired an invented end time. |

The first attempt using September 4 fixtures wrote `No meetings today.` on
both runs. This was not a product defect: the exact prompt SQL also returned
zero rows because the isolated daemon's host clock still reported September
3. Rebuilding the two fixtures for the daemon-local day made the same
production query and prompt pass.

## Section 2: spawn-time stdio MCP injection

| Check | Result | Evidence |
|---|---|---|
| Literal handoff ID `workiq` while disabling Copilot `workiq` | Fail | Copilot's process signature showed the injected local server and `disabled:["workiq"]`; the run made no Work IQ call and retained `Email summary unavailable (Work IQ not connected).` The disable flag applies to both servers when their IDs collide. |
| Distinct Notesmith ID `notesmith-workiq` | Pass | Changing only the external server ID while retaining `--disable-mcp-server workiq` exposed the full `notesmith-workiq-*` tool set. Copilot called `notesmith-workiq-ask`, and the daily note received a live inbox summary. |
| Product path, not manual additional-config wrapper | Pass | The Copilot command line contained Notesmith's generated `--additional-mcp-config=@/var/.../notesmith-mcp-*.json`; the wrapper supplied only the built-in-server disable and debug-log flags. |
| Generated config shape and permissions | Pass | The observed temporary file was mode `0600` and contained a `local` server using the preinstalled absolute Work IQ binary, `args = ["mcp"]`, and `timeout = 55000`. |
| ACP rejection absent | Pass | The renamed run contained zero `Rejecting non-http/sse MCP server "notesmith-workiq" from client` messages. |
| Session cleanup | Pass | The generated `notesmith-mcp-*.json` file count was zero after the agent session exited. |
| Notesmith persistence boundary | Pass | Scanned 79 scratch-vault, vault-cache, and isolated-daemon files for the actual raw-body phrase `You are receiving this because your review was requested.`; zero files matched. Quoted-reply markers, `Message-ID` headers, and bearer-token shapes also had zero matches. |
| Claude/Codex ACP stdio path | Not exercised | Copilot was the required product-path target; no additional agent CLI was used. |

The productized injection works, but the handoff's sample `id = "workiq"` is
incompatible with the required `--disable-mcp-server workiq` isolation step.
Use a distinct external ID such as `notesmith-workiq`, matching the successful
phase-3 experiment and the narrative example in
`docs/copilot-acp-stdio-workaround.md`.

## Section 3: effect-based job outcomes

| Check | Result | Evidence |
|---|---|---|
| Real briefing write attribution | Pass | `job run daily-briefing` completed `succeeded` with `writes: 4` and `sections_written` exactly `briefing/attention`, `briefing/email`, `briefing/meetings`, and `briefing/tasks`. |
| Exit-zero no-write classification | Pass | A temporary write-enabled agent job replied without tools, exited 0, and recorded `status: no_writes`, `writes: 0`, not `succeeded`. |
| SSE event | Pass | The live event stream emitted `job.started` followed by `job.no_writes` for the temporary job; no `job.failed` event represented the quiet run. |
| `last_success` behavior | Pass | The temporary job had no prior success; `last_success` remained unset after the `no_writes` run. |
| Satisfied `success_when` | Pass | `SELECT path FROM v_notes WHERE path = 'Daily/2026-09-03.md'` authoritatively overrode zero writes to `succeeded` while preserving `writes: 0`. |
| Unsatisfied `success_when` | Pass | Pointing the same predicate at `Daily/never-exists.md` recorded `failed`; the daemon logged `success_when predicate not satisfied`. |
| Temporary job cleanup | Pass | The throwaway job and prompt were removed after the checks. |

## Section 4: calendar-sync connector

| Check | Result | Evidence |
|---|---|---|
| Live Work IQ fetch | Pass | The connector used `/me/calendarView` for daemon-local September 3 through September 10 with `$select=id,subject,start,end,attendees,organizer,isCancelled` and `$top=100`; no body field was requested. |
| Note creation | Pass | The first run succeeded and created 23 notes under `Calendar/YYYY/MM/`. |
| Required schema | Pass | All 23 notes contained `kind`, `event_id`, `start`, `end`, `attendees`, `audience`, `customers`, `organizer`, and `tags`; each body contained only the machine-owned calendar-record comment. The normal daemon-added `created` and `updated` keys were also present. |
| Event identity | Pass | There were 23 distinct `event_id` values for 23 notes. |
| Audience classification | Pass | With `github.com` and `microsoft.com` configured as corporate domains, 21 notes classified as internal and 2 as external. |
| Customer-domain mapping | Pass | A scratch `Arctic Wolf` customer note mapped `arcticwolf.com`; one live event persisted `customers: ["[[Arctic Wolf]]"]`. |
| Idempotent rerun | Pass | The second run remained at 23 files; both the complete file set and the exact `event_id`-to-path map were unchanged, proving no duplicate creation. |
| Update in place after human calendar edit | Not exercised | The user chose not to modify a real calendar event during this run. Existing records were still updated by `event_id` on the duplicate-free rerun. |
| `after` gating | Pass | Before calendar sync, `daily-briefing` reported `waiting_on: ["calendar-sync"]`; after the successful sync, `waiting_on` was empty. |

Calendar attendee addresses and event metadata are intentionally persisted by
ADR 0025. No event body was requested or stored.

## Section 5: email-summary connector

| Check | Result | Evidence |
|---|---|---|
| Judgment-tier coexistence | Pass | With a live agent-written summary already present, `job run email-summary` exited successfully and the complete daily note remained byte-identical. |
| Fallback replacement | Pass | After setting the managed interior to `Email summary unavailable (Work IQ not connected).`, the connector replaced it with a `14 unread:` count and 14 sender/subject/time bullets. |
| Metadata-only request | Pass | Captured argv was exactly `fetch -u /me/mailFolders/inbox/messages?$filter=isRead+eq+false&$select=id,subject,from,receivedDateTime,isRead&$top=25&$orderby=receivedDateTime+desc`; it requested no `body`, `bodyPreview`, `uniqueBody`, headers, or attachments. |
| Outside-marker preservation | Pass | Bytes before the email begin marker and after the email end marker were identical across fallback replacement. |
| Idempotency | Pass | A second run with the same inbox left the complete daily note byte-identical; no duplicate bullets appeared. |
| Raw-email boundary | Pass | Re-scanned 80 scratch-vault, vault-cache, and isolated-daemon files for `You are receiving this because your review was requested.`; zero files matched. Quoted-reply markers, `Message-ID` headers, and bearer-token shapes also remained at zero. |

Only sender names, subjects, received times, and the unread count persisted in
the managed section. Mailbox-derived values are intentionally not reproduced
in this report.

## Cleanup

- The scratch vault remains at
  `/Users/surdy/vaults/verify-work-phase4-2026-09-04` for inspection.
- All scratch jobs were disabled after verification.
- The temporary no-write job and prompt were removed.
- The temporary Copilot spawn config was deleted automatically on session
  drop.
- The isolated daemon was stopped and the normal Notesmith application was
  restored.
