---
title: Transcript-sync spike — questions to answer on the work laptop
date: 2026-09-04
tags:
  - notesmith
  - spike
  - workiq
  - transcripts
  - handoff
status: open
---

# Transcript-sync spike — questions to answer on the work laptop

Phase 5 of [[integrations-control-center-plan]] (`transcript-sync`) is the last
unbuilt feature, and unlike phase 3 it is **blocked on facts, not effort**. The
plan's phase-0 spike listed two transcript questions and neither has been
answered anywhere in this repo. This document turns them into commands you can
paste, with what each answer changes.

Related:

- [[integrations-control-center-plan]] — feature 2 and the phase-0 spike list
- [[work-integrations-phase4-verification-results]] — the last verification run
- `docs/adr/0025-work-system-integrations.md` — Decision 4 commits transcripts
  to `kind: transcript` sidecar notes joined by `event_id`
- `docs/adr/0023-local-whisper-transcription-worker.md` §7 — the shared
  transcript-note body format ADR 0025 says to reuse

Everything here is a **read-only** query against your own mailbox/calendar. No
writes, no vault changes.

## What is already settled (do not re-derive)

Established in this repo, verified by tests or by the phase-4 run:

- The Work IQ CLI contract is `workiq fetch -u <graph path>`, and its **stdout
  is parsed as JSON** (`calendar-sync.py:341`). Auth is the CLI's own cache;
  Notesmith holds no corp credentials.
- Event notes carry `event_id`, `start`, `end`, `attendees`, `audience`,
  `customers`, `organizer` — and **nothing that identifies the Teams meeting**.
  The connector's `$select` is `id,subject,start,end,attendees,organizer,isCancelled`
  (`calendar-sync.py:362`); `onlineMeeting` is not requested.
- The shared renderer (`notesmith-transcribe::render_transcript_note`) emits
  `title`, `source_url`, `source_type`, `duration`, `language`, `ingested_at`,
  `tags` — **no `kind`, no `event_id`** — and its `TranscriptSegment` is
  `{start, end, text}` with **no speaker field** (`notesmith-transcribe/src/lib.rs:73`).
- The work-notes kit's `vault.toml` configures **no ingest folder**, so the
  drop-folder fallback is not wired today.

## The questions

### A. Can a *delegated* Work IQ token read transcripts at all?

**Why it decides everything.** Graph has historically gated meeting
transcripts behind **application** permissions
(`OnlineMeetingTranscript.Read.All`) plus a Teams application access policy —
not delegated user consent. If that is still true, the Work IQ CLI (which acts
as you) cannot read transcripts no matter how the connector is written, and
phase 5's "deterministic MCP client" design is dead on arrival. Verify; do not
assume my reading of the permission model is current.

```sh
# 1. What can this token see? Find a meeting you own that was recorded.
workiq fetch -u "/me/events?\$select=id,subject,start,onlineMeeting&\$top=10&\$orderby=start/dateTime desc"

# 2. Pull the joinUrl out of a recorded meeting above, then:
workiq fetch -u "/me/onlineMeetings?\$filter=joinWebUrl eq '<JOIN_URL>'"

# 3. With the onlineMeeting id from step 2:
workiq fetch -u "/me/onlineMeetings/<MEETING_ID>/transcripts"
```

**Record:** the exact error text if any step fails — an authorization failure
reads very differently from "no transcripts exist for this meeting", and the
difference is the whole spike.

- **All three succeed** → phase 5 proceeds as designed.
- **Step 3 fails with an authorization/scope error** → the connector path is
  closed. Skip to question E; the drop folder becomes the *primary* mechanism,
  not the fallback, and phase 5 gets re-scoped.
- **Step 3 succeeds but returns empty for a meeting you know was transcribed**
  → likely tenant retention or a policy scope; ask IT before concluding.

### B. Is there a join key, and are we persisting it?

`transcript-sync` matches transcripts to meeting notes by `event_id`, but
Graph does not hand you a calendar `event_id` from the transcript side. The
bridge is the join URL: calendar event → `onlineMeeting.joinUrl` → onlineMeeting
→ transcripts.

**We do not store that bridge today.** Confirm the field is populated for your
real meetings:

```sh
workiq fetch -u "/me/calendarView?startDateTime=$(date -v-7d +%Y-%m-%dT00:00:00)&endDateTime=$(date +%Y-%m-%dT00:00:00)&\$select=id,subject,isOnlineMeeting,onlineMeeting&\$top=50"
```

**Record:** what fraction of your meetings have a non-null `onlineMeeting`, and
whether `joinUrl` is stable across a recurring series' instances.

