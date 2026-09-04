#!/usr/bin/env python3
"""email-summary connector (ADR 0025, Decision 3 + the 2026-09-04 amendment).

The *fallback tier* for the daily briefing's email section. A deterministic,
LLM-free connector that fills the daily note's `briefing/email` managed section
with a sender+subject digest of unread inbox mail, for machines whose briefing
agent has no Work IQ tool to read email live and compose the judgment-tier
summary. It is a *connector*, not core code: the daemon's generic `[[jobs]]`
runner invokes it (see `.notesmith/vault.toml`), and it writes through the REST
API. Corp credentials never touch Notesmith -- `workiq` uses its own auth cache.

**Coexistence.** `briefing/email` may be written by the briefing agent OR this
connector, and the connector must NEVER overwrite a real agent summary. It runs
last (its job declares `after = ["daily-briefing"]`), reads the current section
interior, and fills it ONLY when the interior is empty/whitespace or still
carries the agent's "no email tool" fallback (a loose match on
`Work IQ not connected`). A real summary is left untouched.

**Hard boundary (ADR 0025 Decision 4 + 2026-09-04 amendment).** Only sender and
subject metadata may persist. The Graph `$select` is limited to
`id,subject,from,receivedDateTime,isRead` -- `body`, `bodyPreview`, `uniqueBody`,
headers, and attachments are never requested and never stored. The `--self-test`
proves this: it renders a fixture whose messages carry body content and asserts
none of it appears in the output.

The module is structured as pure functions plus a thin `main` so the rendering
can be unit-tested with no network. Run `--self-test` to exercise the pure logic.

Stdlib only (argparse, json, os, re, subprocess, sys, urllib, datetime). No
third-party dependencies.
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

# The managed section this connector owns (docs/managed-sections.md). Kept as a
# module constant so the marker lines are built one way everywhere.
SECTION_ID = "briefing/email"

# The Graph `$select` -- the hard boundary made literal. ONLY these fields are
# ever requested, so no message body/preview/header can enter Notesmith. Do not
# add `body`, `bodyPreview`, `uniqueBody`, or attachment fields here (ADR 0025).
UNREAD_SELECT = "id,subject,from,receivedDateTime,isRead"


# --------------------------------------------------------------------------
# Pure functions (unit-testable, no I/O)
# --------------------------------------------------------------------------


def sender_label(msg: dict) -> str:
    """Display name of a message's sender, falling back to the address.

    Graph nests the sender under ``from.emailAddress`` with ``name`` and
    ``address``. Prefer the human-readable name; use the address when there is
    no name; ``Unknown sender`` when neither is present.
    """
    email = ((msg.get("from") or {}).get("emailAddress")) or {}
    name = (email.get("name") or "").strip()
    if name:
        return name
    address = (email.get("address") or "").strip()
    return address or "Unknown sender"


def one_line_subject(subject) -> str:
    """Collapse a subject to a single trimmed line (``(no subject)`` if empty)."""
    cleaned = re.sub(r"\s+", " ", subject or "").strip()
    return cleaned or "(no subject)"


def received_hhmm(value) -> str:
    """Local ``HH:MM`` from a Graph ``receivedDateTime``.

    Graph returns UTC values like ``2026-09-03T15:04:00Z``. This previously
    rendered the raw wall-clock components and dropped the zone, on the
    reasoning that converting needed a timezone database -- it does not:
    ``astimezone()`` is stdlib and uses the system zone. The result was that a
    digest read by someone in PDT showed every message seven hours late.
    Verified against real synced data on 2026-09-04.

    Unparseable values render ``--:--`` rather than raising, so one odd message
    never fails the whole digest.
    """
    text = (value or "").strip()
    marker = re.search(r"(Z|[+-]\d{2}:?\d{2})$", text)
    body = text[: marker.start()] if marker else text
    if "." in body:
        body = body.split(".", 1)[0]
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"):
        try:
            parsed = datetime.strptime(body, fmt)
        except ValueError:
            continue
        if marker:
            token = marker.group(0)
            if token in ("Z", "z"):
                tzinfo = timezone.utc
            else:
                digits = token[1:].replace(":", "")
                delta = timedelta(hours=int(digits[:2]), minutes=int(digits[2:4]))
                tzinfo = timezone(delta if token[0] == "+" else -delta)
        else:
            # No marker: Graph's mail surfaces are UTC.
            tzinfo = timezone.utc
        return parsed.replace(tzinfo=tzinfo).astimezone().strftime("%H:%M")
    return "--:--"


def message_bullet(msg: dict) -> str:
    """One digest bullet: ``- HH:MM **Sender Name** — Subject``.

    Sender and subject only -- never any body content (the hard boundary).
    """
    when = received_hhmm(msg.get("receivedDateTime", ""))
    return f"- {when} **{sender_label(msg)}** — {one_line_subject(msg.get('subject'))}"


def render_email_section(messages, cap: int) -> str:
    """Render the ``briefing/email`` interior for a list of Graph messages.

    Most recent first (by ``receivedDateTime``), capped at ``cap`` bullets with
    a terse leading count line. The empty case is ``Nothing unread.`` so the
    briefing always reads cleanly.
    """
    if not messages:
        return "Nothing unread."

    ordered = sorted(
        messages,
        key=lambda m: m.get("receivedDateTime", ""),
        reverse=True,
    )
    shown = ordered[: max(cap, 0)]

    lines = [f"{len(ordered)} unread:"]
    lines.extend(message_bullet(msg) for msg in shown)
    if len(ordered) > len(shown):
        lines.append(f"- … and {len(ordered) - len(shown)} more")
    return "\n".join(lines)


def unread_query_url(config: dict) -> str:
    """Build the Graph inbox query URL from config.

    ``max_messages`` caps ``$top``; ``unread_only`` (default true) adds the
    ``isRead eq false`` filter. The ``$select`` is the fixed metadata-only set
    -- config can widen the window or the cap, never the fields (the boundary
    is not user-tunable).
    """
    cap = int(config.get("max_messages", 25))
    params = []
    if bool(config.get("unread_only", True)):
        params.append(("$filter", "isRead eq false"))
    params.extend(
        [
            ("$select", UNREAD_SELECT),
            ("$top", str(cap)),
            ("$orderby", "receivedDateTime desc"),
        ]
    )
    return "/me/mailFolders/inbox/messages?" + urllib.parse.urlencode(params, safe="$,")


# The agent's "no email tool" fallback line, normalized to its significant
# words. `should_fill` matches this loosely inside the section interior so the
# connector recognizes it regardless of surrounding punctuation (the prompt
# writes "Email summary unavailable (Work IQ not connected).").
_AGENT_FALLBACK_MARKER = "work iq not connected"


def _normalize(text: str) -> str:
    """Lowercase and collapse any non-alphanumeric run to a single space."""
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()


def extract_section_interior(content: str, section_id: str):
    """Return the interior of a managed section, or ``None`` if the pair is absent.

    The interior is the text *between* the begin/end marker lines (each on its
    own line). A missing or malformed pair yields ``None`` -- the caller then
    treats the section as fillable (the write appends the block).
    """
    begin = f"<!-- notesmith:section:begin {section_id} -->"
    end = f"<!-- notesmith:section:end {section_id} -->"
    begin_idx = end_idx = None
    lines = content.splitlines()
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped == begin:
            begin_idx = i
        elif stripped == end:
            end_idx = i
    if begin_idx is None or end_idx is None or end_idx <= begin_idx:
        return None
    return "\n".join(lines[begin_idx + 1 : end_idx])


def should_fill(interior) -> bool:
    """Whether the connector may fill ``briefing/email``.

    Fill ONLY when the section is empty/whitespace, its markers are absent
    (``interior is None``), or it still carries the agent's "Work IQ not
    connected" fallback. Any other content is a real agent summary -- leave it.
    """
    if interior is None:
        return True
    if not interior.strip():
        return True
    return _AGENT_FALLBACK_MARKER in _normalize(interior)


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


def _daily_url(date: str) -> str:
    return f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/daily/{date}"


def ensure_daily_note(date: str) -> dict:
    """POST /daily/{date} -- create today's note if missing (idempotent)."""
    return _http_json("POST", _daily_url(date))


