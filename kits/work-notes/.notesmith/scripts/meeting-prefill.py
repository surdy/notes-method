#!/usr/bin/env python3
"""meeting-prefill pre-render hook (integrations plan, feature 1).

Reads the template render context on stdin and, when a `kind: event` note
synced by `calendar-sync.py` overlaps *now*, returns the meeting's identity as
extra context on stdout. Templates use it to fill the title, customer,
attendees and `event_id` of a meeting note without the user retyping them.

This is a **pre-render hook**, not a connector: it never touches the network
and never reads the cache directly. The template's `context_queries` do the
SQL (the engine already holds the cache handle); this script only picks the
right row and flattens it into scalars a template can interpolate.

Contract (docs/vault-configuration.md):
  stdin   the current render context as a JSON object
  stdout  a JSON object merged into that context
  Failure of any kind must degrade to "no match" — a hook that errors, times
  out or prints nothing leaves the template rendering a blank meeting note.

Stdlib only (json, sys, re, datetime), matching the connectors.
"""

import json
import re
import sys
from datetime import datetime, timedelta

# How far either side of an event we still call it "the current meeting".
# Ten minutes covers joining early and running over into the note-taking.
WINDOW_MINUTES = 10

# Kept byte-identical to calendar-sync.py's `_UNSAFE_PATH_CHARS` so a subject
# that produced `Calendar/.../0930 Acme Q3 sync roadmap.md` produces the same
# spelling in the meeting note's filename.
_UNSAFE_PATH_CHARS = re.compile(r'[\\/:*?"<>|\x00-\x1f]')

# `Calendar/YYYY/MM/YYYY-MM-DD HHMM <subject>.md` — the path shape
# calendar-sync.py writes. The subject is not stored as a frontmatter field,
# so the basename is the only place to recover it from.
_EVENT_BASENAME = re.compile(r"^(?P<date>\d{4}-\d{2}-\d{2}) (?P<hhmm>\d{4}) (?P<subject>.+)$")

# The context queries this hook consumes. Mirrored in the meeting templates'
# `context_queries` blocks; `golden_vault_prompts.rs` executes the templates'
# copies against the real schema so the two cannot drift silently.
#
# The day predicate spans yesterday..tomorrow rather than just today. A call
# that starts at 23:55 is still running at 00:05, and `date('now')` alone would
# have dropped it; the precise +/-10m decision belongs to `select_event`, not to
# SQL. A three-day window is a few dozen rows.
CANDIDATES_SQL = (
    "SELECT n.path AS path, "
    "MAX(CASE WHEN f.key = 'event_id' THEN f.value END) AS event_id, "
    "MAX(CASE WHEN f.key = 'start' THEN f.value END) AS start, "
    "MAX(CASE WHEN f.key = 'end' THEN f.value END) AS end, "
    "MAX(CASE WHEN f.key = 'audience' THEN f.value END) AS audience, "
    "MAX(CASE WHEN f.key = 'organizer' THEN f.value END) AS organizer "
    "FROM v_notes n "
    "JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path "
    "WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event') "
    "GROUP BY n.path "
    "HAVING substr(MAX(CASE WHEN f.key = 'start' THEN f.value END), 1, 10) "
    "BETWEEN date('now', 'localtime', '-1 day') "
    "AND date('now', 'localtime', '+1 day')"
)

MEMBERS_SQL = (
    "SELECT note_path AS path, key AS key, ordinal AS ordinal, value AS value "
    "FROM v_field_values "
    "WHERE key IN ('attendees', 'customers') "
    "AND note_path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event') "
    "AND note_path IN (SELECT note_path FROM v_fields WHERE key = 'start' "
    "AND substr(value, 1, 10) BETWEEN date('now', 'localtime', '-1 day') "
    "AND date('now', 'localtime', '+1 day')) "
    "ORDER BY note_path, key, ordinal"
)


# --------------------------------------------------------------------------
# Pure functions (unit-testable, no I/O)
# --------------------------------------------------------------------------


def sanitize_segment(text: str) -> str:
    """Make a title safe for use as a filename segment.

    Same rules as calendar-sync.py's `sanitize_subject`, so a calendar subject
    round-trips to the same characters in both the event note's path and the
    meeting note's path.
    """
    cleaned = _UNSAFE_PATH_CHARS.sub(" ", text or "")
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned


def parse_naive(value) -> "datetime | None":
    """Parse a `YYYY-MM-DDTHH:MM:SS` (or space-separated) stamp, or None.

    Values come from the cache exactly as calendar-sync.py wrote them —
    naive local wall clock, no zone. Anything unparseable is treated as
    absent rather than raising: a single malformed event must not blank out
    the whole prefill.
    """
    if not isinstance(value, str):
        return None
    text = value.strip().replace("T", " ")
    if not text:
        return None
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"):
        try:
            return datetime.strptime(text, fmt)
        except ValueError:
            continue
    return None


