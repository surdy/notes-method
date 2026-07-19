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
| `list_notes` | `type?`, `fields?` (key → exact value; list fields match by membership), `archived?` |
| `list_tasks` | `status?`, `fields?` (effective fields: task inline fields override the containing note's frontmatter per key) |
| `vault_stats` | `top?` |
| `update_task_status` | `note_path`, `task_hash`, `status` |
| `capture` | `content`, `title?` |
| `create_daily_note` | `date?` (`YYYY-MM-DD`) |
| `create_from_template` | `template_name`, `prompts?` |
| `youtube_transcript` | `url` |
| `read_document` | `path`, `save?`, `folder?` |

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

Each lifecycle call operates in the vault served by that MCP binding. Embedded
chat can attach a configured companion memory vault beside the active vault, so
agents invoke these tools on the companion binding when managing memory.

#### `memory_delete`

Hard-deletes a fact note for mistakes or sensitive material only.

- requires `confirm_delete: true`;
- requires fresh `expected_hash`;
- rejects example facts (`facts/examples/...` or `example`-tagged notes).

### `list_notes` / `list_tasks` field filters

Both tools take an optional `fields` object mapping field keys to **exact**
values. Multiple keys AND together. A list-valued field (e.g. `customers`)
matches when any member equals the value — backed by the normalized
`v_field_values` index, so `{"customers": "[[Acme]]"}` finds every note
involving Acme without substring false positives.

`list_tasks` matches against **effective** task fields: a task inherits its
containing note's frontmatter, and a task-level inline field (e.g.
`[customers:: [[Solo]]]`) overrides the inherited value for that key. Each
returned task carries a `fields` object of its effective values (arrays, since
list fields can contribute several values) plus a `due` convenience column.
The same shape is queryable in SQL via the `v_task_effective_fields` view.

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

### `vault_stats`

Summarises the vault's structure from the note index so an agent can reason
about its shape for PKM/cleanup. Embedding-independent.

- `top?` — how many rows each ranked list returns (default 20, capped at 200).

The response contains:

- `totals` — `notes`, `tags` (distinct), `links` (resolved note→note edges),
  `tasks`, `words`, and `orphans`.
- `tags` — the most-used tags with their `note_count`.
- `backlinks` — the most-linked-to notes (`path`, `title`, `backlink_count`).
- `orphans` — notes with no resolved incoming or outgoing links (`path`,
  `title`).

Wikilink targets are resolved to notes by title (which defaults to the filename
stem) or path, so `[[Some Note]]` counts toward that note's backlinks.


### `youtube_transcript`

Fetches the published caption transcript for a YouTube URL via the captions
API and returns the transcript text with timestamps. It is a thin wrapper over
the shared YouTube source module in `notesmith-clip`
([ADR 0020](adr/0020-web-clipper.md) §8.4) — a single SSRF-guarded, bounded
`GET`. The tool never transcribes audio: videos without a published caption
track return a clear, non-fatal `no_captions` result rather than an error
(ADR 0019 §4 / ADR 0020 §8.3).

- `url` — a YouTube video URL (`youtube.com/watch?v=…`, `youtu.be/…`, etc.).

Response when captions exist:

- `status` — `"captions"`.
- `source_url`, `video_id`, `title`, `channel`, `published`, `duration` —
  provenance metadata (some fields may be `null` when the source omits them).
- `text` — the joined transcript, one `[m:ss] text` line per segment.
- `segments` — an array of `{ start, end, text }` (seconds).

Response when no captions are available:

- `status` — `"no_captions"`.
- `source_url`, `video_id`, and a `message` explaining the video has no
  published captions.

Invalid URLs, blocked (SSRF-guarded) targets, and fetch failures surface as a
tool error.

Example:

```json
{
  "name": "youtube_transcript",
  "arguments": { "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ" }
}
```

### `read_document`

Extracts text from a PDF or EPUB stored in the vault (referenced by its
vault-relative `path`) into plain text plus fixed-size chunks and provenance
metadata. It is a thin wrapper over the pure-Rust `notesmith-document` parser
([ADR 0019](adr/0019-media-ingestion-pipeline.md) §PDF/EPUB): `pdf-extract` for
PDF pages and `epub` + `htmd` for EPUB chapters. There is **no OCR** — an
image-only/scanned PDF extracts little or no text and returns a non-fatal
"no extractable text" error rather than crashing. Parsing is panic-isolated per
document (ADR 0009), so a malformed or encrypted file returns a typed error.

- `path` — vault-relative path to a `.pdf` or `.epub` file (traversal outside
  the vault is rejected).
- `save` — when `true`, also write a normalized note (default `false`).
- `folder` — vault folder for the saved note (default `attachments`).

Response:

- `source_path`, `source_type` (`"pdf"`\|`"epub"`), `title`, `author` —
  provenance metadata (`title`/`author` may be `null`).
- `unit_label` (`"page"`\|`"chapter"`), `unit_count` — structural units.
- `text` — the full extracted, normalized text.
- `chunks` — an array of `{ index, char_start, char_end, text }`.
- `chunk_count` — number of chunks.
- `frontmatter`, `body`, `note_markdown` — the normalized note, split into
  structured frontmatter + body and the fully rendered markdown.
