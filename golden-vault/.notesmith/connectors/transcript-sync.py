#!/usr/bin/env python3
"""transcript-sync connector (ADR 0025, Decision 4 + the 2026-09-04 amendments).

Pulls Teams meeting transcripts for recently-ended calendar events and writes
them as sidecar `kind: transcript` notes, linked to their meeting by `event_id`.
It is a *connector*, not core: the daemon's generic `[[jobs]]` runner invokes
it, and it holds no corp credentials of its own.

How the join works, which is the whole difficulty:

    event note (`join_url`) -> online meeting -> transcripts

A calendar event exposes no transcript link. Its Teams join URL resolves to an
online meeting, and transcripts hang off that. But **recurring occurrences reuse
one join URL**, so the online meeting is the *series*, not the occurrence. Each
transcript's `createdDateTime` is therefore matched back to a specific
occurrence's time window before that occurrence's `event_id` is written into
the note. Verified against a real series with a 13-day margin; see
`spikes/transcript-occurrence-matching/FINDINGS.md`.

Occurrences come from the *local cache*, not from Graph: `calendar-sync` has
already synced them, with `start`/`end` converted to local wall clock. Reading
them here keeps this connector off the calendarView pagination path entirely.

The transcript body is rendered by core, not here. `notesmith transcribe
--from-vtt -` owns the shared `[M:SS] Name: text` format so this connector
cannot drift from it, and piping on stdin means transcript text never touches
disk.

Stdlib only (json, os, re, subprocess, sys, urllib, datetime, argparse).
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone

# A transcript is finalized around the time the meeting ends, but not to the
# second. Beyond this distance from an occurrence's window we refuse to guess.
MATCH_TOLERANCE = timedelta(hours=4)

# Two candidate occurrences closer together than this make the choice a
# coin-flip. An unfiled transcript is recoverable; one attached to the wrong
# customer meeting is not.
AMBIGUOUS_MARGIN = timedelta(hours=1)

# Graph paginates with an absolute nextLink; `workiq fetch -u` takes a path.
_GRAPH_PREFIX = re.compile(r"^https://graph\.microsoft\.com/(?:v1\.0|beta)", re.I)
MAX_PAGES = 20

_UNSAFE_PATH_CHARS = re.compile(r'[\\/:*?"<>|\x00-\x1f]')


# --------------------------------------------------------------------------
# Pure functions (unit-testable, no I/O)
# --------------------------------------------------------------------------


def parse_local(value) -> "datetime | None":
    """Parse a cached `YYYY-MM-DDTHH:MM:SS` stamp as naive local, or None.

    These are values `calendar-sync` wrote, already converted to local wall
    clock. Anything unparseable is treated as absent rather than raising: one
    malformed event must not stop the whole sync.
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


def parse_graph_utc(value) -> "datetime | None":
    """Parse a Graph timestamp and convert it to naive **local** wall clock.

    Transcript `createdDateTime` carries a trailing `Z`. The occurrences it is
    compared against are local, so converting here is what keeps the comparison
    honest — the same class of bug that made every synced event seven hours
    wrong until 2026-09-04.
    """
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip()
    marker = re.search(r"(Z|[+-]\d{2}:?\d{2})$", text)
    body = text[: marker.start()] if marker else text
    body = body.replace("T", " ")
    if "." in body:
        body = body.split(".", 1)[0]
    parsed = None
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"):
        try:
            parsed = datetime.strptime(body, fmt)
            break
        except ValueError:
            continue
    if parsed is None:
        return None
    if marker and marker.group(0) not in ("Z", "z"):
        token = marker.group(0)
        digits = token[1:].replace(":", "")
        delta = timedelta(hours=int(digits[:2]), minutes=int(digits[2:4]))
        tzinfo = timezone(delta if token[0] == "+" else -delta)
    else:
        # No marker on a Graph timestamp still means UTC.
        tzinfo = timezone.utc
    return parsed.replace(tzinfo=tzinfo).astimezone().replace(tzinfo=None)


