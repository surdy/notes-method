# ADR 0025 — Work-System Integrations: Connectors, Jobs, and Data Landing

## Status

Accepted (2026-08-04). Decisions settled interactively; full design in
[`plans/integrations-control-center-plan.md`](../../plans/integrations-control-center-plan.md).
Phases 1–4 are implemented. The transcript-access and storage-policy gates for
phase 5 were resolved on 2026-09-04; its remaining implementation constraints
are recorded in the amendment below and
[`plans/transcript-sync-spike-results.md`](../../plans/transcript-sync-spike-results.md).

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

## Amendment (2026-09-02) — managed-section interiors are a core operation

Decision 5 shipped the section-marker convention with **no core enforcement**:
prompts and `skill.md` told the agent to read the note, splice between the
markers, and write the whole note back through `update_note`. Real-machine
verification of the daily-briefing job disproved that stance
(`plans/work-integrations-post-fix-rerun-handoff.md`, Finding 1). A compliant
agent — one that reported having preserved all human content — still stripped
trailing spaces from human text outside the markers, and the save pipeline's
automatic `updated:` stamp changed the frontmatter on every run. Prompt
instructions cannot guarantee byte preservation.

**Decision (partially revises Decision 5).** The marker convention itself is
unchanged and still lives in vault config. But **replacing a section's interior
is now a deterministic core operation**, not agent behaviour: a pure transform
in `notesmith-vault` that replaces only the byte range between the marker
lines, surfaced as `POST /api/v/{vault}/notes-section/{path...}` and the
`update_managed_section` MCP tool (write-only, rejected on `/mcp-ro`).
Malformed layouts (duplicate, inverted, unpaired markers) are structured
refusals that write nothing, and the write is hash-guarded so a concurrent
human edit conflicts rather than being overwritten.

**Managed-section writes do not touch automatic note metadata.** This is the
one write path that skips the save pipeline: no `updated:` restamp, no key
sorting, no whitespace trimming. "Outside the markers is inviolable" includes
the frontmatter — a machine refreshing its own region is not a human editing
the note. Callers that want `updated:` refreshed do that explicitly through
`PATCH /notes/{path...}`.

"Core ships only mechanisms" still holds: this is a generic note-editing
primitive with no knowledge of daily notes or briefings. What changes is that
the byte-preservation half of the convention is now enforced by code rather
than requested in a prompt.

## Amendment (2026-09-02) — scope of the raw-email boundary

Decision 4 states that **raw email is never stored** — "not in the vault, not
staged on disk". Verification raised the obvious follow-up: raw email
unavoidably *exists* somewhere while a briefing is composed, so where exactly
does the boundary run? This amendment states it precisely; it narrows nothing
Decision 4 promised, it just says which side of the line each component is on.

**The boundary is Notesmith's processes and storage, not "any disk anywhere".**
Raw email must never enter:

- the daemon or any Notesmith process (no fetching, parsing, or buffering of
  message bodies);
- rendered prompts — the text Notesmith composes and hands to an agent;
- job history, run records, captured stdout, or error/diagnostic output;
- vault files, templates, or note bodies;
- connector state, caches, or any Notesmith-owned database.