- `saved`, `saved_path` — present only when `save: true`; the created note path.

Missing files, paths outside the vault, unsupported extensions, and malformed
or encrypted documents surface as a tool error.

Example:

```json
{
  "name": "read_document",
  "arguments": { "path": "attachments/paper.pdf", "save": true }
}
```


## Structuring a transcript note (agent workflow)

Transcription (audio/YouTube → note) produces a **timestamped transcript note**
only — a `source_type: youtube|podcast|audio` frontmatter block plus a body of
`H:MM:SS` segments ([ADR 0023](adr/0023-local-whisper-transcription-worker.md)
§7). Notesmith deliberately does **not** summarize the transcript, extract action
items, or pull out decisions itself: per
[ADR 0015](adr/0015-ai-agent-integration-roadmap.md) Option A the daemon runs no
chat LLM. The "structured note (summary + action items + decisions)" outcome of
[#204](https://github.com/surdy/notes-method/issues/204) is produced by **the
user's ACP agent** calling the MCP tools already listed above over the transcript
note — there is no Notesmith-side model or endpoint for it.

A typical agent pass:

1. **Find the transcript note.** `search_notes`/`vault_search` for the topic, or
   `list_notes` / `query_sql` filtered to the vault's `transcribed/` folder
   (`[transcribe] notes_dir`). Recently drained items surface via `time_query`.
2. **Read it.** `get_note` returns the frontmatter (provenance: `source_url`,
   `title`, `channel`, `published`, `duration`) and the timestamped body. The
   agent summarizes/extracts in its **own** context — Notesmith just serves text.
3. **Write the structured note.** `create_note` (e.g. under `summaries/`) with a
   Summary / Action items / Decisions body, linking back to the transcript via a
   `[[wikilink]]` and carrying the same `source_url` for provenance. To enrich
   the transcript in place instead, `append_to_note` (add a summary section) or
   `update_note`.
4. **Track action items as tasks.** Write action items as Markdown task lines
   (`- [ ] …`) in the structured note; the indexer picks them up so `list_tasks`
   and `update_task_status` manage them like any other task.

The agent chooses and sequences these tools; Notesmith adds no transcript-specific
tool for this step — structuring is composition over the existing surface.

## Vault lint / knowledge-health (agent workflow)

The third Karpathy "LLM Wiki" workflow (after Ingest and Query) is **Lint**: a
repeatable health pass that surfaces contradictions, stale claims, orphan notes,
missing cross-links, and concepts referenced but never written up. Per
[ADR 0015](adr/0015-ai-agent-integration-roadmap.md) the daemon runs no LLM, so
the *reasoning* is done by the user's ACP agent — Notesmith's job is to expose
the raw, read-only **signals** the agent composes into findings (issue #265).

### Lint signal inventory

| Signal | Provided by | How |
|--------|-------------|-----|
| **Orphan notes** (no inbound or outbound resolved links) | `vault_stats` | `orphans` array + `totals.orphans` count |
| **Dangling links / concepts with no note** | `query_sql` on `v_dangling_links` | Wikilink targets that resolve to no note; group by `raw_target` to rank missing concepts |
| **Missing cross-links / near-duplicates** | `vault_search` | Semantic+lexical neighbours of a note that aren't already linked (candidate `[[wikilinks]]`) |
| **Stale claims** (source changed after the note) | `query_sql` on `v_fields` + `time_query` | Compare provenance `source_mtime`/`source_hash` fields against note `updated`; ingest re-writes provenance on change ([ADR 0022](adr/0022-local-drop-folder-ingestion.md)) |
| **Structure / hotspots** | `vault_stats` | `backlinks` (most-referenced), `tags`, totals |

### A typical lint pass

1. **Snapshot health.** `vault_stats` for orphans, top backlinks, tags, totals.
2. **Find missing concepts.** `query_sql`:
   ```sql
   SELECT raw_target, COUNT(*) AS refs
   FROM v_dangling_links
   GROUP BY raw_target ORDER BY refs DESC, raw_target;
   ```
   High-`refs` targets are concepts worth creating a note for.
3. **Find missing cross-links.** For key notes, `vault_search` the title/topic and
   flag strong neighbours not already in `v_backlinks` — candidate links.
4. **Flag stale provenance.** `query_sql` over `v_fields` for ingested notes whose
   `source_*` provenance post-dates the note body, corroborated with `time_query`.
5. **Report, don't edit.** The agent returns a findings list (orphans, dangling
   targets, suggested links, stale notes). **Lint is read-only by default** —
   creating notes, adding `[[wikilinks]]`, or resolving staleness is a separate,
   explicitly-invoked step using `create_note` / `append_to_note` / `update_note`.

Lint adds no dedicated tool: every signal is a read-only view or existing tool,
so an operator runs the whole pass over MCP via `query_sql`, `vault_stats`, and
`vault_search`.

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
