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
SELECT path, title, status, priority, customer
FROM v_notes
WHERE type = 'stream' AND status = 'In Progress'
ORDER BY priority
```

## Recent Notes

```notesmith sql
SELECT path, title, updated FROM v_notes ORDER BY updated DESC LIMIT 10
```
