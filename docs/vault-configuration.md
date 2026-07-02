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
| Per-vault | `<vault>/.notesmith/templates/*.md` | Markdown + YAML front matter + Minijinja | Note templates (legacy `Assets/templates/*.md.j2` also loads) |
| Per-vault | `<vault>/.notesmith/skill.md` | Markdown | AI instruction file |
| Per-vault | `<vault>/.notesmith/prompts/*.md` | Markdown + YAML front matter | Agent prompt templates |
| Per-vault / Global | `.notesmith/{agents,skills,instructions}/*.md` (vault) and `~/.config/notesmith/{agents,skills,instructions}/*.md` | Markdown + YAML front matter | Custom agent personas, skills, and always-on instructions |

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

### AI agent discovery (`[agents]`)

The desktop app auto-detects external agent CLIs (Copilot, Claude, Codex,
Gemini, OpenCode) on your `PATH`. The optional `[agents]` section is the manual
escape hatch — override a built-in's launch command, add a custom ACP agent, or
turn on discovery diagnostics. Everything here is optional; omit the section to
rely purely on auto-detection.

```toml
[agents]
debug = false                          # opt-in discovery diagnostics (default off)

[agents.copilot]                       # override a built-in agent's binary
command = "/opt/copilot/bin/copilot"
args = ["--acp"]

[agents.my-agent]                      # add a custom ACP agent
display_name = "My Agent"
command = "node"
args = ["~/projects/agent/index.js", "--acp"]
enabled = true
[agents.my-agent.env]
FOO = "bar"
```

- `agents.debug` — when `true`, the discovery pipeline records a step-by-step
  trace surfaced by **Settings → AI Agent → Run diagnostics** (default `false`)
- `agents.<id>.command` — launch program; overrides a built-in or defines a
  custom agent. `~` and `$VAR`/`${VAR}` are expanded
- `agents.<id>.args` — arguments passed to `command`
- `agents.<id>.env` — extra environment variables for the agent process
- `agents.<id>.display_name` — label shown in the picker (defaults to the id)
- `agents.<id>.enabled` — set `false` to hide a built-in agent (default `true`)

A user entry always wins over auto-detection; these settings are also editable
from **Settings → AI Agent** without hand-editing the file.

### External MCP servers (`[mcp]`)

Every chat session always exposes the **active vault's notes** to the agent over
the daemon's built-in MCP endpoint (read-only vs read-write follows the chat
panel's scope toggle); those built-in tools are always on and cannot be removed.
The optional `[mcp]` section adds **external** MCP servers the agent can use
alongside the vault tools. It lives in the **global** config so a server list is
reusable across vaults.

```toml
[[mcp.servers]]                        # a stdio (command) MCP server
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "~/notes"]
display_name = "Files"
enabled = true
[mcp.servers.env]
TOKEN = "secret"

[[mcp.servers]]                        # an HTTP(S) MCP server
id = "remote-tools"
url = "https://tools.example.com/mcp"
enabled = false
```

- `mcp.servers[].id` — stable identifier and the server name surfaced to the
  agent (required)
- `mcp.servers[].command` — program for a **stdio** server; `~` and
  `$VAR`/`${VAR}` are expanded. Mutually exclusive with `url` (command wins)
- `mcp.servers[].args` — arguments passed to `command`
- `mcp.servers[].env` — extra environment variables for a stdio server
- `mcp.servers[].url` — endpoint for an **HTTP(S)** server
- `mcp.servers[].display_name` — label shown in Settings (defaults to the id)
- `mcp.servers[].enabled` — set `false` to keep an entry configured but hide it
  from agent sessions (default `true`)

These servers are editable from **Settings → MCP Servers** without hand-editing
the file. Scope is global today; per-vault overrides are deferred (ADR 0016).
For a step-by-step walkthrough of that Settings screen (including adding stdio
and HTTP servers), see the [MCP Servers guide](ai-mcp-servers.md).

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

[periodic.daily]
folder = "Daily"
template = "daily"
filename = "{{ date }}"
generate_at = "06:00"            # Auto-generate daily note at this time (optional)
timezone = "America/Los_Angeles" # Timezone for daily scheduler (optional)
catch_up = false                 # Create missed daily notes on startup (default: false)

[periodic.weekly]
folder = "Weekly"
template = "weekly"
filename = "Week {{ week }}"

[periodic.monthly]
folder = "Monthly"
template = "monthly"
filename = "{{ month }}"

[periodic.quarterly]
folder = "Quarterly"
template = "quarterly"
filename = "{{ quarter }}"

[periodic.yearly]
folder = "Yearly"
template = "yearly"
filename = "{{ year }}"

