---
title: Work laptop — bootstrap the vault, then settle the transcript access boundary
date: 2026-09-04
tags:
  - notesmith
  - handoff
  - workiq
  - transcripts
  - integrations
status: ready
---

# Work laptop — bootstrap, then settle the access boundary

**Audience:** the agent session on Harpreet's work laptop.

Related:

- [[integrations-control-center-plan]] — phasing; #281 is the last unbuilt phase
- [[transcript-sync-spike-results]] — what the Work IQ path can and cannot do
- `spikes/transcript-access-boundary/README.md` — the probe you will run in task B
- `spikes/transcript-occurrence-matching/FINDINGS.md` — where the 40% number comes from
- `docs/adr/0025-work-system-integrations.md` — the binding decisions

**Pull current `main` first.** Phases 1–5 all landed and were verified; nine
issues (#278, #280, #282–#288) were closed on 2026-09-04. The probe in task B
ships in the same commit as this document; if
`spikes/transcript-access-boundary/` is missing after a pull, your checkout is
behind — say so and stop rather than reimplementing it.

This repository is the channel. There is no live link between the two
machines: you receive work by pulling this document and send results back by
committing one, described under "What to report back".

## State of the world

Everything is built. **Nothing is running.** Every `[[jobs]]` entry in
`kits/work-notes/.notesmith/vault.toml` ships `enabled = false`, because each
needs the `workiq` CLI and a machine-specific config that only exists on your
laptop. The verification runs to date all used throwaway scratch vaults
(`~/vaults/verify-work-phase4-2026-09-04` and friends), never a real one.

So the work is: make it real (task A), then answer the one open question about
it (task B). Task B depends on task A — the probe reads a populated denial
cache and real event notes.

## Task A — issue #281, bootstrap the work vault

The blocker. Acceptance criteria are on the issue; the substance:

1. Daemon auto-starts at login via launchd and is reachable on its local bind
   after a reboot.
2. The work vault is registered in global config, indexed, and served.
3. The Work Notes kit is applied. **After `kit apply`, `chmod +x` the
   connectors** — `calendar-sync.py`, `email-summary.py`, `transcript-sync.py`.
   The kit's own comments say this; it is the most common setup failure.
4. `.notesmith/connectors/calendar-sync.config.json` gets real `corp_domains`.
   Customer-domain mapping comes from customer notes, not this file.
5. `[git]` enabled against the corp-approved remote (ADR 0025 Decision 9 and
   the spike's section F approval), or local-only history if no approved remote
   exists. **Never the homelab** — this is the one hard constraint in this
   document.
6. Enable jobs in dependency order, and let each prove itself before the next:
   `calendar-sync` first, then `transcript-sync` (`after = ["calendar-sync"]`),
   then the briefing pair if you want them. Enabling `transcript-sync` before
   calendar-sync has persisted `join_url` gives it nothing to match against.
7. Write the setup down as a runbook so the machine can be rebuilt.

Judgment calls that are Harpreet's, not yours: which git remote, and whether
the briefing jobs are wanted on day one. Ask rather than assume.

## Task B — settle the 40% transcript coverage question

Run this only after `transcript-sync` has completed a few real passes.

**The question.** `transcript-sync` reaches transcripts via
`event.join_url -> /me/onlineMeetings -> /transcripts`. In a 35-series sample,
21 answered HTTP 403 / Teams `3003: User does not have access to lookup
meeting`. The 2026-09-05 diagnostic ruled out organizer, organizer domain,
recurrence, audience, and join-URL structure, and concluded the boundary is
Microsoft's. That is still consistent with the evidence — but it checked only
whether the join URL's query string was *present*, and never decoded it.

**The hypothesis.** A Teams join URL carries
`?context={"Tid":"<tenant>","Oid":"<organizer>"}`. `/me/onlineMeetings` searches
only *your* tenant's meeting store, so a call hosted from a customer's tenant
is not in yours to look up — which Graph reports as 3003, not as an empty
result. Organizer email domain cannot settle this (guests, forwarded invites
and resource mailboxes all break the correspondence); `Tid` is the meeting's
actual home tenant.

**Running it.** Stage 1 makes no Graph calls and costs nothing: join URLs are
already on the event notes, and the denial cache is keyed by
`sha256(join_url)[:32]`, so denials join back to events by hash.

```sh
cd <repo>/spikes/transcript-access-boundary
export NOTESMITH_VAULT=work
export NOTESMITH_STATE_DIR="$HOME/Library/Application Support/notesmith/work/connector-state/transcript-sync"

python3 probe.py            # free, cache-only
python3 probe.py --probe    # only if stage 1 is underpowered
```

The state-dir path is `<data_dir>/<vault>/connector-state/<job>`
(`crates/notesmith-http/src/jobs/mod.rs:166,172`); the export above is that
path resolved for macOS. If the daemon runs under a different `XDG_DATA_HOME`,
derive it rather than guessing.

Stage 1 may report "not enough classified series" — the cache holds only seven
days of series the sync window actually attempted (six entries when last
observed). `--probe` then widens the sample with one read-only lookup per
series: the same call the connector already makes hourly, so it introduces no
new access and no new cost category.

**Reading the result.**

- No tenant in both groups → the boundary is cross-tenant, 40% is the ceiling
  for this calendar, no connector change can raise it, and there is nothing to
  ask IT.
- A tenant in both groups → those same-tenant denials are the real anomaly and
  the probe has separated them from cross-tenant noise. That short list, and
  only that list, is what goes to IT.

**Caveat:** the probe's pure functions are unit-tested and its `deny_key`
matches `transcript-sync.py:383` exactly, but its I/O paths have never run —
they could not be exercised on the personal machine. A failure will most
likely be in `query_sql` or the state-dir path.

## What to report back

Write `plans/work-laptop-bootstrap-results.md` in the repo, following the
existing results-doc shape ([[work-integrations-phase4-verification-results]] is
the model): environment block, a check/verdict/evidence table per task, and an
explicit list of anything not exercised.

For task B, the probe's contingency table and VERDICT block are the result.
They are already redacted — tenants print as `own-tenant` / `foreign-N`.

Commit that document on a branch and push it, then tell Harpreet the branch
name. Do not merge to `main` yourself. If task A stalls on one of his judgment
calls, still commit what you have with the open question stated — a partial
result that names its blocker is more useful than silence.

## Rules

- **Read-only against M365.** Task B issues no writes. Task A writes only to
  the vault and to config.
- **No customer data in the repo.** No transcripts, join URLs, tenant GUIDs,
  meeting subjects, attendee identities, or raw email. The existing results
  docs show the redaction standard: describe the shape, omit the content.
- **Never sync corporate data to the homelab** (ADR 0025 Decision 6).
- **File nothing upstream.** No issues or comments on any external repo from
  any account.
- If a step fails twice the same way, stop and report it. Do not improvise
  around a permission boundary — for this work, a denial *is* the finding.
