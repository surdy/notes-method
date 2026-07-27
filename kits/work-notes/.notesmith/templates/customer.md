---
name: customer
description: "New customer"
output_path: "Customers/{{ name }}/{{ name }}.md"
prompts:
  - { name: name, type: text, required: true }
---
---
kind: customer
---

# {{ name }}

## Overview

## People

## Streams

```notesmith sql
SELECT n.title, s.value AS status, n.path
FROM v_notes n
JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers' AND c.value = '[[{{ name }}]]'
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'stream'
LEFT JOIN v_field_values s ON s.vault_name = n.vault_name AND s.note_path = n.path
 AND s.key = 'status'
ORDER BY s.value, n.title
```

## Recent meetings

```notesmith sql
SELECT d.value AS date, n.title, n.path
FROM v_notes n
JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers' AND c.value = '[[{{ name }}]]'
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'meeting'
LEFT JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
 AND d.key = 'date'
ORDER BY d.value DESC LIMIT 15
```