def get_daily_note(date: str) -> dict:
    """GET /daily/{date} -- fetch path, content, and frontmatter."""
    return _http_json("GET", _daily_url(date))


def write_section(path: str, section_id: str, content: str) -> dict:
    """Replace the managed section's interior via POST /notes-section/{path}."""
    encoded = "/".join(urllib.parse.quote(part) for part in path.split("/"))
    url = f"{_api_base()}/api/v/{urllib.parse.quote(_vault())}/notes-section/{encoded}"
    return _http_json(
        "POST",
        url,
        {
            "section_id": section_id,
            "content": content,
            "append_if_missing": True,
        },
    )


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


# --------------------------------------------------------------------------
# Config
# --------------------------------------------------------------------------

_DEFAULT_CONFIG = {"unread_only": True, "max_messages": 25}


def load_config() -> dict:
    """Read the sibling `email-summary.config.json`, falling back to defaults."""
    config = dict(_DEFAULT_CONFIG)
    path = os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "email-summary.config.json"
    )
    try:
        with open(path, "r", encoding="utf-8") as handle:
            config.update(json.load(handle))
    except FileNotFoundError:
        pass
    return config


# --------------------------------------------------------------------------
# Runtime entry point
# --------------------------------------------------------------------------


def run_summary() -> int:
    """Fill briefing/email when the agent left it unavailable. Returns exit code."""
    config = load_config()
    cap = int(config.get("max_messages", 25))
    date = datetime.now().strftime("%Y-%m-%d")

    # 1. Ensure today's daily note exists (idempotent) and take its path.
    ensured = ensure_daily_note(date)
    path = ensured.get("path")

    # 2. Read the current briefing/email interior.
    note = get_daily_note(date)
    if not path:
        path = note.get("path")
    interior = extract_section_interior(note.get("content", ""), SECTION_ID)

    # 3. Never overwrite a real agent summary.
    if not should_fill(interior):
        print("email-summary: briefing/email already has a summary; left untouched")
        return 0

    # 4. Fetch metadata-only unread mail, render, and write the section.
    graph = workiq_fetch(unread_query_url(config))
    messages = graph.get("value", []) or []
    content = render_email_section(messages, cap)
    write_section(path, SECTION_ID, content)

    print(f"email-summary: filled briefing/email with {len(messages)} unread")
    return 0


