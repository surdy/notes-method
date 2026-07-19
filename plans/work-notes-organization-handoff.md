# Work Notes Organization Handoff

> **Superseded 2026-07-19** by `plans/work-notes-simplification-design.md`,
> which closed every open question below and shipped the search primitives.
> The "Current Search and Indexing Behavior" sections below are now stale:
> list fields ARE normalized (`v_field_values`), `vault_search` DOES accept
> metadata filters, frontmatter wikilinks DO create link edges, and
> `list_notes`/`list_tasks` take generic field filters. Kept for design
> rationale only.

## Purpose

This document captures the full design discussion about using Notesmith for work
meeting notes. It is intended to let another agent continue the conversation without
having to reconstruct the context or repeat settled questions.

The discussion covered:

- organizing internal and customer-facing meetings;
- representing zero, one, or multiple customers on a meeting;
- choosing folders versus metadata;
- the difference between frontmatter and inline fields;
- current search and indexing capabilities;
- representing streams of work;
- remaining concepts from the existing Work Notes kit.

No implementation changes have been made yet. The existing example kit still reflects
the older, customer-folder-centric model and will need revision after the remaining
design questions are settled.

## User's Original Scenario

The user wants to use Notesmith for notes from work meetings:

- Some meetings are **internal**.
- Some meetings are **external**, meaning customers attend.
- An internal meeting may:
  - concern no customer;
  - concern one customer;
  - concern multiple customers.
- Customer information also needs a durable home for account details and related work.
- The user initially considered placing all customer meetings under that customer's
  folder so an agent could search only that folder.
- The design question was whether folders should encode customer and meeting audience,
  or whether the vault should stay relatively flat and use metadata.

The desired end state must support both human navigation and reliable agent retrieval.

## Decision Status

| Topic | Status | Direction |
|---|---|---|
| Meeting storage | Settled | Store meetings centrally in date-oriented folders rather than under one customer. |
| Customer relationship | Settled | Use a plural `customers` frontmatter property containing zero or more customer-note wikilinks. |
| Internal/external distinction | Settled | Use a scalar metadata field such as `audience: internal` or `audience: external`. |
| Inline customer fields on meetings | Settled | Do not use them for note-level customer relationships. Use frontmatter. |
| Customer folders | Settled | Keep them for durable customer/account information, not as the canonical location of meetings. |
| Streams | Working direction | Model streams as durable entity notes in a global `Streams/` folder, related to customers and meetings through metadata. |
| Tags versus fields | Working direction | Use structured fields for known dimensions and tags for loose themes. |
| Search implementation | Gap identified | Current hybrid/full-text search cannot directly filter by metadata, and list-valued frontmatter is not normalized for membership queries. |
| People/attendees | Unresolved | Decide whether repeated attendees become first-class People notes and how attendee metadata is represented. |
| Tasks/action items | Unresolved | Decide conventions for owners, due dates, customers, streams, and task promotion. |
| Account information | Partially resolved | Customer folders are the home, but the split between the main customer note and specialized account notes remains open. |

## Agreed Conceptual Model

The emerging model separates three different kinds of things:

1. **Customer**: a durable organization/account entity.
2. **Meeting**: a dated event and record of a conversation.
3. **Stream**: an ongoing initiative, project, outcome, or work area.

These relationships are naturally many-to-many:

- a meeting can involve zero, one, or multiple customers;
- a meeting can discuss zero, one, or multiple streams;
- a stream can involve zero, one, or multiple customers;
- a customer can have many meetings and streams.

This is the main reason not to encode the relationships solely through physical
folders. A note has only one path but can participate in many relationships.

## Proposed Vault Layout

```text
Inbox/
Customers/
  Acme/
    Acme.md
    Account.md
  Globex/
    Globex.md
    Account.md
Streams/
  Acme - Renewal 2026.md
  Cross-customer Migration Program.md
Meetings/
  2026/
    07/
      2026-07-17 - Acme - Renewal planning.md
      2026-07-18 - Internal - Migration planning.md
Daily/
Dashboards/
.notesmith/
```

Important points:

