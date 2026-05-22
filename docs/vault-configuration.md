# Notesmith Vault Configuration Guide
Notesmith uses two levels of configuration:
1. **Global config** (`~/.config/notesmith/config.toml`) — daemon settings and vault registry
2. **Per-vault config** (`.notesmith/` inside the vault) — vault-specific behavior

This guide covers every config file and format Notesmith reads today.

---
## Overview
| Scope | Path | Format | Purpose |
|------|------|--------|---------|
| Global | `~/.config/notesmith/config.toml` or `$XDG_CONFIG_HOME/notesmith/config.toml` | TOML | Daemon settings, default vault, named vault registry |
| Per-vault | `<vault>/.notesmith/vault.toml` | TOML | Main vault behavior |
| Per-vault | `<vault>/.notesmith/sidebar-views.yaml` | YAML | SQL-backed sidebar views |
| Per-vault | `<vault>/.notesmith/routing.yaml` | YAML | Capture routing rules |
| Per-vault | `<vault>/Assets/templates/*.md.j2` | Markdown + YAML front matter + Minijinja | Note templates |
| Per-vault | `<vault>/.notesmith/skill.md` | Markdown | AI instruction file |
| Per-vault | `<vault>/.notesmith/prompts/*.md` | Markdown + YAML front matter | Agent prompt templates |

Vault resolution order:
1. Walk upward from `$PWD` looking for `.notesmith/vault.toml`
2. Use `--vault <name>` if provided
3. Fall back to the default vault from global config

---
## Global Configuration
File: `~/.config/notesmith/config.toml` (or `$XDG_CONFIG_HOME/notesmith/config.toml`)

```toml
default_vault = "work"

[daemon]
bind = "127.0.0.1:27183"
auto_start = true

[vaults.work]
path = "/Users/me/Notes/work"

[vaults.personal]
path = "/Users/me/Notes/personal"
```

Fields:
- `default_vault` — vault name used when no `--vault` is specified
- `daemon.bind` — address and port for the HTTP daemon (default: `127.0.0.1:27183`)
- `daemon.auto_start` — whether CLI commands auto-start the daemon (default: `true`)
- `vaults.<name>.path` — filesystem path to each vault root

---
## Per-Vault Configuration
All per-vault config lives in the `.notesmith/` directory at the vault root.

```text
my-vault/
└── .notesmith/
    ├── vault.toml
    ├── sidebar-views.yaml
    ├── routing.yaml
    ├── skill.md
    └── prompts/
```

---
## `vault.toml`
Main vault config file. `name` identifies the vault. All sections are optional with sensible defaults.

```toml
schema_version = 1
name = "work"
homepage = "Home.md"

[capture]
folder = ""                   # Default capture folder (vault root)
template = "generic-note"     # Default template for captured notes

[daily]
folder = ""                   # Where daily notes are created (vault root by default)
template = "daily-note"       # Template for daily notes
generate_at = "06:00"         # Auto-generate daily note at this time (optional)
timezone = "America/Los_Angeles"  # Timezone for daily scheduler (optional)
catch_up = false              # Create missed daily notes on startup (default: false)

[editor]
live_preview = true           # Enable Live Preview mode (default: true)
default_mode = "source"       # Default view mode: "source", "live-preview", or "reading"
strict_line_breaks = false   # Use standard Markdown soft breaks instead of Obsidian-style single-newline breaks
show_line_numbers = true      # Show line numbers in Source and Live Preview modes (default: true)

[git]
enabled = false               # Enable git integration (default: false)
auto_commit_every = "5m"      # Auto-commit interval (optional, e.g. "5m", "1h")
auto_pull_every = "10m"       # Auto-pull interval (optional)
auto_push_every = "10m"       # Auto-push interval (optional)
commit_message = "vault sync" # Custom commit message (optional)

[hooks]
on_note_create = "scripts/on-create.sh"   # Script to run when a note is created (optional)
on_daily_create = "scripts/on-daily.sh"   # Script to run when a daily note is created (optional)
```

Top-level fields:
- `schema_version` — vault config schema version. Existing files without this field default to `1`, and daemon/runtime loads reject newer unknown versions.
- `name` — vault identifier used in CLI and API output
- `homepage` — vault-relative path to the home note

`[capture]`:
- `folder` — default capture folder (default: `""`, meaning the vault root)
- `template` — default template for captured notes (default: `generic-note`)

`[daily]`:
- `folder` — where daily notes are created (default: `""`, meaning the vault root)
- `template` — template for daily notes (default: `daily-note`)
- `generate_at` — local time for scheduled daily note creation
- `timezone` — IANA timezone for the scheduler
- `catch_up` — create missed daily notes on startup (default: `false`)

`[editor]`:
- `live_preview` — enable Live Preview mode (default: `true`)
- `default_mode` — `source`, `live-preview`, or `reading` (default: `source`)
- `strict_line_breaks` — require standard Markdown line breaks; when `false`, single newlines render as line breaks like Obsidian (default: `false`)
- `show_line_numbers` — show line numbers in Source and Live Preview editor modes (default: `true`)

`[git]`:
- `enabled` — enable per-vault git integration (default: `false`)
- `auto_commit_every` — auto-commit interval such as `5m` or `1h`
- `auto_pull_every` — auto-pull interval
- `auto_push_every` — auto-push interval
- `commit_message` — commit message used by automatic sync

`[hooks]`:
- `on_note_create` — script to run when a note is created
- `on_daily_create` — script to run when a daily note is created

---
## `sidebar-views.yaml`
Custom sidebar views with SQL-powered data sources.

