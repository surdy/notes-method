#!/usr/bin/env python3
"""calendar-sync connector (ADR 0025, Decisions 1/3/4).

A deterministic, LLM-free connector that pulls the user's Microsoft 365
calendar via the official Work IQ CLI (`workiq fetch`) and upserts each event
as a `kind: event` vault note keyed by `event_id`. It is a *connector*, not
core code: the daemon's generic `[[jobs]]` runner invokes it on a schedule
(see `.notesmith/vault.toml`), and it writes through the REST API. Corp
credentials never touch Notesmith -- `workiq` uses its own auth cache.

The module is structured as pure functions plus a thin `main` so the note
shape can be unit-tested with no network. Run `--self-test` to exercise the
pure logic against an embedded Graph fixture.

Stdlib only (json, os, sys, subprocess, urllib, datetime, pathlib, re,
argparse). No third-party dependencies.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone

try:  # Python 3.9+; absent only on very old interpreters.
    from zoneinfo import ZoneInfo
except ImportError:  # pragma: no cover - fallback keeps the connector running
    ZoneInfo = None

# --------------------------------------------------------------------------
# Pure functions (unit-testable, no I/O)
# --------------------------------------------------------------------------

# Characters the note-rename API rejects, plus control chars. Kept in sync
# with docs/http-api.md's rename rules so a synced subject never produces an
# unwritable path.
_UNSAFE_PATH_CHARS = re.compile(r'[\\/:*?"<>|\x00-\x1f]')


def sanitize_subject(subject: str) -> str:
    """Make an event subject safe for use as a filename segment.

    Strips path-hostile characters, collapses whitespace, and trims. Falls
    back to ``Untitled`` for an empty or all-unsafe subject so the path is
    always well-formed.
    """
    cleaned = _UNSAFE_PATH_CHARS.sub(" ", subject or "")
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned or "Untitled"


# Graph spells UTC several ways depending on the surface.
_UTC_ALIASES = {"utc", "gmt", "z", "coordinated universal time"}

_ZONE_MARKER = re.compile(r"(Z|[+-]\d{2}:?\d{2})$")


def _offset_tz(token: str) -> timezone:
    """`+02:00` / `-0700` -> a fixed-offset tzinfo."""
    digits = token[1:].replace(":", "")
    delta = timedelta(hours=int(digits[:2]), minutes=int(digits[2:4]))
    return timezone(delta if token[0] == "+" else -delta)


def _zone_from_name(name: str):
    """A tzinfo for a Graph ``timeZone`` value; UTC when it is not recognised.

    calendarView returns UTC unless the request carries a
    ``Prefer: outlook.timezone`` header, and the Work IQ CLI gives us no way to
    send one -- so UTC is both the default and the overwhelmingly likely value.
    A Windows zone name ("Pacific Standard Time") is not IANA and would not
    resolve; we say so on stderr rather than silently guessing.
    """
    text = (name or "").strip()
    if not text or text.lower() in _UTC_ALIASES:
        return timezone.utc
    if ZoneInfo is not None:
        try:
            return ZoneInfo(text)
        except Exception:  # noqa: BLE001 - any resolution failure means fall back
            pass
    print(
        f"calendar-sync: unrecognised timeZone {text!r}; treating it as UTC",
        file=sys.stderr,
    )
    return timezone.utc


def parse_graph_datetime(value: str, zone: str = "UTC") -> datetime:
    """Parse a Graph dateTime into a naive **local wall-clock** ``datetime``.

    Graph sends calendarView times as a zone-less ``dateTime`` plus a sibling
    ``timeZone`` field -- UTC in practice, per `zone` -- while other surfaces
    use a trailing ``Z`` or an offset. All three are converted to the local
    wall clock, because that is what the rest of the vault means by a time:
    `date:` fields, the briefing's `date('now', 'localtime')` queries, and
    meeting-prefill's window around `now` are all local.

    This previously *dropped* the zone and kept the raw components, which
    stored UTC clock values labelled as local -- a 7-hour error in PDT that
    also rolled evening meetings onto the following day. Verified against real
    synced notes on 2026-09-04; see
    `spikes/transcript-occurrence-matching/FINDINGS.md`.
    """
    text = (value or "").strip()
    marker = _ZONE_MARKER.search(text)
    body = text[: marker.start()] if marker else text
    if "." in body:
        body = body.split(".", 1)[0]

    parsed = None
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"):
        try:
            parsed = datetime.strptime(body, fmt)
            break
        except ValueError:
            continue
    if parsed is None:
        raise ValueError(f"unparseable Graph dateTime: {value!r}")

    if marker:
        token = marker.group(0)
        tzinfo = timezone.utc if token in ("Z", "z") else _offset_tz(token)
    else:
        tzinfo = _zone_from_name(zone)
    return parsed.replace(tzinfo=tzinfo).astimezone().replace(tzinfo=None)


def _iso_naive(dt: datetime) -> str:
    """Render a naive datetime as ``YYYY-MM-DDTHH:MM:SS`` (no zone)."""
    return dt.strftime("%Y-%m-%dT%H:%M:%S")


def _utc_z(dt: datetime) -> str:
    """Render an aware datetime as a UTC ``...Z`` stamp for a Graph query.

    A zone-less bound is interpreted by Graph as UTC, so sending local midnight
    bare shifted the whole sync window by the local offset.
    """
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def event_start(event: dict) -> datetime:
    """The event's start as a naive local datetime (raises on absent/invalid)."""
    start = event["start"]
    return parse_graph_datetime(start["dateTime"], start.get("timeZone"))