If populated, the pre-req is a small `calendar-sync.py` change: add
`isOnlineMeeting,onlineMeeting` to `$select` and persist `join_url` on the event
note. That is cheap and worth doing regardless — but only after A says
transcripts are reachable. Do not add a field we have no consumer for.

### C. Can `workiq fetch` return a transcript *body* at all?

Graph serves transcript content as **WebVTT or docx**, not JSON:

```sh
workiq fetch -u "/me/onlineMeetings/<MEETING_ID>/transcripts/<TRANSCRIPT_ID>/content?\$format=text/vtt"
```

**Why it matters:** `calendar-sync.py` does `json.loads(proc.stdout)`. If the
CLI insists on JSON, wraps the body, base64s it, or errors on a non-JSON
response, then even a reachable transcript cannot be *retrieved* through this
CLI, and the connector needs a different transport (direct Graph call with a
token the CLI can mint, if it can).

**Record:** the first ~20 lines of output verbatim (this is your own meeting;
redact anything customer-identifying before pasting into the results doc), and
whether it is VTT, JSON-wrapped, or an error.

### D. What does a real Teams transcript look like — and does it fit our note format?

This is the design question the spike exists to inform, and it is why C asks
for a real sample.

ADR 0025 Decision 4 commits to "one Transcript Note concept … shared body
format via `notesmith-transcribe`'s renderer". But that renderer's segment
model is `{start, end, text}` with **no speaker**, and a Teams transcript's
entire value is *who said what*. A speaker-less rendering of a customer call is
close to useless.

**Record:** whether the VTT carries speaker labels (Teams typically emits
`<v Speaker Name>` voice tags), and roughly how many segments a one-hour
meeting produces.

Then pick, with a real sample in hand:

- **Extend the shared model** with `speaker: Option<String>` and have
  `transcript_body` render `[M:SS] Name: text`. Touches the YouTube and audio
  paths (both would pass `None`), keeps ADR 0025's one-concept promise. Small
  core change — which phase 5 already budgets as "none, then small".
- **Render Teams separately**, leaving the shared renderer alone. Cheaper now,
  but breaks the ADR's unification commitment and needs an amendment saying so.

Either way the renderer also needs `kind: transcript` and `event_id`, which it
does not emit today. Note this in the results.

### E. Is eoriq the same data or a separate system? — **resolved: neither**

**`eoriq` was a typo for `workiq`.** It named no system, so there is nothing to
ask. The name reached the phase-0 list from a single line in the session that
wrote the plan (`claude-code:e6f4911c`, 2026-08-05): "maybe I use eoriq/ teams
integration to pull meeting transcripts into the note". That same message
spells `workiq` correctly two sentences earlier, `w`->`e` is an adjacent key,
and the `k` dropped.

Consequence: no source-specific drop-folder fallback is a deliverable. A–D
settle transcripts entirely. The generic ingest folder remains worth having on
its own merits, but not as a hedge against a second transcript source.

### F. Corp policy on storing verbatim transcripts

Worth asking explicitly, because transcripts are the **inverse** of the email
decision. ADR 0025 forbids raw email from ever touching disk, yet commits
transcripts to plain-text vault notes — full verbatim customer speech, on a
local vault, pushed by the `[git]` timers to a corp remote.

That asymmetry is deliberate (transcripts are content you read and link; email
bodies are not), but it was decided by us, not by your IT policy. A recorded
customer call may carry consent or retention obligations that a calendar
subject line does not.

**Record:** whether verbatim transcripts of customer calls may live in a local
markdown vault, and whether the corp git remote is an approved destination for
them specifically. A "no" here does not block phases 1–4 — it scopes phase 5 to
internal meetings only, or kills it.

### G. Retention window

Tenants commonly auto-delete transcripts. If yours does, `transcript-sync`'s
polling cadence has a deadline and the backfill has a floor.

```sh
# Try a meeting from a month ago and one from last week; compare.
workiq fetch -u "/me/onlineMeetings/<OLD_MEETING_ID>/transcripts"
```

**Record:** how far back transcripts remain fetchable.

## How to report

Write the answers to `plans/transcript-sync-spike-results.md` in the same shape
as [[work-integrations-phase4-verification-results]] — a table per question
with a Pass/Fail/Blocked verdict and the evidence, plus the exact error text
for anything that failed. Keep real customer names, addresses and transcript
content out of the results file; the phase-4 run's redaction discipline
applies here more than anywhere, since transcript text is the most sensitive
material any of these connectors has touched.

Findings stay in this repo. Nothing here gets filed upstream.

## Decision gate

Answer **A** first and stop if it fails — B, C, D and G all assume transcripts
are reachable. E and F are independent and can be answered any time; F in
particular is a conversation with a human, not a command, so start it early.
