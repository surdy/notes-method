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
