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
