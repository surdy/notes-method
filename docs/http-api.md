# Notesmith HTTP API Reference

The daemon listens on `127.0.0.1:27183` by default (configurable via `--bind` or `daemon.bind` in config).

All API endpoints are unauthenticated — the daemon is designed for local use.

---

## Health

### `GET /ping`

Health check.

**Response:** `200 OK`
```json
{ "status": "ok" }
```

---

## Notes

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

### `POST /api/v/{vault}/notes`

Create a new note. The server writes `{folder}/{title}.md`, defaults `folder` to `Inbox`, runs the save pipeline, and returns the written hash.

**Request body:**
```json
{
  "title": "Follow Up",
  "folder": "Inbox",
  "content": "Body text",
  "frontmatter": {
    "status": "draft"
  }
}
```

**Response:** `201 Created`
```json
{
  "path": "Inbox/Follow Up.md",
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
  "path": "Inbox/Follow Up.md",
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
  "path": "Inbox/Follow Up.md",
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
  "path": "Inbox/Follow Up.md",
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
  "from": "Inbox/Follow Up.md",
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
{ "sql": "SELECT title, state FROM v_customers ORDER BY title" }
```

**Response:** `200 OK`
```json
{
  "columns": ["title", "state"],
  "rows": [
    ["Acme Corp", "Active"],
    ["Globex Industries", "Onboarding"]
  ],
  "row_count": 2
}
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

See [SQL Views Reference](sql-views.md) for available views.

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
  "new_status": "done"
}
```

**Response:** `200 OK`
```json
{ "path": "Customers/Acme/Streams/Migration to v2.md", "hash": "ef456..." }
```

**Errors:**
- `404` — task hash not found in the note
- `409` — hash matches more than one task (collision)
- `422` — invalid status string or disallowed status transition
