# SQL Views Reference

Notesmith exposes stable SQL views as its query API. Views are the public contract — underlying tables may change between versions.

Query views through the CLI or HTTP API:

```bash
# CLI (requires daemon)
notesmith query sql "SELECT * FROM v_notes LIMIT 5"

# HTTP
curl -s http://127.0.0.1:27183/api/v/work/query/sql \
  -H 'content-type: application/json' \
  -d '{"sql":"SELECT * FROM v_notes LIMIT 5"}'
```

---

## v_notes

All notes in the vault with metadata from frontmatter.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `path` | TEXT | Relative path from vault root |
| `title` | TEXT | Note title (file stem) |
| `type` | TEXT | Frontmatter type: `customer`, `meeting`, `stream`, `daily`, `note`, `dashboard`, `contact`, `account-info`, `glossary`, `milestones`, `other` |
| `customer` | TEXT | Customer name (if applicable) |
| `stream` | TEXT | Work stream name (if applicable) |
| `state` | TEXT | Customer state (e.g., `Active`, `Churned`) |
| `status` | TEXT | Stream status (e.g., `active`, `paused`) |
| `date` | TEXT | Date for daily/meeting notes |
| `created_at` | TEXT | Creation timestamp from frontmatter |
| `updated_at` | TEXT | Last update timestamp from frontmatter |
| `archived` | INTEGER | 1 if archived, 0 otherwise |
| `mtime_unix` | INTEGER | File modification time (unix epoch) |
| `frontmatter_json` | TEXT | Full frontmatter as JSON |

**Examples:**

```sql
-- Active customers
SELECT title, state FROM v_notes WHERE type = 'customer' AND state = 'Active';

-- Recent meetings
SELECT title, customer, date FROM v_notes WHERE type = 'meeting' ORDER BY date DESC LIMIT 10;

-- Notes by type
SELECT type, COUNT(*) as count FROM v_notes GROUP BY type ORDER BY count DESC;
```

---

## v_tasks

All tasks extracted from notes, with status and metadata.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `task_hash` | TEXT | Content-hash anchor for stable identification |
| `note_path` | TEXT | Path to the note containing the task |
| `heading_path` | TEXT | Heading context within the note |
| `ordinal` | INTEGER | Position within the note (0-indexed) |
| `status` | TEXT | `todo`, `in_progress`, `blocked`, `waiting`, `on_hold`, `done`, `cancelled` |
| `text` | TEXT | Task text content |
| `customer` | TEXT | From `[customer:: ...]` inline field |
| `stream` | TEXT | From `[stream:: ...]` inline field |
| `owner` | TEXT | From `[owner:: ...]` inline field |
| `due` | TEXT | Due date (from 📅 emoji) |
| `scheduled` | TEXT | Scheduled date (from ⏳ emoji) |
| `start_date` | TEXT | Start date (from 🛫 emoji) |
| `done_at` | TEXT | Completion date (from ✅ emoji) |
| `priority` | INTEGER | 1 (lowest) to 5 (highest) |

**Examples:**

```sql
-- Open tasks due this week
SELECT text, due, note_path FROM v_tasks
WHERE status IN ('todo', 'in_progress') AND due IS NOT NULL
ORDER BY due;

-- Blocked tasks by customer
SELECT text, customer, note_path FROM v_tasks
WHERE status = 'blocked' AND customer IS NOT NULL;

-- Task counts by status
SELECT status, COUNT(*) as count FROM v_tasks GROUP BY status ORDER BY count DESC;
```

---

## v_backlinks

Links pointing to each note (reverse link graph).

| Column | Type | Description |
|--------|------|-------------|
| `note_path` | TEXT | The note being linked to |
| `backlink_path` | TEXT | The note containing the link |
| `kind` | TEXT | Link type: `wikilink`, `embed`, `heading_ref`, `block_ref`, `markdown_link` |
| `heading_ref` | TEXT | Heading reference (if `heading_ref` kind) |
| `block_ref` | TEXT | Block reference (if `block_ref` kind) |

**Examples:**

```sql
-- What links to Acme Corp?
SELECT backlink_path, kind FROM v_backlinks
WHERE note_path LIKE '%Acme Corp%';

-- Most-linked notes
SELECT note_path, COUNT(*) as backlink_count FROM v_backlinks
GROUP BY note_path ORDER BY backlink_count DESC LIMIT 10;
```

---

## v_customers

Convenience view — filters `v_notes` to `type = 'customer'`. Same columns as `v_notes`.

```sql
SELECT title, state FROM v_customers ORDER BY title;
```

---

## v_streams

Convenience view — filters `v_notes` to `type = 'stream'`. Same columns as `v_notes`.

```sql
SELECT title, customer, status FROM v_streams WHERE status = 'active';
```