- Customer folders remain useful for durable account information and human browsing.
- Meetings are centrally stored by date, avoiding the need to select one customer as
  the physical owner of a multi-customer meeting.
- Streams are global entities rather than children of customer folders.
- Year/month meeting folders avoid putting an unlimited number of files into one
  filesystem directory without encoding business relationships in paths.
- Status should normally remain metadata rather than causing files to move between
  `Active/`, `Blocked/`, and `Done/` folders.

## Meeting Metadata

### Internal meeting about no customer

```yaml
---
kind: meeting
audience: internal
customers: []
streams: []
date: 2026-07-17
---
```

### Internal meeting about one customer

```yaml
---
kind: meeting
audience: internal
customers:
  - "[[Acme]]"
streams:
  - "[[Acme - Renewal 2026]]"
date: 2026-07-17
---
```

### Internal meeting about multiple customers

```yaml
---
kind: meeting
audience: internal
customers:
  - "[[Acme]]"
  - "[[Globex]]"
streams:
  - "[[Cross-customer Migration Program]]"
date: 2026-07-17
---
```

### Customer-attended meeting

```yaml
---
kind: meeting
audience: external
customers:
  - "[[Acme]]"
streams:
  - "[[Acme - Renewal 2026]]"
date: 2026-07-17
---
```

`audience` answers whether customers attended. It should not be overloaded with the
meeting's business purpose. If useful, a separate field can describe that purpose:

```yaml
meeting_type: qbr
```

Possible values might include `qbr`, `discovery`, `status`, `planning`,
`retrospective`, or `escalation`. This vocabulary has not been settled.

### YAML quoting requirement

Wikilinks must be quoted in YAML:

```yaml
customers:
  - "[[Acme]]"
```

Do not use this:

```yaml
customers:
  - [[Acme]]
```

Unquoted square brackets have YAML structural meaning. Notesmith should retain normal
YAML behavior rather than inventing a special parser exception.

The equivalent compact form is valid:

```yaml
customers: ["[[Acme]]", "[[Globex]]"]
```

## Why `customers` Is a List

The plural list directly models all valid cases:

- `customers: []`: no customer;
- one list item: one customer;
- several list items: multi-customer work.

This avoids:

- duplicating a meeting into multiple customer folders;
- selecting an arbitrary "primary" customer;
- inventing fields such as `customer_1` and `customer_2`;
- using customer-specific tags as the main relationship model;
- treating folder location as if it were relational metadata.

The same plural convention should be considered for `streams`, `attendees`, and
`owners` when those relationships can genuinely contain more than one value.

## Frontmatter Versus Inline Fields

There was temporary confusion around this example:

```markdown
Customers: [customer:: [[Acme]]] [customer:: [[Globex]]]
```

The leading `Customers:` text is ordinary prose and has no indexing meaning. Each
outer bracket expression is an inline field:

```markdown
[customer:: [[Acme]]]
```

Its parts are:

- outer brackets: inline-field syntax;
- `customer::`: the field key;
- `[[Acme]]`: the value, which is also a wikilink.

The meeting design explicitly moved away from this representation. Meeting customers
belong to the whole note, so they should use frontmatter:

```yaml
customers:
  - "[[Acme]]"
  - "[[Globex]]"
```

### When inline fields are appropriate

Inline fields remain useful when metadata belongs to a specific task, paragraph,
decision, or local statement:

```markdown
- [ ] Send revised proposal [customer:: [[Acme]]] [due:: 2026-07-20]

Decision: Delay the rollout [owner:: [[Alice]]]
```

Their advantages are:

- the metadata remains next to the content it qualifies;
- the same key can occur many times in one note;
- task-level aggregation can preserve the local relationship;
- a value can be edited without moving to the note-level metadata panel.

They should not be used merely to work around a missing list-membership index. The
indexing limitation should be fixed instead.

### Line-field syntax was considered but is unnecessary here

A possible syntax was discussed:

```markdown
Customers:: [[Acme]], [[Globex]]
```

Notesmith does not currently support this Dataview-style line field. It could make the
visible label and field key the same, but it is unnecessary for note-level customer
metadata. A single colon should not become implicit metadata because ordinary prose
frequently contains `Label: value` lines.

