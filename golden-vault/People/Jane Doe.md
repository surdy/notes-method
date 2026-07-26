---
kind: person
org: "[[Acme Corp]]"
role: CTO
created: 2024-06-01 10:00
updated: 2025-01-14 14:00
---

# Jane Doe

## Context

CTO at [[Acme Corp]]. Owns the technical side of the v2 migration.

## Meetings

```notesmith sql
SELECT d.value AS date, n.title, n.path
FROM v_notes n
JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
 AND a.key = 'attendees' AND a.value = '[[Jane Doe]]'
LEFT JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
 AND d.key = 'date'
ORDER BY d.value DESC
```