def render_now(context: dict) -> datetime:
    """The render's own clock, falling back to the system clock.

    The engine puts `now` in the static context before calling us. Using it
    rather than our own `datetime.now()` keeps the hook's idea of "now"
    identical to the `{{ date }}`/`{{ time }}` the same render interpolates,
    and makes the self-test deterministic.
    """
    return parse_naive(context.get("now")) or datetime.now()


def unlink(value: str) -> str:
    """`[[Acme Corp]]` -> `Acme Corp`; anything else unchanged."""
    text = (value or "").strip()
    if text.startswith("[[") and text.endswith("]]"):
        return text[2:-2].strip()
    return text


def subject_from_path(path: str) -> str:
    """Recover the event subject from a calendar-sync note path."""
    stem = (path or "").rsplit("/", 1)[-1]
    if stem.endswith(".md"):
        stem = stem[:-3]
    match = _EVENT_BASENAME.match(stem)
    return match.group("subject") if match else stem


def link_target_from_path(path: str) -> str:
    """The wikilink target for an event note — its basename without `.md`.

    Matches how the daily-briefing prompt links events (`[[2026-09-03 0930
    Standup]]`), so a meeting note and the briefing point at the same target.
    """
    stem = (path or "").rsplit("/", 1)[-1]
    return stem[:-3] if stem.endswith(".md") else stem


def select_event(events, now: datetime, window_minutes: int = WINDOW_MINUTES):
    """The event overlapping `now`, or None.

    An event matches when `now` falls between its start and end, extended by
    `window_minutes` on both sides. An event with no `end` is treated as a
    point in time, so it matches only within the window around its start.
    Among matches the nearest start wins; ties break on start then path so
    the choice is deterministic for back-to-back meetings.
    """
    window = timedelta(minutes=window_minutes)
    best_key = None
    best_event = None
    for event in events or []:
        if not isinstance(event, dict):
            continue
        start = parse_naive(event.get("start"))
        if start is None:
            continue
        end = parse_naive(event.get("end")) or start
        if end < start:
            end = start
        if not (start - window <= now <= end + window):
            continue
        key = (
            abs((start - now).total_seconds()),
            start.isoformat(),
            str(event.get("path") or ""),
        )
        if best_key is None or key < best_key:
            best_key = key
            best_event = event
    return best_event


def members_for(rows, path: str, key: str) -> list:
    """Ordered `field_values` members of one list field on one note."""
    picked = []
    for row in rows or []:
        if not isinstance(row, dict):
            continue
        row_path = row.get("path") or row.get("note_path")
        if row_path != path or row.get("key") != key:
            continue
        ordinal = row.get("ordinal")
        picked.append((ordinal if isinstance(ordinal, int) else 0, row.get("value") or ""))
    picked.sort(key=lambda pair: pair[0])
    return [value for _, value in picked if value]


def empty_result(context: dict) -> dict:
    """The no-match context.

    Every key the templates read is always present, so a template never needs
    `is defined` guards and a blank calendar renders the same shape as a
    matched one — just with the user's typed values.
    """
    typed_title = str(context.get("title") or "").strip()
    typed_customer = unlink(str(context.get("customer") or ""))
    title = typed_title or "Untitled"
    return {
        "event_matched": False,
        "event_id": "",
        "event_note": "",
        "event_link": "",
        "event_subject": "",
        "event_audience": "",
        "event_organizer": "",
        "event_attendees": [],
        "event_customers": [],
        "meeting_title": title,
        "meeting_slug": sanitize_segment(title) or "Untitled",
        "meeting_customer": typed_customer,
        "meeting_customers": [typed_customer] if typed_customer else [],
        "meeting_date": str(context.get("date") or ""),
        "meeting_time": str(context.get("time") or ""),
        "meeting_attendees": [],
    }