## Fields Versus Tags

The recommendation is:

- use **fields** for dimensions with known meaning and query semantics;
- use **tags** for loose, cross-cutting topics.

Structured fields:

```yaml
kind: meeting
audience: internal
customers:
  - "[[Acme]]"
streams:
  - "[[Acme - Renewal 2026]]"
date: 2026-07-17
```

Loose topical tags:

```yaml
tags:
  - renewal
  - escalation
```

Do not make tags such as `customer/acme` the canonical customer relationship unless
there is a deliberate reason to accept rename brittleness and duplicate relationship
semantics.

The current Work Notes kit uses tags as note kinds (`meeting`, `stream`, `customer`).
The working recommendation is to make `kind` the canonical note-kind field and keep
tags topical. This has not yet been explicitly confirmed by the user.

## Streams

### Definition

A stream is an ongoing work entity with some combination of:

- an objective or intended outcome;
- status and lifecycle;
- ownership;
- priority;
- decisions;
- open questions;
- tasks;
- related meetings;
- related customers.

A stream should be a note when the subject has lifecycle or state. A loose theme that
does not have ownership, status, or intended outcomes should usually remain a tag.

### Proposed stream note

```yaml
---
kind: stream
customers:
  - "[[Acme]]"
status: active
priority: P1
owners:
  - me
started: 2026-07-01
target: 2026-09-30
---

# Acme - Renewal 2026

## Objective

## Current state

## Decisions

## Open questions

## Outcomes
```

The exact `owners` representation has not been settled. It may eventually contain
People-note wikilinks rather than free text.

### Stream relationships

- A stream's `customers` property can contain zero, one, or multiple customers.
- A meeting's `streams` property can contain zero, one, or multiple streams.
- Tasks can use task-level inline fields because the relationship belongs to the task:

```markdown
- [ ] Send revised proposal [stream:: [[Acme - Renewal 2026]]]
```

- Customer notes can show related streams through a metadata query.
- Stream notes can show related meetings and aggregate tasks through queries.
- Stream names should be globally distinctive because wikilink title collisions make
  resolution ambiguous. A customer prefix is useful for customer-specific streams.

Examples:

```text
Acme - Renewal 2026
Acme - Data Migration
Cross-customer Migration Program
Internal - Support Process Redesign
```

## Current Search and Indexing Behavior

This section describes the code as it exists now, not the desired final behavior.

### MCP search tools

The MCP interface currently exposes:

```text
search_notes(query, limit?)
vault_search(query, limit?)
query_sql(sql)
list_notes(type?, customer?, archived?)
get_note(path)
```

`search_notes` performs lexical Tantivy/BM25 search.

`vault_search` performs hybrid lexical and embedding search when embeddings are
available, with lexical-only fallback.

Neither search tool currently accepts:

- a customer filter;
- arbitrary field filters;
- a tag filter;
- a path prefix;
- an audience filter;
- a date filter as part of the same search request.

The tool definitions are in `crates/notesmith-mcp/src/lib.rs`.

### What full-text search indexes

The Tantivy schema currently indexes:

- vault name;
- path;
- title;
- body;
- note type.

Search queries are parsed against title and body. Generic frontmatter fields such as
`customers` and `audience` are not part of the lexical text query.

Relevant implementation:

- `crates/notesmith-index/src/search.rs`
- `crates/notesmith-ops/src/lib.rs`

Consequences:

- putting `customers` only in frontmatter does not make `vault_search("Acme")`
  reliably find the meeting;
- including customer and stream names in the title or visible body improves broad
  discovery today;
- broad search is not guaranteed to be exclusive to one customer.

### Current field storage

Frontmatter fields are indexed in SQLite's `fields` table and exposed through
`v_fields`.

The table has these useful indexes:

```text
(vault_name, note_path)
(vault_name, key)
(vault_name, key, value)
```

Exact scalar field queries are therefore efficient.

However, a YAML sequence is currently converted to one serialized YAML string:

