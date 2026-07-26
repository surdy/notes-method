---
tags:
  - dashboard
created: 2025-01-01 08:00
updated: 2025-01-16 08:00
---

# Home Dashboard

## Active streams by priority

```notesmith sql
SELECT p.value AS priority, n.title, n.path
FROM v_notes n
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'stream'
JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
 AND s.key = 'status' AND s.value = 'active'
LEFT JOIN v_field_values p ON p.vault_name = n.vault_name AND p.note_path = n.path
 AND p.key = 'priority'
ORDER BY p.value, n.title
```

## Blocked and waiting streams

```notesmith sql
SELECT s.value AS status, n.title, n.path
FROM v_notes n
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'stream'
JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
 AND s.key = 'status' AND s.value IN ('blocked', 'waiting')
ORDER BY s.value, n.title
```

## Stale active streams — no meeting in 30 days

```notesmith sql
SELECT n.title, n.path
FROM v_notes n
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'stream'
JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
 AND s.key = 'status' AND s.value = 'active'
WHERE NOT EXISTS (
  SELECT 1
  FROM v_field_values ms
  JOIN v_field_values md ON md.vault_name = ms.vault_name AND md.note_path = ms.note_path
   AND md.key = 'date' AND md.value >= date('now', '-30 days')
  WHERE ms.vault_name = n.vault_name
    AND ms.key = 'streams' AND ms.value = '[[' || n.title || ']]'
)
ORDER BY n.title
```

## Open tasks by due date

```notesmith sql
SELECT t.text, t.note_path, due.value AS due
FROM v_tasks t
LEFT JOIN v_task_effective_fields due
  ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
WHERE t.status_group = 'open'
ORDER BY due.value IS NULL, due.value, t.note_path
LIMIT 20
```

## Open tasks I owe Acme Corp

```notesmith sql
SELECT t.text, t.note_path, due.value AS due
FROM v_tasks t
JOIN v_task_effective_fields c
  ON c.vault_name = t.vault_name AND c.task_id = t.id
 AND c.key = 'customers' AND c.value = '[[Acme Corp]]'
LEFT JOIN v_task_effective_fields due
  ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
WHERE t.status_group = 'open'
ORDER BY due.value IS NULL, due.value
```

## Inbox triage

```notesmith sql
SELECT path, title FROM v_notes WHERE path LIKE 'Inbox/%' ORDER BY path
```

## Meetings missing customers or audience

```notesmith sql
SELECT n.path, n.title
FROM v_notes n
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'meeting'
WHERE NOT EXISTS (
        SELECT 1 FROM v_field_values c
        WHERE c.vault_name = n.vault_name AND c.note_path = n.path AND c.key = 'customers'
      )
   OR NOT EXISTS (
        SELECT 1 FROM v_field_values a
        WHERE a.vault_name = n.vault_name AND a.note_path = n.path AND a.key = 'audience'
      )
ORDER BY n.path
```

## External meetings breaking the one-customer invariant

```notesmith sql
SELECT n.path, n.title, COUNT(c.value) AS customer_count
FROM v_notes n
JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
 AND a.key = 'audience' AND a.value = 'external'
LEFT JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers'
GROUP BY n.vault_name, n.path, n.title
HAVING customer_count != 1
```
