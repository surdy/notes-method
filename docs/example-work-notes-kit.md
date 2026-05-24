# Example: Work Notes Kit

This document shows how to configure Notesmith for a customer-facing work workflow using the generic data model. This is **one possible configuration** — not built into the product.

## Overview

The Work Notes kit organizes notes around customers, streams of work, and meetings. It demonstrates how routing rules, templates, field definitions, and dashboards compose into a complete workflow on top of Notesmith's generic primitives.

## Folder Structure (suggested)

```text
Inbox/
Daily/
Tasks/
Customers/
  <Customer>/
    <Customer>.md
    Account Info/
    Internal Meetings/
    External Meetings/
    Streams/
General/
  Journal/
Dashboards/
  Home.md
  Inbox Triage.md
  Customers.md
  Streams.md
Assets/
  templates/
  scripts/
.notesmith/
  vault.toml
  fields.toml
  routing.yaml
  views.sql
  sidebar.yaml
  templates/
  prompts/
  skill.md
```

## Field Registry (`.notesmith/fields.toml`)

```toml
[customer]
type = "link"
suggest_from = "tags includes 'customer'"

[stream]
type = "link"
suggest_from = "tags includes 'stream'"

[status]
type = "enum"
values = ["inbox", "active", "waiting", "blocked", "done", "archived"]

[priority]
type = "enum"
values = ["P0", "P1", "P2", "P3"]

[owner]
type = "string"
values = ["me", "customer"]

[meeting_type]
type = "enum"
values = ["internal", "external"]

[kind]
type = "enum"
values = ["note", "meeting", "stream", "customer", "account-info"]

[date]
type = "date"

[started]
type = "date"

[target]
type = "date"
```

## Task Statuses (in `vault.toml`)

```toml
[task_statuses]
" " = { label = "Todo", group = "open", icon = "circle" }
"x" = { label = "Done", group = "done", icon = "check" }
"/" = { label = "In Progress", group = "open", icon = "half-circle" }
"b" = { label = "Blocked", group = "open", icon = "stop" }
"w" = { label = "Waiting", group = "open", icon = "clock" }
"h" = { label = "On Hold", group = "open", icon = "pause" }
"-" = { label = "Cancelled", group = "done", icon = "dash" }
```

## Routing Rules (`.notesmith/routing.yaml`)

```yaml
version: 1
defaults:
  on_exists: rename

rules:
  - id: route-external-meeting
    when:
      all:
        - tags_include: [meeting]
        - field.meeting_type: "external"
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/External Meetings/{{ filename }}"
      set_fields:
        status: archived
      remove_tags: [inbox]

  - id: route-internal-meeting
    when:
      all:
        - tags_include: [meeting]
        - field.meeting_type: "internal"
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/Internal Meetings/{{ filename }}"
      set_fields:
        status: archived
      remove_tags: [inbox]

  - id: route-stream
    when:
      all:
        - tags_include: [stream]
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/Streams/{{ filename }}"
      remove_tags: [inbox]

  - id: route-customer-note
    when:
      all:
        - path: "Inbox/**"
        - field.customer: "*"
        - not:
            tags_include: [meeting, stream]
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/{{ filename }}"
      remove_tags: [inbox]

  - id: route-general
    when:
      all:
        - path: "Inbox/**"
        - not:
            field_exists: customer
    then:
      move_to: "General/{{ filename }}"
      remove_tags: [inbox]

  - id: archive-daily
    when:
      all:
        - path: "Daily/**"
        - field.date: "< today - 30d"
    then:
      move_to: "General/Journal/{{ field.date | strftime('%Y/%m') }}/{{ filename }}"
```

## User-Defined Views (`.notesmith/views.sql`)

```sql
CREATE VIEW user_streams AS
SELECT
  n.path,
  n.title,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'customer' LIMIT 1) AS customer,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'status' LIMIT 1) AS status,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'priority' LIMIT 1) AS priority,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'owner' LIMIT 1) AS owner,
  n.updated_at
FROM notes n
JOIN tags t ON t.note_path = n.path AND t.tag = 'stream'
WHERE n.vault = :vault;

CREATE VIEW user_customers AS
SELECT
  n.path,
  n.title,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'state' LIMIT 1) AS state,
  n.updated_at
FROM notes n
JOIN tags t ON t.note_path = n.path AND t.tag = 'customer'
WHERE n.vault = :vault;

CREATE VIEW user_meetings AS
SELECT
  n.path,
  n.title,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'customer' LIMIT 1) AS customer,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'meeting_type' LIMIT 1) AS meeting_type,
  (SELECT f.value FROM fields f WHERE f.note_path = n.path AND f.key = 'date' LIMIT 1) AS date
FROM notes n
JOIN tags t ON t.note_path = n.path AND t.tag = 'meeting'
WHERE n.vault = :vault;
```