```yaml
views:
  - id: all-notes
    name: All Notes
    icon: 📄
    data_source: "SELECT path, title, type FROM v_notes ORDER BY path"

  - id: tasks
    name: Tasks
    icon: ✅
    data_source: "SELECT note_path AS path, text AS title, status FROM v_tasks ORDER BY status, note_path, ordinal"
    group_by: status

  - id: recent
    name: Recent
    icon: 🕐
    data_source: "SELECT path, title, type, updated_at FROM v_notes ORDER BY mtime_unix DESC LIMIT 30"

  - id: capture
    name: Capture
    icon: ⚡
    data_source: "SELECT path, title FROM v_notes WHERE path LIKE 'Capture/%' ORDER BY path"
    badge_query: "SELECT COUNT(*) as count FROM v_notes WHERE path LIKE 'Capture/%'"
```

Fields:
- `id` — unique identifier for the view
- `name` — display name in the sidebar
- `icon` — emoji icon shown next to the name
- `data_source` — SQL query returning at least `path` and `title`
- `group_by` — optional column name used to group results
- `badge_query` — optional SQL query returning a `count` column

Rules:
- `data_source` must return at least `path` and `title`.
- `badge_query` must return a `count` column.
- Views should query public SQL views: `v_notes`, `v_tasks`, `v_backlinks`, `v_customers`, `v_streams`.
- See [SQL Views Reference](sql-views.md) for view schemas.

---
## `routing.yaml`
Rules for automatically routing captured notes to their destination folders. See [CLI docs](cli.md) for `notesmith route` commands.

```yaml
version: 1
default_on_exists: skip

rules:
  - id: external-meeting
    when:
      type: meeting
      meeting-kind: external
    then:
      move_to: "Customers/{{ customer | unwikilink }}/External Meetings/"

  - id: daily
    when:
      type: daily
    then:
      move_to: "General/Journal/{{ date | year }}/{{ date | month }}/"

  - id: note-general
    when:
      type: note
    then:
      move_to: "General/"
```

Fields:
- `version` — schema version (always `1`)
- `default_on_exists` — what to do if destination exists (`skip` or `overwrite`)
- `rules[].id` — unique rule identifier
- `rules[].when` — frontmatter field matchers
- `rules[].then.move_to` — destination folder template

Behavior:
- Rules are evaluated top-to-bottom; first match wins.
- Matchers are exact matches unless the value is `"*"`.
- `"*"` means any non-null value.
- `move_to` is a Minijinja template with frontmatter values.

Available Minijinja filters:
- `unwikilink` — strips `[[` / `]]`
- `slug` — makes a filename-friendly slug
- `year` — extracts the year from a date
- `month` — extracts the month from a date

---
## Templates
Templates live in `Assets/templates/` as Minijinja `.md.j2` files.

```text
Assets/templates/
├── generic-note.md.j2
├── daily-note.md.j2
├── external-meeting.md.j2
├── internal-meeting.md.j2
├── customer-index.md.j2
├── stream.md.j2
├── account-info.md.j2
├── glossary.md.j2
└── milestones.md.j2
```

Each template has YAML front matter in a `notesmith:` block:

```markdown
---
notesmith:
  name: generic-note
  description: A generic blank note
  output_path: "{% if folder %}{{ folder }}/{% endif %}{{ title | slug }}.md"
  prompts:
    - name: title
      type: text
      required: true
    - name: folder
      type: text
      required: false
---
# {{ title }}
```

Template metadata fields:
- `name` — template identifier used in CLI and API
- `description` — human-readable description
- `output_path` — Minijinja expression for the output file path
- `prompts` — list of user inputs required to render the template
- `prompts[].name` — prompt identifier
- `prompts[].type` — `text` (more types may be added)
- `prompts[].required` — whether the prompt is required

---
## Hooks
Hook scripts run as subprocesses when certain events occur. Configure them in `vault.toml` under `[hooks]`.

Scripts receive a JSON payload on stdin:

```json
{
  "event": "on_note_create",
  "vault": "work",
  "path": "New Note.md",
  "frontmatter": { "type": "note" },
  "source": "api"
}
```

Hook failures are logged but do not fail the originating operation.

---
## Skill File
`.notesmith/skill.md` is an AI instruction file that tells AI agents how to interact with the vault. Print it with:

```bash
notesmith skill print
```

It typically contains command cheat sheets, vault structure, note type schemas, and common workflow recipes.

---
## Agent Prompts
`.notesmith/prompts/` contains prompt templates for agent-driven workflows. The daily note prompt (`daily-note.md`) can include YAML front matter with `context_queries` — SQL queries whose results are injected as markdown tables.

```yaml
---
context_queries:
  - name: open_tasks
    sql: "SELECT text, due, customer FROM v_tasks WHERE status IN ('todo', 'in_progress')"
---
# Daily Note Prompt
Today's date: {{ today }}
### Open Tasks
{{ open_tasks }}
```

Fields:
- `context_queries` — list of SQL queries whose results are injected into the prompt
- `context_queries[].name` — variable name for the rendered markdown table
- `context_queries[].sql` — read-only SQL query to execute

---
## Vault Directory Structure
A typical Notesmith vault:

```text
my-vault/
├── .notesmith/
│   ├── vault.toml              # Vault configuration
│   ├── routing.yaml            # Routing rules
│   ├── sidebar-views.yaml      # Sidebar view definitions
│   ├── skill.md                # AI skill file
│   └── prompts/
│       └── daily-note.md       # Agent daily prompt
├── Assets/
│   └── templates/              # Note templates (.md.j2)
├── Capture/                    # Optional dedicated capture folder
├── Daily/                      # Optional dedicated daily folder
├── Customers/                  # Per-customer folders
│   └── Acme Corp/
│       ├── Acme Corp.md        # Customer index note
│       ├── Account Info/
│       ├── External Meetings/
│       ├── Internal Meetings/
│       └── Streams/
└── General/                    # Non-customer notes
```
