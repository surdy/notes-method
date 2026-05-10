# Notesmith CLI Reference

Notesmith ships a single binary: **`notesmith`**.

```
notesmith [--vault <name|path>] [--format text|json] <command>
```

**Global flags:**

| Flag | Description | Default |
|------|-------------|---------|
| `--vault <name\|path>` | Override vault detection (name from config or path) | auto-detect |
| `--format text\|json` | Output format | `text` (JSON when piped) |

---

## daemon

### `daemon start`

Start the Notesmith HTTP daemon. Loads all configured vaults, builds caches, and starts the Axum server.

```bash
notesmith daemon start [--bind 127.0.0.1:27183]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--bind <addr>` | Bind address for the HTTP server | `127.0.0.1:27183` |

The daemon indexes all registered vaults on startup and watches for file changes.

---

## mcp

### `mcp start`

Start the Notesmith MCP server over stdio for local MCP clients such as Claude Desktop.

```bash
notesmith mcp start [--vault <name>]
```

The command detects the target vault, scans and indexes it in memory, then serves MCP tools and resources over standard input/output.

---

## vault

### `vault list`

List all registered vaults from `~/.config/notesmith/config.toml`.

```bash
notesmith vault list
```

### `vault detect`

Show which vault would be selected for the current directory.

```bash
notesmith vault detect
```

Detection order:
1. Walk upward from `$PWD` looking for `.notesmith/vault.toml`
2. Honor `--vault <name>` flag
3. Fall back to default vault from global config

### `vault info`

Show vault configuration summary (name, root, inbox/daily/editor/git settings).

```bash
notesmith vault info
```

### `vault reindex`

Rebuild the SQLite cache and Tantivy search index from scratch.

```bash
notesmith vault reindex
```

Output: `Reindexed 42 notes for work into ~/.cache/notesmith/work/cache.sqlite`

---

## query

### `query sql`

Execute read-only SQL against the daemon's SQLite cache. Requires the daemon to be running.

```bash
notesmith query sql "SELECT title, type FROM v_notes LIMIT 10"
```

Text output renders a formatted table. JSON output returns the full `QueryResult` object.

**Examples:**

```bash
# List active customers
notesmith query sql "SELECT title, state FROM v_customers WHERE state = 'Active'"

# Find blocked tasks
notesmith query sql "SELECT text, note_path FROM v_tasks WHERE status = 'blocked'" --format json | jq '.'

# Count notes by type
notesmith query sql "SELECT type, COUNT(*) as count FROM v_notes GROUP BY type ORDER BY count DESC"
```

---

## note

Note CRUD commands go through the running daemon and operate on the detected vault.

### `note create`

Create a note in `Inbox/` by default.

```bash
notesmith note create "Follow Up" [--folder Customers/Acme] [--content "Body text"]
```

### `note get`

Fetch a note by vault-relative path.

```bash
notesmith note get Inbox/Follow\ Up.md
```

Text output prints just the note body. JSON output prints the full HTTP note payload, including frontmatter, links, tasks, and hash.

### `note put`

Replace a note's content.

```bash
notesmith note put Inbox/Follow\ Up.md --content "# Replaced"
printf '# Replaced from stdin\n' | notesmith note put Inbox/Follow\ Up.md --from-stdin
```

### `note append`

Append content to an existing note.

```bash
notesmith note append Inbox/Follow\ Up.md "Next line"
```

### `note delete`

Delete a note.

```bash
notesmith note delete Inbox/Follow\ Up.md
```

### `note move`

Move a note to a new vault-relative path.

```bash
notesmith note move Inbox/Follow\ Up.md Customers/Acme/Follow\ Up.md
```

All create/put/append writes run through the save pipeline, which trims trailing whitespace, normalizes the trailing newline, and auto-maintains `created`/`updated` frontmatter fields when frontmatter is present.

---

## inbox

Inbox quick-capture commands go through the running daemon.

### `inbox add`

Quick-capture a note to the inbox folder. Generates a timestamped filename.

```bash
notesmith inbox add "<text>" [--title <title>]
```

| Arg/Flag | Description |
|----------|-------------|
| `<text>` | Note body content |
| `--title <title>` | Optional title used in filename slug |

