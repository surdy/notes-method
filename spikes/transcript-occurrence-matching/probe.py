#!/usr/bin/env python3
"""Probe: can transcript timestamps pick the right calendar occurrence?

Phase 5 of the integrations plan hinges on one assumption that
`plans/transcript-sync-spike-results.md` recorded but did not test:

    Recurring calendar occurrences reuse one Teams join URL, so join URL alone
    resolves to a *series*. Transcript sync must therefore match each
    transcript's timestamps to the right occurrence before assigning that
    occurrence's `event_id`.

The spike verified a *single* meeting's transcript. It never checked that
timestamp matching actually picks the right occurrence out of a recurring
series, which is the assumption most likely to be wrong -- and getting it wrong
attaches a customer call's transcript to the wrong meeting note, silently.

This probe answers that against your real calendar. It is **read-only**: it
issues Microsoft Graph GETs through `workiq fetch` and writes nothing, not to
the vault, not to disk. It never requests transcript *content*, only metadata.

    python3 probe.py                     # auto-pick the best series to test
    python3 probe.py --days-back 60
    python3 probe.py --join-url '<url>'  # test one specific series
    python3 probe.py --redact            # omit subjects, for pasting into FINDINGS.md
    python3 probe.py --self-test         # offline; no network, no Work IQ

Stdlib only, mirroring the connectors.
"""

import argparse
import json
import subprocess
import sys
import urllib.parse
from datetime import datetime, timedelta

# A transcript is finalized around the time the meeting ends, but not to the
# second. Beyond this distance from an occurrence's window we refuse to guess.
MATCH_TOLERANCE = timedelta(hours=4)

# Below this gap between consecutive occurrences, timestamp matching is at real
# risk of picking the wrong one -- a twice-daily standup, say.
TIGHT_SERIES_GAP = timedelta(hours=6)


# --------------------------------------------------------------------------
# Pure functions (unit-testable, no I/O)
# --------------------------------------------------------------------------


def parse_graph_datetime(value):
    """Parse a Graph datetime to a naive datetime, or None.

    Handles `2026-08-04T09:30:00.0000000`, a trailing `Z`, and offsets. The
    zone is *recorded separately* by `zone_of` rather than applied -- see the
    timezone warning this probe prints, which is half its point.
    """
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip()
    for suffix in ("Z", "z"):
        if text.endswith(suffix):
            text = text[:-1]
            break
    else:
        # Strip a +HH:MM / -HH:MM offset if present (after the time part).
        if len(text) > 19 and text[19] in "+-":
            text = text[:19]
    text = text.replace("T", " ")
    if "." in text:
        text = text.split(".", 1)[0]
    try:
        return datetime.strptime(text, "%Y-%m-%d %H:%M:%S")
    except ValueError:
        try:
            return datetime.strptime(text, "%Y-%m-%d %H:%M")
        except ValueError:
            return None


def zone_of(value: str) -> str:
    """`Z`, `+02:00`, or `naive` — what zone marker a Graph datetime carries."""
    if not isinstance(value, str):
        return "missing"
    text = value.strip()
    if text.endswith(("Z", "z")):
        return "Z (UTC)"
    if len(text) > 19 and text[19] in "+-":
        return text[19:]
    return "naive (no zone marker)"


def distance_to_window(moment, start, end):
    """Seconds from `moment` to the [start, end] interval; 0.0 when inside."""
    if end < start:
        start, end = end, start
    if start <= moment <= end:
        return 0.0
    if moment < start:
        return (start - moment).total_seconds()
    return (moment - end).total_seconds()


