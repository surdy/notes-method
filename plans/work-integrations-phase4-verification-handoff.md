---
title: Work integrations phase 4 verification handoff
date: 2026-09-03
tags:
  - notesmith
  - verification
  - workiq
  - handoff
status: ready
---

# Work integrations phase 4 verification handoff

**Audience:** the agent session on Harpreet's work laptop — the one that
produced the phase-3 reports.

Related (your prior reports):

- [[work-integrations-phase3-remaining-results]] (A–C rerun, `--additional-mcp-config` experiment, `workiq fetch` capture)
- [[work-integrations-phase3-functional-f-results]] (Work IQ briefing via Copilot's plugin)
- [[work-integrations-phase3-auth-fixture-results]] (auth-fixture transport)

## What landed since your last round

Pull current `main` (tip `9241fb1`). Five things built on this machine need
real-hardware verification — tests cover their logic, but not a live daemon +
a real agent + real Work IQ auth:

1. **Meeting end-times grounded** (`a4d431a`) — the briefing prompt no longer
   lets the agent invent meeting end times.
2. **Spawn-time stdio MCP injection** (`00ead65`) — your `--additional-mcp-config`
   experiment is now a *product feature*: Notesmith injects stdio
   `[[mcp.servers]]` into Copilot itself. Needs verification through Notesmith's
   own config path (not your manual wrapper script).
3. **Effect-based job outcomes** (`20b6fa7`, `3a9d822`) — an agent job that
   exits 0 but writes nothing records `no_writes` (not a false `succeeded`);
   optional `success_when` SQL predicate.
4. **calendar-sync connector** (`160bf7e`) — M365 events → `kind: event` notes.
5. **email-summary connector** (`9241fb1`) — deterministic sender/subject
   digest into `briefing/email`, metadata-only.

## Prerequisites

- Pull `main` at `9241fb1`; `cargo test --workspace` green; `cargo build --release`.
- Reuse your scratch vault, or a fresh `kit apply work-notes` one. **After
  `kit apply`, `chmod +x .notesmith/connectors/*.py`** — the kit installs them
  as text (no exec bit).
- The `workiq` CLI installed and authenticated (its own OAuth cache), for §2/§4/§5.
- An agent CLI (Copilot for §2; any for others). Ask Harpreet for anything
  only a human can do (auth, corp domains). Ground rules unchanged: scratch
  vault, dummy data, nothing filed upstream.

---

## 1. Meeting end-time grounding (quick)

The dummy-data table in [[work-integrations-verification-handoff]] §2 was
updated: Standup now has an `end`, Acme sync deliberately has **none**. With
that data, run the briefing (`notesmith ai prompt daily-note --agent <id>
--allow-writes`) and check `briefing/meetings`:

- Standup renders `HH:MM–HH:MM [[Standup]] (internal)` (it has an `end`).
- Acme sync renders `HH:MM [[Acme sync]] (external)` — **start only**. An
  invented end time (e.g. `14:00–15:00`) is a **fail**. Re-run and confirm the
  no-`end` meeting never grows a fabricated range.

## 2. Spawn-time stdio MCP injection via Notesmith's own config (the real path)

Your phase-3 experiment proved `--additional-mcp-config` works via a wrapper
script. This verifies Notesmith now does it itself. **Disable Copilot's own
Work IQ plugin for this test** so any Work IQ access is unambiguously through
Notesmith's config.

1. Configure `workiq mcp` as a **stdio** external server in the global config
   (`~/.config/notesmith/config.toml`), NOT a wrapper:
   ```toml
   [[mcp.servers]]
   id = "workiq"
   command = "/abs/path/to/workiq"   # pre-installed binary, not npx (60s init budget)
   args = ["mcp"]
   enabled = true
   ```
2. Run the briefing with `--agent copilot --allow-writes`.
3. **Pass criteria:** no `Rejecting non-http/sse MCP server "workiq" from
   client` in the Copilot logs; the agent can call Work IQ tools; `briefing/email`
   gets a live summary (not the fallback). This proves Notesmith wrote the
   per-session `--additional-mcp-config` file and Copilot accepted the stdio
   server. Confirm the temp config file is gone after the run (deleted on
   session drop) and the vault has no raw email (boundary holds — same scan as
   your phase-F run).
4. Re-run with a Claude/Codex agent if available: the same stdio entry should
   reach it over ACP normally (no injection needed).

## 3. Effect-based job outcomes

The point is confirming the run-id write attribution flows through a **real**
agent session end to end (tests stub it).

1. **Write count on a real run.** Run the briefing (`job run daily-briefing`
   with the job enabled, or `ai prompt … --allow-writes`). Then
   `notesmith job list` — the daily-briefing run shows a **write count > 0**
   (the four section writes). This proves `X-Notesmith-Run-Id` reached the
   daemon and writes were counted for a real Copilot session.
2. **`no_writes`.** Add a throwaway agent job whose prompt does nothing (e.g. a
   one-line prompt "Reply with OK and write nothing."), `allow_writes = true`,
   and `job run` it. Expect: `job list` shows **`no writes`** (not
   `succeeded`), a `job.no_writes` SSE event (not `job.failed`), and
   `last_success` unchanged. Remove the throwaway job after.
3. **`success_when` (optional).** Add `success_when = "SELECT path FROM v_notes
   WHERE path = 'Daily/<today>.md'"` to a test job; run it after the daily note
   exists → `succeeded`; point it at a nonexistent path → `failed` with the
   predicate reason. Remove after.

## 4. calendar-sync connector (live)

Needs the `workiq` CLI authenticated. Enable it and set corp domains.

1. `chmod +x`; edit `.notesmith/connectors/calendar-sync.config.json` →
   `corp_domains` to Harpreet's real corp domain(s); set `enabled = true` on the
   `calendar-sync` job. Add a customer note with a `domains: ["<a real customer
   domain>"]` field so the mapping has something to resolve (or accept
   `customers: []` for all).
2. `notesmith job run calendar-sync`. Expect `Calendar/YYYY/MM/*.md` notes,
   `kind: event`, with `event_id`, `start`/`end`, `attendees`, `audience`
   (external iff a non-corp attendee), `customers` (wikilinks for mapped
   domains, `[]` otherwise), `organizer`, `tags: [calendar]`.
3. **Idempotency:** run it again — no duplicate notes; existing events updated
   in place (PATCH by `event_id`), not re-created. Change an event's time in the
   calendar, re-run, confirm the note updates.
4. **Boundary note:** calendar events *are* stored (allowed by ADR 0025) — this
   is not the email boundary. Just confirm no attendee email *bodies* or
   unexpected fields leak (there's no body in calendar data anyway).
5. **`after` chain (optional):** with `calendar-sync` and `daily-briefing` both
   enabled, confirm the briefing waits for calendar-sync to succeed today
   (`job list` shows `waiting on calendar-sync` until it runs).

## 5. email-summary connector (live + boundary)

The deterministic fallback for `briefing/email`. Metadata-only — this is the
one with the hard boundary.

1. `chmod +x`; `enabled = true` on `email-summary`. It has
   `after = ["daily-briefing"]`, but **`notesmith job run email-summary`
   bypasses `after`**, so you can test it directly.
2. Ensure today's daily note exists (`notesmith daily ensure`, or the connector
   POSTs `/daily` itself). `job run email-summary`. Expect `briefing/email`
   filled with `- HH:MM **Sender** — Subject` bullets for unread mail (a
   `N unread:` count line; `Nothing unread.` if the inbox is clear).
3. **Boundary scan (the critical check):** grep the entire vault, the vault's
   Notesmith state dir, and the daemon log for any email *body* content — pick a
   distinctive phrase from an actual unread email's body and confirm it appears
   **nowhere**. Only sender names, subjects, and times may persist. Also confirm
   the connector never requested a body: the `$select` in its `workiq fetch`
   call is `id,subject,from,receivedDateTime,isRead` only.
4. **Coexistence (does not clobber the judgment tier):**
   - Manually write a real summary into `briefing/email` (something without the
     words "Work IQ not connected"), `job run email-summary` → the section is
     **left untouched** (connector no-ops).
   - Set `briefing/email` to `Email summary unavailable (Work IQ not
     connected).`, run → the connector **replaces** it with the digest.
5. **Idempotency:** run twice with the same inbox → `briefing/email` converges
   (no duplicate bullets), and content *outside* the markers is byte-identical.

---

## Report back

One results table per section (1–5): check, pass / fail / not-exercised, and
for failures the exact command, output, and note content. Boundary claims (§2,
§5) must state the phrase you scanned for and that it was absent. Keep the
scratch vault for inspection and note its path. Nothing gets filed upstream.
