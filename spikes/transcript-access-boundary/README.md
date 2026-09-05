---
title: Transcript access boundary — probe
date: 2026-09-04
tags:
  - notesmith
  - spike
  - workiq
  - transcripts
status: open
---

# Transcript access boundary — probe

`transcript-sync` reaches transcripts through
`event.join_url -> /me/onlineMeetings -> /transcripts`. Observed coverage is
**40%**: 21 of 35 sampled series answer HTTP 403 with Teams
`3003: User does not have access to lookup meeting`.

The 2026-09-05 diagnostic ruled out every cached event field — organizer,
organizer domain, recurrence, audience, and join-URL structure — and concluded
"the boundary is Microsoft's". That conclusion is still consistent with the
evidence, but one thing was checked only for *presence*, never decoded: the
join URL's query string.

## The hypothesis this tests

A Teams join URL carries `?context={"Tid":"<tenant>","Oid":"<organizer>"}`.
`/me/onlineMeetings` searches **your tenant's** meeting store only. A call
hosted from a customer's tenant is not in yours to look up, and Graph reports
that as 3003 rather than as an empty result.

Organizer email domain does not settle this, which is why the earlier pass
missed it: an external organizer can appear under an internal-looking address
(guest accounts, forwarded invites, resource mailboxes), and an internal
organizer can forward a meeting hosted somewhere else. `Tid` is the meeting's
actual home tenant.

## Why stage 1 is free

Every join URL is already persisted on the event notes, and the connector's
denial cache is keyed by `sha256(join_url)[:32]` — so cached denials join back
to events by hash with **no Graph calls at all**. `probe.py` recomputes that
key with the connector's exact formula.

```sh
export NOTESMITH_VAULT=work
export NOTESMITH_STATE_DIR=...     # the dir the transcript-sync job uses
python3 probe.py
```

The cache only holds series denied in the last 7 days that the sync window
actually attempted (6 entries when last observed), so stage 1 alone may be too
small a sample to conclude from. `--probe` widens it with one read-only
lookup per unclassified series:

```sh
python3 probe.py --probe            # add --limit N to cap the calls
python3 probe.py --probe --show-guids
```

Tenants print as `own-tenant` / `foreign-N` so output can be pasted into a
findings doc without carrying raw GUIDs.

## Reading the result

- **No tenant appears in both groups** → the boundary is cross-tenant. Nothing
  in the connector can reach those meetings, 40% is the ceiling for this
  calendar, and there is nothing to ask IT.
- **A tenant appears in both groups** → those same-tenant denials are the real
  question, and the probe has separated them from cross-tenant noise. That
  short list is what to take to IT.

## What this cannot answer

Tenant-side settings are not readable through a delegated token: the Teams
meeting policy (`Get-CsTeamsMeetingPolicy`) and any application access policy
are admin surface. This probe decides whether that conversation is warranted —
it does not substitute for it.