def match_transcript(moment, occurrences, tolerance=MATCH_TOLERANCE):
    """Pick the occurrence a transcript created at `moment` belongs to.

    Returns ``(best, margin_seconds, reason)``:

    - ``best`` is the chosen occurrence dict, or None when nothing is within
      `tolerance` -- refusing to guess is the whole point, since a wrong match
      silently misfiles a customer call.
    - ``margin_seconds`` is how much closer the winner is than the runner-up.
      A small margin is the danger sign; the connector should treat it as
      unmatched rather than coin-flip.
    - ``reason`` explains an absent match.

    This is the function `transcript-sync.py` will use, kept here so the probe
    tests the real logic rather than a sketch of it.
    """
    if moment is None:
        return None, None, "transcript has no usable timestamp"
    scored = []
    for occ in occurrences:
        if occ.get("start") is None or occ.get("end") is None:
            continue
        scored.append((distance_to_window(moment, occ["start"], occ["end"]), occ))
    if not scored:
        return None, None, "no occurrence has a usable start/end"

    scored.sort(key=lambda pair: (pair[0], pair[1]["start"]))
    best_distance, best = scored[0]
    if best_distance > tolerance.total_seconds():
        return (
            None,
            None,
            f"nearest occurrence is {best_distance / 3600:.1f}h away, beyond the "
            f"{tolerance.total_seconds() / 3600:.0f}h tolerance",
        )
    margin = (scored[1][0] - best_distance) if len(scored) > 1 else None
    return best, margin, ""


def min_gap(occurrences):
    """The smallest gap between consecutive occurrence starts, or None."""
    starts = sorted(o["start"] for o in occurrences if o.get("start"))
    if len(starts) < 2:
        return None
    return min(b - a for a, b in zip(starts, starts[1:]))


# --------------------------------------------------------------------------
# I/O (network / subprocess)
# --------------------------------------------------------------------------


def workiq_fetch(entity_url: str):
    """`workiq fetch -u <url>` → parsed JSON. Same contract as the connectors."""
    try:
        proc = subprocess.run(
            ["workiq", "fetch", "-u", entity_url], capture_output=True, text=True
        )
    except FileNotFoundError:
        raise RuntimeError(
            "the Work IQ CLI (`workiq`) is not on PATH.\n"
            "This probe only runs on the work laptop, against a signed-in CLI.\n"
            "Use `--self-test` to exercise the matching logic anywhere."
        ) from None
    if proc.returncode != 0:
        raise RuntimeError(
            f"workiq fetch failed ({proc.returncode}) for {entity_url}\n"
            f"{proc.stderr.strip()}"
        )
    try:
        return json.loads(proc.stdout)
    except ValueError as error:
        raise RuntimeError(
            f"workiq returned non-JSON for {entity_url}: {error}"
        ) from None


def fetch_occurrences(days_back: int):
    """Calendar occurrences over the window (calendarView expands recurrences)."""
    now = datetime.now()
    start = (now - timedelta(days=days_back)).replace(
        hour=0, minute=0, second=0, microsecond=0
    )
    params = {
        "startDateTime": start.strftime("%Y-%m-%dT%H:%M:%S"),
        "endDateTime": now.strftime("%Y-%m-%dT%H:%M:%S"),
        "$select": "id,subject,start,end,isOnlineMeeting,onlineMeeting,isCancelled",
        "$top": "200",
    }
    url = "/me/calendarView?" + urllib.parse.urlencode(params, safe="$,")
    payload = workiq_fetch(url)
    events = []
    for raw in payload.get("value", []):
        if raw.get("isCancelled") or not raw.get("isOnlineMeeting"):
            continue
        join = ((raw.get("onlineMeeting") or {}).get("joinUrl") or "").strip()
        if not join:
            continue
        events.append(
            {
                "event_id": raw.get("id"),
                "subject": raw.get("subject") or "(no subject)",
                "start": parse_graph_datetime((raw.get("start") or {}).get("dateTime")),
                "end": parse_graph_datetime((raw.get("end") or {}).get("dateTime")),
                "start_raw": (raw.get("start") or {}).get("dateTime"),
                "start_tz": (raw.get("start") or {}).get("timeZone"),
                "join_url": join,
            }
        )
    return events, payload.get("@odata.nextLink") is not None


def resolve_online_meeting(join: str):
    """join URL → online meeting id, or None."""
    quoted = join.replace("'", "''")
    url = "/me/onlineMeetings?$filter=" + urllib.parse.quote(
        f"joinWebUrl eq '{quoted}'", safe="$= '"
    )
    payload = workiq_fetch(url)
    values = payload.get("value") or []
    return values[0].get("id") if values else None


def fetch_transcripts(meeting_id: str):
    """Transcript *metadata* for an online meeting. Never content."""
    payload = workiq_fetch(f"/me/onlineMeetings/{meeting_id}/transcripts")
    out = []
    for raw in payload.get("value", []):
        created = raw.get("createdDateTime")
        out.append(
            {
                "id": raw.get("id"),
                "created_raw": created,
                "created": parse_graph_datetime(created),
                "end": parse_graph_datetime(raw.get("endDateTime")),
            }
        )
    return out


