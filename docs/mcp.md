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
| `memory_recall` | `query`, `scope?`, `limit?` |
| `memory_list` | `scope?`, `status?`, `limit?` |
| `memory_save` | `title`, `claim`, `scope`, `certainty`, `description?`, `subject?`, `source?`, `confirmed?`, `supersedes?`, `tags?`, `acknowledge_inference?`, `confirm_apply?`, `preview_token?` |
| `memory_update` | `path`, `expected_hash`, `title?`, `claim?`, `description?`, `body?`, `scope?`, `subject?`, `certainty?`, `source?`, `status?`, `confirmed?`, `tags?`, `acknowledge_inference?`, `confirm_apply?`, `preview_token?` |
| `memory_supersede` | `path`, `expected_hash`, `new_title`, `new_claim`, `scope`, `certainty`, `description?`, `subject?`, `source?`, `confirmed?`, `tags?`, `acknowledge_inference?`, `confirm_apply?`, `preview_token?` |
| `memory_delete` | `path`, `expected_hash`, `confirm_delete` |
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

### `memory_recall`

Recalls **active fact notes** (`type: fact`) using the same retrieval stack as
`vault_search`: Tantivy lexical search plus embedding search fused with RRF,
with clean lexical-only fallback when embeddings are unavailable.

- `query` — required search text.
- `scope?` — when supplied, returns facts with `scope: user` plus facts whose
  `scope` exactly matches the supplied value (for example `vault:notes-method`).
  When omitted, recall searches active facts across all scopes in the current
  vault.
- `limit?` — maximum facts to return (default 20, maximum 100).

Recall excludes:

- non-fact notes;
- facts with `status: superseded` or `status: retracted`;
- facts tagged `example`;
- defensive example-path matches such as `facts/examples/...`.

The response is stable across lexical-only and hybrid modes and includes
grounding metadata: `path`, `title`, `claim`, `scope`, `certainty`, `source`,
`snippet`, `score`, overall `rank`, `lexical_rank`, `semantic_rank`, and
`char_start` / `char_end` citation offsets when available.

### Fact lifecycle tools

The specialized fact-memory tools operate on ordinary Markdown fact notes under
`facts/` (`type: fact`). They reuse the normal save pipeline, optimistic hashes,
and MCP permission preview flow — there is no separate store.

#### `memory_list`

Lists non-example fact notes in a stable structured shape.

- `scope?` — when supplied, includes `scope: user` plus the exact scope.
- `status?` — `active` (default), `superseded`, or `retracted`.
- `limit?` — default 50, maximum 100.

Each result includes `path`, current `hash`, `title`, `claim`, `description`,
`scope`, `subject`, `certainty`, `source`, `status`, `confirmed`,
`supersedes`, `tags`, `created`, and `updated`.

#### `memory_save`

Creates a new fact note under `facts/`, but **defaults to preview mode**:

- preview (`confirm_apply` omitted / false) returns similar active-fact
  candidates plus the exact proposed `path`, `content`, and `preview_token`;
- apply requires `confirm_apply: true` **and** the fresh `preview_token` from
  that preview.

Safety rules:

- exact-duplicate candidates are surfaced explicitly and block apply;
- `observed` facts require a nonblank `source`;
- `inferred` facts require `acknowledge_inference: true`;
- the tool generates its own safe `facts/...` path from `title`;
- `preview_token`s are MAC-protected, short-lived, and bound to the current
  daemon process, so a daemon restart invalidates them and callers must rerun
  preview before apply.

#### `memory_update`

Updates an existing fact note. It always requires `expected_hash` from a fresh
read or list result; stale hashes fail with the same write-conflict convention
used elsewhere.

- claim-changing updates preview similar active facts before writes;
- preview/apply uses the same `confirm_apply` + `preview_token` contract as
  `memory_save`;
- changing only `claim` preserves the existing `description` unless a new
  `description` is explicitly supplied;
- unknown frontmatter and unrelated body content are preserved unless `body`
  is explicitly supplied as replacement content;
- mutation rejects notes whose current type is not `fact`.

#### `memory_supersede`

Replaces an active fact with a new fact note:

- preview returns the proposed replacement note and a fresh `preview_token`;
- apply requires `confirm_apply: true`, the `preview_token`, and the old
  fact's `expected_hash`;
- `preview_token`s are process-bound and must be regenerated after daemon
  restart;
- the old fact is marked `status: superseded`, the new fact gets
  `supersedes: [[Old Title]]`, and both note bodies are linked.

Companion-vault attachment is still future work; these tools operate only in
the currently mounted vault.

#### `memory_delete`

Hard-deletes a fact note for mistakes or sensitive material only.

- requires `confirm_delete: true`;
- requires fresh `expected_hash`;
- rejects example facts (`facts/examples/...` or `example`-tagged notes).

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