## Templates

### Meeting Template (`.notesmith/templates/meeting.md`)

```markdown
---
name: meeting
description: "Create a new meeting note"
output_path: "Inbox/{{ date }} - {{ customer }} - {{ meeting_type }} - {{ title }}.md"
prompts:
  - { name: customer, type: field-picker, field: customer, required: true }
  - { name: meeting_type, type: field-picker, field: meeting_type, required: true }
  - { name: title, type: text, required: true }
  - { name: stream, type: field-picker, field: stream, required: false }
---
---
tags: [meeting]
customer: "[[{{ customer }}]]"
meeting_type: {{ meeting_type }}
date: {{ date }}
{% if stream %}stream: "[[{{ stream }}]]"{% endif %}
---

# {{ date }} — {{ customer }} — {{ title }}

## Attendees

## Discussion

## Decisions

## Tasks

- [ ] 
```

### Stream Template (`.notesmith/templates/stream.md`)

```markdown
---
name: stream
description: "Create a new stream of work"
output_path: "Inbox/{{ title }}.md"
prompts:
  - { name: customer, type: field-picker, field: customer, required: true }
  - { name: title, type: text, required: true }
  - { name: priority, type: field-picker, field: priority, required: false }
---
---
tags: [stream]
customer: "[[{{ customer }}]]"
status: active
{% if priority %}priority: {{ priority }}{% endif %}
owner: me
started: {{ date }}
---

# {{ title }}

[customer:: [[{{ customer }}]]]
[status:: active]

## Goal

## Current State

## Tasks

- [ ] 
```

### Customer Template (`.notesmith/templates/customer.md`)

```markdown
---
name: customer
description: "Create a new customer"
output_path: "Customers/{{ name }}/{{ name }}.md"
prompts:
  - { name: name, type: text, required: true }
---
---
tags: [customer]
state: Active
---

# {{ name }}

## Overview

## People

## Active Streams

```notesmith sql
SELECT title, path, status
FROM user_streams
WHERE customer = '[[{{ name }}]]' AND status != 'done'
ORDER BY priority, title;
```
```

## Dashboard Examples

### Inbox Triage (`Dashboards/Inbox Triage.md`)

```markdown
# Inbox Triage

## Unprocessed Notes

```notesmith sql
SELECT title, path, created_at
FROM v_notes
WHERE path LIKE 'Inbox/%'
ORDER BY created_at DESC;
```

## Open Tasks in Inbox

```notesmith sql
SELECT t.text, t.note_path, fields_json
FROM v_tasks t
WHERE t.note_path LIKE 'Inbox/%' AND t.status_group = 'open'
ORDER BY t.note_path;
```
```

### Streams Dashboard (`Dashboards/Streams.md`)

```markdown
# Active Streams

```notesmith sql
SELECT title, customer, status, priority, path
FROM user_streams
WHERE status NOT IN ('done', 'archived')
ORDER BY priority, customer, title;
```

## Blocked Streams

```notesmith sql
SELECT title, customer, path
FROM user_streams
WHERE status = 'blocked';
```
```

## Hook Examples

### Auto-Notify on Status Change

```toml
# vault.toml
[hooks.on_field_change.status-notify]
command = "scripts/notify-status.sh"
watch_fields = ["status"]
```

`scripts/notify-status.sh`:
```bash
#!/bin/bash
# Reads JSON from stdin, sends notification when a stream becomes blocked
STATUS=$(echo "$1" | jq -r '.changes[] | select(.key == "status") | .new')
if [ "$STATUS" = "blocked" ]; then
  # Send Slack notification, create reminder, etc.
  echo "Stream blocked: $(echo "$1" | jq -r '.note.title')"
fi
```

## Skill File (`.notesmith/skill.md`)

```markdown
# Notesmith Vault: Work Notes

This vault organizes customer-facing work using these conventions:

## Note Kinds (via tags)
- `#customer` — Customer index notes in `Customers/<Name>/<Name>.md`
- `#stream` — Streams of work with status/priority/owner fields
- `#meeting` — Meeting notes with meeting_type (internal/external) and customer

## Key Fields
- `customer` — wikilink to customer note
- `stream` — wikilink to stream note
- `status` — inbox/active/waiting/blocked/done/archived
- `priority` — P0/P1/P2/P3
- `owner` — me/customer
- `meeting_type` — internal/external

## Common Commands
- `notesmith capture "text"` — quick capture to Inbox/
- `notesmith route apply <path>` — move note to destination per routing rules
- `notesmith periodic open daily` — open today's daily note
- `notesmith query sql "SELECT * FROM user_streams WHERE status = 'active'"` — query streams

## Workflow
1. Capture ideas/notes quickly to Inbox/
2. Enrich with fields (customer, tags, status)
3. Route to permanent location
4. Track work via dashboards and task views
```