def distance_to_window(moment, start, end) -> float:
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

    Returns ``(occurrence, reason)``; `occurrence` is None when we decline.

    Verified in `spikes/transcript-occurrence-matching/`: this is that probe's
    function, with the ambiguity guard its findings asked for. Declining is the
    designed outcome, not a failure — a transcript attached to the wrong
    customer call is a quiet, hard-to-notice error, while an unfiled one is
    visible and recoverable.
    """
    if moment is None:
        return None, "transcript has no usable timestamp"
    scored = []
    for occ in occurrences:
        if occ.get("start") is None or occ.get("end") is None:
            continue
        scored.append((distance_to_window(moment, occ["start"], occ["end"]), occ))
    if not scored:
        return None, "no occurrence has a usable start/end"

    scored.sort(key=lambda pair: (pair[0], pair[1]["start"]))
    best_distance, best = scored[0]
    if best_distance > tolerance.total_seconds():
        return None, (
            f"nearest occurrence is {best_distance / 3600:.1f}h away, beyond the "
            f"{tolerance.total_seconds() / 3600:.0f}h tolerance"
        )
    if len(scored) > 1:
        margin = scored[1][0] - best_distance
        if margin < AMBIGUOUS_MARGIN.total_seconds():
            return None, (
                f"ambiguous: two occurrences within {margin / 60:.0f} minutes of "
                "each other by this timestamp"
            )
    return best, ""


def sanitize_segment(text: str) -> str:
    """Make a title safe as a filename segment (same rules as calendar-sync)."""
    cleaned = _UNSAFE_PATH_CHARS.sub(" ", text or "")
    cleaned = re.sub(r"\s+", " ", cleaned).strip()
    return cleaned or "Untitled"


def subject_from_event_path(path: str) -> str:
    """Recover the meeting subject from a `calendar-sync` note path."""
    stem = (path or "").rsplit("/", 1)[-1]
    if stem.endswith(".md"):
        stem = stem[:-3]
    match = re.match(r"^\d{4}-\d{2}-\d{2} \d{4} (?P<subject>.+)$", stem)
    return match.group("subject") if match else stem


def transcript_note_path(occurrence: dict) -> str:
    """`Meetings/Transcripts/YYYY-MM-DD HHMM - <subject> (transcript).md`.

    Derived from the occurrence, not the transcript, so a re-run of the same
    meeting lands on the same path.

    Plan §E sketched this without the time. Real data disagreed: a recurring
    standup and a same-day repeat of the same subject both collapse onto one
    path, and the loser of that collision is dropped on a 409. `HHMM` matches
    the shape `calendar-sync` already uses for event notes.
    """
    start = occurrence["start"]
    subject = sanitize_segment(subject_from_event_path(occurrence.get("path", "")))
    return (
        f"Meetings/Transcripts/{start:%Y-%m-%d %H%M} - {subject} (transcript).md"
    )


def suffixed_path(path: str, n: int) -> str:
    """`... (transcript).md` -> `... (transcript 2).md`."""
    base = path[:-3] if path.endswith(".md") else path
    if base.endswith(" (transcript)"):
        return f"{base[:-1]} {n}).md"
    return f"{base} ({n}).md"


def source_url_for(transcript_id: str) -> str:
    """The dedup key. Stable per transcript, so a re-run is a no-op."""
    return f"teams:{transcript_id}"


def transcript_frontmatter(occurrence: dict, transcript: dict, meeting_path) -> dict:
    """Vault-model frontmatter for a Transcript Note (plan §E, ADR 0025).

    Core renders the body; the vault's model lives here, which is the split
    ADR 0025 Decision 5 asks for.
    """
    start = occurrence["start"]
    frontmatter = {
        "kind": "transcript",
        "source_type": "teams",
        "source_url": source_url_for(transcript["id"]),
        "event_id": occurrence.get("event_id") or "",
        "event": f"[[{link_target(occurrence.get('path'))}]]",
        "date": f"{start:%Y-%m-%d}",
        "customers": list(occurrence.get("customers") or []),
        "tags": ["transcript"],
    }
    if meeting_path:
        frontmatter["meeting"] = f"[[{link_target(meeting_path)}]]"
    return frontmatter


def link_target(path) -> str:
    """A note path's wikilink target — its basename without `.md`."""
    stem = (path or "").rsplit("/", 1)[-1]
    return stem[:-3] if stem.endswith(".md") else stem


