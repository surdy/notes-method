# Integrations & Control-Center Plan

> Status: **decisions settled 2026-08-04** (interactive review with Harpreet).
> Turns Notesmith into the daily work dashboard by integrating external work
> systems (calendar, email, meeting transcripts via Microsoft Work IQ) while
> keeping every integration out of core — config-declared, subprocess-based,
> and removable. Builds on the entity model settled in
> `plans/work-notes-simplification-design.md`. The load-bearing decisions and
> their trade-offs are recorded in `docs/adr/0025-work-system-integrations.md`;
> new vocabulary is in `CONTEXT.md` (Integrations & Jobs). Only the Phase-0
> spike items at the end remain open; they are facts to verify, not decisions.

## Settled decisions

| # | Decision | Choice |
|---|---|---|
| 1 | Placement | Everything work-related (vault, daemon, connectors, jobs) on the work laptop. Homelab keeps personal vaults only. Work vault never syncs to homelab. |
| 2 | Access path | Microsoft **Work IQ**, consumed via its **remote MCP server** (the sanctioned, governed surface). |
| 3 | Consumption split | Mechanical syncs = deterministic MCP-client scripts (no LLM). Judgment tasks = headless agent jobs with Work IQ MCP in `[mcp.servers]`. No email staging files — the agent reads mail live; only its summary persists. |
| 4 | Calendar model | Per-event notes (`kind: event`, upsert by `event_id`, `Calendar/YYYY/MM/`). Meeting notes stay authoritative, linked by `event_id`. |
| 5 | Scheduling | Build a generic `[[jobs]]` runner in the daemon; Phase 1 prototypes connectors on plain cron first. |
| 6 | Daily note | Agent-owned, refreshable **marked sections** inside the daily note; human sections untouched. Flow defined entirely in vault config (prompt + jobs entry); core gains only a generic section-marker convention. |
| 7 | Transcripts | Scheduled Work IQ connector matched by `event_id`; stored as sidecar `kind: transcript` notes linked both ways. Drop-folder ingest as fallback. |
| 8 | Customer matching | `domains` list field on customer notes; connector maps attendee domains → customer wikilinks, derives `audience`; unmatched external domains left for triage. |
| 9 | Backup | Existing `[git]` timers → corp-approved remote (policy check in spike); fallback local-only git + corp device backup. |

## Goals

1. **Calendar-aware note creation** — creating a meeting note during a calendar
   event pre-fills kind/audience/customers/attendees from the event.
2. **Transcript pull** — Teams meeting transcripts land as sidecar notes linked
   to the corresponding meeting note.
3. **Morning daily note** — generated on schedule, containing: summary of
   important emails since last workday, today's calendar, tasks due/planned
   today, attention items.
4. **Pluggable by config** — connectors are external programs declared in
   config, never compiled into Notesmith. Core gains only *generic* capability
   (a job scheduler, a connector contract, a section-marker convention).

Non-goal: building corp-specific clients into the OSS codebase. Work IQ
endpoints, credentials, and quirks stay in private connector scripts and vault
config.

## Work IQ (the access path)

