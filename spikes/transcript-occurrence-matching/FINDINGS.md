# Transcript Occurrence Matching Spike - Findings

**Date:** 2026-09-04
**Environment:** Work laptop with the delegated Work IQ CLI
**Privacy:** Subjects, customer names, attendee identities, meeting IDs, join
URLs, and transcript IDs are omitted.

## Executive Summary

Timestamp matching is viable for the retained recent transcript that could be
compared with its recurring calendar series. It selected one occurrence with a
runner-up margin of **13 days, 23:35:12**, so the observed series is nowhere
near the one-hour ambiguity threshold.

The timezone prerequisite also passes semantically: `calendarView` returned a
naive `dateTime` paired with `timeZone: UTC`, while transcript
`createdDateTime` used a trailing `Z`. Both surfaces therefore describe UTC,
and there is no offset disagreement in the sampled data. The differing
serialization still confirms that production code must use the calendar
`timeZone` field and normalize both values explicitly rather than relying on
naive wall-clock parsing.

**Decision:** Move `match_transcript` into `transcript-sync.py` unchanged.
Unmatched transcripts remain unfiled rather than guessed. Production sync must
also follow `calendarView` pagination and preserve explicit timezone
information.

## Probe Results

### 1. Calendar sample

The 30-day query returned a bounded first page:

| Measurement | Result |
|---|---:|
| Online-meeting occurrences | 44 |
| Distinct join URLs | 28 |
| Join URLs reused by multiple occurrences | 7 |
| Calendar response paged | Yes |

The retained recurring series used for the matching check had three visible
occurrences and two transcript metadata records. Consecutive visible
occurrences were 14 days apart.

### 2. Timezone relationship

| Surface | Representation | Effective zone |
|---|---|---|
| `calendarView.start.dateTime` | No inline zone marker | UTC from the adjacent `timeZone` field |
| Transcript `createdDateTime` | Trailing `Z` | UTC |

The zones agree. However, the current `calendar-sync.py` parser discards the
calendar `timeZone` field and persists a naive timestamp. That is safe only
while Graph continues returning UTC clock values. Calendar and transcript sync
should instead normalize aware values to UTC before comparison or persistence.

### 3. Matching margins

| Transcript | Outcome | Evidence |
|---|---|---|
| Recent retained transcript | Matched | Runner-up margin was 1,208,112 seconds (13 days, 23:35:12). |
| Older retained transcript | Unmatched in the 30-day sample | The nearest visible occurrence was 671.8 hours away, beyond the four-hour tolerance. |

The unmatched result is the correct safe behavior. It does not indicate an
ambiguous match: the required occurrence was not present in the bounded
calendar sample.

### 4. Sixty-day retry

The suggested `--days-back 60` retry did not include the target series because
the probe requests only the first 200 calendar records and does not follow
`@odata.nextLink`. The response was paged, and the first page contained zero
occurrences for the tested join URL. This result is not evidence against
timestamp matching.

Production `transcript-sync.py` must follow pagination. The probe should also
follow pagination before it is reused to investigate older transcripts.

### 5. Work IQ response behavior

During automatic series selection, some online-meeting lookups exited zero
with empty stdout. Identical isolated requests returned valid JSON, while other
series continued to return zero-byte responses. The current probe aborts on
the first such response, which can prevent it from reaching a later series
that has retained transcripts.

This did not affect the tested series, but production sync should surface the
empty response as a failed lookup and continue processing unrelated series.

## Batched Work-Laptop Verification

### Calendar `join_url`

The updated work-notes kit was applied to the isolated phase-4 verification
vault and calendar sync was run against live Work IQ data. It created five
event notes, updated fourteen, and skipped six cancelled events.

The live Graph window contained nineteen active events:

| Event class | Graph | Persisted correctly |
|---|---:|---:|
| Online event with `join_url` | 11 | 11 |
| Offline event without `join_url` | 8 | 8 |

All nineteen events were found by `event_id`. There were zero missing events,
zero URL mismatches, and no offline event acquired a `join_url`. The connector
bridge prerequisite is verified. Because calendar sync has no lookback, this
coverage begins with events synced after the updated connector was applied.

### Calendar local-time correction

