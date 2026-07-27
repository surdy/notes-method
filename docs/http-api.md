# Notesmith HTTP API Reference

The daemon listens on `127.0.0.1:27183` by default (configurable via `--bind` or `daemon.bind` in config).

All API endpoints are unauthenticated — the daemon is designed for local use.

## Version Negotiation

All daemon API and admin responses include compatibility headers:

- `X-Notesmith-Server-Version` — daemon package version (for example `0.1.0`)
- `X-Notesmith-Schema-Version` — API schema version integer (currently `1`)

Rich clients should send `X-Notesmith-Client-Version` on requests and compare the returned server version/schema before assuming compatibility.

---

## Health

### `GET /api/status`

Rich daemon status for resilient clients and diagnostics.

**Response:** `200 OK`
```json
{
  "status": "ok",
  "version": "0.1.0",
  "api_schema": 1,
  "pid": 12345,
  "started_at": "2026-05-14T19:00:00Z",
  "binary_path": "/path/to/notesmith",
  "vaults": [{
    "name": "work",
    "state": "ready",
    "notes": 421,
    "parse_warning_count": 1,
    "parse_warnings_truncated": false,
    "parse_warnings": [
      {
        "path": "Inbox/Bad.md",
        "stage": "frontmatter",
        "reason": "mapping values are not allowed in this context at line 2 column 8",
        "occurred_at": "2026-05-14T19:00:00Z"
      }
    ]
  }],
  "watchers": [{ "vault": "work", "state": "healthy", "message": null }],
  "indexes": [{ "vault": "work", "state": "healthy", "last_reindex": "2026-05-14T19:00:00Z" }],
  "resources": {
    "rss_bytes": 52428800,
    "open_fds": 47,
    "sse_connections": 2,
    "cache_size_bytes": 1048576
  }
}
```

**Notes:**
- `api_schema` is the daemon compatibility contract version.
- `vaults[*].state` is `"ready"` or `"rebuilding"`.
- `vaults[*].parse_warning_count` is the number of notes in the vault that parsed in a degraded way (e.g. valid `---` frontmatter delimiters wrapping invalid YAML, which is silently dropped during indexing).
- `vaults[*].parse_warnings` lists up to 100 of those notes. Each entry has `path` (vault-relative), `stage` (currently `"frontmatter"`), `reason` (the parser error), and `occurred_at`.
- `vaults[*].parse_warnings_truncated` is `true` when more than 100 notes have warnings and the list was capped.
- `watchers[*].state` is `"healthy"`, `"degraded"`, or `"polling"`.
- `watchers[*].message` is present when the daemon has an operator hint (for example, a network-drive warning or automatic resync message).
- `indexes[*].state` currently returns `"healthy"`.
- `last_reindex` is derived from the cache artifact timestamp when available.

**Example:**
```bash
curl http://127.0.0.1:27183/api/status
```

### `GET /ping`

Lightweight health check alias.

**Response:** `200 OK`
```json
{ "status": "ok" }
```

### `GET /admin/logs`

Returns recent daemon log lines as plain text.

**Query parameters:**
- `tail` (optional, default: `200`, max: `1000`) — number of lines to return

**Response:** `200 OK` with `text/plain`

**Errors:**
- `404` — no daemon log file exists yet
- `500` — daemon log file could not be read

**Example:**
```bash
curl http://127.0.0.1:27183/admin/logs?tail=100
```

---

## Capabilities

### `GET /api/capabilities`

Returns server capabilities and deployment mode. Used by the frontend to determine which settings are available.

**Response:** `200 OK`
```json
{
  "deployment_mode": "desktop",
  "can_edit_global_config": true,
  "can_edit_vault_config": true,
  "can_open_local_paths": true,
  "restart_required_fields": ["daemon.bind"],
  "folder_picker": false,
  "vaults_root": null,
  "embeddings": {
    "compiled_in": false,
    "model": "bge-small-en-v1.5",
    "dim": 384
  },
  "transcription": {
    "compiled_in": false
  }
}
```

The `embeddings` block advertises process-global facts only: `compiled_in` is
`true` when the daemon was built with the `local-embed` feature (embed-capable),
and `model`/`dim` name the model an embed-capable build uses. Whether embeddings
are actually *on* is **per vault** — read `embed.enabled` from
`GET /api/v/{vault}/config`, not from here (ADR 0018 §9.3).

The `transcription` block is likewise process-global: `compiled_in` is `true`
when the daemon was built with the `local-whisper` feature (whisper.cpp
compiled in). Whether transcription is actually *on* is **per vault** — read
`transcribe.enabled` from `GET /api/v/{vault}/config`, not from here (ADR 0023).

**Example:**
```bash
curl http://127.0.0.1:27183/api/capabilities
```

---

## Vault Config

### `GET /api/v/{vault}/config`

Read the vault configuration from `.notesmith/vault.toml`. Returns the parsed config, a blake3 hash for ETag-based conflict detection, and any validation warnings. Older supported config schemas are migrated on load before the response is returned.

The response includes an `ETag` header with the config hash for use with `PUT` requests.

**Response:** `200 OK`
```json
{
  "config": {
    "schema_version": 1,
    "name": "work",
    "capture": { "folder": "", "template": "generic-note" },
    "daily": { "folder": "", "template": "daily-note", "generate_at": "06:00", "timezone": "America/Los_Angeles", "catch_up": false },
    "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
    "git": { "enabled": true, "auto_commit_every": "5m", "auto_pull_every": "10m", "auto_push_every": "10m" },
    "hooks": {}
  },
  "hash": "a1b2c3d4...",
  "path": ".notesmith/vault.toml",
  "warnings": {}
}
```

**Headers:**
- `ETag: "a1b2c3d4..."` — blake3 hash of the raw TOML file

**Errors:**
- `404` — vault not found
- `500` — config file missing or unreadable

**Example:**
```bash
curl -i http://127.0.0.1:27183/api/v/work/config
```

### `PUT /api/v/{vault}/config`

Update the vault configuration. Requires an `If-Match` header with the current config hash (from a prior `GET`) for optimistic concurrency control. The `WriteGuard` extractor checks the `Origin` header — only `tauri://localhost`, `notesmith-app://localhost`, `http://notesmith-app.localhost`, `https://notesmith-app.localhost`, `http://localhost`, and `http://127.0.0.1` origins are allowed for writes. Requests with no `Origin` header (curl, CLI) are permitted.

**Request headers:**
- `If-Match: "a1b2c3d4..."` — required, config hash from prior GET
- `Origin` — optional, checked against allowed origins

**Request body:** Full `VaultConfig` object:
```json
{
  "schema_version": 1,
  "name": "work",
  "capture": { "folder": "", "template": "generic-note" },
  "daily": { "folder": "", "template": "daily-note", "catch_up": false },
  "editor": { "live_preview": true, "default_mode": "source", "strict_line_breaks": false, "show_line_numbers": true, "hide_duplicate_h1": true, "paste_url_image_whitelist": "" },
  "git": { "enabled": false },
  "hooks": {}
}
```

**Response:** `200 OK`
```json
{
  "config": { "..." },
  "hash": "e5f6a7b8...",
  "path": ".notesmith/vault.toml",
  "warnings": {}
}
```

**Headers:**
- `ETag: "e5f6a7b8..."` — new hash after write

**Validation rules:**
- `daily.generate_at` must be `HH:MM` format (00:00–23:59)
- `daily.timezone` must be a valid IANA timezone
- `git.auto_commit_every`, `git.auto_pull_every`, `git.auto_push_every` must be duration strings like `5m`, `1h`, `30s`
- Missing capture/daily folders produce warnings (non-blocking)

**Errors:**
- `403` — origin not allowed (cross-origin write attempt)
- `404` — vault not found
- `409` — conflict (config was modified externally since your GET; response includes current config and hash)
- `422` — validation failed (response includes `errors` map)
- `428` — `If-Match` header missing