[editor]
live_preview = true               # Enable Live Preview mode (default: true)
default_mode = "source"           # Default view mode: "source", "live-preview", or "reading"
strict_line_breaks = false        # Use standard Markdown soft breaks instead of Obsidian-style single-newline breaks
show_line_numbers = true          # Show line numbers in Source and Live Preview modes (default: true)
hide_duplicate_h1 = true          # Hide a first H1 that duplicates the note title in reading/live preview
paste_url_image_whitelist = ""    # One regex per line; empty disables automatic ![]() embeds on paste

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

`[periodic.<kind>]`:
- `folder` — folder for this periodic kind
- `template` — optional template name used when creating notes for this kind
- `filename` — Minijinja filename template (without `.md`)
- `generate_at` — local time for scheduled creation (daily only)
- `timezone` — IANA timezone for the scheduler (daily only)
- `catch_up` — create missed daily notes on startup (daily only, default: `false`)

Supported kinds are `daily`, `weekly`, `monthly`, `quarterly`, and `yearly`.
Legacy `[daily]` settings are still read and mapped to `periodic.daily` for backward compatibility.

`[editor]`:
- `live_preview` — enable Live Preview mode (default: `true`)
- `default_mode` — `source`, `live-preview`, or `reading` (default: `source`)
- `strict_line_breaks` — require standard Markdown line breaks; when `false`, single newlines render as line breaks like Obsidian (default: `false`)
- `show_line_numbers` — show line numbers in Source and Live Preview editor modes (default: `true`)
- `hide_duplicate_h1` — hide a first H1 that duplicates the note title in reading view and live preview (default: `true`)
- `paste_url_image_whitelist` — newline-delimited regex patterns that turn pasted URLs into image embeds when they match; empty disables the behavior (default: empty string)

`[git]`:
- `enabled` — enable per-vault git integration (default: `false`)
- `auto_commit_every` — auto-commit interval such as `5m` or `1h` (local, no remote needed)
- `auto_pull_every` — auto-pull interval (remote sync)
- `auto_push_every` — auto-push interval (remote sync)
- `commit_message` — commit message used by automatic commits

**Local-only versioning:** The three intervals are independent, and syncing is
entirely optional. To keep local snapshots without any remote, set `enabled = true`
and `auto_commit_every`, and leave `auto_pull_every`/`auto_push_every` unset — no
`origin` remote is required. `auto_pull_every` and `auto_push_every` are the only
fields that talk to a remote (they use `origin`).

Git integration never runs `git init` for you: the vault root must already be a git
repository, otherwise the timers are skipped with a warning. Initialize it once with
`git init` in the vault directory.

```toml
[git]
enabled = true
auto_commit_every = "15m"
commit_message = "notesmith: {{ operation }}"
# auto_pull_every / auto_push_every omitted → local-only, no remote sync
```

`[hooks]`:
- `on_note_create` — script to run when a note is created
- `on_periodic_create` — script to run when a periodic note is created
- `on_daily_create` — legacy alias for `on_periodic_create`

---
## `sidebar-views.yaml`
Custom sidebar views with SQL-powered data sources.

```yaml
views:
  - id: all-notes
    name: All Notes
    icon: 📄
    data_source: "SELECT path, title, updated_at FROM v_notes ORDER BY path"

  - id: tasks
    name: Tasks
    icon: ✅
    data_source: "SELECT note_path AS path, text AS title, status_group FROM v_tasks ORDER BY status_group, note_path, line_number"
    group_by: status_group

  - id: recent
    name: Recent
    icon: 🕐
    data_source: "SELECT path, title, updated_at FROM v_notes ORDER BY updated_at DESC LIMIT 30"

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
- Views should query public SQL views: `v_notes`, `v_fields`, `v_tasks`, `v_task_fields`, `v_backlinks`, and `v_periodic`.
- See [SQL Views Reference](sql-views.md) for view schemas.

---
## `routing.yaml`
Rules for automatically routing captured notes to their destination folders. See [CLI docs](cli.md) for `notesmith route` commands.

```yaml
version: 1
defaults:
  on_exists: rename

rules:
  - id: route-meeting
    auto: false
    when:
      all:
        - path: "Inbox/**"
        - tags_include: [meeting]
        - field.customer: "*"
        - field.meeting_type: external
        - not:
            tags_include: [archived]
    then:
      move_to: "Customers/{{ field.customer | unwikilink }}/Meetings/{{ filename }}"
      set_fields:
        status: filed
      remove_fields: [temp_notes]
      add_tags: [archived]
      remove_tags: [inbox]
