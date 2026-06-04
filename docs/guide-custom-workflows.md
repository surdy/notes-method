# Building Custom Workflows

Notesmith doesn't prescribe how you organize notes. Instead, it provides generic primitives — fields, tags, tasks, routing, and templates — that you compose into any workflow.

This guide walks through building a customer/project tracking workflow from scratch.

---

## Core Concepts

| Primitive | How it works |
|-----------|-------------|
| **Tags** | `tags: [customer, active]` in frontmatter. Used for categorization and filtering. |
| **Fields** | Any YAML key in frontmatter (e.g. `status: Active`). Indexed into SQLite for querying. |
| **Inline fields** | `[key:: value]` in body text. Same query surface as frontmatter fields. |
| **Task fields** | Inline fields on task lines: `- [ ] Do thing [owner:: me] [due:: 2026-06-15]` |
| **SQL views** | Query any indexed data with `SELECT` statements. |
| **Routing rules** | Automatically move/file notes based on their tags and fields. |
| **Templates** | Generate notes with pre-filled frontmatter, prompts, and computed paths. |

---

## Step 1: Define Your Note Kinds with Tags

Instead of a hardcoded type system, use tags to categorize notes:

```yaml
---
tags: [customer]
state: Active
---
# Acme Corp
```

```yaml
---
tags: [stream]
customer: "[[Acme Corp]]"
status: active
priority: P1
---
# Migration to v2
```

```yaml
---
tags: [meeting, external]
customer: "[[Acme Corp]]"
date: 2026-06-04
---
# Q2 Planning Call
```

**Why tags?** They're multi-value, fast to query (`JOIN tags ... AND tag = 'stream'`), and show as visual pills in the UI.

---

## Step 2: Register Fields for Autocomplete

Create `.notesmith/fields.toml` to get autocomplete suggestions when editing frontmatter:

```toml
[customer]
type = "link"
suggest_from = "tags includes 'customer'"

[status]
type = "enum"
values = ["active", "waiting", "blocked", "done", "archived"]

[priority]
type = "enum"
values = ["P0", "P1", "P2", "P3"]

[owner]
type = "string"
values = ["me", "customer", "team"]
```

This is advisory only — Notesmith doesn't enforce field values.

---

## Step 3: Query Your Data

### In the CLI

```bash
# List all active streams
notesmith query sql "
  SELECT n.title, status.value AS status, cust.value AS customer
  FROM v_notes n
  JOIN tags t ON t.vault_name = n.vault_name AND t.note_path = n.path AND t.tag = 'stream'
  LEFT JOIN v_fields status ON status.vault_name = n.vault_name AND status.note_path = n.path AND status.key = 'status'
  LEFT JOIN v_fields cust ON cust.vault_name = n.vault_name AND cust.note_path = n.path AND cust.key = 'customer'
  WHERE status.value = 'active'
  ORDER BY n.title
"

# List open tasks for a customer
notesmith task list --field customer=Acme --status todo
```

### In Dashboard Notes (live SQL blocks)

````markdown
```sql
SELECT n.title, status.value AS status, priority.value AS priority
FROM v_notes n
JOIN tags t ON t.vault_name = n.vault_name AND t.note_path = n.path AND t.tag = 'stream'
LEFT JOIN v_fields status ON status.vault_name = n.vault_name AND status.note_path = n.path AND status.key = 'status'
LEFT JOIN v_fields priority ON priority.vault_name = n.vault_name AND priority.note_path = n.path AND priority.key = 'priority'
WHERE status.value NOT IN ('done', 'archived')
ORDER BY priority.value, n.title;
```
````

These render as live tables in the editor.

---

## Step 4: Create Reusable Views

Simplify queries by defining views in `.notesmith/views.sql`:

```sql
CREATE VIEW my_streams AS
SELECT
  n.path,
  n.title,
  (SELECT f.value FROM fields f WHERE f.vault_name = n.vault_name AND f.note_path = n.path AND f.key = 'customer' LIMIT 1) AS customer,
  (SELECT f.value FROM fields f WHERE f.vault_name = n.vault_name AND f.note_path = n.path AND f.key = 'status' LIMIT 1) AS status,
  (SELECT f.value FROM fields f WHERE f.vault_name = n.vault_name AND f.note_path = n.path AND f.key = 'priority' LIMIT 1) AS priority,
  n.updated_at
FROM notes n
JOIN tags t ON t.vault_name = n.vault_name AND t.note_path = n.path AND t.tag = 'stream';
```

Now your dashboard queries become simple:

````markdown
```sql
SELECT title, customer, status, priority FROM my_streams WHERE status = 'active';
```
````

---

## Step 5: Automate Filing with Routing Rules

