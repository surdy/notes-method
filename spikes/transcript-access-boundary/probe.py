#!/usr/bin/env python3
"""Does the transcript access boundary follow the meeting's home tenant?

Read-only. Run on the work laptop, with the Notesmith daemon up.

Background: `transcript-sync` resolves an event's `join_url` to an online
meeting before it can list transcripts. 21 of 35 sampled series answered
HTTP 403 / Teams `3003: User does not have access to lookup meeting`, and the
2026-09-05 diagnostic found no cached event field that predicted which --
organizer, organizer domain, recurrence, audience, and "join URL uses
thread.v2 and carries a query string" were identical across both groups.

That diagnostic checked whether the query string was *present*. It never
decoded it. A Teams join URL carries `?context={"Tid":"<tenant>","Oid":
"<organizer>"}`, and `/me/onlineMeetings` searches only YOUR tenant's meeting
store. If a customer hosts the call from their own tenant, the meeting is not
in yours to look up -- which Graph reports as 3003, not as an empty result.
Organizer email domain does not settle this: an external organizer can appear
under an internal-looking address, and an internal organizer can forward a
meeting hosted elsewhere.

Stage 1 needs no Graph calls and costs nothing: every join URL is already
persisted on the event notes, and the denial cache the connector writes is
keyed by sha256(join_url)[:32], so denials join back to events by hash.

Stage 2 (--probe, requires `workiq`) widens the sample by replaying the
online-meeting lookup for series the cache has not classified. One read-only
call per series.

Usage:
    export NOTESMITH_VAULT=work
    export NOTESMITH_STATE_DIR=...        # same dir the transcript-sync job uses
    python3 probe.py                      # stage 1, free
    python3 probe.py --probe              # stage 1 + live lookups
    python3 probe.py --probe --show-guids # unredacted tenant GUIDs
"""

import argparse
import collections
import hashlib
import json
import os
import subprocess
import urllib.parse
import urllib.request

DENY_FILE = "transcript-sync-denied.json"

EVENT_SQL = (
    "SELECT n.path AS path, "
    "MAX(CASE WHEN f.key = 'join_url' THEN f.value END) AS join_url, "
    "MAX(CASE WHEN f.key = 'organizer' THEN f.value END) AS organizer, "
    "MAX(CASE WHEN f.key = 'audience' THEN f.value END) AS audience, "
    "MAX(CASE WHEN f.key = 'start' THEN f.value END) AS start "
    "FROM v_notes n "
    "JOIN v_fields f ON f.vault_name = n.vault_name AND f.note_path = n.path "
    "WHERE n.path IN (SELECT note_path FROM v_fields WHERE key = 'kind' AND value = 'event') "
    "GROUP BY n.path"
)


# ---------------------------------------------------------------- pure helpers

def deny_key(join_url):
    """Must match transcript-sync.py's deny_key exactly, or the join is empty."""
    return hashlib.sha256((join_url or "").encode("utf-8")).hexdigest()[:32]


def parse_context(join_url):
    """-> (tenant_id, organizer_oid). Either may be None."""
    try:
        query = urllib.parse.urlparse(join_url).query
        raw = urllib.parse.parse_qs(query).get("context", [None])[0]
        if not raw:
            return None, None
        ctx = json.loads(raw)
    except (ValueError, TypeError):
        return None, None
    if not isinstance(ctx, dict):
        return None, None
    tid = ctx.get("Tid") or ctx.get("tid")
    oid = ctx.get("Oid") or ctx.get("oid")
    return (tid.lower() if isinstance(tid, str) else None,
            oid.lower() if isinstance(oid, str) else None)


def thread_id(join_url):
    """The `19:meeting_...@thread.v2` segment, which identifies the series."""
    try:
        path = urllib.parse.urlparse(join_url).path
    except ValueError:
        return None
    for part in path.split("/"):
        part = urllib.parse.unquote(part)
        if "@thread." in part:
            return part
    return None


