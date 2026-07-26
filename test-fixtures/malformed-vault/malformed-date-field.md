---
kind: meeting
audience: internal
date: not-a-date
customers:
  - "[[Acme Corp]]"
---

# Malformed Date Field

Regression fixture: the Work Notes routing rules file meetings by
`{{ field.date | year }}/{{ field.date | month }}`. The `year`/`month` filters
used to slice the raw string blindly, so a non-ISO `date` produced a nonsense
destination (`Meetings/not-/-d/…`) — and a `date:` holding a YAML list produced
an empty path segment (`Meetings/nest//…`). Routing must now decline instead.

Parsing, indexing, and embedding this note must still degrade gracefully rather
than fail the batch (ADR 0009).
