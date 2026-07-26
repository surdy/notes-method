---
kind: meeting
audience: external
meeting_type: status
date: 2025-01-14
customers:
  - "[[Acme Corp]]"
streams:
  - "[[Migration to v2]]"
attendees:
  - "[[John Smith]]"
  - "[[Jane Doe]]"
tags:
  - migration
created: 2025-01-14 14:00
updated: 2025-01-14 15:30
---

# Customer Check-in — 2025-01-14

## Attendees

- [[John Smith|John]] (VP Engineering, Acme)
- [[Jane Doe|Jane]] (CTO, Acme)

## Notes

Walked through the migration timeline. John raised concerns about [[Widget API#^pricing-block]].

> [!important] Customer Feedback
> Acme wants zero-downtime migration.
> This changes our rollback strategy significantly.
> We need to revisit the cutover plan.

## Decisions

Rollback strategy will be revisited before the cutover.

## Tasks

- [ ] Prepare zero-downtime proposal [due:: 2025-01-20] ⏫
- [w] Waiting for Acme to share their SLA requirements 📅 2025-01-21

[sentiment:: positive]
[follow-up:: true]
