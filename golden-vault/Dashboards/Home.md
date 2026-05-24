---
type: dashboard
tags:
  - dashboard
  - home
created: 2025-01-01 08:00
updated: 2025-01-15 08:00
---

# Home Dashboard

## Inbox Count

```notesmith sql
SELECT COUNT(*) as inbox_count FROM v_notes WHERE path LIKE 'Inbox/%'
```

## Active Streams

```notesmith sql
SELECT
  n.path,
  n.title,
  status.value AS status,
  priority.value AS priority,
  customer.value AS customer
FROM v_notes n
JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type'
LEFT JOIN v_fields status ON status.vault_name = n.vault_name AND status.note_path = n.path AND status.key = 'status'
LEFT JOIN v_fields priority ON priority.vault_name = n.vault_name AND priority.note_path = n.path AND priority.key = 'priority'
LEFT JOIN v_fields customer ON customer.vault_name = n.vault_name AND customer.note_path = n.path AND customer.key = 'customer'
WHERE note_type.value = 'stream' AND status.value = 'In Progress'
ORDER BY priority.value
```

## Recent Notes

```notesmith sql
SELECT path, title, updated_at
FROM v_notes
ORDER BY updated_at DESC
LIMIT 10
```
