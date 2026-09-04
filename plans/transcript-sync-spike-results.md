---
title: Transcript-sync spike results
date: 2026-09-04
tags:
  - notesmith
  - spike
  - workiq
  - transcripts
  - handoff
status: complete
---

# Transcript-sync spike results

Related:

- [[transcript-sync-spike]]
- [[integrations-control-center-plan]]
- [[work-integrations-phase4-verification-results]]
- `docs/adr/0025-work-system-integrations.md`
- `docs/adr/0023-local-whisper-transcription-worker.md`

## Environment and handling

- Repository: `surdy/notes-method`, `main` at `473198f`
- Verification date: September 4, 2026
- Work IQ MCP: hosted, authenticated delegated-user connection
- Work IQ CLI:
  `1.0.0.28144+10c4074955aee0affce923a5fb04d7ed22c5a09e`
- Operations: read-only Microsoft 365 entity queries

No raw transcript was written to the repository, vault, or a persistent test
artifact. CLI content checks used temporary files that were deleted on command
exit. Customer names, attendee identities, meeting IDs, join URLs, transcript
IDs, and spoken content are omitted below.

## A. Delegated transcript access

| Check | Verdict | Evidence |
|---|---|---|
| Calendar event exposes online-meeting bridge | Pass | A known transcribed meeting organized by the signed-in user returned `isOnlineMeeting: true` and a populated `onlineMeeting.joinUrl`. |
| Join URL resolves to an online meeting | Pass | `/me/onlineMeetings?$filter=joinWebUrl eq '<redacted>'` returned HTTP 200 and one online-meeting entity. |
| Transcript collection is readable | Pass | `/me/onlineMeetings/<redacted>/transcripts` returned HTTP 200 and one transcript entity for a meeting independently confirmed as transcribed. |
| Authorization/scope error | Not observed | No 401, 403, access-policy, missing-scope, or consent error occurred. |

**Conclusion:** the delegated Work IQ token can read Teams transcript metadata.
Phase 5's deterministic Work IQ connector is viable; the drop folder does not
need to become the primary path for permission reasons.

An initial probe against another organized online meeting returned HTTP 200
with an empty transcript collection. Repeating against a meeting independently
confirmed as transcribed produced one result, proving that an empty collection
means "no currently exposed transcript for this meeting", not a transport or
authorization failure.

## B. Join-key coverage and recurring meetings

| Check | Verdict | Evidence |
|---|---|---|
| Recent calendar coverage | Pass with limitation | In the complete August 28–September 4 window, 13 of 35 events (37.1%) had a non-null `onlineMeeting.joinUrl`. Non-online events correctly had no bridge. |
| Broader sample | Partial | In the first 100 events from August 5–September 4, 64 had a join URL. The response carried `@odata.nextLink`, so 64% is only a bounded sample, not the complete-month fraction. |
| Recurring-series stability | Pass in sample | Fourteen recurring series had multiple instances in the bounded month sample; all 14 reused one join URL and none varied. |
| Bridge persisted by calendar connector | Fail/pre-requisite | Event notes currently persist neither `isOnlineMeeting` nor `join_url`; the existing `$select` does not request them. |

The calendar connector must add `isOnlineMeeting,onlineMeeting` to `$select`
and persist the join URL on event notes before transcript sync can work.

The stable recurring join URL exposes a second join problem: calendar
`event_id` identifies one occurrence, while the online-meeting ID identifies
the recurring meeting/thread shared by multiple occurrences. Transcript sync
therefore cannot join a transcript to an occurrence by join URL alone. It must
match transcript `createdDateTime` / `endDateTime` to the corresponding event's
time window, then carry that occurrence's `event_id` into the Transcript Note.

## C. Transcript content through the Work IQ CLI

| Check | Verdict | Evidence |
|---|---|---|
| Content endpoint | Pass | `workiq fetch -u "/me/onlineMeetings/<redacted>/transcripts/<redacted>/content?$format=text/vtt"` exited 0 with empty stderr and 20,277 stdout bytes. |
| CLI output shape | Pass with required parser change | Stdout is valid JSON whose top-level value is a JSON **string** containing WebVTT. It is not a Graph object and not raw unquoted VTT. |
| VTT validity | Pass | Decoding the JSON string yielded `WEBVTT`, 350 lines, and 116 timed cues. |

The connector may continue using `json.loads(proc.stdout)`, but the result for
content is a `str`, not a `dict`. Code modeled on `calendar-sync.py` must branch
on the endpoint: collection calls return Graph objects; transcript content
returns a JSON-string-wrapped VTT document.

