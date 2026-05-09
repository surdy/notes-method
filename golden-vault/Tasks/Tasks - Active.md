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
SELECT path, content, due_date, priority
FROM v_tasks
WHERE status IN ('todo', 'in_progress')
ORDER BY due_date ASC
```

## By Customer

```notesmith sql
SELECT customer, COUNT(*) as task_count
FROM v_tasks
WHERE status IN ('todo', 'in_progress')
GROUP BY customer
ORDER BY task_count DESC
```