def event_note_path(event: dict) -> str:
    """Deterministic vault path for an event.

    ``Calendar/YYYY/MM/YYYY-MM-DD HHMM <sanitized subject>.md`` -- stable
    across resyncs (same subject + start => same path), so a create is only
    needed once and later syncs upsert in place.
    """
    start = event_start(event)
    subject = sanitize_subject(event.get("subject", ""))
    return (
        f"Calendar/{start:%Y}/{start:%m}/"
        f"{start:%Y-%m-%d} {start:%H%M} {subject}.md"
    )


def _domain_of(address: str) -> str:
    """Lowercased domain part of an email address, or '' if none."""
    if not address or "@" not in address:
        return ""
    return address.rsplit("@", 1)[1].strip().lower()


def attendee_addresses(event: dict) -> list:
    """Sorted, deduped attendee email addresses from a Graph event."""
    seen = set()
    for attendee in event.get("attendees", []) or []:
        address = (
            (attendee.get("emailAddress") or {}).get("address") or ""
        ).strip().lower()
        if address:
            seen.add(address)
    return sorted(seen)


def organizer_address(event: dict) -> str:
    """The organizer's email address (lowercased), or '' if absent."""
    return (
        ((event.get("organizer") or {}).get("emailAddress") or {}).get("address")
        or ""
    ).strip().lower()


def attendee_domains(addresses) -> list:
    """Distinct, sorted domains for a list of email addresses."""
    return sorted({d for d in (_domain_of(a) for a in addresses) if d})


def derive_audience(domains, corp_domains) -> str:
    """`external` if any attendee domain is outside the corp set, else `internal`.

    An event with no attendees (or only corp attendees) is `internal`.
    """
    corp = {d.strip().lower() for d in corp_domains if d.strip()}
    for domain in domains:
        if domain not in corp:
            return "external"
    return "internal"


def map_customers(domains, domain_to_customer) -> list:
    """Map attendee domains to customer wikilinks via the vault's mapping.

    Returns a sorted, deduped list of ``[[Customer Title]]`` strings for the
    domains that resolve; ``[]`` when none match (unmatched external domains
    are left for manual triage, surfaced in the daily briefing).
    """
    links = {
        domain_to_customer[d]
        for d in domains
        if d in domain_to_customer
    }
    return sorted(links)


def join_url(event: dict) -> str:
    """The Teams join URL for an online meeting, or '' when there is none.

    This is the bridge transcript-sync needs: a calendar event carries the join
    URL, the join URL resolves to an online meeting, and transcripts hang off
    that (ADR 0025's 2026-09-04 amendment). The event itself exposes no
    transcript link, so without this the two can never be connected.

    Note the URL identifies the *series*, not the occurrence -- recurring
    instances reuse one URL (verified in the spike). Transcript sync therefore
    matches transcript timestamps to the occurrence before assigning
    `event_id`; this field only gets it to the right meeting thread.
    """
    if not event.get("isOnlineMeeting"):
        return ""
    return ((event.get("onlineMeeting") or {}).get("joinUrl") or "").strip()