# --------------------------------------------------------------------------
# Self-test (no network) -- what the Rust test invokes
# --------------------------------------------------------------------------

# Fixture messages deliberately carry `body` / `bodyPreview` fields in the
# INPUT so the self-test can prove they never reach the rendered output. The
# body tokens are chosen to share no word with any sender or subject, so a
# leak is unambiguous.
_SELF_TEST_MESSAGES = [
    {
        "id": "AAMk-selftest-0001",
        "subject": "Contract renewal — sign by Friday",
        "from": {"emailAddress": {"name": "Alice Adams", "address": "alice@acme.com"}},
        "receivedDateTime": "2026-09-03T15:04:00Z",
        "isRead": False,
        "bodyPreview": "Confidential pricing attachment enclosed herein",
        "body": {"contentType": "html", "content": "<p>Wire deposit routingnumber 12345</p>"},
    },
    {
        # No display name -> the sender label falls back to the address.
        "id": "AAMk-selftest-0002",
        "subject": "Re: standup notes",
        "from": {"emailAddress": {"address": "teammate@corp.example.com"}},
        "receivedDateTime": "2026-09-03T09:12:00Z",
        "isRead": False,
        "bodyPreview": "Internal passcode rotation reminder",
    },
    {
        # Messy multiline subject -> collapsed to one trimmed line.
        "id": "AAMk-selftest-0003",
        "subject": "   Weekly   digest\n(multiline)   ",
        "from": {"emailAddress": {"name": "News Bot", "address": "news@example.com"}},
        "receivedDateTime": "2026-09-03T06:00:00Z",
        "isRead": False,
        "body": {"contentType": "text", "content": "Newsletter unsubscribe footer boilerplate"},
    },
]


def _forbidden_body_values(messages) -> list:
    """Every body/bodyPreview string in the fixture -- none may appear rendered."""
    values = []
    for msg in messages:
        preview = msg.get("bodyPreview")
        if preview:
            values.append(preview)
        body = msg.get("body") or {}
        content = body.get("content")
        if content:
            values.append(content)
    return values


class _PinnedZone:
    """Pin the process timezone so time assertions do not depend on the host."""

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