def build_context(context: dict) -> dict:
    """The hook's whole job: context in, extra context out."""
    events = context.get("calendar_events")
    event = select_event(events, render_now(context))
    if event is None:
        return empty_result(context)

    path = str(event.get("path") or "")
    members = context.get("calendar_event_members")
    attendees = members_for(members, path, "attendees")
    customers = [unlink(value) for value in members_for(members, path, "customers")]
    customers = [value for value in customers if value]

    subject = subject_from_path(path)
    start = parse_naive(event.get("start"))

    # The user's typed values always win — the hook fills blanks, it does not
    # overrule what was entered at the prompt.
    typed_title = str(context.get("title") or "").strip()
    typed_customer = unlink(str(context.get("customer") or ""))
    title = typed_title or subject or "Untitled"
    customer = typed_customer or (customers[0] if customers else "")
    resolved_customers = [typed_customer] if typed_customer else customers

    return {
        "event_matched": True,
        "event_id": str(event.get("event_id") or ""),
        "event_note": path,
        "event_link": link_target_from_path(path),
        "event_subject": subject,
        "event_audience": str(event.get("audience") or ""),
        "event_organizer": str(event.get("organizer") or ""),
        "event_attendees": attendees,
        "event_customers": customers,
        "meeting_title": title,
        "meeting_slug": sanitize_segment(title) or "Untitled",
        "meeting_customer": customer,
        "meeting_customers": resolved_customers,
        "meeting_date": start.strftime("%Y-%m-%d") if start else str(context.get("date") or ""),
        "meeting_time": start.strftime("%H:%M") if start else str(context.get("time") or ""),
        "meeting_attendees": attendees,
    }


# --------------------------------------------------------------------------
# Entry point
# --------------------------------------------------------------------------


def main(argv) -> int:
    if "--self-test" in argv:
        return self_test()

    # A hook that raises leaves the template with nothing. Degrade to the
    # no-match shape instead, and say why on stderr (the engine logs it at
    # debug level).
    try:
        raw = sys.stdin.read()
        context = json.loads(raw) if raw.strip() else {}
        if not isinstance(context, dict):
            context = {}
    except (ValueError, OSError) as error:
        print(f"meeting-prefill: unreadable context: {error}", file=sys.stderr)
        context = {}

    try:
        result = build_context(context)
    except Exception as error:  # noqa: BLE001 - degrade, never break the render
        print(f"meeting-prefill: {error}", file=sys.stderr)
        result = empty_result(context)

    json.dump(result, sys.stdout)
    return 0


# --------------------------------------------------------------------------
# Self-test (no network, no cache) -- what the Rust test invokes
# --------------------------------------------------------------------------

_EXTERNAL = {
    "path": "Calendar/2026/08/2026-08-04 0930 Acme Q3 sync roadmap.md",
    "event_id": "AAMkAGI2-selftest-0001",
    "start": "2026-08-04T09:30:00",
    "end": "2026-08-04T10:00:00",
    "audience": "external",
    "organizer": "alice@acme.com",
}

_INTERNAL = {
    "path": "Calendar/2026/08/2026-08-04 1400 Internal standup.md",
    "event_id": "AAMkAGI2-selftest-0002",
    "start": "2026-08-04T14:00:00",
    "end": "2026-08-04T14:15:00",
    "audience": "internal",
    "organizer": "harpreet@corp.example.com",
}

_MEMBERS = [
    {"path": _EXTERNAL["path"], "key": "attendees", "ordinal": 0, "value": "alice@acme.com"},
    {"path": _EXTERNAL["path"], "key": "attendees", "ordinal": 1, "value": "harpreet@corp.example.com"},
    {"path": _EXTERNAL["path"], "key": "customers", "ordinal": 0, "value": "[[Acme Corp]]"},
    {"path": _INTERNAL["path"], "key": "attendees", "ordinal": 0, "value": "teammate@corp.example.com"},
]


def _context(now: str, **overrides) -> dict:
    base = {
        "now": now,
        "date": now[:10],
        "time": now[11:16],
        "calendar_events": [_EXTERNAL, _INTERNAL],
        "calendar_event_members": _MEMBERS,
    }
    base.update(overrides)
    return base


