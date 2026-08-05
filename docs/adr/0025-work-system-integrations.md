# ADR 0025 — Work-System Integrations: Connectors, Jobs, and Data Landing

## Status

Accepted (2026-08-04). Decisions settled interactively; full design in
[`plans/integrations-control-center-plan.md`](../../plans/integrations-control-center-plan.md).
Implementation pending — blocked on that plan's Phase-0 spike (Work IQ auth,
transcript availability, corp policy). This ADR records the decisions and
trade-offs so they survive until (and beyond) implementation.

Scopes — does not supersede — [ADR 0019](0019-media-ingestion-pipeline.md)
(see Decision 1) and carves a deliberate exception to the derived-state
placement principle established by ADR 0012/0018/0022 (see Decision 4).

## Context

Notesmith is becoming the daily work dashboard: calendar-aware note creation,
meeting transcripts attached to meeting notes, and a scheduled morning daily
note (email summary, today's meetings, tasks due). The external systems are
corporate — Microsoft 365 via **Work IQ**, whose sanctioned surface is a
remote MCP server with user-scoped corp SSO, central governance, and
usage-based billing.

Two existing architectural stances are in tension with this work:

- **ADR 0019 §1** mandates a compiled-in `Source`/`Fetcher` trait for new
  ingestion sources ("adding PDF, EPUB, newsletters, or future feeds should
  add a source module, not rewrite the worker").
- Derived, refetchable state (index cache, embeddings, chat transcripts,
  permissions) is deliberately kept **outside** the vault so it neither
  clutters nor syncs.

Meanwhile scheduling is bespoke: four hand-rolled tokio modules (daily,
ingest, embed, transcribe) share a supervisor pattern but no abstraction, so
each new scheduled behavior has meant a new module.

## Decision

### 1. Corp integrations are external subprocess connectors, not compiled-in Sources

A **connector** is a user-provided executable (in `.notesmith/connectors/`)
invoked on a schedule by the daemon. It authenticates to the external system
itself (env files or OS keychain — Notesmith never stores credentials) and
writes results through the REST API or designated folders, with idempotency
via stable external IDs in frontmatter (upsert on resync).

This *scopes* ADR 0019 rather than reversing it: the `Source` trait remains
the right boundary for compiled-in, credential-free media sources (YouTube,
PDF, feeds). Corp systems fail every assumption behind compiling in — their
client code can't ship in an OSS binary, their credentials are per-user corp
SSO, and their APIs churn independently of Notesmith releases. The
alternatives — a plugin host (dylib/wasm) or building Graph/Work IQ clients
into core — were rejected: Notesmith's extension philosophy is already
uniformly "subprocess, external MCP server, or file drop", and this keeps it
that way.

### 2. One generic `[[jobs]]` runner replaces bespoke scheduler modules

Per-vault `[[jobs]]` config declares scheduled work: kind `command`
(connector subprocess) or `agent` (headless `notesmith ai` run with a named
prompt); `every`/`at` schedules with catch-up-on-wake (persisted last-run);
same-day `after` ordering (not DAGs); `job.*` events on the existing bus;
manual trigger via CLI and REST. Built on the existing supervisor pattern
with hot-reloaded `enabled`. No cron expressions until needed.

Rationale: OS-level cron/launchd was rejected as the permanent home because
laptops sleep through fire times (cron misses silently), ordering degrades to
sleep-and-hope, and config would live outside the vault, invisible to
Notesmith. Existing workers (ingest/embed/transcribe) may migrate onto the
runner later but are not required to.

### 3. Deterministic syncs and judgment tasks use different Work IQ modes

Mechanical, repeatable pulls (calendar events, transcripts) are connector
scripts acting as **plain MCP clients** — no LLM, bounded billing,
predictable output. Tasks requiring judgment (email importance, briefing
composition) are **headless agent jobs** with Work IQ MCP registered in
`[mcp.servers]` alongside the vault's own tools. An LLM never sits in a
15-minute poll loop; a script never decides what's "important".

### 4. Imported work data lands as vault notes — a deliberate exception, with a hard boundary for email

Calendar events become notes (`kind: event`, upserted by `event_id`);
transcripts become sidecar notes (`kind: transcript`, linked to their meeting
by `event_id`, never inlined into the meeting note). This contradicts the
"derived state stays outside the vault" principle on purpose: unlike caches
and embeddings, this data is *content* the user reads, links, and queries —
and once it is notes, every existing capability (SQL views, template context
queries, routing, filtered search, agent access) works on it with zero new
query surface. The meeting note remains the authoritative record; events are
imported context.

The hard boundary: **raw email is never stored** — not in the vault, not
staged on disk. The briefing agent reads mail live over Work IQ MCP and only
its human-facing summary persists in the daily note.

Transcript notes unify with the existing transcription domain: one
**Transcript Note** concept (`kind: transcript`, shared body format via
`notesmith-transcribe`'s renderer) regardless of origin, with `source_type`
distinguishing teams/youtube/audio.

### 5. Flows are vault config; core ships only mechanisms

The daily briefing is a `[[jobs]]` entry + a prompt file + a template +
routing rules — all vault-level and kit-installable. Core gains only generic
mechanisms: the job runner and a section-marker convention
(`<!-- notesmith:section:begin/end <id> -->`) for machine-managed regions
inside human-owned notes (re-runs replace marked sections; content outside is
never touched). No hardwired "daily briefing feature".

### 6. Placement: work-related integration runs laptop-local

Daemon, work vault, connectors, and jobs run on the corp laptop. Work IQ auth
is only sanely available there; corp data on personal homelab infra is a
compliance exposure; and the daemon has no auth, so cross-machine writes have
no security story. The runner is placement-agnostic, so a future hybrid (for
non-sensitive personal integrations) stays possible — gated on daemon auth
first. The work vault backs up via the existing `[git]` timers to a
corp-approved remote, never the homelab.

## Consequences

- Adding an integration = writing a script + a `[[jobs]]` entry + kit
  config. No Notesmith release required; still no plugin host.
- `[mcp.servers]` HTTP entries must be able to carry Work IQ's auth
  (headers/OAuth) — verify in the spike; small core addition if missing.
- The scheduler's ignored-timezone bug and the
  `create_daily_note`/`ensure_periodic_note` path divergence must be fixed
  when the runner lands, since automation stacks on both.
- Event/transcript notes make vault content partially machine-written;
  note-level git history becomes the undo mechanism for bad agent runs.
- Vocabulary: "event" (calendar note kind) vs `VaultEvent` (runtime), and
  "Transcript Note" vs the chat `TranscriptStore`, are disambiguated in
  `CONTEXT.md`.