def graph_event_to_frontmatter(event, corp_domains, domain_to_customer) -> dict:
    """Build the `kind: event` frontmatter dict for a Graph event."""
    addresses = attendee_addresses(event)
    domains = attendee_domains(addresses)
    frontmatter = {
        "kind": "event",
        "event_id": event["id"],
        "start": _iso_naive(event_start(event)),
        "attendees": addresses,
        "audience": derive_audience(domains, corp_domains),
        "customers": map_customers(domains, domain_to_customer),
        "tags": ["calendar"],
    }
    end_field = event.get("end") or {}
    if end_field.get("dateTime"):
        frontmatter["end"] = _iso_naive(
            parse_graph_datetime(end_field["dateTime"], end_field.get("timeZone"))
        )
    organizer = organizer_address(event)
    if organizer:
        frontmatter["organizer"] = organizer
    join = join_url(event)
    if join:
        frontmatter["join_url"] = join
    return frontmatter


# YAML emission order for a rendered note. Any keys not listed fall to the end
# in sorted order. (The daemon's save pipeline re-sorts keys alphabetically on
# write; this order is for human-readable rendering and the self-test.)
_FRONTMATTER_ORDER = [
    "kind",
    "event_id",
    "start",
    "end",
    "attendees",
    "audience",
    "customers",
    "organizer",
    "join_url",
    "tags",
]


def _yaml_scalar(value: str) -> str:
    """Quote a scalar when needed; bare otherwise (matches note conventions)."""
    if value == "":
        return '""'
    # Datetimes and plain identifiers can stay bare; quote anything with
    # characters that would confuse a YAML parser.
    if re.fullmatch(r"[A-Za-z0-9_@.:+-]+", value):
        return value
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _yaml_list(values) -> str:
    """Render a list in flow style: ``["a", "b"]`` (``[]`` when empty)."""
    if not values:
        return "[]"
    return "[" + ", ".join('"' + str(v).replace('"', '\\"') + '"' for v in values) + "]"


# The machine-owned note body. Minimal on purpose: the meeting note is the
# authoritative record, this is just the calendar's copy.
NOTE_BODY = (
    "<!-- Machine-owned calendar record synced by calendar-sync.py "
    "(ADR 0025). The meeting note is the authoritative record; edits here "
    "may be overwritten on the next sync. -->\n"
)


def render_frontmatter(frontmatter: dict) -> str:
    """Render the YAML frontmatter block (including the ``---`` fences)."""
    keys = [k for k in _FRONTMATTER_ORDER if k in frontmatter]
    keys += sorted(k for k in frontmatter if k not in _FRONTMATTER_ORDER)

    lines = ["---"]
    for key in keys:
        value = frontmatter[key]
        if isinstance(value, list):
            lines.append(f"{key}: {_yaml_list(value)}")
        else:
            lines.append(f"{key}: {_yaml_scalar(str(value))}")
    lines.append("---")
    return "\n".join(lines) + "\n"


def render_note(frontmatter: dict) -> str:
    """Render a full event note (YAML frontmatter + machine-owned body)."""
    return render_frontmatter(frontmatter) + "\n" + NOTE_BODY


# --------------------------------------------------------------------------
# I/O helpers (network / subprocess) -- exercised at runtime, not in self-test
# --------------------------------------------------------------------------


def _api_base() -> str:
    return os.environ.get("NOTESMITH_API_BASE", "http://127.0.0.1:27183").rstrip("/")


def _vault() -> str:
    vault = os.environ.get("NOTESMITH_VAULT")
    if not vault:
        raise SystemExit("NOTESMITH_VAULT is not set")
    return vault


def _http_json(method: str, url: str, payload=None) -> dict:
    """Issue a JSON request and return the parsed body (or {} on 204)."""
    data = None
    headers = {"Accept": "application/json"}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(request, timeout=30) as response:
        body = response.read()
    if not body:
        return {}
    return json.loads(body)


def query_sql(sql: str):
    """Run a read-only SELECT via the REST API and return list-of-row-dicts."""
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/query/sql"
    result = _http_json("POST", url, {"sql": sql})
    columns = result.get("columns", [])
    return [dict(zip(columns, row)) for row in result.get("rows", [])]


# The mapping query the connector depends on. Kept as a module constant so the
# Rust test (golden_vault_prompts) can assert this exact SQL stays valid
# against the real index schema.
DOMAIN_MAP_SQL = (
    "SELECT d.value AS domain, n.title AS title "
    "FROM v_field_values d "
    "JOIN v_notes n ON n.vault_name = d.vault_name AND n.path = d.note_path "
    "WHERE d.key = 'domains'"
)