```yaml
customers:
  - "[[Acme]]"
  - "[[Globex]]"
```

Conceptually becomes one `v_fields` row resembling:

```text
key = customers
value = "- '[[Acme]]'\n- '[[Globex]]'"
value_type = list
```

Membership must currently use a brittle `LIKE` query. It cannot use the exact
`(vault_name, key, value)` index to find one list member.

Relevant implementation:

- `crates/notesmith-index/src/indexer.rs`
- `crates/notesmith-index/src/schema.rs`
- `docs/sql-views.md`

### Current `list_notes(customer=...)` limitation

`list_notes` supports only a singular frontmatter `customer` string. Its
implementation:

1. loads every note from SQLite;
2. loads each note's frontmatter;
3. compares the singular `customer` value in Rust.

It does not support plural `customers`, and it is not the preferred scalable query
path for this design.

Relevant implementation:

- `crates/notesmith-ops/src/lib.rs`, `LocalOps::list_notes`

### Current link and backlink behavior

Notesmith currently parses wikilinks from the note body, not arbitrary frontmatter
fields. Therefore:

```yaml
customers:
  - "[[Acme]]"
```

is structured metadata but does not currently create a normal backlink from the
meeting to the Acme note.

Relevant implementation:

- `crates/notesmith-vault/src/parser.rs`, `parse_note`
- `crates/notesmith-index/src/indexer.rs`, `index_links`

Customer and stream dashboards should therefore query structured metadata. If
backlinks are also desired, either:

- author a visible body link;
- enhance link extraction for registered link/list-of-link fields;
- provide a separate relationship view derived from normalized metadata.

This behavior should be considered when designing People and attendee relationships.

### Internal filtered-search support already exists

The search internals already support restricting both lexical and semantic search to
an explicit set of allowed note paths:

- `SearchIndex::search_in_paths`
- `HybridSearch::search_filtered`
- embedding search `search_with_allowed_paths`

`memory_recall` already uses this allowed-path mechanism after identifying eligible
fact notes.

The missing piece for normal vault search is:

1. resolve requested metadata filters to a set of paths;
2. pass that path set into filtered hybrid search;
3. expose filters in the MCP/API/tool schema.

This means the desired filtered search fits the existing architecture rather than
requiring a separate search engine.

## Current Agent Workflow

Until metadata-filtered hybrid search exists, an agent can perform a two-step query:

1. Use `query_sql` to identify candidate paths.
2. Use `get_note` to read the matching notes.

For a scalar singular customer field, an exact query would be:

```sql
SELECT DISTINCT n.path, n.title
FROM v_notes n
JOIN v_fields c
  ON c.vault_name = n.vault_name
 AND c.note_path = n.path
WHERE c.key = 'customer'
  AND c.value = '[[Acme]]';
```

For the agreed plural list, the current serialized storage requires a temporary query
such as:

```sql
SELECT DISTINCT n.path, n.title
FROM v_notes n
JOIN v_fields c
  ON c.vault_name = n.vault_name
 AND c.note_path = n.path
WHERE c.key = 'customers'
  AND c.value LIKE '%[[Acme]]%';
```

This is functional but not the desired permanent API.

An agent can also use broad search with both customer and topic terms:

```text
vault_search("Acme renewal risks")
```

That is discovery, not a strict customer scope. The customer name should appear in the
title or body if this fallback is expected to work reliably.

## Folder Versus Metadata Performance

There is no meaningful search-performance advantage to putting each meeting under a
customer folder.

Reasons:

- Tantivy searches the indexed vault rather than walking customer directories.
- Embedding search operates on indexed chunks rather than filesystem hierarchy.
- The current MCP search tools do not accept a path-prefix filter anyway.
- An exact normalized metadata query can use SQLite indexes.
- A multi-customer note cannot naturally live under every customer without duplication.

A per-customer folder can still improve manual file-tree browsing, but it does not make
current full-text or hybrid search automatically customer-scoped.

The actual performance concerns are:

- serialized list membership uses `LIKE` instead of an exact indexed value;
- `list_notes(customer=...)` scans notes and loads frontmatter;
- a two-step SQL-then-read workflow can consume latency and agent context when a
  customer has hundreds of meetings;