**Example:**
```bash
# 1. GET current config and hash
RESPONSE=$(curl -s http://127.0.0.1:27183/api/v/work/config)
HASH=$(echo "$RESPONSE" | jq -r .hash)

# 2. PUT with If-Match
curl -X PUT http://127.0.0.1:27183/api/v/work/config \
  -H "Content-Type: application/json" \
  -H "If-Match: \"$HASH\"" \
  -d '{"schema_version":1,"name":"work","capture":{"folder":"","template":"generic-note"},"daily":{"folder":"","template":"daily-note","catch_up":false},"editor":{"live_preview":true,"default_mode":"source","strict_line_breaks":false,"show_line_numbers":true,"hide_duplicate_h1":true,"paste_url_image_whitelist":""},"git":{"enabled":false},"hooks":{}}'
```

## Field Registry

### `GET /api/v/{vault}/fields`

Read `.notesmith/fields.toml` for a vault and return the parsed registry as JSON.

**Response:** `200 OK`
```json
{
  "version": 1,
  "fields": {
    "status": {
      "type": "enum",
      "description": "Customer status",
      "values": ["active", "paused", "closed"]
    },
    "customer": {
      "type": "string",
      "suggest_from": "SELECT DISTINCT value FROM v_fields WHERE key = 'customer' ORDER BY value"
    }
  }
}
```

**Errors:**
- `404` — vault not found

**Example:**
```bash
curl http://127.0.0.1:27183/api/v/work/fields
```

### `GET /api/v/{vault}/fields/{key}/suggest?q=partial`

Return autocomplete suggestions for a registered field.

Behavior:
- If the field defines `values`, the response returns values whose prefix matches `q`.
- If the field defines `suggest_from`, Notesmith runs the SQL against the vault cache and returns the first column from matching rows.
- If the field is unknown or has no suggestion source, the response is an empty array.

**Response:** `200 OK`
```json
["paused"]
```

**Errors:**
- `404` — vault not found

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/fields/status/suggest?q=pa"
```

### `GET /api/v/{vault}/notes`

List all notes in a vault from the SQLite cache.

**Response:** `200 OK`
```json
[
  {
    "path": "Customers/Acme Corp/Acme Corp.md",
    "title": "Acme Corp",
    "type": "customer",
    "customer": "Acme Corp",
    "stream": null,
    "state": "Active",
    "status": null,
    "date": null,
    "created_at": "2026-01-15",
    "updated_at": "2026-04-01",
    "archived": false,
    "mtime_unix": 0,
    "frontmatter": { "..." }
  }
]
```

### `GET /api/v/{vault}/notes/{path...}`

Fetch a single note with full metadata (body, links, tasks, inline fields, blocks).

**Example:**
```bash
curl http://127.0.0.1:27183/api/v/work/notes/Customers/Acme%20Corp/Acme%20Corp.md
```

**Response:** `200 OK` — full `Note` object including `frontmatter`, `body`, `tasks`, `links`, `inline_fields`, `blocks`, and `hash`.

**Errors:**
- `404` — vault or note not found

### `GET /api/v/{vault}/html/{path...}`

Render a single note body to HTML using the daemon's markdown renderer. YAML frontmatter is stripped before rendering.

**Query parameters:**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `inline_styles` | When `true`, returns a complete HTML document with embedded CSS and plain `href` links for portable clipboard/email pasting. | `false` |

**Example:**
```bash
curl http://127.0.0.1:27183/api/v/work/html/Customers/Acme%20Corp/Acme%20Corp.md

# Portable HTML for clipboard/email paste
curl "http://127.0.0.1:27183/api/v/work/html/Customers/Acme%20Corp/Acme%20Corp.md?inline_styles=true"
```

**Response:** `200 OK` — HTML markup as `text/html`

**Errors:**
- `404` — vault or note not found

### `POST /api/v/{vault}/notes`

Create a new note. The server writes `{folder}/{title}.md`, defaults `folder` to the `capture.folder` config value, runs the save pipeline, and returns the written hash.

**Request body:**
```json
{
  "title": "Follow Up",
  "folder": "",
  "content": "Body text",
  "frontmatter": {
    "status": "draft"
  }
}
```

**Response:** `201 Created`
```json
{
  "path": "Follow Up.md",
  "hash": "2d7d0d..."
}
```

**Errors:**
- `409` — note already exists

### `PUT /api/v/{vault}/notes/{path...}`

Replace a note's content. If `expected_hash` is supplied, the write is rejected on mismatch.

**Request body:**
```json
{
  "content": "---\ntitle: Follow Up\n---\nReplaced body",
  "expected_hash": "2d7d0d..."
}
```

**Response:** `200 OK`
```json
{
  "path": "Follow Up.md",
  "hash": "9a5b62..."
}
```

**Errors:**
- `404` — vault or note not found
- `409` — write conflict

### `PATCH /api/v/{vault}/notes/{path...}`

Merge frontmatter fields into the current note, then run the save pipeline before writing.

**Request body:**
```json
{
  "frontmatter": {
    "owner": "me",
    "status": "active"
  },
  "expected_hash": "9a5b62..."
}
```

**Response:** `200 OK`
```json
{
  "path": "Follow Up.md",
  "hash": "35cb45..."
}
```

### `DELETE /api/v/{vault}/notes/{path...}`

Delete a note.

**Response:** `204 No Content`

### `POST /api/v/{vault}/notes-append/{path...}`

Append content to an existing note, then run the save pipeline.

**Request body:**
```json
{
  "content": "Next line"
}
```

**Response:** `200 OK`
```json
{
  "path": "Follow Up.md",
  "hash": "0f18d0..."
}
```

### `POST /api/v/{vault}/notes-move/{path...}`

Move a note to another vault-relative path.

**Request body:**
```json
{
  "destination": "Customers/Acme/Follow Up.md"
}
```

**Response:** `200 OK`
```json
{
  "from": "Follow Up.md",
  "to": "Customers/Acme/Follow Up.md"
}
```

### `POST /api/v/{vault}/notes-rename/{path...}`

Rename a note within its current folder. The `.md` extension is added automatically; users supply only the bare name. Case-only renames (e.g. `Foo.md` → `foo.md`) are supported on case-insensitive filesystems.

After the rename succeeds, the daemon rewrites every `[[wikilink]]` and `![[embed]]` whose target matches the old basename across the vault. Frontmatter, fenced code blocks, and inline code spans are left untouched. The wikilink rewrite is best-effort: if it fails, the rename still succeeds and an error is logged.

**Request body:**
```json
{
  "name": "New Name"
}
```

**Response:** `200 OK`
```json
{
  "from": "Inbox/Old Name.md",
  "to": "Inbox/New Name.md",
  "references_rewritten": 3
}
```

**Errors:**
- `400` — empty name, contains `/`, `\`, or reserved chars (`:`, `*`, `?`, `"`, `<`, `>`, `|`).
- `404` — source note does not exist.
- `409` — destination filename already exists in the same folder.

### `POST /api/v/{vault}/folders-rename/{path...}`

Rename a folder within its current parent folder. If the folder contains a same-name folder note, the daemon also renames that note to match the new folder name. Wikilinks and embeds are not rewritten.

**Request body:**
```json
{
  "name": "Globex"
}
```

**Response:** `200 OK`
```json
{
  "from": "Customers/Acme",
  "to": "Customers/Globex",
  "folder_note_from": "Customers/Acme/Acme.md",
  "folder_note_to": "Customers/Globex/Globex.md"
}
```

When no same-name folder note exists, `folder_note_from` and `folder_note_to` are `null`.

**Errors:**
- `400` — unsafe folder path or folder name
- `404` — vault or source folder not found
- `409` — destination folder exists, or the synced folder-note filename would collide with an existing note

### Save pipeline