def load_domain_to_customer() -> dict:
    """Build ``{domain: '[[Customer Title]]'}`` from customer-note metadata."""
    mapping = {}
    for row in query_sql(DOMAIN_MAP_SQL):
        domain = (row.get("domain") or "").strip().lower()
        title = (row.get("title") or "").strip()
        if domain and title:
            mapping[domain] = f"[[{title}]]"
    return mapping


def find_note_by_event_id(event_id: str):
    """Return the existing note path for an event_id, or None."""
    safe = event_id.replace("'", "''")
    rows = query_sql(
        "SELECT note_path FROM v_field_values "
        f"WHERE key = 'event_id' AND value = '{safe}'"
    )
    if rows:
        return rows[0].get("note_path")
    return None


def create_note(path: str, frontmatter: dict) -> None:
    """Create a new note at a deterministic Calendar path via POST /notes."""
    folder, _, filename = path.rpartition("/")
    title = filename[:-3] if filename.endswith(".md") else filename
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/notes"
    _http_json(
        "POST",
        url,
        {
            "title": title,
            "folder": folder,
            "content": NOTE_BODY,
            "frontmatter": frontmatter,
        },
    )


def update_note(path: str, frontmatter: dict) -> None:
    """Upsert frontmatter into an existing note via PATCH (merge)."""
    encoded = "/".join(urllib.parse.quote(part) for part in path.split("/"))
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/notes/{encoded}"
    _http_json("PATCH", url, {"frontmatter": frontmatter})


def workiq_fetch(entity_url: str) -> dict:
    """Shell out to `workiq fetch -u <url>` and return the parsed Graph JSON."""
    proc = subprocess.run(
        ["workiq", "fetch", "-u", entity_url],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"workiq fetch failed ({proc.returncode}): {proc.stderr.strip()}"
        )
    return json.loads(proc.stdout)


def calendar_view_url(days_ahead: int) -> str:
    """Graph calendarView entity URL for [start of today, +days_ahead]."""
    today = (
        datetime.now()
        .astimezone()
        .replace(hour=0, minute=0, second=0, microsecond=0)
    )
    end = today + timedelta(days=days_ahead)
    params = {
        "startDateTime": _utc_z(today),
        "endDateTime": _utc_z(end),
        "$select": (
            "id,subject,start,end,attendees,organizer,isCancelled,"
            "isOnlineMeeting,onlineMeeting"
        ),
        "$top": "100",
    }
    return "/me/calendarView?" + urllib.parse.urlencode(params, safe="$,")


# --------------------------------------------------------------------------
# Config
# --------------------------------------------------------------------------

_DEFAULT_CONFIG = {"corp_domains": [], "sync_days_ahead": 7}


def load_config() -> dict:
    """Read the sibling `calendar-sync.config.json`, falling back to defaults."""
    config = dict(_DEFAULT_CONFIG)
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                        "calendar-sync.config.json")
    try:
        with open(path, "r", encoding="utf-8") as handle:
            config.update(json.load(handle))
    except FileNotFoundError:
        pass
    return config


# --------------------------------------------------------------------------
# Runtime entry point
# --------------------------------------------------------------------------


def run_sync() -> int:
    """Fetch the calendar window and upsert event notes. Returns exit code."""
    config = load_config()
    corp_domains = config.get("corp_domains", [])
    days_ahead = int(config.get("sync_days_ahead", 7))

    domain_to_customer = load_domain_to_customer()
    graph = workiq_fetch(calendar_view_url(days_ahead))

    created = 0
    updated = 0
    skipped = 0
    for event in graph.get("value", []):
        if event.get("isCancelled"):
            skipped += 1
            continue
        frontmatter = graph_event_to_frontmatter(
            event, corp_domains, domain_to_customer
        )
        existing = find_note_by_event_id(frontmatter["event_id"])
        if existing:
            update_note(existing, frontmatter)
            updated += 1
        else:
            create_note(event_note_path(event), frontmatter)
            created += 1

    print(
        f"calendar-sync: {created} created, {updated} updated, "
        f"{skipped} skipped (cancelled)"
    )
    return 0


# --------------------------------------------------------------------------
# Self-test (no network) -- what the Rust test invokes
# --------------------------------------------------------------------------