- unfiltered vector search scales with the whole vault regardless of folders;
- an enormous single filesystem directory can become unpleasant for humans and the
  file tree, which is why date subfolders are recommended.

For a normal personal work-meeting vault, the flat/date-based structure itself should
not cause a practical performance problem.

## Desired Search and Indexing Enhancement

The agreed content model should not be distorted to fit the current implementation.
The implementation should learn how to query list-valued metadata.

### Normalize list members

The index should expose each list member as an independently queryable value while
preserving the original YAML list.

Two possible approaches:

1. Change `v_fields` semantics so one list produces one row per member.
2. Preserve `v_fields` compatibility and add a normalized table/view such as
   `field_values` / `v_field_values` with:
   - vault name;
   - note path;
   - key;
   - ordinal;
   - scalar item value;
   - scalar item type;
   - source.

The second option is safer because `docs/sql-views.md` currently promises that
`v_fields.value` contains a serialized list. Existing queries use `LIKE` against that
representation.

The normalized representation should support:

```sql
WHERE key = 'customers' AND value = '[[Acme]]'
```

It should work generically for all list fields, including:

- `customers`;
- `streams`;
- `attendees`;
- `owners`;
- `competencies`;
- other user-defined list fields.

### Add metadata filters to `vault_search`

A future tool shape could be:

```json
{
  "query": "renewal risks",
  "limit": 20,
  "filters": {
    "fields": {
      "customers": "[[Acme]]",
      "audience": "internal"
    },
    "tags": ["renewal"],
    "path_prefix": "Meetings/"
  }
}
```

The precise API and AND/OR semantics remain to be designed. Important requirements:

- exact membership for list fields;
- multiple field predicates;
- explicit AND/OR behavior;
- lexical and semantic rankers restricted to the same candidate paths;
- stable lexical-only behavior when embeddings are unavailable;
- no requirement that metadata text appear in the body;
- grounded results with path and snippet;
- performance metrics should report whether a prefilter was used.

### TDD and resilience expectations

If this enhancement is implemented, repository instructions require red-green-refactor
TDD and malformed-content resilience.

Tests should cover at least:

- scalar field filtering;
- one-item and multi-item YAML lists;
- exact membership without substring false positives;
- zero-item lists;
- multiple simultaneous filters;
- lexical-only filtered search;
- hybrid filtered search;
- malformed YAML degrading without panic;
- public SQL view behavior;
- exact frontend or agent query shapes if a UI/API surface consumes the view.

## Existing Work Notes Kit Drift

The current example is:

- `docs/example-work-notes-kit.md`

It still recommends:

```text
Customers/
  <Customer>/
    <Customer>.md
    Account Info/
    Internal Meetings/
    External Meetings/
    Streams/
```

It also currently uses:

- singular `customer`;
- `meeting_type` for internal/external;
- streams physically nested under customers;
- routing rules that move meetings into customer folders;
- duplicated inline fields such as `[customer:: ...]`;
- tags as the canonical note kind;
- SQL views that assume one customer per note.

Those examples no longer match the settled meeting/customer direction. Do not treat
the current kit as the accepted design when continuing the conversation.

Once the remaining concepts are settled, update at least:

- `docs/example-work-notes-kit.md`;
- `notes-method.md` where the work workflow is referenced or specified;
- `.notesmith/fields.toml` examples;
- routing examples;
- SQL views;
- meeting, stream, and customer templates;
- `.notesmith/skill.md` example guidance.

If implementation changes the SQL contract, also update:

- `docs/sql-views.md`;
- `docs/mcp.md`;
- relevant HTTP/API documentation;
- architecture references if the search contract materially changes.

## Remaining Concepts From the Work Notes Kit

### 1. Note kind

Open question:

- Should `kind: meeting|stream|customer|account` be canonical?
- Should tags duplicate kind, or remain purely topical?

Working recommendation:

- use `kind` for the schema/type dimension;
- use tags for themes such as `renewal`, `escalation`, `security`, or `migration`.

### 2. People and attendees

Questions:

