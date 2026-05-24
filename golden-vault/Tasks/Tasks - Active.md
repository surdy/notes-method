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
  t.note_path AS path,
  t.text AS content,
  due.value AS due_date,
  priority.value AS priority
FROM v_tasks t
LEFT JOIN v_task_fields due ON due.vault_name = t.vault_name AND due.task_id = t.id AND due.key = 'due'
LEFT JOIN v_task_fields priority ON priority.vault_name = t.vault_name AND priority.task_id = t.id AND priority.key = 'priority'
WHERE t.status_group = 'open'
ORDER BY due.value IS NULL, due.value ASC, priority.value DESC
```

## By Customer

```notesmith sql
SELECT COALESCE(customer.value, 'Unassigned') AS customer, COUNT(*) as task_count
FROM v_tasks t
LEFT JOIN v_task_fields customer ON customer.vault_name = t.vault_name AND customer.task_id = t.id AND customer.key = 'customer'
WHERE t.status_group = 'open'
GROUP BY COALESCE(customer.value, 'Unassigned')
ORDER BY task_count DESC
```