[Work IQ](https://learn.microsoft.com/en-us/microsoft-365/copilot/extensibility/work-iq/)
is Microsoft's workplace-intelligence layer over M365: mail, calendar, files,
people, Teams chat, and sites, exposed through **A2A, a remote MCP server, and
a REST API**. The MCP server collapses M365 into ~10 generic verb-style tools
with runtime-discoverable resource paths. Requests are user-scoped
(permission-aware), centrally governed (Rego policy engine, audit logs, rate
limits in the M365 admin center), and **usage-billed** independent of Copilot
licensing.

Two consumption modes, chosen per task (decision 3):

- **Deterministic MCP client** — a connector script speaks MCP as a plain
  client (no LLM) to the remote server. Used for mechanical, repeatable syncs:
  calendar events, transcript fetch. Cheap, predictable, bounded billing.
- **Headless agent** — Work IQ MCP registered in Notesmith's existing
  `[mcp.servers]` alongside the vault's own MCP; the `notesmith ai` /
  agent-job path uses both. Used where judgment is required: email importance,
  summarization, briefing composition. This also serves interactive chat: ask
  the vault agent about your mail/calendar ad hoc.

Because the agent reads mail live over MCP, **no raw email content is ever
staged or stored** — only the human-facing summary written into the daily note
persists. (This supersedes the earlier staging-file design.)

## Architectural principles

### 1. Connectors are external subprocesses, not core code

Notesmith's extension philosophy is consistent: subprocess hooks, external MCP
servers, file-drop customization, drop-folder ingest — deliberately no plugin
host. Integrations follow the same pattern: a **connector** is an executable in
`.notesmith/connectors/` invoked by the job runner, talking to Work IQ with its
own credentials and writing results through the REST API / CLI / designated
folders. Core provides the scheduler, the write API, and the data model.

### 2. External data lands in the vault as notes

Calendar events and transcripts become notes. Once they are notes with
frontmatter, everything existing works — `v_fields`/`v_field_values` queries,
template `context_queries`, routing, `vault_search` filters, agent access via
MCP. No parallel database. Exception: raw email (see above — never stored).

### 3. Anything requiring judgment goes through the agent path

Fetching events is mechanical → script. Deciding which emails matter is
judgment → agent job. Don't put an LLM inside a connector; don't make an agent
do a 15-minute poll.

### 4. Flows are vault config, not core features

The daily-briefing flow is: a `[[jobs]]` entry + a prompt file + a template +
routing rules — all vault-level, kit-installable, user-editable. Core ships
mechanisms (job runner, section markers), never a hardwired "daily briefing
feature".

## Placement (decision 1)

All work-related pieces run on the work laptop: daemon + work vault +
connectors + jobs, local via launchd (`auto_start` exists; add a launchd
agent). Rationale:

- Work IQ auth is user-scoped corp SSO — only sanely available on the managed
  device.
- Corp/customer data on personal homelab infra is a compliance exposure.
- The daemon has no auth (`deploy/README.md`), so cross-machine writes would
  need an auth story first.
- Offline-safe (plane, VPN drop); catch-up-on-wake covers sleep-through
  schedules, since every output is only consumed at the laptop anyway.

Homelab is unchanged (personal/memory vaults). Hybrid remains a *future*
option for non-sensitive personal integrations only — the job runner is
placement-agnostic — but requires daemon auth first, and corp data never takes
that path.

## What exists today (reuse, don't rebuild)

| Capability | Where | Role in this plan |
|---|---|---|
| Daily/periodic scheduler with catch-up | `crates/notesmith-http/src/scheduler.rs` | Pattern + shared `ensure_periodic_note` path |
| Worker + supervisor pattern (interval loop, re-reads `enabled` each tick, respawn) | `ingest_scheduler.rs`, `embed_scheduler.rs`, `transcribe_scheduler.rs` | Generalize into the job runner |
| Event bus (17 events) + hook listener | `crates/notesmith-http/src/events.rs`, `hooks.rs` | Emit `job.*` events; hooks can chain off them |
| Subprocess hooks (6 events, JSON stdin) | `crates/notesmith-hooks` | On-demand automation (e.g. nudge on unmatched transcript) |
| Template `context_queries` (SQL → context) + `pre_render_hook` (subprocess JSON → context) | `crates/notesmith-templates/src/lib.rs` | Calendar-aware meeting creation |
| `daily/agent-create` + `.notesmith/prompts/daily-note.md` | `routes/daily.rs:129` | The briefing agent job — extend the prompt's context |
| Headless agent CLI ("for scripting and cron") | `notesmith ai`, `crates/notesmith-cli/src/commands/ai.rs` | Agent-kind jobs |
| External MCP servers config | `[mcp.servers]` in global config | Attach Work IQ MCP to agents/chat |
| Routing DSL | `crates/notesmith-routing` | File synced events/transcripts into place |
| Drop-folder ingest worker | `crates/notesmith-ingest` | Transcript fallback path |
| SQLite job queue precedent | `crates/notesmith-transcribe/src/queue.rs` | Model for job-run history if needed |
| Field registry, SQL views (`v_field_values`, `v_task_effective_fields`) | `.notesmith/fields.toml`, `notesmith-index` | Event/customer/task queries, domain mapping |

## What to build

### A. Generic job scheduler (`notesmith-http`, new `jobs` module) — the main core build

One generic runner driven by per-vault config, replacing the
bespoke-module-per-feature pattern for new work:

```toml
# .notesmith/vault.toml
[[jobs]]
name = "calendar-sync"
enabled = true
every = "15m"                     # interval jobs
command = ".notesmith/connectors/calendar-sync.py"
timeout = "120s"

[[jobs]]
name = "transcript-sync"
enabled = true
every = "30m"
command = ".notesmith/connectors/transcript-sync.py"

[[jobs]]
name = "daily-briefing"
enabled = true
at = "07:30"                      # time-of-day jobs, with catch-up
weekdays_only = true
after = ["calendar-sync"]         # same-day ordering, not DAGs
agent = { prompt = "daily-note", allow_writes = true }
```

Semantics (all reusing existing patterns):

- Two job kinds: `command` (subprocess, cwd = vault root, hook-style env plus
  `NOTESMITH_API_BASE`, `NOTESMITH_VAULT`, `NOTESMITH_STATE_DIR`) and `agent`
  (drives the `notesmith ai` / agent-create path with a named prompt from
  `.notesmith/prompts/`).
- Supervisor task per the ingest/embed pattern; `enabled` re-read each tick
  (hot toggling without restart, unlike today's `[hooks]`).
- `at` jobs get catch-up: if the daemon slept past the fire time, run once on
  wake (persist last-run timestamp in the XDG state dir).
- `after` = skip until the named jobs have succeeded today. Resist DAGs.
- Emit `job.started` / `job.succeeded` / `job.failed` on the event bus (SSE
  already exposes it; hooks can react; UI can show status later).
- Manual trigger: `notesmith job run <name>` +
  `POST /api/v/{vault}/jobs/{name}/run` — essential for connector development.
- `every`/`at` only; no cron expressions until actually needed.

### B. Generic section-marker convention (small core/kit piece)

A vault-wide convention for machine-managed regions inside human-owned notes:

```markdown
<!-- notesmith:section:begin briefing/meetings -->
…agent-written content…
<!-- notesmith:section:end briefing/meetings -->
```

Re-runs replace marked sections in place; content outside markers is never
touched. Minimum viable: the convention lives in the prompt/skill guidance and
agents do read-replace-write via existing `update_note`. Optional core helper
later: a `replace_section` op/endpoint for atomic replacement. Nothing
daily-note-specific in core.

### C. Connector contract (docs + kit, not code)

`docs/connectors.md` defining: invocation env; cursor/state location
(`NOTESMITH_STATE_DIR`, never the vault); writing via REST (upsert semantics)
or file drop; idempotency (stable external IDs in frontmatter, upsert on
resync); MCP-client usage against Work IQ (auth/token handling per the spike);
secrets guidance (env files `chmod 600` or macOS keychain via `security`
inside the connector — Notesmith never stores credentials). Ship a reference
connector skeleton in the work-notes kit.

### D. Calendar event data model (kit/config, not code)

Per-event notes with deterministic paths so resync is an upsert:

```yaml
# Calendar/2026/08/2026-08-04 0930 Acme sync.md
---
kind: event
event_id: "AAMkAGI2..."        # stable external id — the upsert key
start: 2026-08-04T09:30:00
end: 2026-08-04T10:00:00
attendees: ["alice@acme.com", "bob@corp.com"]
audience: external              # derived: any non-corp attendee domain
customers: ["[[Acme]]"]         # derived via domain mapping (below)
organizer: "alice@acme.com"
tags: [calendar]
---
```

**Customer/domain mapping (decision 8):** customer notes gain a `domains` list
field (`domains: ["acme.com"]`, registered in `fields.toml`). The connector
resolves external attendee domains → customer wikilinks via a
`v_field_values` query (`key='domains'`), sets `audience: external` when any
non-corp attendee is present, and leaves `customers: []` on unmatched external
domains for manual triage (surfaced in the daily note's Attention section).
The mapping is vault metadata — teaching the connector = editing a customer
note.

Events are *records of the calendar*, distinct from meeting notes
(`kind: meeting`). A meeting note links to its event by `event_id` /
wikilink; the meeting note remains the authoritative record.

### E. Transcript data model (kit/config, not code)

Sidecar notes, never inlined into the meeting note (transcripts are long and
noisy; inlining bloats the authoritative record and skews search/embedding
chunks):

```yaml
# Meetings/Transcripts/2026-08-04 - Acme sync (transcript).md
---
kind: transcript
event_id: "AAMkAGI2..."
customers: ["[[Acme]]"]
meeting: "[[2026-08-04 - Acme - Sync]]"   # back-link when matched
date: 2026-08-04
---
```

The connector links both ways (adds a transcript link/section pointer on the
meeting note when one exists, matched by `event_id`). Raw transcript text stays
searchable; the meeting note stays the distilled record.

**Unified Transcript Note concept (settled 2026-08-04):** `kind: transcript`
is one concept regardless of origin — Teams, YouTube captions, whisper-
transcribed audio — sharing the existing `notesmith-transcribe` body renderer,
with `source_type` distinguishing the source. Meeting transcripts are the
Teams-sourced case, adding `event_id`/`meeting` links.

### F. Work-notes kit additions

Templates, prompts, routing rules, `fields.toml` entries, connector skeletons,
and `[[jobs]]` examples for all of the above, installable via
`notesmith kit apply` — the pluggability story for the data-model half,
mirroring connectors as the pluggability story for the code half.

## What to modify in existing functionality

1. **Unify daily-note creation.** `LocalOps::create_daily_note`
   (`crates/notesmith-ops/src/lib.rs:2331`) ignores `[periodic.daily] filename`
   and disagrees with `ensure_periodic_note` about the target path. Route it
   through the same logic before stacking automation on daily notes.
2. **Extend the daily-note prompt context.** Add context queries for: today's
   events (`kind=event`, `start` today, via `v_field_values`), tasks with
   `due <= today` from `v_task_effective_fields`, unmatched-customer events.
   Email content comes live via Work IQ MCP in the agent job, not via context
   queries.
3. **Timezone in the scheduler.** `compute_delay_until` (`scheduler.rs:291`)
   accepts and ignores `timezone`. Fix while building the job runner since
   both share the time-of-day math.
4. **`[mcp.servers]` auth for remote HTTP servers.** Work IQ's remote MCP
   server will require Entra-backed auth. Verify whether the HTTP entry
   supports auth headers/OAuth; if not, add it (small core change, spike
   confirms the exact mechanism).
5. **Hot-reload jobs config** from day one (ingest pattern); consider
   retrofitting `[hooks]`.
6. **(Only if hybrid ever happens) daemon auth.** Bearer-token middleware
   before any cross-machine write path. Not needed for the laptop-local plan.

Explicitly *not* proposed: implementing the ADR-0019 `Source`/`Fetcher` trait
for these integrations — that trait is for compiled-in sources; corp
connectors can't be compiled in, so the subprocess contract is the right
boundary here.

## Feature designs

### 1. Calendar-aware meeting creation

- `calendar-sync` job (every 15m during work hours) upserts event notes (§D).
- Meeting templates gain a `pre_render_hook` (existing mechanism) — a small
  script picking the event overlapping now (±10m) and returning
  `{title, customers, attendees, event_id, ...}` as JSON context. Blank
  fallback when no event matches. No network at note-creation time.
  **Built** — `.notesmith/scripts/meeting-prefill.{sh,py}`, kit-only, no core
  changes. Two deviations from this sketch, both forced by the engine:
  - The hook does *no* SQL. `pre_render_hook` gets no db handle or path (the
    cache lives outside the vault root), so the templates' `context_queries`
    fetch the candidate rows and the hook only picks among them.
  - Prompts are collected and validated *before* the hook runs, so a
    prefilled-but-editable prompt field is not achievable in vault config.
    What ships instead: `title`/`customer` became `required: false`, and
    leaving one blank takes the calendar's value. True confirm/override needs
    a core change — either run the hook before prompt collection, or let
    `prompts[].default` reference context.
- Optional polish: a `url-actions.yaml` action / sidebar button "note for
  current meeting".

### 2. Transcripts

- `transcript-sync` job (deterministic MCP client) fetches transcripts for
  recently-ended meetings from Work IQ, writes sidecar transcript notes (§E),
  matches to meeting notes by `event_id`, links both ways. An
  `on_note_create` hook can nudge when a transcript arrived with no matching
  meeting note.
- Fallback for sources Work IQ can't reach (eoriq exports?): drop files into
  the existing ingest folder; routing files them.

### 3. Morning daily note (decision 6)

One 07:30 weekday agent job (`after = ["calendar-sync"]`):

1. Ensure the daily note exists via `ensure_periodic_note` (minimal template,
   including empty marked sections).
2. Run the `daily-note` prompt (vault-configurable) with vault MCP + Work IQ
   MCP attached. The agent fills the marked sections: **Today's meetings**
   (local event notes), **Email summary** (read live via Work IQ; only the
   summary persists), **Tasks due/planned** (`v_task_effective_fields`),
   **Attention** (blocked/stale streams, unmatched-customer events).
3. Human sections (Focus/Notes) are never touched. Manual re-run
   (`notesmith job run daily-briefing`) refreshes marked sections in place.

The entire flow is vault config: jobs entry + prompt + template. Editing the
prompt file changes the briefing; core is uninvolved.

### 4. Tasks due/planned today

Pure query work — `v_task_effective_fields` already inherits `due`,
`customers`, `streams` onto tasks. Context query in the daily prompt;
optionally a Dashboards note with a `notesmith sql` fence.

## Backup (decision 9)

Enable `[git]` on the work vault using the existing auto-commit/auto-push
timers, pushing to a **corp-approved private remote** (corp GitHub / Azure
DevOps — confirm in spike). History matters more once agents write to notes
(undoing a bad agent run). If no approved remote exists: local-only git for
history + corp device backup for off-device safety. Never the homelab.

## Phasing

| Phase | Scope | Core code? |
|---|---|---|
| **0. Spike** | Verify the open facts below. Blocks everything; do first. | none |
| **1. Local daemon + calendar connector, cron-scheduled** | Work vault + daemon on laptop (launchd). `calendar-sync.py` as a deterministic Work IQ MCP client writing event notes via REST; scheduled with cron/launchd directly. Add `domains` field + event template/routing. Proves auth, billing behavior, and the data model with zero core changes. | none |
| **2. Job runner** | `[[jobs]]` (command + agent kinds, `every`/`at`, catch-up, `after`, `job.*` events, manual run). Port calendar-sync into it. Fix daily-note path divergence + timezone. `[mcp.servers]` auth if the spike says it's missing. | yes — the main build |
| **3. Calendar-aware meeting template** | `pre_render_hook` script + template updates + optional url-action. **Built** (url-action not done). | none (kit only) — confirmed, but see the deviations under feature 1: editable prompt defaults would need core |
| **4. Daily briefing** | `daily-note` prompt with marked sections, `daily-briefing` agent job with Work IQ MCP attached, section-marker convention. | small (prompt-context plumbing, optional `replace_section` helper) |
| **5. Transcripts** | `transcript-sync` connector, sidecar notes, two-way linking; drop-folder fallback documented. **Blocked** on `plans/transcript-sync-spike.md`. | none, then small |

Each phase is independently useful; stop anywhere.

## Phase-0 spike items (facts to verify, not decisions)

- **Work IQ MCP auth for a headless local client**: Entra flow (device code?),
  token refresh, whether corp tenant admin has enabled Work IQ and what the
  usage billing looks like at our polling cadence.
- **Transcript availability**: are Teams transcripts exposed through Work IQ
  with a joinable calendar event id? **Still unanswered** — written up as
  runnable questions in `plans/transcript-sync-spike.md`, which also finds that
  no join key is persisted today and that the shared transcript renderer has no
  speaker field. Phase 5 is blocked on it.
- **eoriq**: separate system or the same Teams/Work IQ data? Determines
  whether the drop-folder fallback is actually needed. Also still unanswered;
  question E of the transcript spike.
- **`[mcp.servers]` HTTP auth support**: can Notesmith's remote-MCP config
  carry the required auth headers/OAuth today, or is that a core addition?
- **Corp policy**: local markdown vault of meeting/email summaries on the
  managed laptop; git push to which remote; any data-class restrictions on
  what the vault may contain.