Redacted first lines:

```text
WEBVTT

00:00:03.447 --> 00:00:06.567
<v [SPEAKER]>[REDACTED]

00:00:07.527 --> 00:00:26.727
<v [SPEAKER]>[REDACTED]
```

## D. Teams transcript shape and shared renderer

| Check | Verdict | Evidence |
|---|---|---|
| Speaker labels | Pass | Every one of the 116 cues carried a `<v Speaker>` voice tag; the sample contained two distinct speakers. |
| Segment volume | Partial | The available sample covered about 16 minutes and contained 116 cues. A one-hour transcript was not sampled, so no exact one-hour count is claimed. |
| Existing segment model | Fail/pre-requisite | `TranscriptSegment { start, end, text }` cannot preserve the observed speaker data. |
| Existing frontmatter | Fail/pre-requisite | The shared renderer does not emit `kind: transcript` or the matched occurrence's `event_id`. |

**Decision:** extend the shared model with `speaker: Option<String>` and render
speaker-bearing segments as `[M:SS] Name: text`. YouTube and local-audio paths
pass `None`. This preserves ADR 0025's one Transcript Note concept instead of
creating a Teams-only renderer. The renderer or its caller must also supply
`kind: transcript`, `event_id`, and the meeting/customer links required by the
work-notes model.

The sample included overlapping cue intervals, so a VTT parser must preserve
source order and must not assume each cue starts after the prior cue ends.

## E. eoriq

| Check | Verdict | Evidence |
|---|---|---|
| Identify the system | Blocked | The user does not recognize the name `eoriq`. A public exact-name search found no relevant meeting/transcript product. |
| Export behavior | Blocked | No system was identified, so bulk versus per-meeting export cannot be tested. |

Treat `eoriq` as an unresolved internal name or typo. It does not block the
now-viable Work IQ connector. Do not build a special fallback for it unless a
real separate source and export contract are identified.

## F. Corporate transcript-storage policy

| Check | Verdict | Evidence |
|---|---|---|
| Local Markdown storage | Pass | The user confirmed verbatim customer-call transcripts may be stored in the local work vault. |
| Corporate Git remote | Pass | The user confirmed those transcript notes may be pushed to the corporate Git remote. |

This approval is specific to the work laptop/work vault placement already
required by ADR 0025. It does not permit customer transcripts on the personal
homelab.

## G. Retention window

| Check | Verdict | Evidence |
|---|---|---|
| Recent transcript | Pass | A transcript created August 18 remained fetchable on September 4: an observed retention floor of at least 17 days. |
| Older known-transcribed occurrences | Partial/fail | Meeting records for June 16 and July 21 still reported that transcription occurred, but the shared recurring online-meeting transcript collection exposed only the August 18 transcript. |
| Exact tenant retention period | Blocked | The observations cannot distinguish tenant deletion from recurring-meeting collection semantics. |

Do not promise historical backfill. A 30-minute sync is comfortably inside the
observed 17-day floor, but the initial connector should record a cursor and
ingest promptly. Confirm the formal retention policy with IT; field-test a
fresh transcript and the same item over time if an exact window is required.

## Phase-5 decision

Phase 5 is **unblocked**, with these implementation prerequisites:

1. Extend `calendar-sync.py` to request and persist `isOnlineMeeting` and
   `join_url`.
2. Resolve `join_url` to an online-meeting ID, list transcript metadata, and
   map recurring-series transcripts to event occurrences by timestamps before
   assigning `event_id`.
3. Parse transcript content as a JSON string containing WebVTT.
4. Extend `TranscriptSegment` with `speaker: Option<String>` and preserve VTT
   voice tags through the shared renderer.
5. Emit `kind: transcript`, matched `event_id`, meeting/customer links, and
   `source_type: teams` in the sidecar note.
6. Store a connector cursor in `NOTESMITH_STATE_DIR`; sync promptly and treat
   old-history backfill as best-effort until retention is confirmed.
7. Keep generic drop-folder ingest as a documented manual fallback, but add no
   eoriq-specific work unless that system is identified.

Prerequisite 2's occurrence-selection rule was verified on the work laptop in
`spikes/transcript-occurrence-matching/FINDINGS.md`. The observed retained
recent transcript had a 13-day, 23:35:12 margin over the runner-up, so the
probe's `match_transcript` function can move into the connector unchanged.
Production sync must explicitly normalize UTC timestamps, follow
`calendarView` pagination, and leave out-of-window or ambiguous transcripts
unmatched.