- Should frequently encountered coworkers and customer contacts have People notes?
- Should meeting metadata contain one `attendees` list or separate
  `internal_attendees` and `external_attendees` lists?
- Is `audience` authored directly, or derivable from attendee roles?
- How should organizations, teams, and unknown guests be represented?
- Should attendees create backlinks/relationship rows?
- What fields belong on a work contact note?

Possible simple starting point:

```yaml
attendees:
  - "[[Alice Smith]]"
  - "[[Bob Jones]]"
```

The next agent should avoid creating a complex CRM prematurely. Decide what retrieval
questions the user actually wants to ask, such as:

- "What did Jane say about the renewal?"
- "Show my recent meetings with this customer contact."
- "Which internal stakeholders attended Acme escalations?"

### 3. Tasks and action items

Notesmith already aggregates tasks from all notes. The remaining design is how task
metadata should work.

Questions:

- Is the owner free text or a People-note wikilink?
- Can a task have multiple owners?
- Should task customer be authored explicitly or inherited from the containing note?
- Should task stream be authored explicitly when the meeting references only one stream?
- Which fields are mandatory: `due`, `owner`, `stream`, `customer`, `priority`?
- When does an action item remain in a meeting versus move to a stream/project note?

Likely inline representation:

```markdown
- [ ] Send revised proposal
  [owner:: [[Alice]]]
  [due:: 2026-07-24]
  [stream:: [[Acme - Renewal 2026]]]
```

Task inheritance could reduce duplication but has not been designed.

### 4. Stream lifecycle

Potential fields:

```yaml
status: active
priority: P1
owners:
  - me
started: 2026-07-01
target: 2026-09-30
```

Questions:

- final status vocabulary;
- priority vocabulary;
- whether `waiting` and `blocked` are statuses or separate flags;
- whether completed streams remain in place with metadata or move to an archive;
- whether customer ownership and internal ownership need separate fields.

The current kit proposes:

```text
inbox, active, waiting, blocked, done, archived
```

This has not been accepted as final.

### 5. Account information

The customer folder remains the home for durable account context, but the granularity
is undecided.

Possible structure:

```text
Customers/Acme/
  Acme.md
  Account.md
  Architecture.md
  Commercial.md
  Relationship.md
```

Questions:

- What belongs on the main customer overview?
- When should account information split into separate notes?
- Should people/contacts live under the customer or in a global People folder?
- Which sensitive information should not be stored at all?
- Should account notes use `kind: account` plus `customers: ["[[Acme]]"]`?

### 6. Inbox and routing

The existing kit uses:

1. capture into `Inbox/`;
2. enrich metadata;
3. route to the permanent folder;
4. remove the `inbox` tag.

This remains a sensible workflow but must be revised for centralized meetings and
streams.

Potential routing:

- `kind: meeting` -> `Meetings/YYYY/MM/`;
- `kind: stream` -> `Streams/`;
- `kind: customer` -> `Customers/<name>/<name>.md`;
- `kind: account` -> the matching customer folder;
- notes without a recognized kind remain in Inbox for triage.

Questions:

- Should routing happen automatically after template creation?
- What happens when metadata is incomplete?
- How should filenames be generated and collisions handled?

### 7. Templates

Likely templates:

- meeting;
- stream;
- customer;
- account note;
- possibly person/contact.

Templates should prompt for structured fields and produce consistent headings.

A meeting template likely needs:

- date;
- title;
- audience;
- zero or more customers;
- zero or more streams;
- optional attendees;
- headings for discussion, decisions, and tasks.

The current template system's support for multi-select field prompts should be checked
before promising this exact UX.

### 8. Dashboards and SQL views

Potential dashboards:

- active streams;
- blocked streams;
- recent meetings by customer;
- meetings by stream;
- customer overview;
- open tasks by owner;
- open tasks by customer or stream;
- inbox triage;
- meetings missing customers, audience, or streams;
- stale active streams with no recent meeting/activity.

These should be built on normalized metadata rather than folder assumptions.

### 9. Daily notes

Daily notes are optional chronological context. They can link to:

