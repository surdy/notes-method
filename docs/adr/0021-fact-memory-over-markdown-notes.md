# ADR 0021 — Fact Memory over Markdown Notes

## Status

Accepted (2026-07-10).

Refines the `memory` MCP tool planned by
[ADR 0015](0015-ai-agent-integration-roadmap.md) and issue
[#203](https://github.com/surdy/notes-method/issues/203). Builds on the
chunked embedding index from [ADR 0018](0018-embedding-and-vector-search.md)
and the existing note/template/field primitives.

The storage shape is being dogfooded first in the self-hosted `memory` vault.
Specialized MCP recall/list/lifecycle tools now ship over ordinary fact notes;
embedded chat companion-vault attachment now ships, while stale-review UX
remains backlog work.

## Context

Notesmith already provides a general wiki with structured fields, links,
lexical search, and chunked semantic retrieval. A memory feature must add
high-signal, durable recall without creating an opaque second database or a
competing source of truth.

The earlier #203 sketch proposed a single `memory.md` file. That shape is a
poor fit for the implemented embedding pipeline: unrelated facts would share
256–512-token chunks, while individual IDs, provenance, conflict resolution,
and supersession would require a new block-record parser and mutation model.

The useful distinction is not "stored in memory" versus "stored in Notesmith."
It is:

- **fact memory** — an atomic claim that should reliably affect a future
  agent's behavior or answer; and
- **wiki knowledge** — explanatory, historical, procedural, or evidentiary
  material intended for normal browsing and retrieval.

## Decision

### 1. A fact is an ordinary Markdown note

Facts use one file per claim in a configurable segment (dogfood default:
`facts/`, `type: fact`). Markdown remains canonical; SQLite, Tantivy, and
embeddings remain rebuildable indexes.

The claim lives in the note body so existing lexical and embedding retrieval
can cite it. Frontmatter carries lifecycle and provenance:

```yaml
type: fact
title: Prefer concise commit messages
description: The user prefers concise commit messages.
scope: user
subject: ""
certainty: explicit
source: User statement
status: active
confirmed: 2026-07-10
supersedes: ""
tags: [fact]
```

`certainty` is categorical (`explicit`, `observed`, `inferred`), not a
numeric confidence score. Observations require a cited source; inference
requires explicit user confirmation. `status` is `active`, `superseded`, or
`retracted`. Hard deletion is reserved for mistakes or sensitive material.

### 2. Route information as fact, wiki, both, or session-only

The calling ACP agent applies this rubric:

| Destination | Rule |
|---|---|
| **Fact** | Atomic, durable, and likely to affect future behavior or answers |
| **Wiki** | Needs explanation, history, evidence, procedures, or human browsing |
| **Both** | A concise operational claim matters repeatedly and links to a richer canonical note |
| **Session only** | Temporary, speculative, low-value, or secret |

Explicit "remember that" wording normally selects Fact; "document" or "write
this up" selects Wiki. For ambiguous requests the agent chooses using the
rubric and asks only when the distinction is consequential.

This classification remains agent reasoning under ADR 0015. The daemon does
not run a chat LLM or silently classify content.

### 3. Facts are a curated layer, not duplicated wiki prose

When both representations are useful, the fact contains only the concise
operational claim and links to its subject or source note. The linked wiki
note owns the detailed explanation and evidence.

Before creating a fact, the agent searches both active facts and the likely
wiki segment. Semantic similarity supplies duplicate/conflict candidates; it
does not decide whether claims conflict. The agent updates an exact duplicate
or explicitly supersedes an outdated fact.

### 4. Recall is filtered and dynamic

The planned `memory_recall` operation searches only active fact notes and
supports scope filtering. It reuses hybrid retrieval and returns claim text,
path, scope, provenance, and citation offsets.

Facts are not dumped wholesale into every prompt. The agent calls recall when
personal preferences, environment, identity, or prior decisions are likely to
matter. A small explicitly marked core set may be added to the session
preamble later, with a strict token budget.

### 5. Memory may be a companion vault

The target desktop architecture allows one configured memory vault to be
attached to an ACP session beside the active work vault:

- `scope: user` facts are available across vaults;
- `scope: vault:<name>` facts are filtered to matching work;
- reads may use a read-only MCP binding;
- writes retain the normal per-call permission flow.

This cross-vault binding now ships in embedded chat as one configured companion
memory vault chosen from saved desktop connections. The initial dogfood still
operates directly inside the existing `memory` vault using normal note and
query tools.

## DecisionNode findings

[DecisionNode](https://github.com/decisionnode/DecisionNode) validates several
parts of this design: atomic records, active/deprecated lifecycle, provenance,
global/project scope, pre-write semantic conflict candidates, and tool
descriptions that tell agents when to search first.

Notesmith does not adopt its JSON parallel store, external Gemini dependency,
single-vector records, full-state history snapshots, fixed conflict threshold,
or mutable global project context. DecisionNode also delegates contradiction
resolution to the caller and does not implement confidence decay, temporal
validity, or automatic consolidation.

## Delivery

1. **Dogfood:** `facts/` segment, template, schema, routing rubric, example,
   and structured recall query in the personal memory vault.
2. **Specialized tools:** fact save/recall/list/update/supersede/delete over
   ordinary notes, including similar-fact candidates, preview/apply, and
   optimistic writes. **Shipped in the current vault-local form.**
3. **Companion vault:** attach one configured memory vault to agent sessions
   and instruct the agent to default recall to `scope: vault:<active-vault>`.
4. **Maintenance UX:** stale-fact review, provenance display, and optional
   bounded core-memory injection.

## Consequences

- Facts stay inspectable, editable, portable, linkable, and git-diffable.
- Existing indexing and resilience boundaries are reused.
- One-file-per-fact creates more files, but avoids inventing block-level record
  semantics and provides clean lifecycle boundaries.
- Useful automatic routing depends on the quality of the calling agent and its
  instructions; the daemon remains deterministic.
- Cross-vault memory requires an additional MCP binding and connection
  configuration before it can help every Notesmith workspace automatically.
