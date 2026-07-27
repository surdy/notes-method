---
tags:
  - dashboard
  - weekly
---

# Weekly Review

## Attendees who deserve a People note

```notesmith sql
SELECT fv.value AS person, COUNT(*) AS mentions
FROM v_field_values fv
WHERE fv.key = 'attendees'
  AND NOT EXISTS (
    SELECT 1 FROM v_notes p
    WHERE p.vault_name = fv.vault_name
      AND p.title = replace(replace(fv.value, '[[', ''), ']]', '')
  )
GROUP BY fv.value
ORDER BY mentions DESC, person
```

## Multi-customer work

```notesmith sql
SELECT note_path, COUNT(*) AS customer_count
FROM v_field_values
WHERE key = 'customers'
GROUP BY note_path
HAVING customer_count > 1
ORDER BY customer_count DESC, note_path
```

## Unresolved links worth fixing

```notesmith sql
SELECT raw_target, COUNT(*) AS mentions
FROM v_dangling_links
GROUP BY raw_target
ORDER BY mentions DESC, raw_target
LIMIT 20
```