**Filename format:** `Inbox/{YYYY-MM-DD HH-MM-SS} - {slug}.md`

The slug is derived from `--title` if provided, otherwise from the first 40 characters of the text (sanitized to keep alphanumeric, spaces, and hyphens).

**Examples:**

```bash
notesmith inbox add "Call Sarah about the project"
notesmith inbox add "Meeting notes from standup" --title "Standup Notes"
notesmith inbox add "Quick thought" --format json
```

### `inbox list`

List unarchived notes in the inbox folder.

```bash
notesmith inbox list
```

Returns up to 100 notes sorted by path descending (newest first). Text output shows `path  title` per line.

**Examples:**

```bash
notesmith inbox list
notesmith inbox list --format json | jq '.[].path'
```

---

## task

Task commands go through the running daemon and operate on the detected vault.

### `task list`

List tasks from the vault with optional filters.

```bash
notesmith task list [--status <status>] [--customer <name>] [--due-before <YYYY-MM-DD>] [--limit N]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--status` | Filter by status (`todo`, `in_progress`, `blocked`, `waiting`, `on_hold`, `done`, `cancelled`) | all |
| `--customer` | Filter by customer name | all |
| `--due-before` | Only tasks due before this date | none |
| `--limit N` | Maximum results | 200 |

Text output shows `[marker] text  📅 due  (note_path)` for each task.

**Examples:**

```bash
notesmith task list
notesmith task list --status todo --customer Acme
notesmith task list --due-before 2025-02-01 --format json | jq '.[].text'
```

### `task add`

Add a new To Do task to an existing note.

```bash
notesmith task add <note_path> <description> [--customer <name>] [--stream <name>] [--due <YYYY-MM-DD>] [--priority <level>]
```

| Arg/Flag | Description |
|----------|-------------|
| `note_path` | Vault-relative path to the note |
| `description` | Task text |
| `--customer <name>` | Inline field `[customer:: name]` |
| `--stream <name>` | Inline field `[stream:: name]` |
| `--due <YYYY-MM-DD>` | Due date emoji 📅 |
| `--priority <level>` | `highest`, `high`, `medium`, `low`, or `lowest` |

**Examples:**

```bash
notesmith task add "Customers/Acme/Acme Corp.md" "Follow up on SLA requirements" --customer Acme --due 2025-02-01
notesmith task add Inbox/Daily/2025-01-15.md "Review pull requests" --priority high
```

### `task toggle`

Toggle a task to a new status using its content hash. The hash uniquely identifies the task line and is returned by `note get` or `task list`.

```bash
notesmith task toggle <note_path> <task_hash> <new_status>
```

**Status transitions** (from the notes method):

| From | Allowed next states |
|------|---------------------|
| `todo` | `in_progress`, `blocked`, `waiting`, `on_hold`, `done` |
| `in_progress` | `done`, `blocked`, `waiting`, `on_hold` |
| `blocked` | `todo`, `in_progress`, `done` |
| `waiting` | `todo`, `in_progress`, `done` |
| `on_hold` | `todo`, `in_progress`, `done` |
| `done` | `todo` |
| `cancelled` | `todo` |

Returns `404` if the hash is not found, `409` if it matches more than one task, `422` if the transition is not allowed.

### `task set-status`

Alias for `task toggle` — explicitly set a task's status.

```bash
notesmith task set-status <note_path> <task_hash> <new_status>
```

---

## search

Full-text search across note titles and body content. Requires the daemon to be running.

```bash
notesmith search <terms...> [--limit N]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--limit <N>` | Maximum results | 20 |

**Examples:**

```bash
notesmith search Acme onboarding
notesmith search SSO --limit 5 --format json
```

Text output shows path, title, score, and a context snippet for each result.

---

## template

### `template list`

List available templates with their prompt schemas.

```bash
notesmith template list
```

Text output shows template name and description. JSON output returns the full template metadata including prompts.

### `template render <name> [--prompt KEY=VALUE ...]`

Render a template to stdout without creating a file.

```bash
notesmith template render generic-note --prompt title="Hello World"
```

