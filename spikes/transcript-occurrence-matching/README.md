# Spike: transcript → calendar occurrence matching

## The question

`plans/transcript-sync-spike-results.md` established that phase 5 is viable,
and recorded one consequence that it did not test:

> The stable recurring join URL exposes a second join problem: calendar
> `event_id` identifies one occurrence, while the online-meeting ID identifies
> the recurring meeting/thread shared by multiple occurrences. Transcript sync
> therefore cannot join a transcript to an occurrence by join URL alone. It must
> match transcript `createdDateTime` / `endDateTime` to the corresponding
> event's time window, then carry that occurrence's `event_id` into the
> Transcript Note.

That is a plan, not a verified fact. The spike sampled a *single* meeting's
transcript; it never watched timestamp matching pick one occurrence out of a
real recurring series.

**Why it is worth a probe rather than a guess.** Getting this wrong does not
throw — it attaches a customer call's verbatim transcript to the wrong meeting
note, quietly, and the mistake is only visible to someone who reads both. It is
also the one part of `transcript-sync.py` that cannot be tested offline in any
meaningful way, so building the note-writing half on top of an unverified
assumption risks rebuilding it.

## What it checks

1. **Does timestamp matching resolve unambiguously?** For every transcript in a
   recurring series, which occurrence wins and by what margin over the
   runner-up. A large margin means the approach is sound; a small one means
   `transcript-sync` must refuse rather than guess.
2. **Do the two timestamps even share a timezone?** `calendarView` start/end and
   transcript `createdDateTime` are fetched from different Graph surfaces. If
   one is UTC and the other local wall-clock, *every* match is off by the UTC
   offset — which for a daily series lands on the wrong day. The probe prints
   both zone markers side by side rather than assuming.
3. **How tight do series get?** The smallest gap between consecutive
   occurrences. A weekly sync is trivially matchable; a twice-daily standup is
   where this breaks.

## Running it

On the work laptop, with the Work IQ CLI authenticated:

```sh
python3 probe.py                     # auto-pick the series with the most occurrences
python3 probe.py --days-back 60      # widen if nothing recurring has transcripts
python3 probe.py --join-url '<url>'  # a specific series you know was recorded
python3 probe.py --redact            # omit subjects, for pasting into FINDINGS.md
```

Offline, anywhere: `python3 probe.py --self-test` exercises the matching logic
against synthetic series (weekly, twice-daily, exact ties, malformed input) with
no network and no Work IQ.

**It is read-only.** Graph GETs only; it writes nothing to the vault or to disk,
and it never requests transcript *content* — only metadata. Nothing it prints
contains spoken words.

If the first run finds no recurring series with transcripts, that is not a
failure of the probe: the parent spike observed transcripts absent from older
occurrences of a recurring meeting. Widen `--days-back`, or point `--join-url`
at a recurring meeting you know was recorded recently.

## What the answers mean

| Outcome | What it means for phase 5 |
|---|---|
| Every transcript matched, margins hours or days | Assumption holds. `match_transcript` moves into `transcript-sync.py` as-is. |
| Matches correct but margins under an hour | Approach works, but the connector must treat a small margin as unmatched and leave the transcript for manual filing rather than misattribute it. |
| Zones disagree between the two surfaces | **Fix before anything else.** Normalize to a single zone in the connector; the current `calendar-sync.py` also stores naive local wall-clock, so its `start`/`end` may need the same treatment. |
| Transcripts unmatched or landing on the wrong occurrence | The timestamp approach is insufficient. Look at whether the transcript metadata carries anything else joinable before building on it. |

`match_transcript` in `probe.py` is the function `transcript-sync.py` will use,
kept here so the probe tests the real logic rather than a sketch of it.

## Findings

Record results in `FINDINGS.md` alongside this file, matching the convention in
`spikes/turbovault-spike/`. Keep customer names, subjects, meeting IDs and join
URLs out of it — `--redact` exists for that.