Raw email **may** transit the *agent's* context. The sanctioned judgment-tier
path is a Work IQ tool attached to the agent itself — Copilot's own Work IQ
plugin, or `workiq mcp` as a stdio server for agents that accept
client-supplied stdio servers (ADR 0012's 2026-09-02 amendment: not Copilot).
The agent reads mail live, decides what matters, and writes only its
human-facing summary back through Notesmith's tools. What the agent's own CLI
retains locally — session transcripts, its own caches — is governed by that
agent's retention settings and is **explicitly outside Notesmith's boundary**.
Notesmith does not inspect, mirror, or promise anything about it; a user who
needs that constrained configures it in the agent.

**Consequence: Notesmith deliberately does not splice email content into
rendered prompts.** A "context commands" design — a connector fetches messages
and the prompt renderer interpolates them the way it interpolates SQL context
queries — was considered and rejected. It would put raw bodies through the
prompt renderer, the job runner's record of what it ran, and every error path
that quotes a prompt on failure. That makes Notesmith a custodian of raw email,
and it makes every future change to prompt rendering, job recording, stdout
capture, or error formatting a re-verification of this boundary. Keeping the
raw data on the agent's side of the wire means the boundary is enforced by
architecture rather than by remembering.

**Fallback tier.** A future deterministic (non-LLM) connector for the email
summary is still permitted and stays inside the boundary, provided it persists
only **sender and subject metadata** — which is exactly what the briefing
summary is allowed to contain anyway. Bodies, quoted passages, and raw headers
remain out. That connector is the answer for agents with no Work IQ tool at
all, not a replacement for the judgment tier.

## Amendment (2026-09-04) — agent-job success is effect-based, not exit-code-based

Two field runs of the daily-briefing job recorded `succeeded` while the agent
wrote nothing: it exited 0 having declined, or having hallucinated a missing
write tool. An exit code cannot distinguish "did the work" from "politely did
nothing", so job success can no longer be read from the agent subprocess's
exit status alone.

**Decision.** A scheduled job's recorded outcome is refined by what the run
*actually did to the vault*, in two layers:

- **Default (A) — write attribution.** Vault mutations by an agent session
  already flow through the daemon's MCP write tools, so the daemon can count
  them per run. For an agent job that *can* write (`allow_writes = true`), a
  run that exits 0 with **zero writes** is recorded as a new distinct outcome,
  **`no_writes`**, surfaced as such in `job list` / `GET /jobs` / `job.*` SSE
  — not silently `succeeded`, and not a hard `failed` (a genuinely quiet
  morning is legitimate; `no_writes` is a visible signal the operator judges).
  Read-only agent jobs and command jobs are unaffected. This is automatic, no
  config.
- **Opt-in (C) — declared success criterion.** A job may carry
  `success_when` (a `SELECT` run against the vault index after the run). When
  present it is **authoritative**: a false/empty result records `failed` with
  the predicate as the reason, a true/non-empty result records `succeeded`,
  overriding the layer-A verdict. The write count is still recorded as
  diagnostic metadata. This is the answer for a job whose failure is subtle —
  it wrote *something* but not the right thing.

Precedence is `success_when` if declared, else the write-attribution outcome.
The briefing's own "did all four `briefing/*` sections get filled" check is met
by enriching layer A (attributing *which* managed sections a run's
`update_managed_section` calls touched) rather than authoring a bespoke
predicate — so it needs no `success_when`.

"Core ships mechanisms" holds: write attribution and a post-run SQL predicate
are generic job machinery with no knowledge of briefings. Implementation plan:
`plans/job-success-criteria-plan.md`.

## Amendment (2026-09-04) — transcript transport and occurrence matching

Real-machine verification resolved the Phase-0 transcript gate. A delegated
Work IQ user token can resolve a calendar event's Teams join URL to an online
meeting, list transcript metadata, and fetch transcript content; no application
permission or Teams application-access-policy error occurred. The CLI returns
content as a JSON string containing WebVTT rather than as raw VTT or a Graph
object.

**Decision 4 remains, with a more precise join.** Calendar sync must persist
`isOnlineMeeting` and `join_url`. A join URL identifies the online meeting, but
recurring calendar occurrences reuse one join URL, so it does not identify one
`event_id`. Transcript sync resolves the online meeting, then matches each
transcript's timestamps to the calendar occurrence before writing that
occurrence's `event_id` into the Transcript Note.

The follow-up occurrence probe verified the proposed matcher against a real
recurring series. The retained recent transcript selected one occurrence with
a 13-day, 23:35:12 margin over the runner-up, so `match_transcript` moves into
the connector unchanged. A transcript outside the fetched occurrence window
remained unmatched, which is the required safe failure.

Calendar and transcript timestamps were both UTC, but Graph represented the
calendar value as a naive `dateTime` plus a separate `timeZone: UTC` field and
the transcript value with `Z`. The connector must normalize both explicitly to
UTC rather than discard the zone field. It must also follow `calendarView`
pagination and isolate per-series lookup failures; one missing page or
zero-byte Work IQ response must not cause a guessed match or abort unrelated
series. See `spikes/transcript-occurrence-matching/FINDINGS.md`.

**Delegated meeting resolution is partial, not organizer-only.** A live
35-series sample resolved all three self-organized series and eleven of
thirty-two other-organized series. A diagnostic replay classified all
twenty-one unresolved other-organized lookups as access failures: Work IQ
returned `403 Forbidden` with `3003: User does not have access to lookup
meeting`, rather than Graph returning an empty `value` collection. Resolved and
failed sets both contained single and recurring meetings and both
internal-domain and external-domain organizers. All sampled join URLs used
`thread.v2` and carried a query string, so the available event and URL metadata
does not provide a safe pre-filter. The connector continues per-series
isolation and reports these under `failed`; it must not describe its supported
scope as only organizer-owned meetings.

**The shared transcript model gains an optional speaker.** Real Teams VTT
carries `<v Speaker>` on each cue. `TranscriptSegment` therefore gains
`speaker: Option<String>`, rendered as `[M:SS] Name: text` when present;
YouTube and local Whisper sources pass `None`. A separate Teams renderer is
rejected because it would break Decision 4's one Transcript Note concept.
Teams parsing preserves source order because VTT cue intervals may overlap.

**Retention is best-effort.** A 17-day-old transcript remained fetchable, while
older occurrences whose meeting records still said they were transcribed were
not present in the current recurring-meeting transcript collection. The
connector stores a cursor outside the vault and ingests promptly; historical
backfill is not guaranteed until the tenant retention policy is known.

**Storage policy gate passed.** Verbatim customer-call transcripts are approved
for the local work vault and its corporate Git remote. The laptop-only
placement in Decision 6 remains unchanged.

## Amendment (2026-09-04) — imported times are local wall clock

The occurrence probe's timezone check surfaced a shipped defect rather than a
future risk. Graph returns calendarView times as a zone-less `dateTime` with a
sibling `timeZone: UTC`, and mail `receivedDateTime` with a trailing `Z`. Both
connectors *dropped* the zone and stored the raw components as if they were
local. Real synced notes confirmed it: a 17:00 PDT meeting was stored as
`2026-09-04T00:00:00` — the right instant, the wrong clock, and the wrong day.

This was invisible to every prior verification because phase 4 tested meeting
times against hand-built fixtures and checked the live connector only for
schema and idempotency, never for clock correctness.

**Decision: the vault stores local wall-clock time; connectors normalize at the
boundary.** Everything above the connectors already means local — `date:`
fields, the briefing's `date('now', 'localtime')` queries, meeting-prefill's
window around `now`. Storing UTC would have required changing all of them.
Connectors therefore parse the zone Graph supplies (the `timeZone` sibling, a
`Z`, or an explicit offset) and convert; they never discard it. This refines
the earlier amendment's "normalize both explicitly to UTC": explicit
normalization is the rule, and the canonical form on disk is local.

Transcript sync converts its `Z` timestamps the same way before matching
against occurrence windows.

**Existing data is not repaired by the fix.** Event notes are upserted by
`event_id` and patched in place, so a re-sync corrects `start`/`end` while the
note keeps its old UTC-derived `YYYY-MM-DD HHMM` filename. Bringing paths back
in step means deleting the `Calendar/` tree and letting the connector rebuild
it — safe, since event notes are machine-owned and the meeting note is the
authoritative record, but it only restores events inside the sync window.