# --------------------------------------------------------------------------
# Report
# --------------------------------------------------------------------------


def label(text: str, redact: bool) -> str:
    return "[REDACTED]" if redact else text


def run_probe(days_back: int, only_join: str, redact: bool) -> int:
    print(f"Fetching online-meeting occurrences for the last {days_back} days…")
    occurrences, truncated = fetch_occurrences(days_back)
    if not occurrences:
        print("No online-meeting occurrences found. Nothing to probe.")
        return 1
    if truncated:
        print("! The calendar response was paged; this is a bounded sample.")

    # Group occurrences by join URL — that grouping *is* the series.
    series = {}
    for occ in occurrences:
        series.setdefault(occ["join_url"], []).append(occ)

    recurring = {url: occs for url, occs in series.items() if len(occs) > 1}
    print(
        f"{len(occurrences)} occurrences across {len(series)} join URLs; "
        f"{len(recurring)} of those URLs are shared by more than one occurrence."
    )
    if not recurring and not only_join:
        print(
            "\nNo recurring series in this window — the risky case cannot be\n"
            "tested here. Re-run with a longer --days-back, or pass --join-url\n"
            "for a series you know recurs."
        )
        return 1

    # Timezone relationship: the thing most likely to silently corrupt matching.
    sample = occurrences[0]
    print("\n--- Timezone check ---")
    print(f"calendarView start : {sample['start_raw']}  ({zone_of(sample['start_raw'])})")
    print(f"calendarView timeZone field : {sample['start_tz']}")

    candidates = (
        [(only_join, series.get(only_join, []))]
        if only_join
        else sorted(recurring.items(), key=lambda kv: -len(kv[1]))
    )

    tested = 0
    for join, occs in candidates:
        if not occs:
            print(f"\nNo occurrences found for the given --join-url.")
            return 1
        meeting_id = resolve_online_meeting(join)
        if not meeting_id:
            continue
        transcripts = fetch_transcripts(meeting_id)
        if not transcripts:
            continue

        tested += 1
        occs = sorted(occs, key=lambda o: o["start"] or datetime.min)
        print("\n" + "=" * 68)
        print(f"Series: {label(occs[0]['subject'], redact)}")
        print(f"{len(occs)} occurrences, {len(transcripts)} transcripts")

        gap = min_gap(occs)
        if gap is not None:
            tight = " <-- TIGHT" if gap < TIGHT_SERIES_GAP else ""
            print(f"Smallest gap between occurrences: {gap}{tight}")

        print(f"\nTranscript createdDateTime zone: "
              f"{zone_of(transcripts[0]['created_raw'])}")
        print(
            "If that differs from the calendarView zone above, matching MUST\n"
            "normalize before comparing — otherwise every match is off by the\n"
            "UTC offset and lands on the wrong occurrence.\n"
        )

        for transcript in sorted(
            transcripts, key=lambda t: t["created"] or datetime.min
        ):
            best, margin, reason = match_transcript(transcript["created"], occs)
            stamp = transcript["created_raw"]
            if best is None:
                print(f"  {stamp}  ->  UNMATCHED ({reason})")
                continue
            margin_text = (
                "only occurrence"
                if margin is None
                else f"{margin / 3600:.1f}h clear of the runner-up"
            )
            flag = ""
            if margin is not None and margin < 3600:
                flag = "   <-- AMBIGUOUS, margin under an hour"
            print(
                f"  {stamp}  ->  occurrence starting "
                f"{best['start']}  ({margin_text}){flag}"
            )

    print("\n" + "=" * 68)
    if tested == 0:
        print(
            "No recurring series in this window had any transcripts, so the\n"
            "occurrence-matching assumption is still untested. Try a longer\n"
            "--days-back, or --join-url for a recurring meeting you know was\n"
            "recorded. Note the spike observed transcripts disappearing from\n"
            "older occurrences, so the window may simply be too far back."
        )
        return 1

    print(
        f"Probed {tested} series.\n\n"
        "Record in FINDINGS.md: whether every transcript matched, the margins,\n"
        "any AMBIGUOUS or UNMATCHED lines, the smallest occurrence gap, and\n"
        "above all whether the two timestamp zones agreed."
    )
    return 0


