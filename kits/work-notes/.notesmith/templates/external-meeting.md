---
name: external-meeting
description: "New customer-attended meeting — always exactly one customer"
output_path: "Inbox/{{ date }} - {{ customer }} - {{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
  - { name: customer, type: text, required: true }
  - { name: stream, type: text, required: false }
---
---
kind: meeting
audience: external
date: {{ date }}
customers:
  - "{{ customer | as_wikilink }}"
streams:{% if stream %}
  - "{{ stream | as_wikilink }}"{% else %} []{% endif %}
attendees: []
---

# {{ date }} — {{ customer }} — {{ title }}

## Attendees

## Notes

## Decisions

## Tasks

- [ ]
