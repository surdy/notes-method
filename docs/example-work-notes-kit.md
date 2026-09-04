# Work Notes Kit

The blessed Notesmith configuration for customer-facing work: meetings,
customers, streams, people, and tasks. This is the schema the search
primitives (`v_field_values`, `v_task_effective_fields`, field-filtered
`list_notes`/`list_tasks`) are designed around. The design record lives in
`plans/work-notes-simplification-design.md`.

**Installing it:** everything below ships as an installable kit — you do not
need to copy these files by hand.

```bash
notesmith kit apply work-notes --path ~/vaults/work
```

Existing files are never overwritten (see [`notesmith kit apply`](cli.md#kit-apply)),
so it is safe to run against a vault you already have, and
`POST /api/app/vaults` accepts a `"kit"` field to scaffold a vault as it is
created. The kit's source lives in `kits/work-notes/` and is byte-identical to
the `golden-vault/` fixture, so the config and templates you install are the
ones the test suite exercises.

## The model in one paragraph

Three durable entities — **Customer**, **Stream** (an ongoing initiative with
lifecycle), **Person** — and one event record, the **Meeting**. Relationships
are many-to-many and live in frontmatter lists of wikilinks, never in folder
paths: a meeting lists its `customers`, `streams`, and `attendees`. Folders
exist for humans; metadata is the relationship model. Tasks are checkboxes
anywhere; a task inherits its containing note's frontmatter and only carries
inline fields for exceptions (`[due:: …]`, delegation via `[owner:: …]`, or a
per-task override of `customers`/`streams`).

## Folder structure

```text
Inbox/                      # landing spot until enriched + routed
Meetings/
  2026/
    07/
Streams/
Customers/
  Acme/
    Acme.md                 # kind: customer (folder note)
People/
Daily/
Weekly/
Quarterly/
Calendar/                   # kind: event notes, synced from the calendar
  2026/
    08/
Dashboards/
.notesmith/
  vault.toml
  fields.toml
  routing.yaml
  templates/
  skill.md
  connectors/
    calendar-sync.py        # M365 -> event notes (ADR 0025)
    calendar-sync.config.json
    email-summary.py        # unread mail -> briefing/email (ADR 0025 fallback tier)
    email-summary.config.json
```

Notes on the layout:

- Meetings are filed by date, never under a customer — a meeting can involve
  zero, one, or many customers, and `customers` metadata answers "whose
  meeting" better than any single path could.
- `Customers/<Name>/` holds durable account context. Start with just the
  folder note; split out `Architecture.md` / `Commercial.md`
  (`kind: account` + `customers: ["[[<Name>]]"]`) only when the main note
  gets unwieldy.
- People notes are created **lazily** — link attendees as `"[[Jane Smith]]"`
  from day one, and only create `People/Jane Smith.md` when someone recurs or
  has durable context. Dangling links are fine; see the promotion query below.
- Status is metadata. Done streams stay in `Streams/`; nothing moves because
  its state changed.

## Canonical fields (`.notesmith/fields.toml`)

| Field | On | Values |
|---|---|---|
| `kind` | all | `meeting` `stream` `customer` `account` `person` `event` |
| `date` | meeting | ISO date |
| `audience` | meeting | `internal` `external` (did customers attend?). **External meetings have exactly one customer**; only internal meetings may list several. |
| `meeting_type` | meeting (optional) | The meeting's **format**: `qbr` `discovery` `status` `planning` `retrospective` `1:1`. Single-valued, set during enrichment (never prompted). Themes that came up (`#escalation`) stay tags. |
| `customers` | meeting, stream, account | list of `"[[Customer]]"` wikilinks |
| `streams` | meeting | list of `"[[Stream]]"` wikilinks |
| `attendees` | meeting | list of `"[[Person]]"` wikilinks |
| `status` | stream | `active` `waiting` `blocked` `done` |
| `priority` | stream | `P0` `P1` `P2` `P3` |
| `started` / `target` | stream | ISO date |
| `org` / `role` | person | org may be a customer wikilink; `Internal` for coworkers |
| `event_id` | event, meeting | stable external calendar id (the upsert key); copied onto a meeting note by [meeting prefill](#meeting-prefill) so the two join |
| `event` | meeting (optional) | `"[[<event note>]]"` — wikilink back to the calendar record the meeting was captured from |
| `start` / `end` | event | ISO datetime (string; SQL `date()` parses it) |
| `organizer` | event | organizer email |
| `join_url` | event (online meetings) | Teams join URL — the bridge to meeting transcripts. Identifies the *online meeting*, not the occurrence: recurring instances reuse one URL. |
| `domains` | customer | list of email domains that identify the customer (feeds calendar sync) |

```toml
version = 1

[fields.kind]
type = "enum"
values = ["meeting", "stream", "customer", "account", "person", "event"]

[fields.date]
type = "date"

[fields.audience]
type = "enum"
values = ["internal", "external"]

[fields.meeting_type]
type = "enum"
values = ["qbr", "discovery", "status", "planning", "retrospective", "1:1"]

[fields.customers]
type = "list"
multivalue = true
suggest_from = "SELECT DISTINCT value FROM v_field_values WHERE key = 'customers' ORDER BY value"

[fields.streams]
type = "list"
multivalue = true
suggest_from = "SELECT DISTINCT value FROM v_field_values WHERE key = 'streams' ORDER BY value"

[fields.attendees]
type = "list"
multivalue = true
suggest_from = "SELECT DISTINCT value FROM v_field_values WHERE key = 'attendees' ORDER BY value"

[fields.status]
type = "enum"
values = ["active", "waiting", "blocked", "done"]

[fields.priority]
type = "enum"
values = ["P0", "P1", "P2", "P3"]

[fields.started]
type = "date"

[fields.target]
type = "date"

[fields.org]
type = "string"

[fields.role]
type = "string"

# Calendar event fields (ADR 0025); `domains` lives on customer notes.
[fields.event_id]
type = "string"

[fields.start]
type = "string"

[fields.end]
type = "string"

[fields.organizer]
type = "string"

[fields.domains]
type = "list"
multivalue = true
suggest_from = "SELECT DISTINCT value FROM v_field_values WHERE key = 'domains' ORDER BY value"
```

YAML rule worth repeating: **wikilinks in frontmatter must be quoted** —
`- "[[Acme]]"`, never `- [[Acme]]` (unquoted brackets are YAML syntax).

Task inline fields (`due`, `owner`, plus `customers`/`streams` overrides) are
just fields — no registry entry needed unless you want autocomplete.

## Vault config highlights (`.notesmith/vault.toml`)

```toml
[capture]
folder = "Inbox"
template = "generic-note"

[periodic.daily]
folder = "Daily"
filename = "{{ date }}"
template = "daily"

[periodic.weekly]
folder = "Weekly"
filename = "Week {{ week }}"
template = "weekly"

[periodic.quarterly]
folder = "Quarterly"
filename = "{{ quarter }}"
template = "quarterly"
```

`filename` is a **template, not a strftime pattern** — use the period token for
its kind (`{{ date }}` daily, `{{ week }}` weekly, `{{ month }}`, `{{ quarter }}`,
`{{ year }}`), which renders as `2026-07-24` / `2026-W30` / `2026-07` /
`2026-Q3` / `2026`. The indexer locates a note's period key by splitting the
filename around that token, so a pattern without it (`"%Y-%m-%d"`) silently
stops matching periodic notes for that kind.

Task statuses: the default set (`[ ]` todo, `[x]` done, `[/]` in progress,
`[b]` blocked, `[w]` waiting, …) is already right for this workflow — no
custom statuses needed.

## Routing (`.notesmith/routing.yaml`)

Filing is mechanical and kind-based. Enrichment (adding `customers`,
`streams`, `attendees`) happens in the Inbox — manually or by an agent — and
then routing moves the note. Notes without a recognized `kind` stay in Inbox
for triage. Customer and person notes are created by their templates directly
at their destination, so only meetings and streams need rules.

```yaml
version: 1
defaults:
  on_exists: rename

rules:
  - id: file-meeting
    when:
      all:
        - path: "Inbox/**"
        - field.kind: "meeting"
        - field.date: "*"
    then:
      move_to: "Meetings/{{ field.date | year }}/{{ field.date | month }}/{{ filename }}"
      remove_tags: [inbox]

  - id: file-stream
    when:
      all:
        - path: "Inbox/**"
        - field.kind: "stream"
    then:
      move_to: "Streams/{{ filename }}"
      remove_tags: [inbox]

  - id: file-person
    when:
      all:
        - path: "Inbox/**"
        - field.kind: "person"
    then:
      move_to: "People/{{ filename }}"
      remove_tags: [inbox]
```

## Templates (`.notesmith/templates/`)

### `internal-meeting.md`

```markdown
---
name: internal-meeting
description: "New internal meeting — prefilled from the calendar event in progress; leave the title blank to take the calendar's"
output_path: "Inbox/{{ meeting_date or date }} - {{ meeting_slug or title or 'Untitled' }}.md"
prompts:
  - { name: title, type: text, required: false }
context_queries:      # elided — see the template file, and "Meeting prefill" below
  calendar_events: >- ...
  calendar_event_members: >- ...
pre_render_hook: ".notesmith/scripts/meeting-prefill.sh"
---
---
kind: meeting
audience: internal
date: {{ meeting_date or date }}
customers: []
streams: []
attendees: []
{% if event_id %}event_id: "{{ event_id }}"
event: "[[{{ event_link }}]]"
{% endif %}---

# {{ meeting_date or date }} — {{ meeting_title or title or 'Untitled' }}

## Notes

## Decisions

## Tasks

- [ ]
```

Title-only on purpose: which customers an internal meeting concerns is often
only clear after the discussion, so `customers` (zero, one, or many) is
enrichment, not capture. The title itself is optional because the calendar
usually already knows it — see [Meeting prefill](#meeting-prefill).

### `external-meeting.md`

```markdown
---
name: external-meeting
description: "New customer-attended meeting — prefilled from the calendar event in progress; leave title/customer blank to take the calendar's"
output_path: "Inbox/{{ meeting_date or date }}{% if meeting_customer %} - {{ meeting_customer }}{% endif %} - {{ meeting_slug or title or 'Untitled' }}.md"
prompts:
  - { name: title, type: text, required: false }
  - { name: customer, type: field-picker, field: customers, required: false }
  - { name: stream, type: field-picker, field: streams, required: false }
context_queries:      # elided — see the template file, and "Meeting prefill" below
  calendar_events: >- ...
  calendar_event_members: >- ...
pre_render_hook: ".notesmith/scripts/meeting-prefill.sh"
---
---
kind: meeting
audience: external
date: {{ meeting_date or date }}
customers:{% for name in meeting_customers or [] %}
  - "{{ name | as_wikilink }}"{% else %} []{% endfor %}
streams:{% if stream %}
  - "{{ stream | as_wikilink }}"{% else %} []{% endif %}
attendees: []
{% if event_id %}event_id: "{{ event_id }}"
event: "[[{{ event_link }}]]"
{% endif %}---

# {{ meeting_date or date }} — {% if meeting_customer %}{{ meeting_customer }} — {% endif %}{{ meeting_title or title or 'Untitled' }}

## Attendees

## Notes

## Decisions

## Tasks

- [ ]
```

An external meeting has **exactly one customer** — that's an invariant of the
workflow, not a template limitation, so a single-valued customer prompt is the
correct interaction and multi-select prompts are unnecessary. A dashboard
query flags violations (below). Per-customer external templates (e.g. a QBR
template with a standing agenda) are just extra files in
`.notesmith/templates/` — add one only when a customer earns it.

Prompt types: `text` is a free-text input; `field-picker` offers the values
already in the vault for a registered field, searchable, while still accepting a
name the vault has not seen yet (otherwise a new customer could never be
captured). `field:` names the `fields.toml` key to suggest from — needed here
because the prompt is singular (`customer`) but the field is the plural list
(`customers`). Suggestions come from that field's `values` or `suggest_from`
query; if neither yields anything, the prompt quietly degrades to text.

### `stream.md`

```markdown
---
name: stream
description: "New stream of work"
output_path: "Inbox/{{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
  - { name: customer, type: field-picker, field: customers, required: false }
  - { name: priority, type: field-picker, field: priority, required: false }
---
---
kind: stream
status: active
{% if priority %}priority: {{ priority }}{% endif %}
customers:{% if customer %}
  - "{{ customer | as_wikilink }}"{% else %} []{% endif %}
started: {{ date }}
---

# {{ title }}

## Objective

## Current state

## Decisions

## Open questions

## Tasks

- [ ]
```

Name streams to be globally unambiguous — prefix customer-specific streams
(`Acme - Renewal 2026`), label the rest (`Internal - Support Process
Redesign`, `Cross-customer Migration Program`).

### `customer.md`

```markdown
---
name: customer
description: "New customer"
output_path: "Customers/{{ name }}/{{ name }}.md"
prompts:
  - { name: name, type: text, required: true }
---
---
kind: customer
---

# {{ name }}

## Overview

## People

## Streams

```notesmith sql
SELECT n.title, s.value AS status, n.path
FROM v_notes n
JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers' AND c.value = '[[{{ name }}]]'
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'stream'
LEFT JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
 AND s.key = 'status'
ORDER BY s.value, n.title;
```

## Recent meetings

```notesmith sql
SELECT d.value AS date, n.title, n.path
FROM v_notes n
JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers' AND c.value = '[[{{ name }}]]'
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'meeting'
LEFT JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
 AND d.key = 'date'
ORDER BY d.value DESC LIMIT 15;
```
```

### `person.md`

```markdown
---
name: person
description: "New person (create lazily, when someone recurs)"
output_path: "Inbox/{{ name }}.md"
prompts:
  - { name: name, type: text, required: true }
  - { name: org, type: text, required: false }
  - { name: role, type: text, required: false }
---
---
kind: person
{% if org %}org: "{{ org }}"{% endif %}
{% if role %}role: "{{ role }}"{% endif %}
---

# {{ name }}

## Context

## Meetings

```notesmith sql
SELECT d.value AS date, n.title, n.path
FROM v_notes n
JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
 AND a.key = 'attendees' AND a.value = '[[{{ name }}]]'
LEFT JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
 AND d.key = 'date'
ORDER BY d.value DESC;
```
```

## Task conventions

- Default owner is **you** — write `[owner:: [[Jane]]]` only when a task is
  delegated or owed by someone else.
- `[due:: 2026-07-24]` only for real deadlines.
- A task inside a meeting/stream note **inherits** that note's `customers`,
  `streams`, `date`, etc. Query through `v_task_effective_fields` or
  `list_tasks(fields=…)` — never duplicate the note's metadata onto tasks.
- A task that genuinely belongs elsewhere overrides per key:
  `- [ ] side quest [customers:: [[Other]]]`.
- Manually captured tasks (from Slack, email, hallway) go in today's daily
  note or the relevant stream note and inherit from there.

## Hooks

One script per event, configured flat in `vault.toml`; the payload arrives as
**JSON on stdin** and the script runs with the vault root as its working
directory (`.py` via python3, `.sh` via sh):

```toml
[hooks]
on_field_change = "scripts/notify-blocked.py"
watch_fields = ["status"]
on_periodic_create = "scripts/daily-briefing.py"
```

Hook config is read at daemon startup — restart the daemon after changing it.

### Blocked-stream notification (`scripts/notify-blocked.py`)

`on_field_change` fires with a batched `changes` list diffing the note's
frontmatter against its last-seen state (only keys in `watch_fields`):

```python
#!/usr/bin/env python3
import json, pathlib, subprocess, sys

payload = json.load(sys.stdin)
if any(c.get("key") == "status" and c.get("new") == "blocked"
       for c in payload.get("changes") or []):
    title = pathlib.Path(payload["path"]).stem
    subprocess.run(["osascript", "-e",
        f'display notification "Stream blocked: {title}" with title "Notesmith"'])
```

### Morning briefing (`scripts/daily-briefing.py`)

`on_periodic_create` (with `period_kind == "daily"`) appends an "Attention"
section to the fresh daily note: blocked/waiting streams, stale active
streams, tasks due soon, and the Inbox count — each a
`notesmith --format json query sql` call over the views below.

This hook is the deterministic, LLM-free variant. The kit now also ships the
full morning briefing as a `daily-briefing` agent job (issue #288, disabled by
default in `vault.toml`): the `daily-note` prompt fills the daily template's
managed `briefing/*` sections — meetings, email summary, tasks, attention —
replacing them in place on re-runs. See `docs/managed-sections.md`. Prefer the
agent job when an external agent CLI is available; keep this hook where it
isn't.

## Calendar sync

The kit ships a **connector** (ADR 0025) that turns your Microsoft 365 calendar
into vault notes. A connector is an external executable in
`.notesmith/connectors/` that the daemon's generic `[[jobs]]` runner invokes on
a schedule — not core code, and it holds no corp credentials of its own.

**What it does.** `calendar-sync.py` shells out to the official Work IQ CLI
(`workiq fetch -u "/me/calendarView?..."`), which returns Graph JSON and uses
its *own* auth cache — Notesmith never stores corp credentials. It fetches a
rolling window (start of today through +7 days) and, for each non-cancelled
event, upserts a `kind: event` note keyed by `event_id` via the REST API
(`POST /notes` to create, `PATCH /notes/{path}` to update in place). Re-runs are
idempotent.

**The event note** lands at a deterministic path so a resync is an upsert:

```yaml
# Calendar/2026/08/2026-08-04 0930 Acme Corp sync.md
# start/end are LOCAL wall clock — see "Times are local" below.
---
kind: event
event_id: AAMkAGI2-...        # stable upsert key
start: 2026-08-04T09:30:00
end: 2026-08-04T10:00:00
attendees: ["alice@acme.com", "harpreet@corp.example.com"]
audience: external            # derived: any non-corp attendee domain present
customers: ["[[Acme Corp]]"]  # derived via domains -> customer mapping; [] if none
organizer: alice@acme.com
join_url: "https://teams.microsoft.com/l/..."   # only on online meetings
tags: ["calendar"]
---
```

**The transcript bridge.** `join_url` is requested via `isOnlineMeeting,
onlineMeeting` and persisted so `transcript-sync` can reach Teams transcripts:
a calendar event exposes no transcript link, but its join URL resolves to an
online meeting, and transcripts hang off that (ADR 0025's 2026-09-04
amendment). The URL identifies the online *meeting*, not the occurrence —
recurring instances reuse one URL — so transcript sync matches transcript
timestamps to the occurrence before assigning `event_id`.

Note this is **forward-only**: the connector syncs today through +7 days with no
lookback, so events that already happened before `join_url` shipped will never
acquire one. Combined with the observed transcript retention floor, that means
no transcript catch-up for meetings predating the change — only coverage from
here on.

Events are *records of the calendar*, distinct from `kind: meeting` notes; the
meeting note stays the authoritative record. `todays_meetings` and
`unmatched_events` in the `daily-note` prompt read these notes for the briefing.

**Times are local.** Graph returns calendarView times as a zone-less
`dateTime` with a sibling `timeZone: UTC`, and mail `receivedDateTime` with a
trailing `Z`. Both connectors convert to local wall clock before writing,
because that is what the rest of the vault means by a time — `date:` fields,
the briefing's `date('now', 'localtime')` queries, and meeting-prefill's window
around `now`.

Until 2026-09-04 they *dropped* the zone instead and stored the raw components,
so a 17:00 PDT meeting was written as `2026-09-04T00:00:00`: the right instant
with the wrong clock, and rolled onto the wrong day. The email digest showed
every message at its UTC time for the same reason.

> **Repairing a vault synced before the fix.** Events upsert by `event_id` and
> are patched in place, so a re-sync corrects `start`/`end` but leaves the note
> at its old UTC-derived `YYYY-MM-DD HHMM` filename. To bring paths back in
> step, delete the `Calendar/` tree and let the connector rebuild it — safe,
> since event notes are machine-owned and the meeting note is the authoritative
> record, but it only restores events inside the sync window. Check first
> whether any meeting notes carry an `event:` backlink into that tree.

**Pagination.** `$top` caps a *page*, not the result set, so the connector
follows Graph's `@odata.nextLink` until it runs out (capped at 20 pages). The
nextLink is absolute, and `workiq fetch -u` takes an entity path, so the
service prefix is stripped before the follow-up request — an assumption about
the CLI worth confirming the first time a window actually pages.

**Backfill (`--since`).** Scheduled runs are deliberately forward-only: an
event note only has to exist *before* its transcript appears, which it always
will. That breaks down once when standing a vault up, because
`transcript-sync`'s lookback finds no past occurrences to match against. A
one-time `notesmith job run` cannot pass flags, so run the connector directly:

```sh
python3 .notesmith/connectors/calendar-sync.py --since 2026-09-01
```

`--since` only ever moves the window's start earlier; a future date is a no-op
rather than a way to skip today's events.

**Corp domains and customer mapping.** Classification lives in config, so
teaching the connector means editing config, not code:

- `.notesmith/connectors/calendar-sync.config.json` holds `corp_domains` (your
  own company's email domains) and `sync_days_ahead`. Any attendee whose domain
  is *not* in `corp_domains` makes the event `audience: external`.

  ```json
  { "corp_domains": ["corp.example.com"], "sync_days_ahead": 7 }
  ```

- Customer matching is *vault* metadata: a customer note carries a `domains`
  list (`domains: ["acme.com"]`, registered in `fields.toml`). The connector
  queries `v_field_values` for every `domains` entry, resolves each to its
  customer-note title, and sets `customers: ["[[<Customer>]]"]` on any event
  with a matching attendee domain. Unmatched external domains leave
  `customers: []` for manual triage (the briefing's Attention section surfaces
  them). Teaching the connector a new customer = adding `domains` to that
  customer note.

**Enabling it** (on the machine that has the calendar — the corp laptop):

1. `chmod +x .notesmith/connectors/calendar-sync.py` — `kit apply` writes the
   file without its executable bit (the kit manifest embeds text only).
2. Install and authenticate the Work IQ CLI (`workiq auth login`); confirm
   `workiq fetch -u "/me"` returns JSON.
3. Set your real `corp_domains` in `calendar-sync.config.json`.
4. Flip `enabled = true` on the `calendar-sync` `[[jobs]]` entry in
   `vault.toml` (hot-reloaded; no daemon restart). Develop against it with
   `notesmith job run calendar-sync`.

The `daily-briefing` job declares `after = ["calendar-sync"]`, so when both are
enabled the briefing waits for the day's events to sync before it composes
"Today's meetings". Enable the two together — a briefing whose `after`
prerequisite never runs stays held back.

You can sanity-check the connector's pure logic offline with
`python3 .notesmith/connectors/calendar-sync.py --self-test` (no network; prints
`OK`).

## Meeting prefill

Calendar sync knows what you are in right now. **Meeting prefill** is the
template side of that: create a meeting note mid-call and the title, customer,
attendee roster and `event_id` come from the calendar event instead of your
keyboard.

**How it is wired.** Both meeting templates declare two `context_queries` and a
`pre_render_hook`:

- the queries fetch today's `kind: event` notes (scalars via `v_fields`,
  `attendees`/`customers` members via `v_field_values`) — the engine runs them
  against the cache it already holds open;
- `.notesmith/scripts/meeting-prefill.sh` receives that context as JSON on
  stdin and returns the chosen event flattened into scalars on stdout.

The hook does no SQL and no network — it only picks the right row. That split
is deliberate: note creation stays offline and instant, and the queries stay
visible in the template where you can edit them.

**Which event wins.** The one overlapping *now*, where the window extends ten
minutes either side of the event (joining early, running over). An event with
no `end` matches only within ten minutes of its start. Back-to-back meetings
resolve to the nearer start, deterministically.

The SQL fetches a three-day window (yesterday through tomorrow) and the hook
makes the ±10m decision. That split matters at midnight: a call that starts at
23:55 is still running at 00:05, and a today-only query would have lost it. The
note is dated by when the meeting *started*, so such a call files under the
previous day — which is where you will look for it.

**What it fills.** With nothing typed at the prompt, an external meeting note
created five minutes into an Acme call renders as:

```yaml
# Inbox/2026-08-04 - Acme Corp - Acme Q3 sync.md
---
kind: meeting
audience: external
date: 2026-08-04
customers:
  - "[[Acme Corp]]"
streams: []
attendees: []
event_id: "AAMkAGI2-..."
event: "[[2026-08-04 0930 Acme Q3 sync]]"
---

# 2026-08-04 — Acme Corp — Acme Q3 sync

> 09:30 · organized by alice@acme.com · from [[2026-08-04 0930 Acme Q3 sync]]

## Attendees

<!-- From the calendar. Replace with "[[Person]]" wikilinks in the attendees field during enrichment. -->
- alice@acme.com
- harpreet@corp.example.com
```

Note what does *not* happen: the calendar's raw addresses stay in the body,
because on a meeting note `attendees` means `"[[Person]]"` wikilinks. Turning
the roster into links is enrichment, same as always — the calendar just saves
you from typing the list.

`event_id` and `event` are the join back to the machine-owned calendar record,
in both directions.

**Typed values always win.** The hook fills blanks; it never overrules the
prompt. Type a title and you get your title, with the event identity still
attached. This is why both prompts are now `required: false` — leaving one
blank is how you say "use the calendar's". There is no *displayed* default to
edit: prompts are collected before the hook runs, so a prefilled-and-editable
prompt field would need a core change to the template engine.

**When nothing matches** — a free slot, calendar-sync not enabled, python3
missing, the hook erroring or timing out — every path degrades to the same
place: a clean note from whatever you typed, no calendar residue, no `event_id`.
A meeting note must always be creatable.

Sanity-check the hook's logic offline with
`python3 .notesmith/scripts/meeting-prefill.py --self-test` (no network, no
cache; prints `OK`).

## Teams transcripts

`transcript-sync.py` pulls Teams meeting transcripts for recently-ended calls
and writes them as **sidecar** notes — never inlined into the meeting note,
which stays the distilled record (transcripts are long and noisy, and inlining
them skews search and embedding chunks).

**The join is the hard part.** A calendar event exposes no transcript link:

```text
event note (join_url)  ->  online meeting  ->  transcripts
```

Recurring occurrences reuse **one** join URL, so that lookup lands on the
*series*, not the occurrence. Each transcript's `createdDateTime` is matched
back to a specific occurrence's time window before that occurrence's `event_id`
is written into the note. A real recurring series resolved with a 13-day margin
over the runner-up (`spikes/transcript-occurrence-matching/FINDINGS.md`).

**It declines rather than guesses.** A transcript outside the four-hour
tolerance, or one where two occurrences sit within an hour of each other, is
left unfiled and logged. An unfiled transcript is visible and recoverable; one
attached to the wrong customer's call is a quiet error nobody catches.

**Occurrences come from the local cache, not from Graph.** `calendar-sync` has
already synced them with local timestamps, so this connector never touches
calendarView or its pagination. It does convert the transcript's `Z` stamps to
local before comparing — the same conversion whose absence made every synced
event seven hours wrong.

**The body is rendered by core.** The connector pipes WebVTT into
`notesmith transcribe --from-vtt -` (see [cli.md](cli.md#transcribe)) and takes
back the rendered body, so the `[M:SS] Name: text` format lives in exactly one
place and a subprocess connector cannot drift from it. Piping on stdin also
means transcript text never touches disk.

The note (plan §E):

```yaml
# Meetings/Transcripts/2026-09-09 - Acme sync (transcript).md
---
kind: transcript
source_type: teams
source_url: teams:AAMk...        # the dedup key; a re-run is a no-op
event_id: AAMkAGI2-...           # the matched occurrence
event: "[[2026-09-09 0900 Acme sync]]"
meeting: "[[2026-09-09 - Acme - Sync]]"   # when a meeting note exists
date: 2026-09-09
customers: ["[[Acme Corp]]"]
tags: ["transcript"]
---

[0:03] Alice Smith: Morning, shall we start?
[0:07] Bob Jones: Yes. The renewal is the main thing.
```

The back-link onto the meeting note is a **frontmatter PATCH** adding
`transcript: "[[...]]"`. Meeting notes are human-owned and ship no managed
section, so the body is never touched; frontmatter wikilinks are indexed as
real links, which is how this vault models every other relationship.

Linking happens at creation *and* is reconciled on every run. The create-time
link only fires when the meeting note already exists as the transcript lands —
true for the common flow, where prefill writes the note during the call and the
transcript appears an hour later. It is false for the late one: write the
meeting up the next day and its transcript was ingested (and is now skipped)
long before. Each run therefore re-pairs any transcript and meeting sharing an
`event_id` that are linked on neither side, which is idempotent because a
completed pair produces no patch.

**Enabling it** (on the corp laptop, after `calendar-sync` is working):

1. `chmod +x .notesmith/connectors/transcript-sync.py`.
2. Flip `enabled = true` on the `transcript-sync` `[[jobs]]` entry. It declares
   `after = ["calendar-sync"]` — an event with no synced `join_url` has no
   bridge to its transcript, so enabling it alone does nothing.
3. `lookback_days` in `transcript-sync.config.json` (default 3) bounds how far
   back it looks. The observed tenant retention floor was ~17 days and there is
   no historical backfill, so a larger value costs requests without finding
   more.

Offline check: `python3 .notesmith/connectors/transcript-sync.py --self-test`.

## Deterministic email summary

The daily briefing's email section has **two tiers** (ADR 0025 Decision 3 and
its 2026-09-04 amendment). The **judgment tier** is the briefing agent itself:
when a Work IQ email tool is attached to its session, the agent reads today's
inbox *live*, decides what matters, and writes a short human-facing summary into
`briefing/email` — sender and subject only, one clause of gist per item. That is
always preferred. The **fallback tier** is `email-summary.py`, a deterministic,
LLM-free connector for machines whose briefing agent has *no* email tool at all.

**What it does.** `email-summary.py` shells out to the Work IQ CLI
(`workiq fetch -u "/me/mailFolders/inbox/messages?..."`), which returns Graph
JSON from its *own* auth cache — Notesmith never stores corp credentials. It
renders one bullet per unread message, most recent first, into `briefing/email`:

```markdown
3 unread:
- 15:04 **Alice Adams** — Contract renewal, sign by Friday
- 09:12 **teammate@corp.example.com** — Re: standup notes
- 06:00 **News Bot** — Weekly digest
```

The empty case renders `Nothing unread.` Sender is the display name (or the
address when there is none); the subject is trimmed to one line. The cap comes
from config.

**The hard boundary — only sender and subject persist.** The Graph query's
`$select` is fixed at `id,subject,from,receivedDateTime,isRead`. Message
`body`, `bodyPreview`, `uniqueBody`, headers, and attachments are **never
requested and never stored** — exactly the boundary Decision 4 draws for email.
Config can widen the window or the message cap, but not the fields; the boundary
is not user-tunable. The connector's `--self-test` proves it: it renders a
fixture whose messages carry body content and asserts none of that content
appears in the output.

**Coexistence — the agent's summary is never overwritten.** `briefing/email`
may be written by the agent *or* this connector, so the connector runs last (its
job declares `after = ["daily-briefing"]`) and fills the section **only when the
agent left it unavailable**: it reads the current section interior and writes
only if it is empty/whitespace or still carries the agent's
`Email summary unavailable (Work IQ not connected).` fallback (matched loosely).
Any real summary is left byte-for-byte untouched, and the connector exits 0
(a no-op success). The write goes through
`POST /notes-section/{path}` with `append_if_missing: true`, so re-runs converge
and the human content around the markers is never disturbed
(see `docs/managed-sections.md`).

**Config.** `.notesmith/connectors/email-summary.config.json` is user-edited:

```json
{ "unread_only": true, "max_messages": 25 }
```

`unread_only` (default true) filters to `isRead eq false`; `max_messages` caps
both the Graph `$top` and the rendered bullet count.

**Enabling it** (on a machine whose briefing agent has no Work IQ tool):

1. `chmod +x .notesmith/connectors/email-summary.py` — `kit apply` writes the
   file without its executable bit (the kit manifest embeds text only).
2. Install and authenticate the Work IQ CLI (`workiq auth login`); confirm
   `workiq fetch -u "/me"` returns JSON.
3. Flip `enabled = true` on the `email-summary` `[[jobs]]` entry in `vault.toml`
   (hot-reloaded; no daemon restart). Develop against it with
   `notesmith job run email-summary`.

The job carries `at = "07:35"` — just after the 07:30 briefing — because the
runner requires every job to declare a schedule; the real ordering is the
`after = ["daily-briefing"]` gate, which holds the connector until the agent has
had its turn today. **If you run the email connector *without* the briefing
agent** (no `daily-briefing` job enabled), remove the `after` line — otherwise
the connector waits forever on a prerequisite that never succeeds, and its
`at` fire is recorded as `missed` each day.

Sanity-check the pure logic offline with
`python3 .notesmith/connectors/email-summary.py --self-test` (no network; prints
`OK`).

## Query recipes

All meetings involving Acme (however many customers attended):

```sql
SELECT n.path, n.title
FROM v_notes n
JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
WHERE c.key = 'customers' AND c.value = '[[Acme]]';
```

Open tasks you owe Acme, soonest due first (inherited or task-level):

```sql
SELECT t.text, t.note_path, due.value AS due
FROM v_tasks t
JOIN v_task_effective_fields c
  ON c.vault_name = t.vault_name AND c.task_id = t.id
 AND c.key = 'customers' AND c.value = '[[Acme]]'
LEFT JOIN v_task_effective_fields due
  ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
WHERE t.status_group = 'open'
ORDER BY due.value IS NULL, due.value;
```

Stale active streams — no meeting referencing them in 30 days:

```sql
SELECT n.title, n.path
FROM v_notes n
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'stream'
JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
 AND s.key = 'status' AND s.value = 'active'
WHERE NOT EXISTS (
  SELECT 1
  FROM v_field_values ms
  JOIN v_field_values md ON md.vault_name = ms.vault_name AND md.note_path = ms.note_path
   AND md.key = 'date' AND md.value >= date('now', '-30 days')
  WHERE ms.vault_name = n.vault_name
    AND ms.key = 'streams' AND ms.value = '[[' || n.title || ']]'
);
```

Attendees who deserve a People note (referenced often, note doesn't exist):

```sql
SELECT fv.value AS person, COUNT(*) AS mentions
FROM v_field_values fv
WHERE fv.key = 'attendees'
  AND NOT EXISTS (
    SELECT 1 FROM v_notes p
    WHERE p.vault_name = fv.vault_name
      AND p.title = replace(replace(fv.value, '[[', ''), ']]', '')
  )
GROUP BY fv.value
ORDER BY mentions DESC;
```

External meetings breaking the one-customer invariant:

```sql
SELECT n.path, n.title, COUNT(c.value) AS customer_count
FROM v_notes n
JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
 AND a.key = 'audience' AND a.value = 'external'
LEFT JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers'
GROUP BY n.vault_name, n.path, n.title
HAVING customer_count != 1;
```

Multi-customer work (internal meetings and cross-customer streams):

```sql
SELECT note_path, COUNT(*) AS customer_count
FROM v_field_values
WHERE key = 'customers'
GROUP BY note_path
HAVING customer_count > 1;
```

## Dashboards

`Dashboards/Home.md` — active streams by priority, blocked/waiting streams,
open tasks by due date, Inbox triage list, meetings missing `customers` or
`audience`. All of these are `notesmith sql` blocks over the recipes above;
the stale-streams and promotion queries make good weekly-review sections.

## Agent guidance (`.notesmith/skill.md`)

```markdown
# Work Notes vault

Entity model: meetings (dated event records, `Meetings/YYYY/MM/`), streams
(ongoing initiatives, `Streams/`), customers (`Customers/<Name>/`), people
(`People/`). `kind` is the canonical type field. Tags are topical only.

Relationships are frontmatter lists of quoted wikilinks: `customers`,
`streams`, `attendees`. Folders are for humans — never infer relationships
from paths.

## Retrieval

- Membership queries: `v_field_values` (one row per list member; exact value
  match, e.g. key='customers' AND value='[[Acme]]').
- Task queries: `v_task_effective_fields` — tasks inherit their note's
  frontmatter; task-level inline fields override per key.
- `list_notes` / `list_tasks` take a `fields` map with the same semantics.
- Free-text digging: `vault_search` (hybrid). Time-based: `time_query`.
- Cite notes by path; quote the exact line when reporting decisions.

## Writing

- Meeting/stream/person notes: use `create_from_template`, then enrich
  frontmatter. New notes land in `Inbox/`; routing files them by `kind`.
- Quote wikilinks in YAML: `- "[[Acme]]"`.
- Tasks: plain checkboxes; only add `[due:: ]`/`[owner:: ]` for real
  deadlines/delegation. Don't copy note metadata onto tasks.
- Do not create People notes for one-off attendees; link them and move on.
```
