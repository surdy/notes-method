---
kind: stream
status: active
priority: P1
customers:
  - "[[Acme Corp]]"
started: 2024-11-01
target: 2025-03-31
tags:
  - migration
created: 2024-11-01 09:00
updated: 2025-01-15 16:00
---

# Migration to v2

## Objective

Move Acme onto the v2 platform without downtime.

## Phase 1

Database schema migration completed. ^phase-1-block

- [x] Schema design ✅ 2024-11-15
- [x] Data migration script ✅ 2024-12-01
- [/] Testing in staging ⏳ 2025-01-20

## Phase 2

API endpoint migration — planned.

- [ ] Design new endpoints [due:: 2025-02-01] 🔼
- [b] Blocked on auth service upgrade 📅 2025-01-25
- [w] Awaiting customer sign-off on breaking changes 🛫 2025-02-15

## Decisions

Zero-downtime cutover is a hard requirement.

## Open questions

Does Phase 2 slip if the auth service upgrade slips?

## Related

- [[Acme Corp]]
- [[API Integration]]
- [[2025-01-15]]

> [!warning] Risk
> Timeline depends on auth service team.
> If delayed, Phase 2 slips to Q2.

[effort:: large]
[risk:: medium]
