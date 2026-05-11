---
type: dashboard
tags:
  - tasks
  - dashboard
created: 2025-01-01 08:00
updated: 2025-01-15 08:00
---

# Active Tasks

```notesmith sql
SELECT
  note_path AS path,
  text AS content,
  due AS due_date,
  priority
FROM v_tasks
WHERE status IN ('todo', 'in_progress')
ORDER BY due IS NULL, due ASC, priority DESC
```

## By Customer

```notesmith sql
SELECT COALESCE(customer, 'Unassigned') AS customer, COUNT(*) as task_count
FROM v_tasks
WHERE status IN ('todo', 'in_progress')
GROUP BY COALESCE(customer, 'Unassigned')
ORDER BY task_count DESC
```
