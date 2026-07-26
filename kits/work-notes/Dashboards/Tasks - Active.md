---
tags:
  - dashboard
  - tasks
---

# Active Tasks

Tasks inherit their containing note's frontmatter, so customer/stream columns come
from `v_task_effective_fields` — never from metadata copied onto the task.

## By due date

```notesmith sql
SELECT
  t.note_path AS path,
  t.text AS content,
  due.value AS due_date,
  owner.value AS owner
FROM v_tasks t
LEFT JOIN v_task_effective_fields due
  ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
LEFT JOIN v_task_effective_fields owner
  ON owner.vault_name = t.vault_name AND owner.task_id = t.id AND owner.key = 'owner'
WHERE t.status_group = 'open'
ORDER BY due_date IS NULL, due_date ASC, path
```

## By customer

```notesmith sql
SELECT COALESCE(c.value, 'Unassigned') AS customer, COUNT(*) AS task_count
FROM v_tasks t
LEFT JOIN v_task_effective_fields c
  ON c.vault_name = t.vault_name AND c.task_id = t.id AND c.key = 'customers'
WHERE t.status_group = 'open'
GROUP BY COALESCE(c.value, 'Unassigned')
ORDER BY task_count DESC, customer
```

## Delegated — owned by someone else

```notesmith sql
SELECT owner.value AS owner, t.text, t.note_path
FROM v_tasks t
JOIN v_task_effective_fields owner
  ON owner.vault_name = t.vault_name AND owner.task_id = t.id
 AND owner.key = 'owner' AND owner.source = 'task'
WHERE t.status_group = 'open'
ORDER BY owner, t.note_path
```