Every write path (`POST /notes`, `PUT /notes/{path...}`, `PATCH /notes/{path...}`, and `POST /notes-append/{path...}`) runs the save pipeline:

- parse YAML frontmatter when present
- stamp `created` when missing
- stamp `updated` on every write
- sort frontmatter keys alphabetically
- trim trailing whitespace on every line
- ensure exactly one trailing newline

### Hooks

Vaults can configure `.notesmith/vault.toml` hooks for `on_note_create` and `on_daily_create`. When configured, Notesmith runs the script relative to the vault root, sends a JSON payload on stdin (`event`, `vault`, `path`, optional `frontmatter`, optional `source`), and logs hook failures without failing the originating HTTP request.

---

## Capture

### `POST /api/v/{vault}/capture`

Quick-capture a note to the capture folder. Generates a timestamped filename.

**Request body:**
```json
{
  "text": "Call Sarah about the project",
  "title": "Phone Call"
}
```

Only `text` is required. `title` is optional — when provided it's used as the filename slug, otherwise the slug is derived from the first 40 characters of `text`.

**Filename format:** `{capture_folder}/{YYYY-MM-DD HH-MM-SS} - {slug}.md`

**Response:** `201 Created`
```json
{
  "path": "2026-05-09 16-30-00 - Phone Call.md",
  "hash": "a1b2c3..."
}
```

> **Note:** Folder listings now go through `POST /api/v/{vault}/query/sql` with `WHERE path LIKE 'folder/%'`.

---

## Web Clipper

### `POST /api/v/{vault}/clip`

Clip a web article into the vault. The daemon fetches the URL, extracts the
readable article (title, author, published date, body), converts it to
Markdown, and writes a new note with `source_url`/`source_type: article`
frontmatter and an `inbox` tag. YouTube URLs are handled as an interactive
`source_type: youtube` on this same endpoint (see **YouTube** below).

**Request body:**
```json
{
  "url": "https://example.com/some-article?utm_source=news#top",
  "tags": ["reading"]
}
```

Only `url` is required. `tags` (optional) are added alongside the mandatory
`inbox` tag. The URL is canonicalized before dedup and storage (tracking
parameters such as `utm_*`/click IDs, the fragment, and trailing slashes are
stripped, and query parameters are sorted).

**Destination:** `[clip].folder` if set, otherwise the capture folder.
**Filename format:** `{folder}/{YYYY-MM-DD HH-MM-SS} - {title-slug}.md`

**Images:** when `[clip].download_images` is `true` (default), remote images in
the article are downloaded into `[clip].attachments_folder` (default
`attachments/clips`) and their links rewritten to the local copies. A failed
image download leaves that image's remote URL in place.

**Templates:** per-domain templates (`[[clip.templates]]`) can customize the
frontmatter and body per source host — see `docs/vault-configuration.md`.

**Dedup:** if a note already carries the same canonical `source_url` (checked
both for the requested URL and the post-redirect final URL), no new note is
written.

**YouTube:** a YouTube URL (`youtube.com`/`youtu.be`) is detected automatically
and clipped as `source_type: youtube` on this same endpoint (ADR 0020 §8). The
URL is canonicalized to `https://www.youtube.com/watch?v=<id>` (dropping `t`,
`list`, `index`, and tracking params) for dedup. The daemon fetches the
published caption track (a single bounded, SSRF-guarded `GET` — it never runs
Whisper) and writes a note with media provenance frontmatter (`title`,
`channel`, `published`, `duration`, `source_url`, `source_type: youtube`,
`ingested_at`, `tags`) and a timestamped transcript body. Per-domain templates
apply to `youtube.com`/`youtu.be` like any other host.

**Response:** `201 Created` — new note written
```json
{
  "path": "Inbox/2026-05-09 16-30-00 - Some Article.md",
  "hash": "a1b2c3...",
  "source_url": "https://example.com/some-article",
  "title": "Some Article",
  "images": 3,
  "duplicate": false
}
```

For a YouTube clip the response includes `source_type: "youtube"` and omits
`images`:
```json
{
  "path": "Inbox/2026-05-09 16-30-00 - Never Gonna Give You Up.md",
  "hash": "a1b2c3...",
  "source_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
  "title": "Never Gonna Give You Up",
  "source_type": "youtube",
  "duplicate": false
}
```

**Response:** `200 OK` — URL already clipped (no write)
```json
{
  "path": "Inbox/2026-05-01 09-00-00 - Some Article.md",
  "duplicate": true
}
```

**Response:** `200 OK` — YouTube video with no published captions (non-fatal).
The daemon does not transcribe; it appends an intent row to the vault's
pending-transcription queue (ADR 0023 §5), keyed by canonical `source_url` so
repeated clips are idempotent. The colocated `notesmith transcribe --drain`
worker acquires the audio and renders the note out of process. `queued` is
`true` when a new queue row was inserted, `false` when the video was already
queued. No note is written by this request.
```json
{
  "status": "no_captions",
  "source_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
  "video_id": "dQw4w9WgXcQ",
  "source_type": "youtube",
  "queued": true,
  "message": "no published captions; queued for transcription"
}
```

**Errors:**

| Status | Meaning |
| --- | --- |
| `400 Bad Request` | Invalid URL, or blocked by the SSRF guard (loopback, private, or link-local address) |
| `403 Forbidden` | Clipping disabled for this vault (`[clip].enabled = false`) |
| `404 Not Found` | Vault not found |
| `413 Payload Too Large` | Fetched page exceeded the size limit |
| `422 Unprocessable Entity` | Page could not be parsed into a readable article |
| `502 Bad Gateway` | Upstream fetch failed |

> The daemon fetch is bounded (timeout, max body size, redirect cap) and every
> redirect hop is re-validated against the SSRF guard.

---

## Search

### `GET /api/v/{vault}/search`

Full-text search across note titles and body content using Tantivy.

The query may embed **metadata filter tokens** — `key:value` words that scope
the search instead of matching text (quote values with spaces:
`customer:"Acme Corp"`):

- `tag:renewal` — note must carry the tag;
- `path:Meetings/` — vault-relative path prefix;
- `customer:Acme` / `stream:X` / `attendee:X` — membership in the
  corresponding wikilink list field (value auto-wrapped as `[[...]]`);
- any other `key:value` — exact frontmatter field match (e.g.
  `audience:internal`, `kind:meeting`, `status:blocked`).

Predicates AND together; repeating a key ORs its values. A token-only query
lists the filter matches. Time-like (`12:30`) and URL tokens stay text.

**Query parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `q` | string | yes | — | Search query, optionally with filter tokens |
| `limit` | integer | no | 20 | Maximum results |

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/search?q=renewal+customer:Acme&limit=5"
```

**Response:** `200 OK`
```json
[
  {
    "vault_name": "work",
    "path": "Customers/Acme Corp/Acme Corp.md",
    "title": "Acme Corp",
    "note_type": "customer",
    "score": 4.231,
    "snippet": "...working on <b>Acme</b> <b>onboarding</b> process..."
  }
]
```

Title matches are boosted 2× over body matches. Snippets contain HTML `<b>` tags around matched terms.

---

## Related Notes

### `GET /api/v/{vault}/related/{path...}`

Notes related to the given note (issue #201), ranked by embedding similarity
blended with link-graph proximity (direct links + shared neighbours). Backs the
Relevant section of the desktop right dock. When the vault has no usable
embeddings the ranking degrades to graph-only and `embeddings_used` is `false`.

**Query parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `limit` | integer | no | 10 | Maximum results (clamped to 100) |

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/related/Customers/Acme%20Corp/Acme%20Corp.md?limit=5"
```

