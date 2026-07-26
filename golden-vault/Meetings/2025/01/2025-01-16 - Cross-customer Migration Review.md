---
kind: meeting
audience: internal
meeting_type: retrospective
date: 2025-01-16
customers:
  - "[[Acme Corp]]"
  - "[[Globex]]"
streams:
  - "[[Migration to v2]]"
  - "[[Platform Rollout]]"
attendees:
  - "[[Sarah Chen]]"
  - "[[Dana Kim]]"
  - "[[Mike Alvarez]]"
tags:
  - migration
  - rollout
created: 2025-01-16 15:00
updated: 2025-01-16 16:00
---

# Cross-customer Migration Review — 2025-01-16

An internal review spanning both migration programs — the case where a meeting
legitimately relates to several customers and several streams.

## Notes

Compared the [[Migration to v2]] cutover plan against the [[Platform Rollout]]
pilot. Both hit the same auth service dependency; Sarah owns the shared upgrade.

## Decisions

Auth service upgrade is tracked once, under [[Migration to v2]].

## Tasks

- [ ] Publish a shared auth-upgrade timeline [owner:: [[Sarah Chen]]] [due:: 2025-01-23]
- [ ] Fold pilot feedback into the cutover checklist [due:: 2025-01-30]
