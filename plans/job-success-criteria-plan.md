---
title: Job success criteria — write attribution and declared predicates
date: 2026-09-04
tags:
  - notesmith
  - jobs
  - plan
status: awaiting-signoff
---

# Job success criteria — implementation plan

Implements the [ADR 0025 amendment (2026-09-04)](adr/0025-work-system-integrations.md):
agent-job success is refined by what the run did to the vault, not read from the
subprocess exit code alone. Layer **A** (write attribution) is the default;
layer **C** (a declared `success_when` SQL predicate) is authoritative when
present. This plan is for sign-off before any code.

## Problem recap

`crates/notesmith-http/src/jobs/mod.rs` derives a run's `JobRunStatus`
(`state.rs`: `Succeeded` / `Failed` / `TimedOut` / `Missed`) purely from the
agent subprocess exit status (`outcome.succeeded()`). The daily-briefing agent
exited 0 twice while writing nothing. We need the daemon to know what the run
actually changed.

## Layer A — per-run write attribution (default, no config)

**The lever.** An agent job spawns `notesmith ai prompt`, which connects back to
the daemon's HTTP MCP endpoint (`/mcp/<vault>`) and performs every vault
mutation through a write tool (`update_note`, `append_to_note`,
`update_managed_section`, …). The daemon already sees each write; it just does
not attribute it to a specific run.

**Mechanism — run-id tagging over the existing MCP header path (#283).**

1. **Runner mints a run id.** In `jobs/mod.rs`, generate a `run_id` (uuid) per
   agent-job execution.
2. **Thread it to the CLI.** Pass `--run-id <id>` (or `NOTESMITH_RUN_ID`) to the
   spawned `notesmith ai prompt` (`jobs/run.rs::run_agent_job`).
3. **CLI stamps the vault binding.** In `crates/notesmith-cli/src/commands/ai.rs`,
   add the run id as a request header (`X-Notesmith-Run-Id`) on the daemon HTTP
   vault binding built by `McpBinding::daemon_http`. This reuses the header
   mechanism already shipped for external servers — no new transport surface.
   (The stdio fallback bridge forwards the same header; see risks.)
4. **Daemon counts writes per run id.** In the HTTP MCP write path
   (`notesmith-http` routes / the MCP tool dispatch), when a request carries
   `X-Notesmith-Run-Id`, increment an in-memory per-run counter — total writes,
   and for `update_managed_section`, the set of `section_id`s touched (this is
   what lets the briefing assert "all four `briefing/*` sections written"
   without a bespoke predicate). A short-TTL map keyed by run id; no
   persistence needed (the runner reads it immediately after the run).
5. **Runner reads the tally.** After the subprocess exits, `jobs/mod.rs` reads
   the counter for its run id and refines the outcome:
   - agent job with `allow_writes = true`, exit 0, **0 writes** → `NoWrites`
   - exit 0, ≥1 write → `Succeeded`
   - nonzero exit / timeout → unchanged (`Failed` / `TimedOut`)
   - read-only agent jobs and command jobs → unchanged (A does not apply).

**Type changes**

- `state.rs`: add `JobRunStatus::NoWrites` (serialized `"no_writes"`); add
  `writes: Option<u32>` and `sections_written: Option<Vec<String>>` to
  `JobRunRecord` as diagnostic metadata. `last_success` logic: `NoWrites` is
  **not** a success for `after`-gating purposes (a prerequisite that wrote
  nothing has not delivered), so `last_success` advances only on `Succeeded`
  **(signed off 2026-09-04)**.
- `job list` / `GET /jobs` / `job.*` SSE: render `succeeded (no writes)` and
  include the write count.

## Layer C — declared success predicate (opt-in, authoritative)

**Config surface.** Add optional `success_when: "<SELECT …>"` to the `[[jobs]]`
entry (`crates/notesmith-config/src/jobs.rs`, lenient parse like the rest).
Reuses the `context_queries` execution path (`prompt_render.rs` /
`notesmith_query::execute_sql`): SELECT-only, same read-only guard.

**Evaluation.** After the run (and after the agent's writes are indexed — see
below), `jobs/mod.rs` executes `success_when` against the vault index:

- non-empty / truthy first column → `Succeeded`
- empty / false → `Failed`, reason = `"success_when predicate not satisfied"`
- SQL error → `Failed`, reason carries the SQL error (a broken predicate is a
  job-config failure, surfaced not swallowed).

When `success_when` is present it **overrides** layer A's verdict; the write
tally is still recorded as metadata. When absent, layer A is the verdict.

**Index freshness.** Agent writes go through `Ops`, which reindexes synchronously
on each write (`LocalOps` refreshes indexes after `update_managed_section` et
al.), and the predicate runs only after the subprocess has fully exited — so the
index reflects the run's writes by evaluation time. Add one integration test
that pins this ordering; if a race ever appears, gate the predicate behind an
explicit reindex of the vault.

## The briefing, specifically

No `success_when` needed. Layer A enriched with `sections_written` gives the
runner "did this run write all of `briefing/{meetings,email,tasks,attention}`".
**Decided (signed off 2026-09-04): ship (a)** — plain layer A, `no_writes` only
when the run wrote *nothing*. This covers both field incidents. The per-section
expectation (flag a partial briefing) is a fast follow in phase 2 once
`sections_written` exists.

## Phasing

1. **A-core:** run-id threading, daemon per-run write counter (total only),
   `NoWrites` status + metadata, `job list`/SSE rendering, `last_success`
   semantics. Ships the fix for both field incidents.
2. **A-sections:** add `sections_written` attribution for `update_managed_section`.
3. **C:** `success_when` config + post-run evaluation + precedence.

Each phase is independently shippable and testable.

## Tests

- Runner unit: exit-0-zero-writes → `NoWrites`; exit-0-one-write → `Succeeded`;
  read-only job unaffected; `NoWrites` does not satisfy `after`.
- Daemon: writes carrying a run-id header increment the right counter; writes
  without the header do not; counter is per-run-id isolated.
- C: predicate true → `Succeeded`, false → `Failed`, SQL error → `Failed` with
  reason; `success_when` overrides a `NoWrites` A-verdict; index reflects the
  run's writes before evaluation.
- Config: `success_when` parses; invalid/non-SELECT rejected like other SQL.

## Risks / open questions

- **stdio fallback header.** The run-id header rides the HTTP vault binding
  cleanly. For sessions on the stdio bridge, confirm `notesmith mcp start`
  forwards an injected run-id (or scope A to HTTP-bound sessions initially — the
  briefing uses HTTP on every supported agent after the transport fixes, so this
  is not blocking).
- **Concurrency.** Per-run-id attribution is robust to overlapping sessions
  (unlike a naive before/after vault-wide counter), which is why run-id tagging
  is preferred over a global write clock.
- **Briefing expectation depth** — the (a)/(b) decision above.