- meetings attended that day;
- streams worked on;
- customers discussed;
- ad hoc decisions or follow-ups.

They should not become a second canonical store for meeting content. The dedicated
meeting note remains the authoritative meeting record.

### 10. Hooks and automation

The existing kit demonstrates a hook that notifies when a stream becomes blocked.

Possible future automations:

- notify on blocked stream;
- remind on stale active stream;
- create follow-up tasks after external meetings;
- flag meetings missing attendees or customer metadata;
- generate a customer briefing before an external meeting.

These are optional workflow enhancements, not core data-model requirements.

### 11. Agent guidance

The vault's `.notesmith/skill.md` should eventually explain:

- the entity model;
- canonical field names;
- customer, stream, and meeting relationships;
- which fields are lists;
- how to query normalized list members;
- when to use `query_sql`, `vault_search`, and `get_note`;
- that folders are for human organization rather than the authoritative relationship
  model;
- how agents should cite meeting notes;
- any restrictions on sensitive customer information.

## Recommended Order for Continuing the Conversation

The next agent should continue in this order:

1. **People and attendees**
   - Determine whether people are first-class notes.
   - Decide attendee fields and external/internal representation.
2. **Tasks and action items**
   - Decide owner, due date, customer/stream inheritance, and promotion rules.
3. **Stream lifecycle**
   - Confirm status, priority, owners, dates, and archive behavior.
4. **Account information**
   - Define customer overview versus specialized account notes.
5. **Canonical schema**
   - Confirm exact field names and singular/plural conventions.
6. **Capture, routing, and filenames**
   - Define where templates create notes and how routing completes.
7. **Views and dashboards**
   - Derive required queries from real retrieval questions.
8. **Tooling enhancement**
   - Design normalized list indexing and metadata-filtered `vault_search`.
9. **Update the Work Notes kit**
   - Replace the old singular/customer-folder examples.
10. **Implement with TDD**
    - Only after the model and query contract are agreed.

## Retrieval Questions the Design Should Support

Use concrete questions to test every proposed field and relationship:

- Show all meetings involving Acme.
- Show internal meetings about Acme.
- Show customer-attended meetings with Acme.
- Show meetings involving both Acme and Globex.
- What decisions were made about Acme's renewal?
- Which streams are active for Acme?
- Which meetings discussed the renewal stream?
- Which open tasks came from external Acme meetings?
- What did a particular attendee say across meetings?
- Which active streams have not had a meeting in the last month?
- Summarize all customer-facing activity for Acme this quarter.
- Show work that involves multiple customers.

If a representation cannot answer these without path conventions or fuzzy text
matching, it is probably missing structured metadata.

## Guidance for the Next Agent

- Treat the decisions in this handoff as the current direction.
- Do not revert to putting meetings and streams under a single customer merely because
  the existing example kit does so.
- Do not require inline customer fields on meeting notes.
- Preserve normal YAML and quote wikilinks.
- Do not claim that current `vault_search` can filter by metadata; it cannot.
- Do not claim that arbitrary frontmatter wikilinks create backlinks; they currently
  do not.
- Distinguish current behavior from proposed enhancements.
- Ask design questions in terms of the user's actual retrieval and workflow needs.
- Avoid premature implementation until People, Tasks, lifecycle, and account structure
  are settled.
- Once decisions are made, keep `notes-method.md`,
  `docs/example-work-notes-kit.md`, SQL/MCP docs, templates, and examples consistent.

## Relevant Repository Files

Read these before proposing implementation:

- `notes-method.md`
- `docs/example-work-notes-kit.md`
- `docs/mcp.md`
- `docs/sql-views.md`
- `docs/ai-semantic-search.md`
- `crates/notesmith-mcp/src/lib.rs`
- `crates/notesmith-ops/src/lib.rs`
- `crates/notesmith-ops/src/hybrid.rs`
- `crates/notesmith-index/src/indexer.rs`
- `crates/notesmith-index/src/schema.rs`
- `crates/notesmith-index/src/search.rs`
- `crates/notesmith-vault/src/parser.rs`
- `crates/notesmith-embed/src/search.rs`

