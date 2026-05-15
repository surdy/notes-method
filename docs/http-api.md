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
  "vaults": [{ "name": "work", "state": "ready", "notes": 421 }],
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
  "vaults_root": null
}
```

**Example:**
```bash
curl http://127.0.0.1:27183/api/capabilities
```

---

## Vault Config

### `GET /api/v/{vault}/config`

Read the vault configuration from `.notesmith/vault.toml`. Returns the parsed config, a blake3 hash for ETag-based conflict detection, and any validation warnings.

The response includes an `ETag` header with the config hash for use with `PUT` requests.

**Response:** `200 OK`
```json
{
  "config": {
    "name": "work",
    "capture": { "folder": "", "template": "generic-note" },
    "daily": { "folder": "", "template": "daily-note", "generate_at": "06:00", "timezone": "America/Los_Angeles", "catch_up": false },
    "editor": { "live_preview": true, "default_mode": "source" },
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

Update the vault configuration. Requires an `If-Match` header with the current config hash (from a prior `GET`) for optimistic concurrency control. The `WriteGuard` extractor checks the `Origin` header — only `tauri://localhost`, `http://localhost`, and `http://127.0.0.1` origins are allowed for writes. Requests with no `Origin` header (curl, CLI) are permitted.

**Request headers:**
- `If-Match: "a1b2c3d4..."` — required, config hash from prior GET
- `Origin` — optional, checked against allowed origins

**Request body:** Full `VaultConfig` object:
```json
{
  "name": "work",
  "capture": { "folder": "", "template": "generic-note" },
  "daily": { "folder": "", "template": "daily-note", "catch_up": false },
  "editor": { "live_preview": true, "default_mode": "source" },
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
  -d '{"name":"work","capture":{"folder":"","template":"generic-note"},"daily":{"folder":"","template":"daily-note","catch_up":false},"editor":{"live_preview":true,"default_mode":"source"},"git":{"enabled":false},"hooks":{}}'

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

## Search

### `GET /api/v/{vault}/search`

Full-text search across note titles and body content using Tantivy.

**Query parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `q` | string | yes | — | Search query |
| `limit` | integer | no | 20 | Maximum results |

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/search?q=Acme+onboarding&limit=5"
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

## SQL Query

### `POST /api/v/{vault}/query/sql`

Execute read-only SQL against the SQLite cache. Only `SELECT` and `WITH` statements are allowed.

**Request body:**
```json
{
  "sql": "SELECT title, state FROM v_customers ORDER BY title",
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
| Globex Industries | Onboarding |
```

**Errors:**
- `400` — non-SELECT statement attempted
- `404` — vault not found
- `422` — SQL syntax or execution error

**Example:**
```bash
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT title, state FROM v_customers ORDER BY title"}'
```

```bash
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT title, state FROM v_customers ORDER BY title","format":"markdown","max_rows":25}'
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
| `customer` | Filter by customer name |
| `due_before` | Filter to tasks due before this date (`YYYY-MM-DD`) |
| `limit` | Maximum results (default: 200) |

**Response:** `200 OK`
```json
[
  {
    "task_hash": "abc123...",
    "note_path": "Customers/Acme/Streams/Migration to v2.md",
    "heading_path": null,
    "ordinal": 2,
    "status": "in_progress",
    "text": "Testing in staging",
    "customer": null,
    "stream": null,
    "owner": null,
    "due": null,
    "scheduled": "2025-01-20",
    "start_date": null,
    "done_at": null,
    "priority": null
  }
]
```

**Example:**
```bash
curl "http://127.0.0.1:27183/api/v/work/tasks?status=todo&customer=Acme"
```

### `POST /api/v/{vault}/tasks`

Add a new To Do task to an existing note.

**Request body:**
```json
{
  "note_path": "Customers/Acme/Acme Corp.md",
  "description": "Follow up on SLA requirements",
  "customer": "Acme",
  "stream": "Migration to v2",
  "due": "2025-02-01",
  "priority": "high"
}
```

Only `note_path` and `description` are required. Inline fields (`customer`, `stream`, `owner`) appear as `[key:: value]` in the task line. The `due` date appears as `📅 YYYY-MM-DD`.

**Response:** `201 Created`
```json
{ "path": "Customers/Acme/Acme Corp.md", "hash": "2d7d0d..." }
```

### `POST /api/v/{vault}/tasks/toggle`

Toggle a task to a new status using its content hash (blake3 of the raw task line). The engine finds the matching line, validates the transition, rewrites it in place, and runs the save pipeline.

**Request body:**
```json
{
  "note_path": "Customers/Acme/Streams/Migration to v2.md",
  "task_hash": "abc123...",
  "status": "done"
}
```

`status` is the preferred field name. The server still accepts `new_status` for older clients.

**Response:** `200 OK`
```json
{ "path": "Customers/Acme/Streams/Migration to v2.md", "hash": "ef456..." }
```

**Errors:**
- `404` — task hash not found in the note
- `409` — hash matches more than one task (collision)
- `422` — invalid status string or disallowed status transition

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
      { "name": "title", "type": "text", "required": true },
      { "name": "folder", "type": "text", "required": false }
    ]
  }
]
```

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

Apply routing rules to move notes to their destinations. Stamps `archived: true` and `archived-at` in frontmatter before moving.

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
      "rule_id": "external-meeting"
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

Fetch the daily note for the given date.

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
  "timestamp": "2026-05-09T16:30:00.123-0700"
}
```

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

### `POST /api/app/vaults`

Registers a new vault.

**Body:**
```json
{ "name": "personal", "path": "/home/user/vaults/personal" }
```

**Response:** `201 Created`
```json
{ "name": "personal", "status": "registered" }
```

**Errors:**
- `409` — vault name already registered
- `422` — path does not exist

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

Unregisters a vault. Does not delete files.

**Response:** `204 No Content`

**Errors:**
- `404` — vault not found
- `422` — cannot remove the default vault

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