_SELF_TEST_GRAPH = {
    "value": [
        {
            "id": "AAMkAGI2-selftest-0001",
            "subject": "Acme / Q3 sync: roadmap?",
            "start": {"dateTime": "2026-08-04T09:30:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-08-04T10:00:00.0000000", "timeZone": "UTC"},
            "isCancelled": False,
            "isOnlineMeeting": True,
            "onlineMeeting": {
                "joinUrl": "https://teams.microsoft.com/l/meetup-join/19%3aselftest%40thread.v2/0"
            },
            "organizer": {"emailAddress": {"name": "Alice", "address": "Alice@Acme.com"}},
            "attendees": [
                {"emailAddress": {"name": "Alice", "address": "alice@acme.com"}},
                {"emailAddress": {"name": "Me", "address": "harpreet@corp.example.com"}},
            ],
        },
        {
            "id": "AAMkAGI2-selftest-0002",
            "subject": "Internal standup",
            "start": {"dateTime": "2026-08-04T14:00:00", "timeZone": "UTC"},
            "end": {"dateTime": "2026-08-04T14:15:00", "timeZone": "UTC"},
            "isCancelled": False,
            "organizer": {"emailAddress": {"address": "harpreet@corp.example.com"}},
            "attendees": [
                {"emailAddress": {"address": "teammate@corp.example.com"}},
            ],
        },
    ]
}


class _PinnedZone:
    """Pin the process timezone so conversion assertions are machine-independent.

    `parse_graph_datetime` converts to *local* time, so a self-test that did not
    pin the zone would pass or fail depending on where it ran.
    """

    def __init__(self, name: str):
        self.name = name
        self.previous = os.environ.get("TZ")

    def __enter__(self):
        os.environ["TZ"] = self.name
        time.tzset()
        return self

    def __exit__(self, *_exc):
        if self.previous is None:
            os.environ.pop("TZ", None)
        else:
            os.environ["TZ"] = self.previous
        time.tzset()
        return False


def _test_timezone_conversion() -> None:
    """The 2026-09-04 bug: Graph sends UTC, the vault stores local wall clock.

    Real synced notes showed a 17:00 PDT meeting stored as `2026-09-04T00:00:00`
    -- the right instant, the wrong clock, and the wrong day.
    """
    with _PinnedZone("America/Los_Angeles"):
        # calendarView's shape: zone-less dateTime + a sibling timeZone.
        assert parse_graph_datetime("2026-09-04T00:00:00", "UTC") == datetime(
            2026, 9, 3, 17, 0
        ), "a 00:00 UTC stamp is the previous day at 17:00 in PDT"
        assert parse_graph_datetime("2026-09-03T16:05:00.0000000", "UTC") == datetime(
            2026, 9, 3, 9, 5
        )

        # An explicit marker on the value wins over the sibling field.
        assert parse_graph_datetime("2026-09-03T16:05:00Z", "UTC") == datetime(
            2026, 9, 3, 9, 5
        )
        assert parse_graph_datetime("2026-09-03T18:05:00+02:00", "UTC") == datetime(
            2026, 9, 3, 9, 5
        )
        assert parse_graph_datetime("2026-09-03T18:05:00+0200", "UTC") == datetime(
            2026, 9, 3, 9, 5
        )

        # Standard time, not just daylight time: the offset is -08:00 in January.
        assert parse_graph_datetime("2026-01-15T17:00:00", "UTC") == datetime(
            2026, 1, 15, 9, 0
        ), "DST must come from the zone database, not a fixed offset"

        # A Windows zone name is not IANA; fall back to UTC rather than guess.
        assert parse_graph_datetime(
            "2026-09-03T16:05:00", "Pacific Standard Time"
        ) == datetime(2026, 9, 3, 9, 5)

        # The request window goes out as UTC, so Graph does not reinterpret it.
        url = calendar_view_url(7)
        assert "startDateTime=2026" in url or "startDateTime=" in url, url
        assert "Z&" in url or url.rstrip().endswith("Z"), url

    with _PinnedZone("UTC"):
        # In UTC the conversion is the identity -- the shape the old code
        # accidentally assumed everywhere.
        assert parse_graph_datetime("2026-09-03T16:05:00", "UTC") == datetime(
            2026, 9, 3, 16, 5
        )