After the timezone fix in `30ed221`, the work-notes kit was reapplied to the
isolated vault and `calendar-sync` was run through the Notesmith job runner.
The job completed successfully.

A live event known from Work IQ to begin at 09:00 local time had previously
been stored with:

- frontmatter start: `2026-09-09T16:00:00`
- filename time: `1600`

After the corrected sync, the existing note's frontmatter read
`2026-09-09T09:00:00`, proving that Graph's UTC clock was converted to local
wall-clock time. As expected, the in-place update left the old `1600` filename.

Before rebuilding, `Meetings/` contained no `event:` backlinks. The user
approved replacing the isolated vault's stale Calendar tree. Twenty-eight old
Calendar notes were deleted, and a fresh forward-only sync restored twenty-six
current-window event notes. The same known event then had both:

- frontmatter start: `2026-09-09T09:00:00`
- filename time: `0900`

A full indexed scan after rebuilding found no `kind: meeting` notes carrying an
`event_id`, so the filename change broke no meeting backlinks. Historical
Calendar notes outside the connector's today-through-seven-days window were
not restored.

### Meeting prefill

One live external meeting overlapped the hook's current-time window. Rendering
the external-meeting template with blank prompts selected that event, attached
its `event_id` and event-note backlink, and did not use the `Untitled`
fallback. Instantiating the template created a meeting note in the isolated
vault whose indexed frontmatter retained:

- `kind: meeting`
- the matching `event_id`
- the event-note backlink

The positive mid-call meeting-prefill path is verified.

### Transcript connector end-to-end

The work-notes kit containing `transcript-sync.py` and the paginated calendar
connector was applied to the isolated vault. Both connectors passed their
offline self-tests, and calendar sync then completed successfully through the
Notesmith job runner.

The first transcript-sync run found no eligible cached occurrence. This was
not a matcher failure: the approved Calendar-tree rebuild had intentionally
restored only today through seven days ahead, while transcript sync looks back
three days for recently ended meetings. A one-time September 1-4 calendar
backfill using the corrected timezone conversion restored nineteen past event
notes and produced nine eligible online occurrences across nine series.

Transcript sync then completed successfully and created two sidecar notes.
Both carried:

- `kind: transcript`
- `source_type: teams`
- the matched occurrence's `event_id`
- an event-note wikilink

No matching human meeting note existed, so neither sidecar carried a meeting
wikilink. The user opened the September 2 customer-call transcript and
confirmed from memory that it belonged to the correct meeting and contained
the expected speakers. This is the required ground-truth check that metadata
and schema validation cannot provide.

A second manual transcript-sync run left the note count at two, confirming
idempotence. After this check, both `calendar-sync` and `transcript-sync` were
enabled in the isolated vault; each job hot-reloaded as valid, and
`transcript-sync` had no unmet `after = ["calendar-sync"]` dependency.

**Fresh-install caveat:** a forward-only Calendar tree cannot support
transcript sync's three-day lookback until enough new meetings have accumulated.
Historical testing requires a one-time matching calendar backfill, but normal
scheduled operation does not: calendar events will already exist before their
transcripts become available.

### Late meeting-to-transcript backlink

The September 2 transcript sidecar was selected for the late-write test. Its
`event_id` was used to locate the matching historical event, and the actual
meeting-prefill selector was run with that occurrence's timestamp to create the
meeting note after the transcript already existed.

The next transcript-sync run reported **two links completed**:

- the transcript gained a `meeting` wikilink;
- the meeting gained a `transcript` wikilink.

The meeting body was hashed before and after reconciliation and remained
byte-identical. Only frontmatter changed. A reconcile-only second run reported
**zero links completed**, and the body remained byte-identical again.

The first reconciliation run also reported operational noise that did not
affect the repaired pair:

- seven series lookups returned an empty Work IQ response and were isolated;
- five old transcripts were left unfiled because their nearest cached
  occurrence was far outside the four-hour tolerance;
- one attempted transcript creation hit an existing-path conflict.

The connector still completed the intended backlinks and did not guess at any
of the unfiled transcripts.

### Live calendar pagination

Running:

```text
calendar-sync.py --since 2026-08-01
```

