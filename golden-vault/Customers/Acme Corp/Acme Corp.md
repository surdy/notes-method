---
kind: customer
tags:
  - enterprise
domains:
  - acme.com
created: 2024-06-01 10:00
updated: 2026-05-22 04:13
---

# Acme Corp

## Overview

Acme Corp is our largest enterprise customer. ^summary-block

## People

| Name | Role | Email |
| --- | --- | :---: |
| [[John Smith]] | VP Engineering | john@acme.com |
| [[Jane Doe]] | CTO | jane@acme.com |

[account-tier:: Enterprise]
[renewal-date:: 2025-12-01]
[arr:: $500k]

## Streams

```notesmith sql
SELECT n.title, s.value AS status, n.path
FROM v_notes n
JOIN v_field_values c ON c.vault_name = n.vault_name AND c.note_path = n.path
 AND c.key = 'customers' AND c.value = '[[Acme Corp]]'
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
 AND c.key = 'customers' AND c.value = '[[Acme Corp]]'
JOIN v_field_values k ON k.vault_name = n.vault_name AND k.note_path = n.path
 AND k.key = 'kind' AND k.value = 'meeting'
LEFT JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
 AND d.key = 'date'
ORDER BY d.value DESC LIMIT 15
```

> [!note] Account Notes
> This is a strategic account.
> Handle with care.
> Multiple lines of context here.

#acme #enterprise #strategic
