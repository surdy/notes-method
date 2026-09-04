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
| `event_id` | event | stable external calendar id (the upsert key) |
| `start` / `end` | event | ISO datetime (string; SQL `date()` parses it) |
| `organizer` | event | organizer email |
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
description: "New internal meeting — fastest capture; customers/streams added during enrichment"
output_path: "Inbox/{{ date }} - {{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
---
---
kind: meeting
audience: internal
date: {{ date }}
customers: []
streams: []
attendees: []
---

# {{ date }} — {{ title }}

## Notes

## Decisions

## Tasks

- [ ]
```

Title-only on purpose: which customers an internal meeting concerns is often
only clear after the discussion, so `customers` (zero, one, or many) is
enrichment, not capture.

### `external-meeting.md`

```markdown
---
name: external-meeting
description: "New customer-attended meeting — always exactly one customer"
output_path: "Inbox/{{ date }} - {{ customer }} - {{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
  - { name: customer, type: field-picker, field: customers, required: true }
  - { name: stream, type: field-picker, field: streams, required: false }
---
---
kind: meeting
audience: external
date: {{ date }}
customers:
  - "{{ customer | as_wikilink }}"
streams:{% if stream %}
  - "{{ stream | as_wikilink }}"{% else %} []{% endif %}
attendees: []
---

# {{ date }} — {{ customer }} — {{ title }}

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
---
kind: event
event_id: AAMkAGI2-...        # stable upsert key
start: 2026-08-04T09:30:00
end: 2026-08-04T10:00:00
attendees: ["alice@acme.com", "harpreet@corp.example.com"]
audience: external            # derived: any non-corp attendee domain present
customers: ["[[Acme Corp]]"]  # derived via domains -> customer mapping; [] if none
organizer: alice@acme.com
tags: ["calendar"]
---
```

Events are *records of the calendar*, distinct from `kind: meeting` notes; the
meeting note stays the authoritative record. `todays_meetings` and
`unmatched_events` in the `daily-note` prompt read these notes for the briefing.

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