def missing_links(transcripts, meetings):
    """Frontmatter patches needed to complete meeting<->transcript pairs.

    Returns ``[(note_path, {field: value}), ...]``.

    The create-time link only fires when a meeting note *already exists* as the
    transcript lands. That covers the common flow -- prefill writes the meeting
    note during the call, the transcript appears an hour later -- but not the
    late one: write the meeting up the next day and the transcript has already
    been ingested, so it is skipped on every subsequent run and the link never
    appears. Reconciling on each run closes that, and is idempotent because a
    pair that is already linked produces no patch.
    """
    by_event = {}
    for meeting in sorted(meetings, key=lambda m: m.get("path") or ""):
        event_id = (meeting.get("event_id") or "").strip()
        if event_id:
            by_event.setdefault(event_id, meeting)

    patches = []
    for transcript in sorted(transcripts, key=lambda t: t.get("path") or ""):
        event_id = (transcript.get("event_id") or "").strip()
        if not event_id:
            continue
        meeting = by_event.get(event_id)
        if not meeting:
            continue
        if not (transcript.get("meeting") or "").strip():
            patches.append(
                (transcript["path"], {"meeting": f"[[{link_target(meeting['path'])}]]"})
            )
        if not (meeting.get("transcript") or "").strip():
            patches.append(
                (meeting["path"], {"transcript": f"[[{link_target(transcript['path'])}]]"})
            )
    return patches


def group_by_join_url(occurrences) -> dict:
    """Occurrences keyed by join URL — that grouping is the Teams series."""
    series = {}
    for occ in occurrences:
        join = (occ.get("join_url") or "").strip()
        if join:
            series.setdefault(join, []).append(occ)
    for occs in series.values():
        occs.sort(key=lambda o: o["start"] or datetime.min)
    return series


def transcript_in_window(transcript, now, lookback_days: int) -> bool:
    """Could this transcript possibly match a cached occurrence?

    Occurrences are filtered to the lookback window, so a transcript created
    before it has nothing to match and never will. Without this the same
    permanently-unmatchable transcripts are re-reported on every run -- noise
    that teaches you to stop reading the output, which is precisely when a real
    unmatched transcript slips past.

    A transcript with no timestamp is kept, so `match_transcript` reports it
    once rather than it being dropped here without explanation.
    """
    created = transcript.get("created")
    if created is None:
        return True
    return created >= now - timedelta(days=lookback_days)


def eligible(occurrence, now, lookback_days: int) -> bool:
    """Has this occurrence ended, and is it recent enough to still have one?

    The observed retention floor was 17 days, so a lookback beyond that fetches
    nothing but costs requests.
    """
    end = occurrence.get("end") or occurrence.get("start")
    start = occurrence.get("start")
    if end is None or start is None:
        return False
    return end <= now and start >= now - timedelta(days=lookback_days)


def _relative_graph_url(next_link: str) -> str:
    return _GRAPH_PREFIX.sub("", (next_link or "").strip(), count=1)


# --------------------------------------------------------------------------
# I/O (network / subprocess / REST)
# --------------------------------------------------------------------------


def _api_base() -> str:
    return os.environ.get("NOTESMITH_API_BASE", "http://127.0.0.1:27183").rstrip("/")


def _vault() -> str:
    vault = os.environ.get("NOTESMITH_VAULT")
    if not vault:
        raise SystemExit("NOTESMITH_VAULT is not set")
    return vault


def _http_json(method: str, url: str, payload=None) -> dict:
    data = None
    headers = {"Accept": "application/json"}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(request, timeout=60) as response:
        body = response.read()
    return json.loads(body) if body else {}


def query_sql(sql: str):
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/query/sql"
    result = _http_json("POST", url, {"sql": sql})
    columns = result.get("columns", [])
    return [dict(zip(columns, row)) for row in result.get("rows", [])]


# The occurrence query, kept a module constant so the Rust golden-vault test can
# assert it stays valid against the real index schema.
OCCURRENCE_SQL = (
    "SELECT n.path AS path, "
    "MAX(CASE WHEN f.key = 'event_id' THEN f.value END) AS event_id, "
    "MAX(CASE WHEN f.key = 'start' THEN f.value END) AS start, "
    "MAX(CASE WHEN f.key = 'end' THEN f.value END) AS end, "
    "MAX(CASE WHEN f.key = 'join_url' THEN f.value END) AS join_url "
    "FROM v_notes n "
    "JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path "
    "WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event') "
    "GROUP BY n.path"
)