```

Fields:
- `version` — schema version (always `1`)
- `defaults.on_exists` — collision policy for existing destinations (`skip`, `overwrite`, `rename`)
- `rules[].id` — unique rule identifier
- `rules[].auto` — opt a rule into auto-routing (defaults to `false`)
- `rules[].when` — recursive predicate tree using `all`, `any`, `not`, `field.<key>`, `field_exists`, `tags_include`, `tags_exclude`, and `path`
- `rules[].then.move_to` — destination path template
- `rules[].then.set_fields` / `remove_fields` — frontmatter mutations
- `rules[].then.add_tags` / `remove_tags` — tag mutations

Behavior:
- Rules are evaluated top-to-bottom; first match wins.
- `field.<key>: "*"` means the field exists and is non-empty.
- `field_exists` matches even when the field value is empty.
- `move_to` is rendered with Minijinja using `field.*`, `filename`, `tags`, and legacy top-level field names.
- Routing applies configured mutations, stamps `archived` / `archived-at`, then moves the note.

Available Minijinja filters:
- `unwikilink` — strips `[[` / `]]`
- `slug` — makes a filename-friendly slug
- `year` — extracts the year from a date
- `month` — extracts the month from a date

---
## Templates
Templates live in `.notesmith/templates/` as Minijinja markdown files. Legacy `Assets/templates/*.md.j2` templates are still loaded.

```text
.notesmith/
└── templates/
    ├── generic-note.md
    ├── daily-note.md
    └── stream.md
```

Each template starts with YAML front matter metadata followed by the markdown body:

```markdown
---
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
context_queries:
  open_tasks: "SELECT text, note_path FROM v_tasks WHERE status_group = 'open' LIMIT 20"
pre_render_hook: ".notesmith/scripts/template-context.sh"
---
# {{ title }}
```

Template metadata fields:
- `name` — template identifier used in CLI and API
- `description` — optional human-readable description
- `output_path` — Minijinja expression for the output file path
- `prompts` — list of user inputs required to render the template
- `prompts[].name` — prompt identifier
- `prompts[].type` — `text` (more types may be added)
- `prompts[].required` — whether the prompt is required
- `prompts[].default` — optional default value used when the prompt is omitted
- `context_queries` — map of template variable names to read-only SQL queries; each result becomes an array of row objects
- `pre_render_hook` — optional script path, relative to the vault root; it receives the current context as JSON on stdin and returns extra context JSON on stdout

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

> **Not the same as slash-command prompts.** The chat panel's `/` slash commands
> are a separate feature whose vault overrides live in `<vault>/_prompts/` (no
> `.notesmith` prefix). See [Slash Commands & Custom Prompts](ai-slash-commands.md).

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
## Customizations (personas / skills / instructions)

The chat panel auto-discovers user-authored **custom agents (personas)**,
**skills**, and **instructions** from two scopes (ADR 0016):

| Scope   | Location                                                |
|---------|---------------------------------------------------------|
| Project | `<vault>/.notesmith/{agents,skills,instructions}/`      |
| Global  | `~/.config/notesmith/{agents,skills,instructions}/`     |

Each item is a single `*.md` file. The **file name (stem)** is its id; a project
file overrides a global file with the same id. Malformed files are skipped (the
panel still works without any customizations).

**Frontmatter:**

- **Agent (persona)** — `name`, `description`, optional `backend` (a discovered
  agent id: `copilot`/`claude`/`codex`/…), optional `model`, and optional
  `access` (`read-only` to run the persona without write access; defaults to
  `read-write`). The **body** is the persona's system/preamble prompt. A persona
  is not a separate CLI — it runs on top of one of your installed agents.
- **Skill** / **Instruction** — `name`, `description`; the body is the content.
  Discovered **instructions** are always applied to the session preamble.

```markdown
---
name: Researcher
description: Deep research assistant.
backend: copilot
model: gpt-4o
access: read-only
---
You are a meticulous researcher. Cite sources and prefer primary references.
```

**Read-only personas:** add `access: read-only` to a persona that should only
search and answer, never modify the vault. Selecting it in the chat puts the
session in read-only mode automatically; omit `access` (or use `read-write`) for
personas that can create and edit notes (each write still prompts).

**Using a persona:** pick it from the chat composer's mode dropdown, or type a
leading `@<persona-id>` mention in the composer followed by your message —
e.g. `@researcher summarize this note`. Routing is **session-switch**: the
persona stays active for the rest of the conversation until you change it. See
the [AI Chat Panel guide](ai-chat.md) for where the persona dropdown lives.

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
│   ├── agents/                 # Custom agent personas (*.md)
│   ├── skills/                 # Reusable skills (*.md)
│   ├── instructions/           # Always-on instructions (*.md)
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
