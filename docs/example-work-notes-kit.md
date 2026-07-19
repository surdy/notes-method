# Work Notes Kit

The blessed Notesmith configuration for customer-facing work: meetings,
customers, streams, people, and tasks. This is the schema the search
primitives (`v_field_values`, `v_task_effective_fields`, field-filtered
`list_notes`/`list_tasks`) are designed around. The design record lives in
`plans/work-notes-simplification-design.md`.

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
Dashboards/
.notesmith/
  vault.toml
  fields.toml
  routing.yaml
  templates/
  skill.md
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
| `kind` | all | `meeting` `stream` `customer` `account` `person` |
| `date` | meeting | ISO date |
| `audience` | meeting | `internal` `external` (did customers attend?). **External meetings have exactly one customer**; only internal meetings may list several. |
| `customers` | meeting, stream, account | list of `"[[Customer]]"` wikilinks |
| `streams` | meeting | list of `"[[Stream]]"` wikilinks |
| `attendees` | meeting | list of `"[[Person]]"` wikilinks |
| `status` | stream | `active` `waiting` `blocked` `done` |
| `priority` | stream | `P0` `P1` `P2` `P3` |
| `started` / `target` | stream | ISO date |
| `org` / `role` | person | org may be a customer wikilink; `Internal` for coworkers |

```toml
version = 1

[fields.kind]
type = "enum"
values = ["meeting", "stream", "customer", "account", "person"]

[fields.date]
type = "date"

[fields.audience]
type = "enum"
values = ["internal", "external"]

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
filename = "%Y-%m-%d"
template = "daily"

[periodic.weekly]
folder = "Weekly"
filename = "%Y-W%W"
template = "weekly"

[periodic.quarterly]
folder = "Quarterly"
filename = "%Y-Q%q"
template = "quarterly"
```

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
  - { name: customer, type: field-picker, required: true }
  - { name: stream, type: field-picker, required: false }
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
workflow, not a template limitation, so a single-select customer prompt is the
correct interaction and multi-select prompts are unnecessary. A dashboard
query flags violations (below). Per-customer external templates (e.g. a QBR
template with a standing agenda) are just extra files in
`.notesmith/templates/` — add one only when a customer earns it.

### `stream.md`

```markdown
---
name: stream
description: "New stream of work"
output_path: "Inbox/{{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
  - { name: customer, type: field-picker, required: false }
  - { name: priority, type: field-picker, required: false }
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