CUSTOMERS_SQL = (
    "SELECT note_path AS path, value AS value FROM v_field_values "
    "WHERE key = 'customers' AND note_path IN "
    "(SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event') "
    "ORDER BY note_path, ordinal"
)

MEETING_BY_EVENT_SQL = (
    "SELECT note_path AS path FROM v_field_values "
    "WHERE key = 'event_id' AND note_path IN "
    "(SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'meeting') "
    "AND value = '{event_id}'"
)

TRANSCRIPT_LINKS_SQL = (
    "SELECT n.path AS path, "
    "MAX(CASE WHEN f.key = 'event_id' THEN f.value END) AS event_id, "
    "MAX(CASE WHEN f.key = 'meeting' THEN f.value END) AS meeting "
    "FROM v_notes n "
    "JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path "
    "WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'transcript') "
    "GROUP BY n.path"
)

MEETING_LINKS_SQL = (
    "SELECT n.path AS path, "
    "MAX(CASE WHEN f.key = 'event_id' THEN f.value END) AS event_id, "
    "MAX(CASE WHEN f.key = 'transcript' THEN f.value END) AS transcript "
    "FROM v_notes n "
    "JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path "
    "WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'meeting') "
    "GROUP BY n.path"
)

EXISTING_TRANSCRIPT_SQL = (
    "SELECT note_path AS path FROM v_field_values "
    "WHERE key = 'source_url' AND value = '{source_url}'"
)


def load_occurrences():
    """Synced event notes carrying a join URL, with customers attached."""
    customers = {}
    for row in query_sql(CUSTOMERS_SQL):
        customers.setdefault(row.get("path"), []).append(row.get("value"))

    out = []
    for row in query_sql(OCCURRENCE_SQL):
        join = (row.get("join_url") or "").strip()
        if not join:
            continue
        out.append(
            {
                "path": row.get("path"),
                "event_id": row.get("event_id"),
                "start": parse_local(row.get("start")),
                "end": parse_local(row.get("end")),
                "join_url": join,
                "customers": customers.get(row.get("path"), []),
            }
        )
    return out


def find_meeting_note(event_id: str):
    if not event_id:
        return None
    rows = query_sql(MEETING_BY_EVENT_SQL.format(event_id=event_id.replace("'", "''")))
    return rows[0].get("path") if rows else None


def transcript_already_ingested(source_url: str) -> bool:
    safe = source_url.replace("'", "''")
    return bool(query_sql(EXISTING_TRANSCRIPT_SQL.format(source_url=safe)))


def workiq_fetch(entity_url: str):
    """`workiq fetch -u <url>` → parsed JSON.

    Transcript *content* comes back as a JSON string containing WebVTT rather
    than a Graph object, so callers must expect either shape.
    """
    try:
        proc = subprocess.run(
            ["workiq", "fetch", "-u", entity_url], capture_output=True, text=True
        )
    except FileNotFoundError:
        raise RuntimeError("the Work IQ CLI (`workiq`) is not on PATH") from None
    if proc.returncode != 0:
        raise RuntimeError(
            f"workiq fetch failed ({proc.returncode}) for {entity_url}: "
            f"{proc.stderr.strip()}"
        )
    if not proc.stdout.strip():
        # Observed during the occurrence probe: some lookups exit 0 with an
        # empty body. That is a failed lookup, not an empty result.
        raise RuntimeError(f"workiq returned an empty body for {entity_url}")
    return json.loads(proc.stdout)


def fetch_all_pages(entity_url: str) -> list:
    items = []
    url = entity_url
    for _ in range(MAX_PAGES):
        payload = workiq_fetch(url)
        items.extend(payload.get("value") or [])
        next_link = payload.get("@odata.nextLink")
        if not next_link:
            return items
        url = _relative_graph_url(next_link)
    return items


def resolve_online_meeting(join: str):
    quoted = join.replace("'", "''")
    url = "/me/onlineMeetings?$filter=" + urllib.parse.quote(
        f"joinWebUrl eq '{quoted}'", safe="$= '"
    )
    values = workiq_fetch(url).get("value") or []
    return values[0].get("id") if values else None