def self_test() -> int:
    # Mid-meeting: the external event wins and brings its identity with it.
    out = build_context(_context("2026-08-04 09:45:00"))
    assert out["event_matched"] is True, out
    assert out["event_id"] == "AAMkAGI2-selftest-0001", out
    assert out["event_subject"] == "Acme Q3 sync roadmap", out
    assert out["event_link"] == "2026-08-04 0930 Acme Q3 sync roadmap", out
    assert out["event_note"] == _EXTERNAL["path"], out
    assert out["event_audience"] == "external", out
    assert out["event_attendees"] == ["alice@acme.com", "harpreet@corp.example.com"], out
    assert out["event_customers"] == ["Acme Corp"], out
    assert out["meeting_title"] == "Acme Q3 sync roadmap", out
    assert out["meeting_customer"] == "Acme Corp", out
    assert out["meeting_customers"] == ["Acme Corp"], out
    assert out["meeting_date"] == "2026-08-04", out
    assert out["meeting_time"] == "09:30", out

    # Members of the *other* event never leak into the match.
    assert "teammate@corp.example.com" not in out["event_attendees"], out

    # Joining ten minutes early still matches; eleven does not.
    assert build_context(_context("2026-08-04 09:20:00"))["event_matched"] is True
    assert build_context(_context("2026-08-04 09:19:00"))["event_matched"] is False

    # Running ten minutes over still matches; eleven does not.
    assert build_context(_context("2026-08-04 10:10:00"))["event_matched"] is True
    assert build_context(_context("2026-08-04 10:11:00"))["event_matched"] is False

    # The internal event has no customer: audience carries, customers stay empty.
    internal = build_context(_context("2026-08-04 14:05:00"))
    assert internal["event_id"] == "AAMkAGI2-selftest-0002", internal
    assert internal["event_audience"] == "internal", internal
    assert internal["event_customers"] == [], internal
    assert internal["meeting_customer"] == "", internal
    assert internal["meeting_title"] == "Internal standup", internal

    # Typed values win over the calendar; the event identity still attaches.
    typed = build_context(
        _context("2026-08-04 09:45:00", title="Renewal risk", customer="Globex")
    )
    assert typed["meeting_title"] == "Renewal risk", typed
    assert typed["meeting_customer"] == "Globex", typed
    assert typed["meeting_customers"] == ["Globex"], typed
    assert typed["event_subject"] == "Acme Q3 sync roadmap", typed
    assert typed["event_id"] == "AAMkAGI2-selftest-0001", typed

    # A typed customer already in wikilink form is not double-bracketed.
    linked = build_context(_context("2026-08-04 09:45:00", customer="[[Globex]]"))
    assert linked["meeting_customer"] == "Globex", linked

    # No calendar at all (connector never ran, or a free slot): the typed
    # title still renders and every key is present.
    blank = build_context({"now": "2026-08-04 12:00:00", "date": "2026-08-04", "time": "12:00"})
    assert blank["event_matched"] is False, blank
    assert blank["meeting_title"] == "Untitled", blank
    assert blank["meeting_slug"] == "Untitled", blank
    assert blank["event_attendees"] == [], blank
    assert set(blank) == set(out), "matched and unmatched results must have the same keys"

    typed_blank = build_context({"now": "2026-08-04 12:00:00", "title": "Ad hoc chat"})
    assert typed_blank["meeting_title"] == "Ad hoc chat", typed_blank
    assert typed_blank["meeting_slug"] == "Ad hoc chat", typed_blank

    # Path-hostile subjects: display keeps them, the filename segment does not.
    slashed = dict(_EXTERNAL, path="Calendar/2026/08/2026-08-04 0930 Acme  Q3 sync  roadmap.md")
    out_slashed = build_context(_context("2026-08-04 09:45:00", calendar_events=[slashed]))
    assert out_slashed["meeting_slug"] == "Acme Q3 sync roadmap", out_slashed
    assert sanitize_segment('a/b:c*d?"e') == "a b c d e"
    assert sanitize_segment("   ") == ""

    # An event with no end is a point in time: inside the window, not after.
    pointy = {"path": "Calendar/2026/08/2026-08-04 1600 Quick sync.md", "start": "2026-08-04T16:00:00"}
    assert build_context(_context("2026-08-04 16:09:00", calendar_events=[pointy]))["event_matched"]
    assert not build_context(_context("2026-08-04 16:11:00", calendar_events=[pointy]))["event_matched"]

    # Back-to-back meetings: the nearer start wins, deterministically.
    first = {"path": "Calendar/2026/08/2026-08-04 0900 First.md", "start": "2026-08-04T09:00:00", "end": "2026-08-04T10:00:00"}
    second = {"path": "Calendar/2026/08/2026-08-04 1000 Second.md", "start": "2026-08-04T10:00:00", "end": "2026-08-04T11:00:00"}
    overlap = build_context(_context("2026-08-04 09:58:00", calendar_events=[first, second]))
    assert overlap["event_subject"] == "Second", overlap
    assert (
        build_context(_context("2026-08-04 09:58:00", calendar_events=[second, first]))["event_subject"]
        == "Second"
    ), "selection must not depend on row order"

    # A call running across midnight: matched, and dated by when it *started*.
    # The templates' day predicate spans yesterday..tomorrow for exactly this.
    midnight = {
        "path": "Calendar/2026/08/2026-08-04 2355 Late sync.md",
        "event_id": "AAMkAGI2-selftest-0003",
        "start": "2026-08-04T23:55:00",
        "end": "2026-08-05T00:25:00",
    }
    late = build_context(_context("2026-08-05 00:05:00", calendar_events=[midnight]))
    assert late["event_matched"] is True, late
    assert late["meeting_date"] == "2026-08-04", late
    assert late["meeting_time"] == "23:55", late
    assert late["meeting_title"] == "Late sync", late

    # Malformed rows are skipped, not fatal.
    junk = build_context(
        _context("2026-08-04 09:45:00", calendar_events=[{"start": "not a date"}, "nonsense", _EXTERNAL])
    )
    assert junk["event_id"] == "AAMkAGI2-selftest-0001", junk

    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
