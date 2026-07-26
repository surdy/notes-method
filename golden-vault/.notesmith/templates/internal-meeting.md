---
name: internal-meeting
description: "New internal meeting — fastest capture; customers/streams added during enrichment"
output_path: "Inbox/{{ date }} - {{ title }}.md"
prompts:
  - { name: title, type: text, required: true }
---
---
kind: meeting
audience: internal
date: {{ date }}
customers: []
streams: []
attendees: []
---

# {{ date }} — {{ title }}

## Notes

## Decisions

## Tasks

- [ ]