def list_transcripts(meeting_id: str) -> list:
    raw = fetch_all_pages(f"/me/onlineMeetings/{meeting_id}/transcripts")
    return [
        {"id": item.get("id"), "created": parse_graph_utc(item.get("createdDateTime"))}
        for item in raw
        if item.get("id")
    ]


def fetch_vtt(meeting_id: str, transcript_id: str) -> str:
    """The transcript's WebVTT, unwrapped from the CLI's JSON string."""
    url = (
        f"/me/onlineMeetings/{meeting_id}/transcripts/{transcript_id}"
        "/content?$format=text/vtt"
    )
    payload = workiq_fetch(url)
    if isinstance(payload, str):
        return payload
    # Defensive: if the CLI ever returns a Graph object instead.
    for key in ("value", "content"):
        if isinstance(payload.get(key), str):
            return payload[key]
    raise RuntimeError("unexpected transcript content shape from workiq")


def render_body(vtt: str, title: str, source_url: str) -> str:
    """Hand the VTT to core and take back the rendered body.

    Piped on stdin so transcript text never touches disk, and rendered by
    `notesmith transcribe` so the `[M:SS] Name: text` format stays owned in one
    place (ADR 0025's one Transcript Note concept).
    """
    proc = subprocess.run(
        [
            "notesmith",
            "--format",
            "json",
            "transcribe",
            "--from-vtt",
            "-",
            "--title",
            title,
            "--source",
            source_url,
            "--source-type",
            "teams",
        ],
        input=vtt,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"notesmith transcribe failed: {proc.stderr.strip()}")
    payload = json.loads(proc.stdout)
    body = payload.get("body")
    if not body:
        raise RuntimeError("notesmith transcribe returned an empty body")
    return body


def create_note_at_free_path(path: str, frontmatter: dict, body: str, attempts: int = 5) -> str:
    """Create the note, stepping to `(transcript 2)`, `(3)`… if the path is taken.

    One meeting can produce two transcripts -- a recording stopped and
    restarted yields two records with different ids, which both clear the
    `source_url` dedup and then derive the same path from the same occurrence.
    Letting the second die on a 409 loses a real transcript silently, so take
    the next free name instead.
    """
    for attempt in range(1, attempts + 1):
        candidate = path if attempt == 1 else suffixed_path(path, attempt)
        try:
            create_note(candidate, frontmatter, body)
            return candidate
        except urllib.error.HTTPError as error:
            if error.code != 409:
                raise
    raise RuntimeError(f"no free path for {path} after {attempts} attempts")


def create_note(path: str, frontmatter: dict, body: str) -> None:
    folder, _, filename = path.rpartition("/")
    title = filename[:-3] if filename.endswith(".md") else filename
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/notes"
    _http_json(
        "POST",
        url,
        {"title": title, "folder": folder, "content": body, "frontmatter": frontmatter},
    )


def patch_frontmatter(path: str, fields: dict) -> None:
    """Merge frontmatter fields into a note.

    A PATCH, deliberately: meeting notes are human-owned and ship no managed
    section, so the body is never touched. Frontmatter wikilinks are indexed as
    real links, which is how the rest of this vault models relationships.
    """
    encoded = "/".join(urllib.parse.quote(part) for part in path.split("/"))
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/notes/{encoded}"
    _http_json("PATCH", url, {"frontmatter": fields})


def link_meeting_to_transcript(meeting_path: str, transcript_path: str) -> None:
    """Point a meeting note at its freshly created transcript."""
    patch_frontmatter(
        meeting_path, {"transcript": f"[[{link_target(transcript_path)}]]"}
    )


def reconcile_backlinks() -> int:
    """Complete any meeting<->transcript pair that is linked on neither side.

    Runs every time, independently of whether anything new was ingested: the
    pair may only have become completable because a human wrote the meeting
    note today for a call whose transcript synced last week.
    """
    patches = missing_links(query_sql(TRANSCRIPT_LINKS_SQL), query_sql(MEETING_LINKS_SQL))
    linked = 0
    for path, fields in patches:
        try:
            patch_frontmatter(path, fields)
            linked += 1
        except (urllib.error.URLError, OSError, ValueError) as error:
            print(f"transcript-sync: could not link {path}: {error}", file=sys.stderr)
    return linked


