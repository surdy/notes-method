---
name: stream
description: "New stream of work"
output_path: "Inbox/{{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
  - { name: customer, type: field-picker, field: customers, required: false }
  - { name: priority, type: field-picker, field: priority, required: false }
---
---
kind: stream
status: active
{% if priority %}priority: {{ priority }}
{% endif %}customers:{% if customer %}
  - "{{ customer | as_wikilink }}"{% else %} []{% endif %}
started: {{ date }}
---

# {{ title }}

## Objective

## Current state

## Decisions

## Open questions

## Tasks

- [ ]