def is_access_denied(detail):
    text = (detail or "").lower()
    return (
        '"statuscode":403' in text.replace(" ", "")
        or "forbidden" in text
        or "3003" in text
        or "does not have access to lookup meeting" in text
    )


# ----------------------------------------------------------------------- I/O

def api_base():
    return os.environ.get("NOTESMITH_API_BASE", "http://127.0.0.1:27183").rstrip("/")


def vault():
    name = os.environ.get("NOTESMITH_VAULT")
    if not name:
        raise SystemExit("NOTESMITH_VAULT is not set")
    return name


def query_sql(sql):
    url = f"{api_base()}/api/v/{urllib.parse.quote(vault())}/query/sql"
    body = json.dumps({"sql": sql}).encode("utf-8")
    request = urllib.request.Request(
        url, data=body,
        headers={"Accept": "application/json", "Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        result = json.loads(response.read())
    columns = result.get("columns", [])
    return [dict(zip(columns, row)) for row in result.get("rows", [])]


def load_denials():
    state = os.environ.get("NOTESMITH_STATE_DIR")
    if not state:
        return {}, "NOTESMITH_STATE_DIR unset"
    path = os.path.join(state, DENY_FILE)
    if not os.path.exists(path):
        return {}, f"no {DENY_FILE} at {state} (run transcript-sync first)"
    try:
        with open(path) as handle:
            return json.load(handle) or {}, path
    except (OSError, ValueError) as exc:
        return {}, f"unreadable: {exc}"


def workiq_fetch(entity_url):
    """-> (payload_or_None, detail). Never raises; a denial is data here."""
    try:
        proc = subprocess.run(
            ["workiq", "fetch", "-u", entity_url],
            capture_output=True, text=True, timeout=120,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        return None, f"workiq not runnable: {exc}"
    detail = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0:
        return None, detail.strip()
    try:
        return json.loads(proc.stdout), detail.strip()
    except ValueError:
        return None, detail.strip()


def own_tenant_id():
    """The tenant GUID, so a join URL's Tid can be called own vs foreign.

    /organization's id IS the tenant id. /me's id is a user id and is not a
    fallback for it.
    """
    payload, _ = workiq_fetch("/organization?$select=id")
    if isinstance(payload, dict):
        values = payload.get("value") or []
        if values and isinstance(values[0], dict) and values[0].get("id"):
            return str(values[0]["id"]).lower()
    return None


def resolve_online_meeting(join_url):
    """-> 'resolved' | 'denied' | 'absent' | 'failed', plus detail."""
    quoted = join_url.replace("'", "''")
    url = "/me/onlineMeetings?$filter=" + urllib.parse.quote(
        f"joinWebUrl eq '{quoted}'", safe="$= '"
    )
    payload, detail = workiq_fetch(url)
    if payload is None:
        return ("denied" if is_access_denied(detail) else "failed"), detail
    if is_access_denied(json.dumps(payload)):
        return "denied", detail
    values = payload.get("value") if isinstance(payload, dict) else None
    return ("resolved" if values else "absent"), detail


# --------------------------------------------------------------------- report

def label_tenants(tenants, own, show_guids):
    """Stable, pasteable labels so a findings doc need not carry raw GUIDs."""
    labels = {}
    n = 0
    for tid in sorted(t for t in tenants if t):
        if show_guids:
            labels[tid] = tid
        elif own and tid == own:
            labels[tid] = "own-tenant"
        else:
            n += 1
            labels[tid] = f"foreign-{n}"
    labels[None] = "no-context"
    return labels


def crosstab(rows, key, statuses):
    table = collections.defaultdict(collections.Counter)
    for row in rows:
        table[row[key]][row["status"]] += 1
    return table


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", action="store_true",
                        help="replay the online-meeting lookup for unclassified series (needs workiq)")
    parser.add_argument("--show-guids", action="store_true",
                        help="print raw tenant GUIDs instead of own-tenant/foreign-N labels")
    parser.add_argument("--limit", type=int, default=0,
                        help="cap the number of live lookups in --probe mode")
    args = parser.parse_args()

    events = [e for e in query_sql(EVENT_SQL) if (e.get("join_url") or "").strip()]
    if not events:
        raise SystemExit("No event notes carry join_url. Run calendar-sync first.")

    denials, deny_note = load_denials()

    # Collapse occurrences into series: one join URL is one recurring meeting.
    series = {}
    for event in events:
        join = event["join_url"].strip()
        entry = series.setdefault(join, {
            "join_url": join,
            "occurrences": 0,
            "organizer": event.get("organizer"),
            "audience": event.get("audience"),
        })
        entry["occurrences"] += 1

    own = own_tenant_id() if args.probe else None

    rows = []
    for join, entry in series.items():
        tid, oid = parse_context(join)
        key = deny_key(join)
        rows.append({
            "join_url": join,
            "tenant": tid,
            "organizer_oid": oid,
            "thread": thread_id(join),
            "occurrences": entry["occurrences"],
            "audience": entry["audience"],
            "status": "denied" if key in denials else "unclassified",
            "source": "cache" if key in denials else "-",
        })

    if args.probe:
        todo = [r for r in rows if r["status"] == "unclassified"]
        if args.limit:
            todo = todo[: args.limit]
        print(f"Probing {len(todo)} series with live read-only lookups...\n")
        for row in todo:
            status, detail = resolve_online_meeting(row["join_url"])
            row["status"] = status
            row["source"] = "live"
            if status == "failed":
                row["detail"] = detail[:200]

    labels = label_tenants({r["tenant"] for r in rows}, own, args.show_guids)

    print("=" * 72)
    print("TRANSCRIPT ACCESS BOUNDARY vs MEETING HOME TENANT")
    print("=" * 72)
    print(f"event notes with join_url : {len(events)}")
    print(f"distinct series           : {len(rows)}")
    print(f"denial cache              : {len(denials)} entries ({deny_note})")
    print(f"own tenant id             : {own or 'unknown (use --probe)'}")
    print()

    statuses = sorted({r["status"] for r in rows})
    table = crosstab(rows, "tenant", statuses)
    width = max([len(labels[t]) for t in table] + [12])
    print(f"{'tenant':<{width}}  " + "  ".join(f"{s:>12}" for s in statuses))
    print("-" * (width + 2 + 14 * len(statuses)))
    for tid in sorted(table, key=lambda t: labels[t]):
        counts = table[tid]
        print(f"{labels[tid]:<{width}}  " + "  ".join(f"{counts[s]:>12}" for s in statuses))
    print()

    resolved = [r for r in rows if r["status"] in ("resolved", "absent")]
    denied = [r for r in rows if r["status"] == "denied"]
    if resolved and denied:
        rt = {r["tenant"] for r in resolved}
        dt = {r["tenant"] for r in denied}
        print("VERDICT")
        if not (rt & dt):
            print("  Tenant PREDICTS access: no tenant appears in both groups.")
            print("  The boundary is cross-tenant. Meetings hosted outside your")
            print("  tenant are not in /me/onlineMeetings to look up, and no")
            print("  connector change can reach them. Not a fixable defect.")
        else:
            print(f"  Tenant does NOT fully predict access: {len(rt & dt)} tenant(s)")
            print("  appear in both groups. Same-tenant denials are the ones to")
            print("  take to IT -- those are a policy or roster question, and the")
            print("  probe has now separated them from the cross-tenant noise.")
            for tid in sorted(rt & dt, key=lambda t: labels[t]):
                same = [r for r in denied if r["tenant"] == tid]
                print(f"    {labels[tid]}: {len(same)} denied series worth escalating")
    else:
        print("VERDICT")
        print("  Not enough classified series. Run with --probe to widen the sample.")
    print()
    print("Only IT can confirm the tenant-side settings: the Teams meeting policy")
    print("and any application access policy. This probe decides whether that")
    print("conversation is even warranted, or whether the boundary is simply that")
    print("the meetings belong to someone else's tenant.")


if __name__ == "__main__":
    main()