# --------------------------------------------------------------------------
# Sync
# --------------------------------------------------------------------------


def load_config() -> dict:
    path = os.path.join(".notesmith", "connectors", "transcript-sync.config.json")
    config = {"lookback_days": 3}
    try:
        with open(path, "r", encoding="utf-8") as handle:
            config.update(json.load(handle))
    except FileNotFoundError:
        pass
    except (ValueError, OSError) as error:
        print(f"transcript-sync: ignoring unreadable config: {error}", file=sys.stderr)
    return config


def run_sync() -> int:
    config = load_config()
    lookback_days = int(config.get("lookback_days", 3))
    now = datetime.now()

    occurrences = load_occurrences()
    candidates = [o for o in occurrences if eligible(o, now, lookback_days)]
    if not candidates:
        # Still reconcile: a meeting note written today can complete a pair
        # whose transcript arrived days ago.
        linked = reconcile_backlinks()
        print(
            "transcript-sync: no recently-ended online meetings to check, "
            f"{linked} link(s) completed"
        )
        return 0

    # A series is fetched once even when several of its occurrences qualify.
    wanted = {o["join_url"] for o in candidates}
    series = {
        join: occs for join, occs in group_by_join_url(occurrences).items()
        if join in wanted
    }

    created = 0
    skipped = 0
    unmatched = 0
    failed_series = 0

    for join, occs in series.items():
        # One bad series must not abort the rest (observed in the probe: some
        # lookups exit 0 with an empty body while others succeed).
        try:
            meeting_id = resolve_online_meeting(join)
            if not meeting_id:
                continue
            transcripts = list_transcripts(meeting_id)
        except (RuntimeError, ValueError, urllib.error.URLError) as error:
            failed_series += 1
            print(f"transcript-sync: series lookup failed: {error}", file=sys.stderr)
            continue

        for transcript in transcripts:
            if not transcript_in_window(transcript, now, lookback_days):
                continue
            source_url = source_url_for(transcript["id"])
            if transcript_already_ingested(source_url):
                skipped += 1
                continue

            occurrence, reason = match_transcript(transcript["created"], occs)
            if occurrence is None:
                unmatched += 1
                print(
                    f"transcript-sync: leaving a transcript unfiled ({reason})",
                    file=sys.stderr,
                )
                continue
            if not eligible(occurrence, now, lookback_days):
                skipped += 1
                continue

            try:
                vtt = fetch_vtt(meeting_id, transcript["id"])
                path = transcript_note_path(occurrence)
                body = render_body(vtt, link_target(path), source_url)
                meeting_path = find_meeting_note(occurrence.get("event_id"))
                frontmatter = transcript_frontmatter(occurrence, transcript, meeting_path)
                # The written path may be suffixed, so link against what landed.
                path = create_note_at_free_path(path, frontmatter, body)
                created += 1
                if meeting_path:
                    link_meeting_to_transcript(meeting_path, path)
            except (RuntimeError, ValueError, OSError, urllib.error.URLError) as error:
                failed_series += 1
                print(f"transcript-sync: transcript failed: {error}", file=sys.stderr)

    linked = reconcile_backlinks()
    print(
        f"transcript-sync: {created} created, {skipped} already present, "
        f"{unmatched} left unfiled, {linked} link(s) completed, "
        f"{failed_series} failed"
    )
    return 0


# --------------------------------------------------------------------------
# Self-test (no network) -- what the Rust test invokes
# --------------------------------------------------------------------------


def _occ(day, hour, minute=0, length=30, subject="Acme sync", join="join-a"):
    start = datetime(2026, 9, day, hour, minute)
    return {
        "path": f"Calendar/2026/09/2026-09-{day:02d} {hour:02d}{minute:02d} {subject}.md",
        "event_id": f"evt-{day:02d}-{hour:02d}{minute:02d}",
        "start": start,
        "end": start + timedelta(minutes=length),
        "join_url": join,
        "customers": ["[[Acme Corp]]"],
    }


