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