completed successfully and upserted the extended window. A separate
instrumented read of the same live window fetched **two pages and 132 events**:
the first page carried `@odata.nextLink`, and the second/final page did not.
This verifies that stripping the Graph service prefix produces an entity path
accepted by `workiq fetch`; the connector neither failed mid-run nor silently
stopped at the first page.

### Verification-vault scope

The user approved continued use of
`/Users/surdy/vaults/verify-work-phase4-2026-09-04` for work-laptop integration
verification. The registered `work` fixture and `Customer Notes` vault both
lacked connector installations and Calendar trees, so neither was modified.

### Organizer coverage and transcript-path collision

The hypothesis that `/me/onlineMeetings` only resolves meetings organized by
the signed-in user is **false**.

The isolated vault contained 35 distinct online-meeting series:

| Organizer category | Resolved | Empty Work IQ body |
|---|---:|---:|
| Signed-in user | 3 | 0 |
| Other organizer | 11 | 21 |

The other-organized counterexamples were repeatable: one sampled recurring
external-audience series resolved on three consecutive attempts. Across the
full sample, both resolved and unresolved other-organized sets included:

- single and recurring series;
- internal-domain and external-domain organizers;
- external-audience events.

Those cached fields did not explain the split. Coverage is therefore partial,
but it is not accurate to document it as organizer-owned meetings only.

The collision-fixed transcript run reported:

```text
1 created, 2 already present, 0 left unfiled,
0 links completed, 6 failed
```

The newly created note recovered the transcript previously lost to a path
conflict. The affected event now has two transcript notes. Its older note kept
the legacy date-only filename, while the recovered note used the new
date-and-time filename, so this live migration case did not require a
`(transcript 2)` suffix. The suffix allocator remains covered by the
connector's self-test but was not exercised by live data.

A second run reported:

```text
0 created, 3 already present, 0 left unfiled,
0 links completed, 6 failed
```

This confirms deduplication and shows that the collision and stale-occurrence
fixes reduced `left unfiled` from five to zero. Six current-window series still
returned empty Work IQ bodies; their failure was isolated and did not block
the three available transcripts.

### Empty-body diagnostic classification

The September 4 local-time diagnostic run used the connector from `3de8a32`,
which preserves Work IQ stderr and separates an empty Graph result from a
failed call. The normal three-day run reported:

```text
0 created, 3 already present, 0 left unfiled,
0 links completed, 0 series not in Work IQ, 6 failed
```

All six current-window failures carried the same explicit error. A read-only
replay of only the online-meeting lookup across the original 35-series sample
then produced:

```text
14 resolved, 0 series not in Work IQ, 21 failed
```

One representative stderr line, with only the meeting subject redacted:

```text
transcript-sync: series '<redacted>' lookup failed: workiq exited 0 with an empty body: {"results":[{"data":null,"statusCode":403,"error":{"error":{"code":"Forbidden","message":"3003: User does not have access to lookup meeting","innerError":{"date":"2026-09-05T00:53:59","request-id":"e50b2a7e-ad2f-4d97-8213-251345cdc063","client-request-id":"e99e560f-0122-4da0-a80f-1dbb724f24f5"}}}}],"requestId":"e99e560f-0122-4da0-a80f-1dbb724f24f5"}
```

The unexplained twenty-one are therefore access denials, not successful Graph
lookups with no matching online meeting. Query-string stripping is not the
next fix indicated by this evidence. All 35 sampled join URLs used
`thread.v2` and included a query string, so neither the channel-meeting marker
nor query-string presence distinguished the fourteen resolvable series from
the twenty-one denied series. Organizer identity, organizer domain, recurrence,
audience, and the persisted join-URL structure still do not expose the actual
access boundary.

## Phase-5 Requirements

1. Use the existing `match_transcript` algorithm.
2. Leave results unmatched when no occurrence is within the four-hour
   tolerance or when the best candidate is ambiguous.
3. Normalize calendar and transcript timestamps explicitly to UTC; do not
   discard `calendarView`'s `timeZone` field.
4. Follow every `calendarView` continuation page needed by the sync window.
5. Isolate online-meeting lookup failures per series so one zero-byte Work IQ
   response does not abort the whole sync.