**Response:** `200 OK`
```json
{
  "path": "Customers/Acme Corp/Acme Corp.md",
  "embeddings_used": true,
  "related": [
    {
      "path": "Customers/Acme Corp/Q3 Review.md",
      "title": "Q3 Review",
      "score": 0.82,
      "embedding_similarity": 0.79,
      "directly_linked": true,
      "shared_neighbors": 2
    }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `embeddings_used` | boolean | `false` when the ranking is graph-only (no embedding for the active note) |
| `score` | number | Blended relevance score in `[0, 1]` |
| `embedding_similarity` | number \| null | Cosine similarity to the active note, or null when embeddings weren't used |
| `directly_linked` | boolean | Whether the candidate links to (or is linked from) the active note |
| `shared_neighbors` | integer | Count of shared link neighbours (bibliographic coupling + co-citation) |

**Errors:** `404 Not Found` if the vault or note does not exist.

---

## Embeddings

### `GET /api/v/{vault}/embeddings/stats`

Observability for a vault's embedding index (ADR 0018, issue #244). Reports the
scaling signals from [`docs/embeddings/05-scaling-and-monitoring.md`](embeddings/05-scaling-and-monitoring.md)
— vector count, on-disk size, and rolling search latency — so you can decide when
to move from the brute-force SQLite store to LanceDB.

Reads `embeddings.db` read-only plus the in-process metrics registry. A vault
that has never been embedded reports an empty-but-valid index (zero vectors),
not an error.

**Response:** `200 OK`
```json
{
  "vector_count": 1842,
  "db_bytes": 3145728,
  "dim": 384,
  "embedder_id": "bge-small-en-v1.5",
  "p50_ms": 12.4,
  "p95_ms": 31.8,
  "sample_count": 128,
  "last_ingest_at": 1731000000,
  "running": false,
  "notes_total": 1200,
  "notes_done": 1200,
  "started_at": 1731000000
}
```

| Field | Type | Description |
|-------|------|-------------|
| `vector_count` | integer | Stored chunk vectors for this vault |
| `db_bytes` | integer | Size of `embeddings.db` on disk |
| `dim` | integer \| null | Vector dimensionality (null if never embedded) |
| `embedder_id` | string \| null | Model that produced the vectors (null if never embedded) |
| `p50_ms` / `p95_ms` | number | Rolling search latency percentiles over the recent query window |
| `sample_count` | integer | Number of latency samples backing the percentiles |
| `last_ingest_at` | integer \| null | Unix seconds of the last `embeddings.db` write |
| `running` | boolean | Whether an embed pass is currently running for this vault (#260) |
| `notes_total` | integer | Notes the current/most-recent pass will visit (0 before the first pass) |
| `notes_done` | integer | Notes visited so far in that pass; equals `notes_total` when finished |
| `started_at` | integer \| null | Unix seconds when the current/most-recent pass began |

**Errors:** `404 Not Found` if the vault does not exist.

**Example:**
```bash
curl http://127.0.0.1:27183/api/v/work/embeddings/stats
```

---

## SQL Query

### `POST /api/v/{vault}/query/sql`

Execute read-only SQL against the SQLite cache. Only `SELECT` and `WITH` statements are allowed.

**Request body:**
```json
{
  "sql": "SELECT n.title, state.value AS state FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' LEFT JOIN v_fields state ON state.vault_name = n.vault_name AND state.note_path = n.path AND state.key = 'state' WHERE note_type.value = 'customer' ORDER BY n.title",
  "max_rows": 10000,
  "format": "json"
}
```

**Request fields:**

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `sql` | string | yes | — | Read-only SQL statement |
| `max_rows` | integer | no | `10000` | Maximum rows returned before truncation |
| `format` | `json` \| `markdown` | no | `json` | Response format |

**Response (`format: "json"`):** `200 OK`
```json
{
  "columns": ["title", "state"],
  "rows": [
    ["Acme Corp", "Active"],
    ["Globex Industries", "Onboarding"]
  ],
  "row_count": 2,
  "truncated": false
}
```

**Response (`format: "markdown"`):** `200 OK`
```text
| title | state |
| --- | --- |
| Acme Corp | Active |
| Globex | Inactive |
```

**Errors:**
- `400` — non-SELECT statement attempted
- `404` — vault not found
- `422` — SQL syntax or execution error

**Example:**
```bash
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT n.title, state.value AS state FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = ''type'' LEFT JOIN v_fields state ON state.vault_name = n.vault_name AND state.note_path = n.path AND state.key = ''state'' WHERE note_type.value = ''customer'' ORDER BY n.title"}'
```

```bash
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT n.title, state.value AS state FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = ''type'' LEFT JOIN v_fields state ON state.vault_name = n.vault_name AND state.note_path = n.path AND state.key = ''state'' WHERE note_type.value = ''customer'' ORDER BY n.title","format":"markdown","max_rows":25}'
```

See [SQL Views Reference](sql-views.md) for available views.

---

## Sidebar Config

### `GET /api/v/{vault}/sidebar-config`

Load sidebar view configuration from `.notesmith/sidebar.yaml`. Returns empty views when the file is absent (Files-only mode).

**Response:** `200 OK`
```json
{
  "views": [
    {
      "id": "workflow",
      "name": "Workflow",
      "icon": "⚡",
      "badge_query": "SELECT count(*) FROM v_notes WHERE path LIKE 'Capture/%'",
      "sections": [
        {
          "type": "recently-viewed",
          "label": "Recent",
          "mode": "both",
          "limit": 10
        },
        {
          "type": "custom-folders",
          "label": "Projects",
          "folders": ["Projects/Active", "Customers"]
        },
        {
          "type": "custom-items",
          "label": "Triage",
          "items": [
            {
              "name": "Capture",
              "icon": "⚡",
              "source": { "folder": "Capture", "recursive": true }
            }
          ]
        }
      ]
    }
  ]
}
```

**Example:**
```bash
curl http://127.0.0.1:27183/api/v/work/sidebar-config
```

### `GET /api/v/{vault}/folder-notes`

List notes in a folder with title and body snippet, for use by the sidebar middle pane.

**Query parameters:**

| Parameter | Description |
|-----------|-------------|
| `path` | Folder path within the vault (required) |
| `recursive` | Include subfolders (default: `false`) |
| `limit` | Maximum results (default: 50) |
| `sort` | Sort field: `modified`, `created`, or `name` (default: `modified`) |
| `sort_dir` | Sort direction: `asc` or `desc` (default: `desc`) |

**Response:** `200 OK`
```json
{
  "notes": [
    {
      "path": "Capture/meeting-notes.md",
      "title": "Meeting Notes",
      "snippet": "First two lines of the note body...",
      "modified_at": "2026-05-11T10:00:00",
      "created_at": "2026-05-10T08:00:00"
    }
  ]
}
```

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/folder-notes?path=Capture&recursive=true&limit=20"
```

---

## Tasks

### `GET /api/v/{vault}/tasks`

List tasks from the vault with optional filters.

**Query parameters:**

| Parameter | Description |
|-----------|-------------|
| `status` | Filter by status (`todo`, `in_progress`, `blocked`, `waiting`, `on_hold`, `done`, `cancelled`) |
| `field` | Filter by inline field value (format: `key=value`) |
| `due_before` | Filter to tasks due before this date (`YYYY-MM-DD`) |
| `limit` | Maximum results (default: 200) |

