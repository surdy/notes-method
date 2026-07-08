# Notesmith MCP Adapter

Notesmith exposes an MCP server in two ways:

**1. Hosted by the daemon over HTTP/SSE** — when the daemon is running it mounts a streamable-HTTP MCP endpoint per vault, reusing the daemon's live indexes:

| Endpoint | Capabilities |
|----------|--------------|
| `POST/GET /mcp/{vault}` | Full read and write access |
| `POST/GET /mcp-ro/{vault}` | Read-only (write tools rejected) |

See [`docs/http-api.md`](http-api.md#agent-access-mcp-over-http) for connection details, reverse-proxy/TLS guidance, and the read-only model.

**2. Over stdio** (for stdio-only clients such as Claude Desktop):

```bash
notesmith [--url <daemon-url>] mcp start [--vault <name>] [--read-only]
```

`mcp start` is a **stdio↔HTTP bridge**, not an embedded server. It resolves a
daemon base URL (the global `--url` / `NOTESMITH_URL` when set, otherwise the
local daemon, auto-started on demand), connects to that daemon's `/mcp/{vault}`
endpoint (or `/mcp-ro/{vault}` with `--read-only`), and transparently forwards
every stdio request to it. This means stdio and HTTP clients always share the
daemon's live indexes and the same operation logic (`notesmith-ops`) — there is
no separate in-memory index path to drift.

Both transports expose the same tools and resources.

The MCP operations wrap the existing vault engine, SQLite cache, search index, routing engine, task toggling, daily note creation, and template instantiation.

## Tools

| Tool | Parameters |
|------|------------|
| `create_note` | `title`, `content?`, `folder?`, `frontmatter?` |
| `get_note` | `path` |
| `update_note` | `path`, `content` |
| `append_to_note` | `path`, `content` |
| `archive_note` | `path` |
| `search_notes` | `query`, `limit?` |
| `vault_search` | `query`, `limit?` |
| `query_sql` | `sql` |
| `time_query` | `when`, `date_field?` (`mtime`\|`updated`\|`created`), `query?`, `limit?` |
| `list_notes` | `type?`, `customer?`, `archived?` |
| `list_tasks` | `status?`, `customer?` |
| `update_task_status` | `note_path`, `task_hash`, `status` |
| `capture` | `content`, `title?` |
| `create_daily_note` | `date?` (`YYYY-MM-DD`) |
| `create_from_template` | `template_name`, `prompts?` |

## Resources

| Resource URI | Description |
|--------------|-------------|
| `note:///{vault-path}` | Read an individual note |
| `note:///daily/{date}` | Read a daily note by date |
| `note:///vault/structure` | List all note paths in the vault |

### `vault_search` vs `search_notes`

`search_notes` is pure lexical (Tantivy/BM25) full-text search. `vault_search`
is **hybrid**: it blends the lexical ranking with semantic (embedding) similarity
using Reciprocal Rank Fusion (RRF, ADR 0018 §8) and returns note references with
a `path` and `snippet` for grounding/citation, plus the `lexical_rank` and
`semantic_rank` that contributed each hit. Until the embed worker has produced a
vault's `embeddings.db`, `vault_search` transparently degrades to lexical-only.

> **Cloud embeddings** (higher-quality retrieval via a hosted model) are a
> planned config override and are tracked separately as deferred work; the
> default is the local embedding model.

See [Semantic & Hybrid Search](ai-semantic-search.md) for the user-facing guide
and [Embeddings: Operating & Monitoring](embeddings-operations.md) for running
the worker, enabling local vectors, and monitoring.

### `time_query`

Turns a natural-language time expression into a date range and returns note
references dated within it — pairs with `vault_search` so an agent can cite
real, dated notes.

- `when` — the expression, e.g. `last week`, `yesterday`, `in May`, `May 2021`,
  `this month`, `last 3 days`, `2021`.
- `date_field?` — which indexed date to filter on: `mtime` (file modification
  time, default), `updated` (frontmatter `updated`, falling back to mtime), or
  `created` (frontmatter `created`).
- `query?` — optional keyword to further restrict results (title/body).
- `limit?` — maximum notes to return (default 50).

Periodic notes (daily/weekly/monthly/…) are always included when their period
overlaps the range, regardless of `date_field`, so "in May" surfaces May's
daily notes even when their file mtime is later. Each result carries a `source`
of `note` or `periodic`. The response also echoes the resolved `range_start` /
`range_end` and total `match_count`. This tool is embedding-independent.


## Claude Desktop example

```json
{
  "mcpServers": {
    "notesmith": {
      "command": "notesmith",
      "args": ["mcp", "start", "--vault", "work"]
    }
  }
}
```