def self_test() -> int:
    _test_timezone_conversion()
    # The fixture assertions below read the fixtures' UTC clock values at face
    # value. `parse_graph_datetime` now converts to local, so pin UTC or the
    # expected paths and times would depend on where the test runs.
    with _PinnedZone("UTC"):
        return _self_test_fixtures()


def _self_test_fixtures() -> int:
    corp = ["corp.example.com"]
    mapping = {"acme.com": "[[Acme Corp]]"}
    events = _SELF_TEST_GRAPH["value"]

    external, internal = events[0], events[1]

    # Path: sanitized subject (/ and : removed), date + HHMM prefix.
    assert (
        event_note_path(external)
        == "Calendar/2026/08/2026-08-04 0930 Acme Q3 sync roadmap.md"
    ), event_note_path(external)

    # Audience derivation.
    ext_domains = attendee_domains(attendee_addresses(external))
    assert derive_audience(ext_domains, corp) == "external"
    int_domains = attendee_domains(attendee_addresses(internal))
    assert derive_audience(int_domains, corp) == "internal"

    # Customer mapping (case-insensitive on the organizer address too).
    assert map_customers(ext_domains, mapping) == ["[[Acme Corp]]"]
    assert map_customers(int_domains, mapping) == []

    # Full frontmatter for the external event.
    fm = graph_event_to_frontmatter(external, corp, mapping)
    assert fm == {
        "kind": "event",
        "event_id": "AAMkAGI2-selftest-0001",
        "start": "2026-08-04T09:30:00",
        "end": "2026-08-04T10:00:00",
        "attendees": ["alice@acme.com", "harpreet@corp.example.com"],
        "audience": "external",
        "customers": ["[[Acme Corp]]"],
        "organizer": "alice@acme.com",
        "join_url": "https://teams.microsoft.com/l/meetup-join/19%3aselftest%40thread.v2/0",
        "tags": ["calendar"],
    }, fm

    # Unmatched external domain leaves customers empty for triage.
    unmatched = graph_event_to_frontmatter(external, corp, {})
    assert unmatched["audience"] == "external"
    assert unmatched["customers"] == []

    # An internal event: no organizer key omitted only when absent; here present.
    fm_int = graph_event_to_frontmatter(internal, corp, mapping)
    assert fm_int["audience"] == "internal"
    assert fm_int["customers"] == []

    # The Teams bridge transcript-sync joins on (ADR 0025 2026-09-04).
    assert join_url(external).endswith("thread.v2/0"), join_url(external)
    assert "join_url" not in fm_int, "a non-online meeting has no bridge"

    # A meeting flagged online but carrying no URL yields no key, rather than
    # an empty one that would look like a bridge and resolve to nothing.
    assert join_url({"isOnlineMeeting": True}) == ""
    assert join_url({"isOnlineMeeting": True, "onlineMeeting": {}}) == ""
    assert join_url({"isOnlineMeeting": True, "onlineMeeting": {"joinUrl": "  "}}) == ""

    # A stale joinUrl on an event no longer flagged online is ignored.
    assert (
        join_url({"isOnlineMeeting": False, "onlineMeeting": {"joinUrl": "https://x"}})
        == ""
    )

    # Rendered note round-trips the frontmatter and carries a machine-owned body.
    rendered = render_note(fm)
    assert rendered.startswith("---\nkind: event\n")
    assert 'attendees: ["alice@acme.com", "harpreet@corp.example.com"]' in rendered
    assert 'customers: ["[[Acme Corp]]"]' in rendered
    assert "audience: external" in rendered
    assert (
        'join_url: "https://teams.microsoft.com/l/meetup-join/19%3aselftest%40thread.v2/0"'
        in rendered
    ), rendered
    assert "Machine-owned calendar record" in rendered

    # Empty / unsafe subjects still yield a well-formed path.
    # The calendarView request must ask for the bridge fields; without them
    # `onlineMeeting` comes back absent and every event looks offline.
    url = calendar_view_url(7)
    assert "isOnlineMeeting" in url and "onlineMeeting" in url, url

    assert sanitize_subject("") == "Untitled"
    assert sanitize_subject('a/b:c*d?"e') == "a b c d e"

    print("OK")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run the embedded pure-logic checks (no network) and exit.",
    )
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    try:
        return run_sync()
    except Exception as error:  # noqa: BLE001 -- surface as a failed job
        print(f"calendar-sync: FAILED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
