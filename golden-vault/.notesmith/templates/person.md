---
name: person
description: "New person (create lazily, when someone recurs)"
output_path: "Inbox/{{ name }}.md"
prompts:
  - { name: name, type: text, required: true }
  - { name: org, type: text, required: false }
  - { name: role, type: text, required: false }
---
---
kind: person
{% if org %}org: "{{ org }}"
{% endif %}{% if role %}role: "{{ role }}"
{% endif %}---

# {{ name }}

## Context

## Meetings

```notesmith sql
SELECT d.value AS date, n.title, n.path
FROM v_notes n
JOIN v_field_values a ON a.vault_name = n.vault_name AND a.note_path = n.path
 AND a.key = 'attendees' AND a.value = '[[{{ name }}]]'
LEFT JOIN v_field_values d ON d.vault_name = n.vault_name AND d.note_path = n.path
 AND d.key = 'date'
ORDER BY d.value DESC
```
