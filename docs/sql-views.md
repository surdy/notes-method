# SQL Views Reference

Notesmith exposes stable SQL views as its query API. Views are the public contract — underlying tables may change between versions.

```bash
notesmith query sql "SELECT * FROM v_notes LIMIT 5"
```

## v_notes

Core note metadata.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `path` | TEXT | Relative note path |
| `title` | TEXT | Display title |
| `created_at` | TEXT | `created` frontmatter value when present |
| `updated_at` | TEXT | `updated` frontmatter value when present |
| `word_count` | INTEGER | Body word count |

Example:

```sql
SELECT path, title, updated_at FROM v_notes ORDER BY updated_at DESC LIMIT 10;
```

## v_fields

Flattened note fields from frontmatter and inline note fields. A list-valued
field is stored here as **one row** whose `value` is the serialized YAML list;
for per-element membership queries use [`v_field_values`](#v_field_values).

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `note_path` | TEXT | Note path |
| `key` | TEXT | Field key |
| `value` | TEXT | Stored scalar or serialized list value |
| `value_type` | TEXT | `string`, `date`, `number`, `link`, `list`, or `boolean` |

Example:

```sql
SELECT note_path, value FROM v_fields WHERE key = 'customer' ORDER BY note_path;
```

## v_field_values

Normalized field values: one row per value. Scalar fields appear as a single
row with `ordinal = 0`; **list fields are exploded into one row per element**
(ordinal = position in the list). This is the preferred surface for membership
queries — unlike `v_fields`, where a list is one serialized string, an exact
`key`/`value` match here uses an index and cannot produce substring false
positives.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `note_path` | TEXT | Note path |
| `key` | TEXT | Field key |
| `ordinal` | INTEGER | Position within a list value; `0` for scalars |
| `value` | TEXT | Scalar value or one list element (nested structures serialized) |
| `value_type` | TEXT | `string`, `date`, `number`, `link`, `list`, or `boolean` |
| `source` | TEXT | `frontmatter` or `inline` |

A zero-item list contributes no rows.

Example — all meetings involving Acme, regardless of how many other customers
are on the meeting:

```sql
SELECT n.path, n.title
FROM v_notes n
JOIN v_field_values c
  ON c.vault_name = n.vault_name AND c.note_path = n.path
WHERE c.key = 'customers' AND c.value = '[[Acme]]';
```

## v_tasks

Parsed tasks with generic statuses.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `id` | INTEGER | Task row id |
| `note_path` | TEXT | Note containing the task |
| `line_number` | INTEGER | 1-based line number in the note |
| `text` | TEXT | Task text content |
| `status_char` | TEXT | Raw checkbox marker |
| `status_group` | TEXT | `open` or `done` |
| `note_title` | TEXT | Title of the containing note |

Example:

```sql
SELECT text, note_path FROM v_tasks WHERE status_group = 'open' ORDER BY note_path, line_number;
```

## v_task_fields

Inline task fields such as `[due:: 2026-06-01]`.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `task_id` | INTEGER | Task row id |
| `key` | TEXT | Field key |
| `value` | TEXT | Field value |
| `note_path` | TEXT | Parent note path |

Example:

```sql
SELECT t.text, due.value AS due
FROM v_tasks t
LEFT JOIN v_task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
WHERE t.status_group = 'open';
```

## v_task_effective_fields

Effective task metadata with inheritance: each task's own inline fields
(`source = 'task'`) plus the containing note's **frontmatter** fields
(`source = 'note'`) for every key the task does not override. Note-level
*inline* fields are paragraph-scoped and never inherited. List-valued note
fields contribute one row per member (via `v_field_values`), so membership
queries work here too.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `task_id` | INTEGER | Task row id |
| `note_path` | TEXT | Containing note path |
| `key` | TEXT | Field key |
| `value` | TEXT | Field value (one row per list member for inherited lists) |
| `source` | TEXT | `task` (own inline field) or `note` (inherited frontmatter) |

Example — open tasks for Acme, whether annotated on the task or inherited from
an Acme meeting note:

```sql
SELECT t.text, t.note_path, due.value AS due
FROM v_tasks t
JOIN v_task_effective_fields c
  ON c.vault_name = t.vault_name AND c.task_id = t.id
LEFT JOIN v_task_effective_fields due
  ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
WHERE t.status_group = 'open'
  AND c.key = 'customers' AND c.value = '[[Acme]]'
ORDER BY due.value IS NULL, due.value;
```

## v_backlinks

Resolved inbound links. Wikilinks inside frontmatter values (e.g. a
`customers` list of `"[[Acme]]"` entries) are indexed as link edges too, so
entity notes get backlinks from every note referencing them in metadata.

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `source_path` | TEXT | Note containing the link |
| `target_path` | TEXT | Parsed link target when resolvable |
| `link_text` | TEXT | Alias/markdown text when present |
| `source_title` | TEXT | Title of the source note |
| `source` | TEXT | `body` or `frontmatter` |

Example:

```sql
SELECT source_path, source_title FROM v_backlinks WHERE target_path = 'Acme Corp';
```

## v_dangling_links

Wikilink-style references (`wikilink`, `embed`, `heading_ref`, `block_ref`)
whose target does not resolve to any note in the vault — i.e. concepts referenced
but with no page. This is the **dangling-links lint signal** (issue #265): the
complement of `v_backlinks`, using the same target-resolution rules
(match a note by `title`, exact `path`, `target + '.md'`, or `%/target.md'`).
External and inline Markdown links are never dangling (they carry no note target).

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `source_path` | TEXT | Note containing the unresolved link |
| `source_title` | TEXT | Title of the source note |
| `raw_target` | TEXT | The referenced target as written (e.g. `Acme Corp`) |
| `link_text` | TEXT | Alias/display text when present |
| `kind` | TEXT | `wikilink`, `embed`, `heading_ref`, or `block_ref` |
| `line_number` | INTEGER | 1-based line of the link in the source note; NULL for frontmatter links |
| `source` | TEXT | `body` or `frontmatter` (unresolved frontmatter links — e.g. attendees without a People note — surface here) |

Example — the most-referenced missing concepts (candidates for a new note):

```sql
SELECT raw_target, COUNT(*) AS refs
FROM v_dangling_links
GROUP BY raw_target
ORDER BY refs DESC, raw_target;
```

## v_periodic

| Column | Type | Description |
|--------|------|-------------|
| `vault_name` | TEXT | Vault identifier |
| `note_path` | TEXT | Note path |
| `period_kind` | TEXT | `daily`, `monthly`, or `yearly` |
| `period_key` | TEXT | Canonical period key |
| `period_start` | TEXT | Inclusive period start |
| `period_end` | TEXT | Inclusive period end |
| `title` | TEXT | Note title |

Example:

```sql
SELECT note_path, period_kind FROM v_periodic ORDER BY period_start DESC;
```

## User-defined views (`.notesmith/views.sql`)

Notesmith also loads vault-scoped user views from `.notesmith/views.sql` into the cache database.

Rules:
- Only `CREATE VIEW` statements are loaded.
- Statements are executed independently; a bad statement is logged and skipped.
- User views can query the stable public views above.

Example:

```sql
CREATE VIEW customer_notes AS
SELECT n.path, n.title, customer.value AS customer, status.value AS status
FROM v_notes n
JOIN v_fields note_type
  ON note_type.vault_name = n.vault_name
 AND note_type.note_path = n.path
 AND note_type.key = 'type'
LEFT JOIN v_fields customer
  ON customer.vault_name = n.vault_name
 AND customer.note_path = n.path
 AND customer.key = 'customer'
LEFT JOIN v_fields status
  ON status.vault_name = n.vault_name
 AND status.note_path = n.path
 AND status.key = 'status'
WHERE note_type.value = 'customer';
```