**Response:** `200 OK`
```json
[
  {
    "task_hash": "abc123...",
    "note_path": "Projects/Migration to v2.md",
    "line_number": 12,
    "status": "in_progress",
    "status_char": "/",
    "status_group": "open",
    "text": "Testing in staging",
    "note_title": "Migration to v2",
    "fields": {
      "customer": "Acme",
      "due": "2025-03-15",
      "priority": "high"
    }
  }
]
```

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/tasks?status=todo&field=customer%3DAcme"
```

### `POST /api/v/{vault}/tasks`

Add a new To Do task to an existing note. All inline fields are arbitrary key-value pairs written as `[key:: value]` on the task line.

**Request body:**
```json
{
  "note_path": "Projects/Migration to v2.md",
  "description": "Follow up on SLA requirements",
  "status_char": " ",
  "fields": {
    "customer": "Acme",
    "due": "2025-02-01",
    "priority": "high"
  }
}
```

Only `note_path` and `description` are required. `fields` is an optional object of key-value pairs. `status_char` defaults to space (todo).

**Response:** `201 Created`
```json
{ "path": "Projects/Migration to v2.md", "hash": "2d7d0d..." }
```

### `POST /api/v/{vault}/tasks/toggle`

Toggle a task to a new status using its content hash (blake3 of the raw task line). The engine finds the matching line, rewrites its checkbox marker in place, and runs the save pipeline.

**Request body:**
```json
{
  "note_path": "Projects/Migration to v2.md",
  "task_hash": "abc123...",
  "status": "done"
}
```

`status` is the preferred field name. The server still accepts `new_status` for older clients.

**Response:** `200 OK`
```json
{ "path": "Projects/Migration to v2.md", "hash": "ef456..." }
```

**Errors:**
- `404` — task hash not found in the note
- `409` — hash matches more than one task (collision)
- `422` — invalid status string

---

## Templates

### `GET /api/v/{vault}/templates`

List all available templates.

**Response:** `200 OK`
```json
[
  {
    "name": "generic-note",
    "description": "A generic blank note",
    "output_path": "{% if folder %}{{ folder }}/{% endif %}{{ title | slug }}.md",
    "prompts": [
      { "name": "title", "type": "text", "field": "title", "required": true },
      { "name": "folder", "type": "text", "field": "folder", "required": false }
    ]
  }
]
```

`type` is `text` or `field-picker`. `field` is the `fields.toml` key a
`field-picker` suggests from — always present, defaulting to the prompt name, so
clients can fetch [`/fields/{key}/suggest`](#get-apivvaultfieldskeysuggestqpartial)
without inferring it. It matters when the two differ: a singular `customer`
prompt suggesting from the plural `customers` list field.

### `POST /api/v/{vault}/templates/{name}/render`

Render a template with the given prompt values without creating a file.

**Request body:**
```json
{
  "prompts": {
    "title": "Hello World",
    "folder": "Customers/Acme"
  }
}
```

**Response:** `200 OK`
```json
{
  "path": "Customers/Acme/hello-world.md",
  "content": "# Hello World\n"
}
```

**Errors:**
- `404` — template not found
- `422` — missing required prompts (returns `{ "error": "...", "missing": ["title"] }`)

### `POST /api/v/{vault}/templates/{name}/instantiate`

Render a template and create the note at the computed output path. The save pipeline runs on the rendered content.

**Request body:**
```json
{
  "prompts": {
    "customer": "Acme",
    "title": "Q2 Check-in"
  }
}
```

**Response:** `201 Created`
```json
{
  "path": "Customers/Acme/External Meetings/2026-05-09 Q2 Check-in.md"
}
```

**Errors:**
- `404` — template not found
- `422` — missing required prompts

---

## Prompts

Static custom prompts are named, saved instruction strings sent verbatim to the
chat agent. The merged set is built from two sources:

- **Defaults** — built-in prompts seeded into the daemon config dir
  (`<config>/notesmith/prompts/*.md`) on first run; users may edit them.
- **Vault overrides** — markdown files in the vault's `_prompts/` folder.

The two sets are merged by `name`; a vault entry overrides a default of the same
name. Each prompt file is markdown with YAML frontmatter (`name`, optional
`description`) and a body. Variable substitution is not yet supported; the format
reserves a `variables` frontmatter field for future `{{variable}}` interpolation.

### `GET /api/v/{vault}/prompts`

List the merged static prompts for a vault.

**Response:** `200 OK`
```json
{
  "prompts": [
    {
      "name": "summarize",
      "description": "Concise summary of the current note.",
      "body": "Provide a concise summary of the current note...",
      "source": "default"
    }
  ]
}
```

`source` is `"default"` (config-dir default) or `"vault"` (vault `_prompts/`
override). Malformed prompt files are skipped (logged at `WARN`), so the endpoint
always returns `200` with a possibly-empty list, never a `500`.

**Errors:**
- `404` — vault not found

---

## Customizations

Discovered, user-authored **custom agents (personas)**, **skills**, and
**instructions** for the chat UI (ADR 0016). Items are `*.md` files with optional
YAML frontmatter, discovered from two scopes:

- **Project** — the vault's `.notesmith/{agents,skills,instructions}/` folders.
- **Global** — `~/.config/notesmith/{agents,skills,instructions}/` (XDG-aware).

The file stem is the item `id`; project entries override global entries by id
(project wins). Malformed files are skipped (logged at `WARN`), so the endpoint
always returns `200`, never a `500`.

### `GET /api/v/{vault}/customizations`

List the merged customization set for a vault.

**Response:** `200 OK`
```json
{
  "agents": [
    {
      "id": "researcher",
      "name": "Researcher",
      "description": "Deep research assistant.",
      "backend": "copilot",
      "model": "gpt-4o",
      "body": "You are a meticulous researcher...",
      "source": "project"
    }
  ],
  "skills": [
    { "id": "citations", "name": "Citations", "description": "Cite sources.",
      "body": "Always cite sources.", "source": "global" }
  ],
  "instructions": [
    { "id": "tone", "name": "Tone", "description": "",
      "body": "Be concise.", "source": "project" }
  ]
}
```

For an **agent** (persona), `backend` is an optional ACP agent id
(`copilot`/`claude`/…; `null` = use the session's selected agent) and `model` is
an optional model id (`null` when unset); the `body` is the persona's preamble
prompt. `source` is `"project"` or `"global"`.

**Errors:**
- `404` — vault not found

---

## Routing

### `POST /api/v/{vault}/route/preview`

Preview where a note would be routed based on `.notesmith/routing.yaml` rules.

**Request body:**
```json
{
  "path": "Capture/standup.md"
}
```

**Response:** `200 OK`
```json
{
  "path": "Capture/standup.md",
  "destination": "Customers/Acme Corp/External Meetings/standup.md",
  "rule_id": "external-meeting"
}
```

**Errors:**
- `404` — vault, routing config, or no matching rule
- `409` — note already archived
- `422` — note has no frontmatter

### `POST /api/v/{vault}/route/apply`

Apply routing rules to move notes to their destinations. Routing applies configured field/tag mutations, stamps `archived: true` and `archived-at`, then moves the note.

**Request body (specific notes):**
```json
{
  "paths": ["Capture/standup.md", "Capture/idea.md"]
}
```

**Response:** `200 OK`
```json
{
  "routed": 2,
  "results": [
    {
      "from": "Capture/standup.md",
      "to": "Customers/Acme Corp/External Meetings/standup.md",
      "rule_id": "external-meeting",
      "route_log": {
        "note_path": "Customers/Acme Corp/External Meetings/standup.md",
        "from_path": "Capture/standup.md",
        "to_path": "Customers/Acme Corp/External Meetings/standup.md",
        "rule_id": "external-meeting",
        "mutations_json": {
          "set_fields": { "status": "filed" },
          "remove_fields": [],
          "add_tags": ["archived"],
          "remove_tags": ["inbox"]
        }
      }
    },
    {
      "from": "Capture/idea.md",
      "to": "General/idea.md",
      "rule_id": "note-general"
    }
  ]
}
```

---

## Daily

### `GET /api/v/{vault}/daily/{date}`

Fetch the daily note for the given date. The path is resolved through `periodic.daily`, so custom daily filenames are supported.

**Parameters:**
- `date` — Date in `YYYY-MM-DD` format

**Example:**
```bash
curl http://127.0.0.1:27183/api/v/work/daily/2025-06-15
```

**Response:** `200 OK`
```json
{
  "path": "2025-06-15.md",
  "content": "---\ndate: 2025-06-15\ntype: daily\n---\n# 2025-06-15\n...",
  "frontmatter": { "date": "2025-06-15", "type": "daily" }
}
```

**Errors:**
- `404` — vault or daily note not found

### `POST /api/v/{vault}/daily/{date}`

Create the daily note for the given date using the configured template. Idempotent — returns existing note info if already created.

**Parameters:**
- `date` — Date in `YYYY-MM-DD` format

**Example:**
```bash
curl -X POST http://127.0.0.1:27183/api/v/work/daily/2025-06-15
```

**Response:** `201 Created` (new note)
```json
{
  "path": "2025-06-15.md",
  "created": true
}
```

**Response:** `200 OK` (already exists)
```json
{
  "path": "2025-06-15.md",
  "created": false
}
```

**Errors:**
- `400` — invalid date format
- `404` — vault not found

### `GET /api/v/{vault}/periodic/{kind}/current`

Get the current periodic note for `daily`, `weekly`, `monthly`, `quarterly`, or `yearly`, creating it if missing.

**Query parameters:**
- `offset` — optional integer offset from the current period (`-1`, `0`, `1`, ...)

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/periodic/weekly/current?offset=-1"
```

**Response:** `200 OK`
```json
{
  "created": true,
  "path": "Weekly/Week 2026-W21.md",
  "content": "# 2026-W21\n...",
  "frontmatter": null,
  "period_kind": "weekly",
  "period_key": "2026-W21",
  "period_start": "2026-05-18",
  "period_end": "2026-05-24"
}
```

### `GET /api/v/{vault}/periodic/{kind}/list`

List indexed periodic notes for a kind in a date range.

**Query parameters:**
- `from` — optional start date (`YYYY-MM-DD`)
- `to` — optional end date (`YYYY-MM-DD`)

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/periodic/weekly/list?from=2026-05-18&to=2026-05-31"
```

**Response:** `200 OK`
```json
[
  {
    "path": "Weekly/Week 2026-W21.md",
    "period_kind": "weekly",
    "period_key": "2026-W21",
    "period_start": "2026-05-18",
    "period_end": "2026-05-24"
  }
]
```

### `POST /api/v/{vault}/daily/agent-create`

Agent daily workflow endpoint. In prompt mode, the daemon loads `.notesmith/prompts/daily-note.md`, executes its `context_queries`, renders markdown tables for each result set, replaces placeholders such as `{{ open_tasks }}` and `{{ today }}`, and returns the assembled prompt. In write mode, it writes the provided content directly as the daily note for the requested date.

**Request body (prompt mode):**
```json
{ "date": "2026-05-10" }
```

**Request body (write mode):**
```json
{
  "date": "2026-05-10",
  "content": "---\ntype: daily\ndate: 2026-05-10\n---\n# 2026-05-10\n..."
}
```

**Response:** `200 OK` (prompt mode)
```json
{
  "date": "2026-05-10",
  "prompt": "# Daily Note Prompt\n..."
}
```

**Response:** `201 Created` (write mode)
```json
{
  "path": "2026-05-10.md",
  "created": true
}
```

**Errors:**
- `400` — invalid date format
- `404` — vault or prompt template not found
- `409` — daily note already exists in write mode
- `422` — invalid SQL in a context query

---

## Event Stream

### `GET /api/v/{vault}/events`

Server-Sent Events (SSE) stream for real-time vault change notifications. Each connected client receives events for the specified vault plus any `_system` events. Multiple clients can subscribe concurrently. Global vault-registration updates are broadcast as `vaults.changed` so clients can refetch `/api/app/vaults`.

**Query parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `last_event_id` | integer | Optional replay cursor. Returns buffered events with IDs greater than this value before the live stream resumes. |

The daemon keeps a ring buffer of the most recent 100 SSE events. Clients may also send `Last-Event-ID` and the daemon will replay buffered events after that ID.

**Event types:**

| Event | Emitted when |
|-------|-------------|
| `note.created` | New note created |
| `note.updated` | Note content changed |
| `note.moved` | Note path changed (move or route) |
| `note.deleted` | Note removed |
| `task.updated` | Task added or status toggled |
| `note.captured` | New note captured |
| `daily.created` | Daily note created |
| `cache.rebuilt` | Cache rebuild completed |
| `search.reindexed` | Search index rebuilt |
| `config.changed` | Config file modified and parsed successfully |
| `config.removed` | Config file deleted |
| `config.error` | Config file has a parse error |
| `vaults.changed` | Global vault registrations changed; refetch `/api/app/vaults` |
| `shutting_down` | Daemon is draining and preparing to exit |

**Payload (JSON in `data:` field):**
```json
{
  "id": 41,
  "vault": "work",
  "type": "note.created",
  "path": "Follow Up.md",
  "timestamp": "2026-05-09T16:30:00.123-0700",
  "hash": "8f1c7e..."
}
```

The `hash` field is present on `note.created` and `note.updated` events and
holds the Blake3 hex digest of the saved note contents. Clients can compare
this to the hash returned by their preceding write to recognise echoes of
their own saves and suppress spurious "file changed on disk" warnings.
Events that do not announce a content change (e.g. `note.deleted`,
`note.moved`, `task.updated`) omit the field.

**Config event payloads:**

`config.changed` — Config file modified successfully:
```json
{
  "vault": "work",
  "type": "config.changed",
  "path": ".notesmith/sidebar.yaml",
  "timestamp": "2026-05-11T19:44:23.865-0700",
  "config": {
    "key": "sidebar",
    "status": "changed",
    "error": null
  }
}
```

`config.removed` — Config file deleted:
```json
{
  "vault": "work",
  "type": "config.removed",
  "path": ".notesmith/sidebar.yaml",
  "timestamp": "2026-05-11T19:44:23.865-0700",
  "config": {
    "key": "sidebar",
    "status": "removed",
    "error": null
  }
}
```

`config.error` — Config file has a parse error:
```json
{
  "vault": "work",
  "type": "config.error",
  "path": ".notesmith/sidebar.yaml",
  "timestamp": "2026-05-11T19:44:23.865-0700",
  "config": {
    "key": "sidebar",
    "status": "error",
    "error": "expected ',' or ']' at line 5 column 3"
  }
}
```

Config keys are `sidebar` (for `.notesmith/sidebar.yaml`) and `vault` (for `.notesmith/vault.toml`).

`vaults.changed` uses the standard payload shape and carries an empty `path`:
```json
{
  "vault": "work",
  "type": "vaults.changed",
  "path": "",
  "timestamp": "2026-05-14T19:44:23.865-0700"
}
```

**Example:**
```bash
curl -N http://127.0.0.1:27183/api/v/work/events
```

**Errors:**
- `404` — vault not found

---

## Admin

### `POST /admin/shutdown`

Emit a `shutting_down` SSE event for each configured vault, trigger graceful shutdown, and return `200 OK`. The daemon stops accepting new connections and drains in-flight requests before exiting.

**Response:** `200 OK`

**Example:**
```bash
curl -X POST http://127.0.0.1:27183/admin/shutdown
```

### `POST /admin/restart`

Same behavior as `POST /admin/shutdown`. External supervision (for example Tauri, launchd, or systemd) is responsible for starting the daemon again.

**Response:** `200 OK`

**Example:**
```bash
curl -X POST http://127.0.0.1:27183/admin/restart
```

---

## App shell

### `GET /app/*`

Serve the compiled SvelteKit app shell and static assets from the daemon.

**Example:**
```bash
open http://127.0.0.1:27183/app/
```

Nested `/app/...` routes fall back to the app's `index.html` so client-side routing works.

---

## Git

### `GET /api/v/{vault}/git/status`

Returns git working tree status for the vault.

**Response (200):**
```json
{
  "changed": ["README.md"],
  "staged": [],
  "untracked": ["new-note.md"],
  "clean": false
}
```

**Errors:**
- `400` — vault is not a git repository
- `404` — vault not found

### `POST /api/v/{vault}/git/init`

Initializes a git repository in the vault if one does not already exist. Idempotent:
calling it on an existing repository is a no-op. When a new repository is created it
scaffolds a minimal `.gitignore` (OS cruft only — notes and `.notesmith/` stay tracked)
and records an initial commit.

Enabling `[git]` in the vault config triggers this automatically (see
[vault-configuration.md](vault-configuration.md)); this endpoint is available for
explicit or scripted use.

**Response (200):**
```json
{
  "initialized": true,
  "alreadyRepo": false,
  "sha": "9f3a1c4e7b2d..."
}
```

- `initialized` — `true` when a new repository was created this call.
- `alreadyRepo` — `true` when the vault was already a repository (no changes made).
- `sha` — the initial commit sha when one was created, otherwise `null`.

**Errors:**
- `404` — vault not found
- `500` — repository initialization failed

### `GET /api/v/{vault}/git/log?limit={n}`

Returns rich commit history (newest first) with per-commit diff stats, for the
git-history UI. `limit` defaults to 50 and is clamped to 500.

**Response (200):**
```json
[
  {
    "sha": "9f3a1c4e7b2d...",
    "shortSha": "9f3a1c4",
    "author": "surdy",
    "authorEmail": "surdy@example.com",
    "timestampSecs": 1782846000,
    "subject": "notesmith: checkpoint (note-a.md, note-b.md)",
    "filesChanged": 2,
    "insertions": 14,
    "deletions": 3
  }
]
```

**Errors:**
- `400` — vault is not a git repository
- `404` — vault not found

### `GET /api/v/{vault}/git/diff/{sha}`

Returns the full file-level diff of a single commit against its first parent
(or the empty tree for a root commit). `sha` may be a full or abbreviated SHA.

**Response (200):**
```json
{
  "sha": "9f3a1c4e7b2d...",
  "files": [
    {
      "path": "note-a.md",
      "status": "modified",
      "added": 9,
      "removed": 3,
      "lines": [
        { "kind": "hunk", "oldLine": null, "newLine": null, "text": "@@ -1,6 +1,9 @@" },
        { "kind": "context", "oldLine": 1, "newLine": 1, "text": "# Title" },
        { "kind": "removed", "oldLine": 2, "newLine": null, "text": "old line" },
        { "kind": "added", "oldLine": null, "newLine": 2, "text": "new line" }
      ]
    }
  ]
}
```

- `status` — one of `modified`, `added`, `deleted`, `renamed`.
- `kind` — one of `context`, `added`, `removed`, `hunk`.

**Errors:**
- `400` — vault is not a git repository
- `404` — vault not found, or unknown commit

### `POST /api/v/{vault}/git/commit`

Stages all changed, stageable files and commits them (a "checkpoint"). Used by
the desktop inactivity-checkpoint driver (after flushing unsaved editor buffers
to disk) and for manual "commit now" actions. Requires `git.enabled` for the
vault.

**Request body (optional):**
```json
{ "message": "checkpoint: before refactor" }
```

- `message` (optional) — explicit commit message. When omitted, the vault's
  configured `commit_message` is used; if that is also unset, a message is
  generated from the changed-file list (e.g. `"Update note-a.md, note-b.md and 3 more"`).

**Response (200) — committed:**
```json
{ "committed": true, "sha": "abc1234...", "files": ["note-a.md", "note-b.md"] }
```

**Response (200) — nothing to commit:**
```json
{ "committed": false, "sha": null, "files": [] }
```

**Errors:**
- `400` — git integration is not enabled, or vault is not a git repository
- `404` — vault not found

### `POST /api/v/{vault}/git/sync`

Triggers pull (fast-forward only) then push. Returns combined result.

**Response (200):**
```json
{
  "pull": { "updated": true, "new_head": "abc1234...", "conflict": false },
  "push": { "pushed": true, "error": null }
}
```

If pull conflicts, push is skipped:
```json
{
  "pull": { "updated": false, "new_head": null, "conflict": true },
  "push": null,
  "error": "pull conflict, push skipped"
}
```

**Errors:**
- `400` — vault is not a git repository
- `404` — vault not found
---

## Sidebar Config

### `GET /api/v/{vault}/sidebar-config`

Returns the sidebar configuration with hash for conflict detection.

**Response:** `200 OK`
```json
{
  "config": { "views": [...] },
  "hash": "abc123",
  "path": ".notesmith/sidebar.yaml",
  "warnings": {}
}
```

**Headers:** `ETag: "abc123"`

### `PUT /api/v/{vault}/sidebar-config`

Updates the sidebar configuration. Requires `If-Match` header with the current config hash.

**Headers:** `If-Match: "abc123"`, `Content-Type: application/json`

**Body:** `SidebarConfig` object
```json
{
  "views": [
    {
      "id": "capture",
      "name": "Capture",
      "icon": "bolt",
      "sections": [
        { "type": "custom-folders", "label": "Capture", "folders": ["Capture"] }
      ]
    }
  ]
}
```

**Response:** `200 OK` — same shape as GET

**Errors:**
- `409` — config was modified externally (conflict); response includes current config
- `422` — validation failed; response includes field-level errors
- `428` — missing `If-Match` header

### `GET /api/v/{vault}/folders`

Returns a sorted list of all visible folders in the vault. Used for folder autocomplete.

**Response:** `200 OK`
```json
["Capture", "Daily", "Projects", "Projects/Alpha"]
```

---

## Vault Management

### `GET /api/app/vaults`

Lists all registered vaults with default indicator.

**Response:** `200 OK`
```json
[
  { "name": "work", "path": "/home/user/vaults/work", "is_default": true },
  { "name": "personal", "path": "/home/user/vaults/personal", "is_default": false }
]
```

### `GET /api/app/kits`

List the vault kits built into the daemon. Not vault-scoped — kits ship with the
binary, so this is answerable before any vault exists. Clients use it to offer a
"Start from" choice when registering a vault.

**Response:** `200 OK`
```json
[
  {
    "id": "work-notes",
    "description": "Meetings, customers, streams, people and tasks. …",
    "files": 16,
    "folders": ["Inbox", "Meetings", "Streams", "Customers", "People", "Daily", "Weekly", "Quarterly", "Dashboards"]
  }
]
```

Apply one by passing its `id` as `kit` to `POST /api/app/vaults` below, or from
the CLI with [`notesmith kit apply`](cli.md#kit-apply).

---

### `POST /api/app/vaults`

Registers a new vault.

**Body:**
```json
{ "name": "personal", "path": "/home/user/vaults/personal", "create": false }
```

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | yes | Unique vault name. |
| `path` | string | yes | Absolute path **on the daemon host**. |
| `create` | bool | no (default `false`) | When `true` and `path` does not exist, the daemon creates the directory (recursively) before registering. When `false`, a missing path is rejected with `422 path_not_found`. |
| `kit` | string | no | Kit id to scaffold into the vault (e.g. `work-notes`) — `.notesmith/` config, templates, dashboards, and the folder skeleton. Omit for a bare vault. |

**Response:** `201 Created`
```json
{ "name": "personal", "status": "registered" }
```

With `"kit": "work-notes"`, the response also reports what the scaffold did.
Existing files are **never overwritten** — they come back under `skipped`:

```json
{
  "name": "personal",
  "status": "registered",
  "kit": {
    "id": "work-notes",
    "written": [".notesmith/vault.toml", ".notesmith/routing.yaml", "..."],
    "skipped": [],
    "created_dirs": ["Inbox", "Meetings", "Streams", "..."]
  }
}
```

Scaffolding happens **before** the vault goes live, so the first index pass
already sees its templates and dashboards. The same kits are installable from
the CLI — see [`notesmith kit apply`](cli.md#kit-apply).

> **Available immediately.** Registration writes the global config **and** loads
> the new vault into the daemon's live engine map (starting its filesystem
> watcher) before responding, so `/api/v/{name}/…` routes work right away — no
> polling needed. The config watcher later observes the same on-disk change and
> finds the vault already live (a no-op).

**Errors:**
- `409 vault_exists` — vault name already registered
- `422 path_not_found` — path does not exist and `create` was not `true`
- `422 path_create_failed` — `create:true` but the daemon could not create the
  directory (e.g. the parent is not writable by the daemon's user)
- `422 unknown_kit` — `kit` is not a known kit id; the response lists the
  `available` ids and the vault is **not** registered
- `422 path_not_directory` — path exists but is not a directory

### `PUT /api/app/vaults/{name}`

Updates a vault (rename).

**Body:**
```json
{ "name": "new-name" }
```

**Response:** `200 OK`
```json
{ "name": "new-name", "status": "updated" }
```

**Errors:**
- `404` — vault not found
- `409` — new name conflicts with existing vault

### `DELETE /api/app/vaults/{name}`

Unregisters a vault. Does not delete files by default.

**Query parameters:**
- `delete_files` (optional, default `false`) — when `true`, recursively deletes
  the vault folder on the daemon host after unregistering the vault.

**Response:** `204 No Content`

**Errors:**
- `404` — vault not found
- `422` — file deletion failed, or the registered path is not a directory when
  `delete_files=true`

### `PUT /api/app/default-vault`

Sets the default vault.

**Body:**
```json
{ "name": "work" }
```

**Response:** `200 OK`
```json
{ "default_vault": "work" }
```

**Errors:**
- `404` — vault not found

### `POST /api/app/vaults/{name}/reindex`

Triggers a full reindex of the vault cache and search index.

**Query parameters:**
- `cache_only` (optional, default: `false`) — rebuild only the SQLite cache
- `search_only` (optional, default: `false`) — rebuild only the Tantivy search index

`cache_only=true` and `search_only=true` is rejected with `422`.

**Response:** `200 OK`
```json
{ "vault": "work", "status": "reindexed", "notes": 142 }
```

**Errors:**
- `404` — vault not found
- `422` — invalid reindex mode

---

## Agent transcripts

Per-vault chat history for the agent panel, persisted in the daemon's durable
store (separate from the rebuildable index cache, so it survives restarts and
reindexes — see [ADR 0012](adr/0012-agent-transport-acp-mcp.md) Decision 13).
All routes are vault-scoped: a thread created under one vault is never visible
under another.

A **thread** is `{ id, vault, title, agent, model, created_at, updated_at }`
(`agent`/`model` are optional strings). A **message** is
`{ id, thread_id, seq, role, content, created_at }` where `role` is one of
`user`, `agent`, `system` and `seq` is the 1-based per-thread order.

### `GET /api/v/{vault}/agent/threads`

List the vault's threads, most-recently-updated first. Returns `200` with a JSON
array of threads.

### `POST /api/v/{vault}/agent/threads`

Create a thread. Body: `{ "title": "…", "agent": "copilot"?, "model": "gpt-5"? }`.
Returns `201` with the created thread. `400` if `title` is empty/blank.

### `GET /api/v/{vault}/agent/threads/{thread_id}`

Fetch one thread. `200` with the thread, or `404` if it does not exist in this
vault.

### `POST /api/v/{vault}/agent/threads/{thread_id}/rename`

Body: `{ "title": "…" }`. Returns `200` with the updated thread. `400` for a
blank title, `404` if the thread is not in this vault.

### `POST /api/v/{vault}/agent/threads/{thread_id}/session`

Bind (or clear) the thread's agent ACP `sessionId` so the conversation can be
resumed via ACP `session/load` on reopen (issue #262). Body:
`{ "acp_session_id": "sess-…" }` to bind, or `{ "acp_session_id": null }` to
clear. Returns `200` with the updated thread, or `404` if the thread is not in
this vault. Threads carry `acp_session_id` (nullable) in every thread response.

### `DELETE /api/v/{vault}/agent/threads/{thread_id}`

Delete a thread and its messages (cascade). `204` on success, `404` if absent.

### `GET /api/v/{vault}/agent/threads/{thread_id}/messages`

Load a thread's messages in order. `200` with a JSON array, or `404` if the
thread is not in this vault. Corrupt stored rows are skipped, never fatal.

### `POST /api/v/{vault}/agent/threads/{thread_id}/messages`

Append a message. Body: `{ "role": "user"|"agent"|"system", "content": "…" }`.
Returns `201` with the created message. `404` if the thread is not in this
vault; an unknown `role` is a `4xx` deserialization error (never a `500`).

---

## Agent permissions

Per-vault persisted **"Always Allow"** grants for agent writes (issue #189). The
agent panel gates every write behind a permission prompt offering **Allow Once**,
**Allow This Session**, and **Always Allow**. "Always Allow" is persisted here, in
the daemon's durable store (alongside transcripts, separate from the rebuildable
index cache — see [ADR 0012](adr/0012-agent-transport-acp-mcp.md) Decision 5), so
a future session — even after a daemon/app restart — pre-seeds the grant and
never re-prompts.

Persistence is **frontend-orchestrated**: the chat store fetches a vault's grants
at session start (to pre-seed the ACP session) and POSTs a new grant when the user
picks "Always Allow". Grants are keyed by `(vault, tool)`; all routes are
vault-scoped, so one vault's grants are never visible under another.

### `GET /api/v/{vault}/agent/permissions`

List the vault's granted tool names. Returns `200` with a JSON array of strings,
sorted ascending. Example:

```json
["append_note", "create_note"]
```

### `POST /api/v/{vault}/agent/permissions`

Persist an "Always Allow" grant. Body: `{ "tool": "create_note" }`. Idempotent —
a repeated grant is a no-op. Returns `204`. `400` if `tool` is empty/blank.

```bash
curl -X POST http://127.0.0.1:27183/api/v/work/agent/permissions \
  -H 'Content-Type: application/json' \
  -d '{ "tool": "create_note" }'
```

### `DELETE /api/v/{vault}/agent/permissions/{tool}`

Revoke a persisted grant. Returns `204` (idempotent — revoking an absent grant is
not an error).

---

## Agent access (MCP over HTTP)

The daemon hosts a [Model Context Protocol](https://modelcontextprotocol.io) server for each vault using the streamable-HTTP transport (HTTP + SSE). Agents connect directly to the daemon and reuse its live per-vault indexes — there is no separate process to launch.

Two endpoints are mounted per vault:

| Endpoint | Capabilities |
|----------|--------------|
| `/mcp/{vault}` | Full read **and** write access (all MCP tools) |
| `/mcp-ro/{vault}` | Read-only — write tools are rejected with an error |

Both expose the same tool and resource set as the stdio adapter (see [`docs/mcp.md`](mcp.md)). The read-only endpoint runs the identical handler wrapped so every mutating operation is refused; it guards against agent mistakes, not malicious actors.

### `POST /mcp/{vault}` · `POST /mcp-ro/{vault}`

Streamable-HTTP MCP session endpoint. Send JSON-RPC 2.0 messages (`initialize`, `tools/list`, `tools/call`, …) with:

- `Content-Type: application/json`
- `Accept: application/json, text/event-stream`

The `initialize` response returns an `mcp-session-id` header; include it on subsequent requests. `GET` on the same path opens the server-sent-event stream for the session.

**Errors:**
- `404` — unknown vault (the vault path segment does not resolve against the daemon's loaded vaults; vaults added after startup resolve dynamically, no restart required)

### Reverse proxy / TLS

The daemon serves plain HTTP and defers authentication under the LAN/VPN trust model (see [ADR 0010](adr/0010-agent-access-architecture.md)). Terminate TLS at a reverse proxy. Because MCP uses SSE, disable response buffering for these paths — for nginx:

```nginx
location /mcp/ {
    proxy_pass http://127.0.0.1:27183;
    proxy_buffering off;
    proxy_set_header Connection '';
    proxy_http_version 1.1;
}
```