def _test_local_times() -> None:
    """Digest times are local, not the raw UTC Graph sends."""
    with _PinnedZone("America/Los_Angeles"):
        assert received_hhmm("2026-09-03T15:04:00Z") == "08:04"
        assert received_hhmm("2026-09-04T00:30:00Z") == "17:30", "crosses back a day"
        assert received_hhmm("2026-09-03T17:04:00+02:00") == "08:04"
        # Standard time, not just daylight: -08:00 in January.
        assert received_hhmm("2026-01-15T17:00:00Z") == "09:00"
        # No marker: Graph's mail surfaces are UTC.
        assert received_hhmm("2026-09-03T15:04:00") == "08:04"

    with _PinnedZone("Asia/Kolkata"):
        assert received_hhmm("2026-09-03T15:04:00Z") == "20:34", "half-hour offset"

    # Degradation is unchanged and zone-independent.
    assert received_hhmm("") == "--:--"
    assert received_hhmm(None) == "--:--"
    assert received_hhmm("not a date") == "--:--"


def self_test() -> int:
    _test_local_times()
    # The fixtures below assert their own UTC clock values, so pin UTC.
    with _PinnedZone("UTC"):
        return _self_test_fixtures()


def _self_test_fixtures() -> int:
    cap = 25
    rendered = render_email_section(_SELF_TEST_MESSAGES, cap)

    # (a) Senders and subjects are present.
    assert "Alice Adams" in rendered, rendered
    assert "teammate@corp.example.com" in rendered, rendered  # name-less -> address
    assert "News Bot" in rendered, rendered
    assert "Contract renewal — sign by Friday" in rendered, rendered
    assert "standup notes" in rendered, rendered
    assert "Weekly digest (multiline)" in rendered, rendered  # collapsed subject

    # Bullet shape and ordering (most recent first) and the count line.
    lines = rendered.splitlines()
    assert lines[0] == "3 unread:", lines[0]
    assert lines[1].startswith("- 15:04 **Alice Adams** — "), lines[1]
    assert lines[2].startswith("- 09:12 **teammate@corp.example.com** — "), lines[2]
    assert lines[3].startswith("- 06:00 **News Bot** — "), lines[3]

    # (b) THE BOUNDARY: no body/bodyPreview content leaks into the output --
    # neither whole values nor any of their significant (>=4 char) words.
    for value in _forbidden_body_values(_SELF_TEST_MESSAGES):
        assert value not in rendered, f"body value leaked: {value!r}"
        for word in value.split():
            if len(word) >= 4:
                assert word not in rendered, f"body word leaked: {word!r}"

    # (c) Empty case.
    assert render_email_section([], cap) == "Nothing unread.", "empty case"

    # Cap overflow keeps the digest terse.
    capped = render_email_section(_SELF_TEST_MESSAGES, 1)
    assert capped.splitlines()[0] == "3 unread:", capped
    assert "- … and 2 more" in capped, capped

    # The query never widens past the metadata-only $select.
    url = unread_query_url({"unread_only": True, "max_messages": 25})
    assert "$select=id,subject,from,receivedDateTime,isRead" in url, url
    for banned in ("body", "bodyPreview", "uniqueBody", "attachments"):
        assert banned not in url, f"{banned} must never be in the query: {url}"
    assert "isRead+eq+false" in url or "isRead%20eq%20false" in url, url

    # Coexistence: fill only when empty or the agent's fallback is present.
    assert should_fill(None) is True  # markers absent
    assert should_fill("   \n  ") is True  # whitespace only
    assert should_fill("Email summary unavailable (Work IQ not connected).") is True
    assert should_fill("- 09:00 **Alice** — real summary") is False

    # extract_section_interior round-trips a real note.
    note = (
        "# 2026-09-03\n\n"
        f"<!-- notesmith:section:begin {SECTION_ID} -->\n"
        "Email summary unavailable (Work IQ not connected).\n"
        f"<!-- notesmith:section:end {SECTION_ID} -->\n"
    )
    interior = extract_section_interior(note, SECTION_ID)
    assert interior == "Email summary unavailable (Work IQ not connected).", interior
    assert should_fill(interior) is True
    assert extract_section_interior("# no markers here\n", SECTION_ID) is None

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
        return run_summary()
    except Exception as error:  # noqa: BLE001 -- surface as a failed job
        print(f"email-summary: FAILED: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