def self_test() -> int:
    weekly = [_occ(2, 9), _occ(9, 9), _occ(16, 9)]

    # The verified case: a transcript finalized minutes after an occurrence.
    best, reason = match_transcript(datetime(2026, 9, 9, 9, 34), weekly)
    assert best["event_id"] == "evt-09-0900", (best, reason)

    # Created during the meeting still matches it.
    assert match_transcript(datetime(2026, 9, 9, 9, 15), weekly)[0]["event_id"] == "evt-09-0900"

    # Outside tolerance: decline rather than misfile.
    best, reason = match_transcript(datetime(2026, 9, 12, 12, 0), weekly)
    assert best is None and "beyond the" in reason, reason

    # The guard the probe's findings asked for: two occurrences too close to
    # separate confidently must not be guessed between.
    twice = [_occ(2, 9, length=15), _occ(2, 11, length=15)]
    best, reason = match_transcript(datetime(2026, 9, 2, 10, 0), twice)
    assert best is None, best
    assert "ambiguous" in reason, reason

    # ...but a clear winner among close occurrences is still taken.
    best, _ = match_transcript(datetime(2026, 9, 2, 9, 20), twice)
    assert best["event_id"] == "evt-02-0900", best

    assert match_transcript(None, weekly)[0] is None
    assert match_transcript(datetime(2026, 9, 9, 9, 0), [])[0] is None

    # Timestamps: Graph sends UTC, occurrences are local. Comparing them
    # unconverted is the bug that made every synced event seven hours wrong.
    previous = os.environ.get("TZ")
    os.environ["TZ"] = "America/Los_Angeles"
    try:
        import time as _time

        _time.tzset()
        assert parse_graph_utc("2026-09-09T16:00:00Z") == datetime(2026, 9, 9, 9, 0)
        assert parse_graph_utc("2026-09-09T18:00:00+02:00") == datetime(2026, 9, 9, 9, 0)
        # No marker on a Graph stamp still means UTC.
        assert parse_graph_utc("2026-09-09T16:00:00") == datetime(2026, 9, 9, 9, 0)
        assert parse_graph_utc("") is None
        assert parse_graph_utc("nonsense") is None
    finally:
        if previous is None:
            os.environ.pop("TZ", None)
        else:
            os.environ["TZ"] = previous
        import time as _time

        _time.tzset()

    # Cached occurrence stamps are already local; parsed as-is.
    assert parse_local("2026-09-09T09:00:00") == datetime(2026, 9, 9, 9, 0)
    assert parse_local("") is None
    assert parse_local(None) is None

    # Note identity: derived from the occurrence, so a re-run is stable.
    occ = _occ(9, 9)
    assert (
        transcript_note_path(occ)
        == "Meetings/Transcripts/2026-09-09 0900 - Acme sync (transcript).md"
    ), transcript_note_path(occ)
    # `calendar-sync` sanitizes the subject before building its path, so the
    # subject recovered here is already filename-safe and carries through.
    assert transcript_note_path(_occ(9, 9, subject="Acme Q3 sync roadmap")) == (
        "Meetings/Transcripts/2026-09-09 0900 - Acme Q3 sync roadmap (transcript).md"
    )
    # The guard is still there for a path that somehow is not.
    assert sanitize_segment('a/b:c*d?"e') == "a b c d e"
    assert sanitize_segment("   ") == "Untitled"
    # A path that does not match the connector's shape degrades to its stem.
    assert subject_from_event_path("Calendar/Odd name.md") == "Odd name"
    assert source_url_for("T1") == "teams:T1"

    # Frontmatter: the vault model, with the joins that make it findable.
    fm = transcript_frontmatter(occ, {"id": "T1"}, "Meetings/2026/09/2026-09-09 - Acme - Sync.md")
    assert fm == {
        "kind": "transcript",
        "source_type": "teams",
        "source_url": "teams:T1",
        "event_id": "evt-09-0900",
        "event": "[[2026-09-09 0900 Acme sync]]",
        "date": "2026-09-09",
        "customers": ["[[Acme Corp]]"],
        "tags": ["transcript"],
        "meeting": "[[2026-09-09 - Acme - Sync]]",
    }, fm

    # No meeting note yet: the transcript still lands, just without the backlink.
    assert "meeting" not in transcript_frontmatter(occ, {"id": "T1"}, None)

    # Eligibility: ended, and recent enough to still have a transcript.
    now = datetime(2026, 9, 9, 12, 0)
    assert eligible(_occ(9, 9), now, 3) is True
    assert eligible(_occ(9, 14), now, 3) is False, "has not ended yet"
    assert eligible(_occ(2, 9), now, 3) is False, "older than the lookback"
    assert eligible({"start": None, "end": None}, now, 3) is False

    # Series grouping is by join URL, which is what Teams shares across a series.
    grouped = group_by_join_url([_occ(9, 9), _occ(2, 9), _occ(9, 14, join="join-b")])
    assert set(grouped) == {"join-a", "join-b"}
    assert [o["event_id"] for o in grouped["join-a"]] == ["evt-02-0900", "evt-09-0900"]

    # Path identity carries the time: a same-day repeat of a subject, or a
    # recurring standup, must not collapse onto one path.
    assert transcript_note_path(_occ(9, 9)) == (
        "Meetings/Transcripts/2026-09-09 0900 - Acme sync (transcript).md"
    ), transcript_note_path(_occ(9, 9))
    assert transcript_note_path(_occ(9, 9)) != transcript_note_path(_occ(9, 14))

    # And a genuine collision (two transcripts, one occurrence) steps aside.
    assert suffixed_path(
        "Meetings/Transcripts/2026-09-09 0900 - Acme sync (transcript).md", 2
    ) == "Meetings/Transcripts/2026-09-09 0900 - Acme sync (transcript 2).md"
    assert suffixed_path("A/B.md", 3) == "A/B (3).md"

    # Transcripts older than the lookback can never match a cached occurrence,
    # so they are dropped rather than re-reported as unfiled on every run.
    now = datetime(2026, 9, 9, 12, 0)
    assert transcript_in_window({"created": datetime(2026, 9, 9, 9, 0)}, now, 3) is True
    assert transcript_in_window({"created": datetime(2026, 8, 1, 9, 0)}, now, 3) is False
    assert transcript_in_window({"created": None}, now, 3) is True, "reported once, not dropped"

    # Back-link reconciliation: the late-write case the create-time link misses.
    t_path = "Meetings/Transcripts/2026-09-09 - Acme sync (transcript).md"
    m_path = "Meetings/2026/09/2026-09-09 - Acme - Sync.md"
    unlinked_t = [{"path": t_path, "event_id": "evt-1", "meeting": None}]
    unlinked_m = [{"path": m_path, "event_id": "evt-1", "transcript": None}]
    assert missing_links(unlinked_t, unlinked_m) == [
        (t_path, {"meeting": "[[2026-09-09 - Acme - Sync]]"}),
        (m_path, {"transcript": "[[2026-09-09 - Acme sync (transcript)]]"}),
    ], missing_links(unlinked_t, unlinked_m)

    # Idempotent: an already-linked pair needs no patch.
    linked_t = [{"path": t_path, "event_id": "evt-1", "meeting": "[[x]]"}]
    linked_m = [{"path": m_path, "event_id": "evt-1", "transcript": "[[y]]"}]
    assert missing_links(linked_t, linked_m) == []

    # Half-linked: only the missing side is patched.
    assert missing_links(linked_t, unlinked_m) == [
        (m_path, {"transcript": "[[2026-09-09 - Acme sync (transcript)]]"})
    ]

    # No counterpart, or no event_id: nothing to do, and no crash.
    assert missing_links(unlinked_t, []) == []
    assert missing_links([{"path": t_path, "event_id": "", "meeting": None}], unlinked_m) == []
    assert missing_links([], []) == []

    # A transcript whose meeting is a different event is not cross-linked.
    other_m = [{"path": m_path, "event_id": "evt-2", "transcript": None}]
    assert missing_links(unlinked_t, other_m) == []

    assert (
        _relative_graph_url("https://graph.microsoft.com/v1.0/me/x?$skiptoken=a")
        == "/me/x?$skiptoken=a"
    )
    assert link_target("Meetings/2026/09/A.md") == "A"

    print("OK")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Sync Teams transcripts into the vault")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    try:
        return run_sync()
    except (RuntimeError, urllib.error.URLError) as error:
        print(f"transcript-sync: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
