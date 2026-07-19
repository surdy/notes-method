# Work Notes v2: Simplified Method + Search Primitives

Continuation of `plans/work-notes-organization-handoff.md`. That doc's settled
decisions stand (central date-foldered meetings, plural `customers` wikilinks,
`audience` field, streams as global entities). This doc records the decisions
made on 2026-07-19 that close the handoff's open questions, defines the
simplified vault method, and specifies the search primitives to build.

## Framing decision

Notesmith the **app keeps its full surface** — multi-vault, remote daemons,
embedded ACP chat, fact memory, embeddings, media ingestion (clip / drop-folder
/ transcription / YouTube) all stay. What gets simplified is the **method**: the
vault-side schema, fields, rules, and kit ceremony. The generic
note/field/tag/link engine stays generic; the work-notes schema becomes the
blessed, documented default rather than "one possible configuration."

Search primitives are built **generically** (normalized list fields, filtered
search) rather than as per-type bespoke tools — less code, and the schema can
evolve without code changes.

## Closed questions (decided 2026-07-19)

| Question | Decision |
|---|---|
| People/attendees | Single `attendees` list of wikilinks on meetings. Person notes created **lazily** — only when someone recurs or has durable context. Dangling attendee links are acceptable; `v_dangling_links` ranks who deserves a note. `audience` stays explicitly authored. |
| Task metadata | Tasks **inherit** customers/streams/date from their containing note (SQL join). Inline fields only for exceptions: `[due:: ...]` for real deadlines, `[owner:: [[Jane]]]` only when delegated (default owner = me). Manual/other-source tasks live in the daily note or a stream note and inherit from there. |
| Stream statuses | `active` / `waiting` / `blocked` / `done`. No `inbox` (that's the Inbox folder's job), no `archived` (done is done; files don't move). Priority stays `P0`–`P3`. |
| Routing | Deterministic kind-based rules. Enrichment is manual or agentic; **filing is mechanical**. |
| Note kind | `kind` is the canonical type field. Tags are purely topical (`renewal`, `escalation`, …), never kinds. |
| `meeting_type` | Dropped for now. Business purpose of a meeting is a tag if needed. |
| Stream `owners` | Dropped. Everything is mine; delegation happens at task level via `[owner::]`. |
| Account info granularity | Start with a single `Customers/<Name>/<Name>.md`. Split out extra notes (`Architecture.md`, `Commercial.md`, …) only when the main note hurts. Extra notes use `kind: account` + `customers: ["[[<Name>]]"]`. |
| Periodic notes | Daily (log of what I did), Weekly (summary drafted from dailies, typically by agent), Quarterly (review). Monthly/yearly unconfigured. |
| Templates vs multi-select | Template prompts stay single-value (the prompt system has no multi-select type). Meeting template prompts for zero-or-one customer; multi-customer meetings get their second customer added during enrichment (manual or agent). A multi-select prompt type is deferred until this actually hurts. |

## Vault layout

```text
Inbox/                      # landing spot until enriched + routed
Meetings/YYYY/MM/
Streams/
Customers/<Name>/<Name>.md  # + optional kind:account notes
People/
Daily/
Weekly/
Quarterly/
Dashboards/
facts/                      # fact memory (existing convention)
raw/ ingested/ transcribed/ # ingestion pipeline dirs (existing config)
.notesmith/
```

## Canonical field registry (complete)

| Field | On | Type | Values |
|---|---|---|---|
| `kind` | all | enum | `meeting` `stream` `customer` `account` `person` |
| `date` | meeting | date | |
| `audience` | meeting | enum | `internal` `external` |
| `customers` | meeting, stream, account | list of wikilinks | quoted `"[[Acme]]"` |
| `streams` | meeting | list of wikilinks | |
| `attendees` | meeting | list of wikilinks | |
| `status` | stream | enum | `active` `waiting` `blocked` `done` |
| `priority` | stream | enum | `P0`–`P3` |
| `started`, `target` | stream | date | |
| `org`, `role` | person | text (`org` may be a customer wikilink) | |

Task inline fields: `due`, `owner` (exceptions only), plus `stream`/`customer`
overrides when a task genuinely differs from its containing note.

Dropped from the old kit: singular `customer`, `meeting_type`, stream `owner`,
customer `state`, tags-as-kinds, per-customer meeting/stream folders, inline
`[customer::]` duplication on notes, `inbox`/`archived` statuses.

## Routing rules (entire file)

```yaml
version: 1
rules:
  - kind: meeting  → Meetings/{{ date | strftime('%Y/%m') }}/
  - kind: stream   → Streams/
  - kind: customer → Customers/{{ title }}/{{ title }}.md
  - kind: account  → Customers/<linked customer>/
  - kind: person   → People/
# no recognized kind → stays in Inbox for triage
```

(Exact YAML to be authored during implementation; the point is: five one-line
kind→folder rules, no customer-folder logic, no boolean combinators needed.)

## Search primitives to build (priority order)

The four retrieval classes to serve: customer-scoped digging, person-scoped
recall, stream/status rollups, task/commitment tracking.

### P1 — Normalized list-field membership (`v_field_values`) — **DONE 2026-07-19**

New table + view exposing **one row per field value**: scalar fields as-is,
list fields exploded per element with an ordinal. Columns: `vault_name`,
`note_path`, `key`, `ordinal`, `value`, `value_type`, `source`. `v_fields`
keeps its documented serialized-list contract untouched.

Enables exact indexed membership: `WHERE key='customers' AND value='[[Acme]]'`.
Generic for all list fields (`customers`, `streams`, `attendees`, future ones).
The precedent already exists in-schema: `tags` and `task_fields` are
element-normalized; generic list fields just never got the same treatment.

### P2 — Metadata-filtered `vault_search`

Add a `filters` parameter to `vault_search` (and the HTTP API):

```json
{ "query": "renewal risks", "limit": 20,
  "filters": { "fields": { "customers": "[[Acme]]", "audience": "internal" },
               "tags": ["renewal"], "path_prefix": "Meetings/" } }
```

Semantics: AND across keys; a JSON array value for one key = OR within that
key; list-field predicates use exact membership via P1. Implementation: resolve
filters to an allowed-path set in SQL, then feed the existing
`SearchIndex::search_in_paths` / `HybridSearch::search_filtered` /
`search_with_allowed_paths` machinery (the same allowed-path mechanism
`memory_recall` already uses). Lexical-only fallback unchanged. Search metrics
report whether a prefilter was applied.

This is the primitive for customer-scoped digging ("renewal risks, customers
contains [[Acme]]") and person-scoped recall ("pricing, attendees contains
[[Jane Smith]]").

### P3 — Frontmatter wikilinks become link edges

Extract wikilinks from frontmatter values into the links table with
`source='frontmatter'` (body-link parsing unchanged). Effects:

- Customer/stream/person notes get real backlinks from every meeting that
  references them — human navigation and `vault_stats` both improve.
- `v_dangling_links` starts surfacing unresolved attendees — which is exactly
  the "who deserves a People note" lint signal.

### P4 — Task inheritance view + de-CRM the generic API — **DONE 2026-07-19**

- Shipped as `v_task_effective_fields` (one row per effective field, not a
  wide per-task view): each task's own inline fields (`source='task'`) plus
  the containing note's frontmatter fields (`source='note'`) for keys the
  task doesn't override. Note-level *inline* fields are paragraph-scoped and
  deliberately not inherited. Makes "open tasks I owe Acme, by due date" a
  single indexed query — the task/commitment-tracking primitive.
- `list_notes`/`list_tasks` (Ops trait + MCP tools) now take a generic
  `fields` map (exact values, list membership, AND across keys) instead of
  the hardcoded singular `customer` param; `list_notes` is SQL-driven (the
  load-every-note-and-compare implementation is gone) and its `type` filter
  now honors `kind` as well as legacy `type`. `list_tasks` filters and
  reports **effective** fields and keeps a `due` convenience column.

### P5 — Deferred

- Multi-select template prompt type (only if single-customer prompting hurts).
- `meeting_type` vocabulary.
- Any dedicated People/CRM tooling beyond Person notes.

## Stream rollups (no new primitives needed after P1–P4)

Dashboards / `query_sql` recipes over the new views:

- Active streams by priority; blocked/waiting streams.
- **Stale streams**: active streams with no meeting referencing them (via
  `v_field_values` key=`streams`) in the last 30 days.
- Recent meetings per customer; meetings missing `customers`/`audience`.
- Open tasks by customer / stream / owner / due (via `v_tasks_effective`).
- Inbox triage (unrouted notes).

## Retrieval-question coverage check

Every question from the handoff maps to a primitive: "all meetings involving
Acme" (P1), "internal meetings about Acme" (P1, two predicates), "meetings with
both Acme and Globex" (P1, self-join), "what did Jane say about X" (P2 with
attendees filter), "decisions about Acme's renewal" (P2 with customers+streams
filter), "active streams for Acme" (P1), "open tasks from external Acme
meetings" (P4), "stale active streams" (P1 + date join), "summarize Acme
activity this quarter" (P2 + `time_query`), "multi-customer work" (P1 count>1).

## Doc/kit updates once implemented

- Rewrite `docs/example-work-notes-kit.md` as **the** Work Notes kit (this
  schema; drop the customer-folder model and "one possible configuration"
  framing).
- Update `notes-method.md` where it references the kit.
- `docs/sql-views.md`: add `v_field_values`, `v_tasks_effective`; fix
  `v_periodic` doc drift (it indexes all five period kinds, not three).
- `docs/mcp.md` + HTTP docs: `vault_search` filters, generalized
  `list_notes`/`list_tasks`.
- New templates: meeting, stream, customer, person, daily, weekly, quarterly.
- Rewrite vault `.notesmith/skill.md`: the entity model, which fields are
  lists, exact query recipes (`v_field_values`, filtered `vault_search`,
  `v_tasks_effective`), folders-are-not-relationships, citation expectations.

## Implementation expectations

Repo rules apply: red-green-refactor TDD, malformed-content resilience
(malformed YAML lists degrade without panic), and the handoff's test matrix
(scalar filters, one/multi-item lists, zero-item lists, exact membership
without substring false positives, multiple simultaneous filters, lexical-only
and hybrid filtered search, public view contracts).

Suggested build order: P1 → P4 (pure index/SQL, immediately useful via
`query_sql`) → P2 (MCP/API surface) → P3 → kit/doc/template rewrite.