Text output prints just the rendered content. JSON output returns `{ path, content }`.

### `template instantiate <name> [--prompt KEY=VALUE ...]`

Render and create the note at the computed output path.

```bash
notesmith template instantiate external-meeting --prompt customer=Acme --prompt title="Q2 Check-in"
```

| Flag | Description |
|------|-------------|
| `--prompt KEY=VALUE` | Supply a prompt value (repeatable) |

**Available templates:**

| Name | Description |
|------|-------------|
| `generic-note` | A generic blank note |
| `daily-note` | Daily note for today |
| `external-meeting` | External customer meeting note |
| `internal-meeting` | Internal team meeting about a customer |
| `account-info` | Account information for a customer |
| `customer-index` | Top-level customer index note |
| `glossary` | Glossary of terms for a customer |
| `milestones` | Dates and milestones for a customer |
| `stream` | Customer stream or initiative |

**Examples:**

```bash
notesmith template list
notesmith template render daily-note
notesmith template instantiate stream --prompt customer=Acme --prompt title="Migration to v2"
notesmith template instantiate account-info --prompt customer="Globex Industries" --format json
```

---

## route

### `route preview`

Preview where a note would be routed without moving it.

```bash
notesmith route preview <path>
```

| Arg | Description |
|-----|-------------|
| `path` | Vault-relative path to the note |

Text output shows `source -> destination (rule: rule_id)`.

**Examples:**

```bash
notesmith route preview "Inbox/standup.md"
notesmith route preview "Inbox/idea.md" --format json
```

### `route apply`

Apply routing to move note(s) to their destination folder. Stamps `archived: true` and `archived-at` in frontmatter before moving.

```bash
notesmith route apply <path>
notesmith route apply --inbox
```

| Arg/Flag | Description |
|----------|-------------|
| `path` | Route a single note by vault-relative path |
| `--inbox` | Route all eligible notes in the inbox folder |

One of `path` or `--inbox` is required.

**Examples:**

```bash
# Route a single note
notesmith route apply "Inbox/standup.md"

# Route all inbox notes
notesmith route apply --inbox

# Route with JSON output
notesmith route apply --inbox --format json
```

---

## daily

### `daily ensure [--date YYYY-MM-DD]`

Create a daily note for the given date (defaults to today) if it doesn't exist. Uses the configured `daily-note` template.

```bash
notesmith daily ensure
notesmith daily ensure --date 2025-06-15
```

| Flag | Description | Default |
|------|-------------|---------|
| `--date <YYYY-MM-DD>` | Date for the daily note | today |

**Examples:**

```bash
notesmith daily ensure
notesmith daily ensure --date 2025-01-15
notesmith daily ensure --format json
```

### `daily open [--date YYYY-MM-DD]`

Open a daily note for the given date (defaults to today). Creates it if missing, then displays the content.

```bash
notesmith daily open
notesmith daily open --date 2025-06-15
```

| Flag | Description | Default |
|------|-------------|---------|
| `--date <YYYY-MM-DD>` | Date for the daily note | today |

**Examples:**

```bash
notesmith daily open
notesmith daily open --date 2025-01-15 --format json
```

### `daily agent-create [--date YYYY-MM-DD] [--content "..."]`

Agent-oriented daily note workflow. Without `--content`, the daemon assembles and returns the saved prompt template from `.notesmith/prompts/daily-note.md`. With `--content`, the daemon writes that pre-generated content as the day's daily note and rejects conflicts if the note already exists.

```bash
notesmith daily agent-create
notesmith daily agent-create --date 2025-06-15
notesmith daily agent-create --date 2025-06-15 --content "---\ntype: daily\ndate: 2025-06-15\n---\n# 2025-06-15"
```

| Flag | Description | Default |
|------|-------------|---------|
| `--date <YYYY-MM-DD>` | Date for the daily note or prompt assembly | today |
| `--content <markdown>` | Write pre-generated content instead of printing a prompt | prompt mode |

---

## skill

### `skill print`

Print the detected vault's `.notesmith/skill.md` file so agents can load vault-specific operating instructions.

```bash
notesmith skill print
notesmith --format json skill print
```