Define rules in `.notesmith/routing.yaml` to move notes from your Inbox to their permanent location:

```yaml
version: 1
defaults:
  on_exists: rename

rules:
  - id: route-meeting
    when:
      all:
        - tags_include: [meeting]
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/Meetings/{{ filename }}"
      remove_tags: [inbox]

  - id: route-stream
    when:
      all:
        - tags_include: [stream]
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/Streams/{{ filename }}"
      remove_tags: [inbox]

  - id: route-to-customer
    when:
      all:
        - path: "Inbox/**"
        - field.customer: "*"
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/{{ filename }}"
      remove_tags: [inbox]
```

Apply routing manually:
```bash
notesmith route apply "Inbox/Q2 Planning Call.md"
```

Or set rules to `auto: true` for automatic routing on save.

---

## Step 6: Create Templates for Repeated Notes

Templates live in your vault's template directory. Each has a YAML header defining prompts and output path:

### Meeting Template

```markdown
---
name: meeting
description: "New meeting note"
output_path: "Inbox/{{ date }} {{ title }}.md"
prompts:
  - { name: customer, type: field-picker, field: customer, required: true }
  - { name: title, type: text, required: true }
---
---
tags: [meeting, inbox]
customer: "[[{{ customer }}]]"
date: {{ date }}
---

# {{ title }}

## Attendees

## Discussion

## Action Items

- [ ] [customer:: [[{{ customer }}]]]
```

Use it:
```bash
notesmith template instantiate meeting --prompt customer=Acme --prompt title="Q2 Planning"
```

---

## Step 7: Add Tasks with Inline Fields

Tasks are checkbox lines with optional inline metadata:

```markdown
- [ ] Send SOW to legal [customer:: [[Acme Corp]]] [due:: 2026-06-15] [priority:: P1]
- [/] Review staging deployment [stream:: [[Migration to v2]]] [owner:: me]
- [x] Schedule kickoff call [customer:: [[Acme Corp]]]
```

Query tasks from the CLI:
```bash
# All open tasks for a customer
notesmith task list --field customer=Acme

# Tasks due this week
notesmith task list --due-before 2026-06-10

# Add a task programmatically
notesmith task add "Projects/migration.md" "Update staging config" \
  -f customer=Acme -f priority=P1 -f due=2026-06-10
```

---

## Step 8: Configure the Sidebar

Define custom sidebar views in `.notesmith/sidebar.yaml`:

```yaml
views:
  - id: triage
    name: "Triage"
    icon: "⚡"
    badge_query: "SELECT count(*) FROM v_notes WHERE path LIKE 'Inbox/%'"
    sections:
      - type: recently-viewed
        label: "Recent Notes"
        mode: both
        limit: 10
      - type: custom-items
        label: "Work"
        items:
          - name: "Inbox"
            icon: "📥"
            source:
              folder: "Inbox"
              recursive: true
              sort: modified
              sort_dir: desc
          - name: "Open Tasks"
            icon: "✅"
            source:
              query: >
                SELECT text AS title, status_group AS subtitle, note_path AS path, line_number AS line
                FROM v_tasks
                WHERE status_group = 'open'
                ORDER BY note_path, line_number
              title_column: "title"
              badge_columns: ["subtitle"]
      - type: custom-folders
        label: "Customers"
        folders:
          - "Customers/Acme"
          - "Customers/Globex"
```

---

## Putting It All Together

The workflow cycle:

1. **Capture** → `notesmith capture "thought"` puts a note in Inbox/
2. **Enrich** → Add tags (`#stream`, `#meeting`) and fields (`customer`, `status`)
3. **Route** → `notesmith route apply <path>` moves it to the right folder
4. **Track** → Dashboard SQL blocks and sidebar views show live status
5. **Complete** → Toggle tasks: `notesmith task set-status <path> <hash> done`

---

## Other Workflow Ideas

The same primitives work for any system:

| Workflow | Tags | Key Fields |
|----------|------|------------|
| **PARA Method** | `#project`, `#area`, `#resource`, `#archive` | `status`, `area` |
| **Zettelkasten** | `#literature`, `#permanent`, `#fleeting` | `source`, `author`, `related` |
| **CRM** | `#contact`, `#deal`, `#company` | `stage`, `value`, `next_action` |
| **Bug Tracking** | `#bug`, `#feature`, `#enhancement` | `severity`, `component`, `assignee` |
| **Content Calendar** | `#draft`, `#published`, `#idea` | `publish_date`, `platform`, `status` |

Each just needs:
1. Tags for categorization
2. Fields for structured metadata
3. Routing rules for organization
4. Templates for quick creation
5. SQL views for dashboards

See [Example: Work Notes Kit](example-work-notes-kit.md) for a complete reference configuration.