# --------------------------------------------------------------------------
# Self-test (no network) — the matching logic, offline
# --------------------------------------------------------------------------


def _occ(day, hour, minute=0, length=30):
    start = datetime(2026, 8, day, hour, minute)
    return {
        "event_id": f"evt-{day:02d}-{hour:02d}{minute:02d}",
        "subject": "Weekly sync",
        "start": start,
        "end": start + timedelta(minutes=length),
    }


def self_test() -> int:
    # A weekly series: matching should be trivially unambiguous.
    weekly = [_occ(4, 9), _occ(11, 9), _occ(18, 9)]

    # Transcript finalized a few minutes after the second occurrence ended.
    best, margin, reason = match_transcript(datetime(2026, 8, 11, 9, 34), weekly)
    assert best["event_id"] == "evt-11-0900", (best, reason)
    assert margin > 6 * 24 * 3600, margin  # ~a week clear

    # Created *during* the meeting still matches it.
    best, _, _ = match_transcript(datetime(2026, 8, 18, 9, 15), weekly)
    assert best["event_id"] == "evt-18-0900", best

    # Well outside every window: refuse to guess rather than misfile.
    best, _, reason = match_transcript(datetime(2026, 8, 14, 12, 0), weekly)
    assert best is None, best
    assert "beyond the" in reason, reason

    # The dangerous shape: twice-daily standups 3h apart. A transcript landing
    # between them must still resolve, but with a small margin the caller can
    # see -- that margin is what makes the risk visible.
    twice = [_occ(4, 9, length=15), _occ(4, 12, length=15)]
    best, margin, _ = match_transcript(datetime(2026, 8, 4, 9, 20), twice)
    assert best["event_id"] == "evt-04-0900", best
    assert margin is not None and margin < 3 * 3600, margin

    # Exactly between two occurrences: the earlier one wins deterministically,
    # and the zero margin is the signal not to trust it.
    tie = [_occ(4, 9, length=0), _occ(4, 11, length=0)]
    best, margin, _ = match_transcript(datetime(2026, 8, 4, 10, 0), tie)
    assert best["event_id"] == "evt-04-0900", best
    assert margin == 0.0, margin

    # Missing/garbage inputs degrade rather than raise.
    assert match_transcript(None, weekly)[0] is None
    assert match_transcript(datetime(2026, 8, 4, 9, 0), [])[0] is None
    assert match_transcript(datetime(2026, 8, 4, 9, 0), [{"start": None, "end": None}])[0] is None

    # An occurrence with end before start is tolerated, not fatal.
    inverted = [{"event_id": "x", "start": _occ(4, 10)["start"], "end": _occ(4, 9)["start"]}]
    assert match_transcript(datetime(2026, 8, 4, 9, 30), inverted)[0]["event_id"] == "x"

    # Timestamp parsing across the shapes Graph actually emits.
    assert parse_graph_datetime("2026-08-04T09:30:00.0000000") == datetime(2026, 8, 4, 9, 30)
    assert parse_graph_datetime("2026-08-04T09:30:00Z") == datetime(2026, 8, 4, 9, 30)
    assert parse_graph_datetime("2026-08-04T09:30:00+02:00") == datetime(2026, 8, 4, 9, 30)
    assert parse_graph_datetime("") is None
    assert parse_graph_datetime(None) is None
    assert parse_graph_datetime("not a date") is None

    # Zone reporting — the check the probe exists to surface.
    assert zone_of("2026-08-04T09:30:00Z") == "Z (UTC)"
    assert zone_of("2026-08-04T09:30:00+02:00") == "+02:00"
    assert zone_of("2026-08-04T09:30:00.0000000") == "naive (no zone marker)"

    # Gap detection flags a tight series.
    assert min_gap(weekly) == timedelta(days=7)
    assert min_gap(twice) == timedelta(hours=3)
    assert min_gap([_occ(4, 9)]) is None

    print("OK")
    return 0


def main(argv) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--days-back", type=int, default=30)
    parser.add_argument("--join-url", default="")
    parser.add_argument("--redact", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    try:
        return run_probe(args.days_back, args.join_url, args.redact)
    except RuntimeError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
