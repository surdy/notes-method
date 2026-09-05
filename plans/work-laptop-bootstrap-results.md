---
title: Work laptop bootstrap and transcript access-boundary results
date: 2026-09-05
tags:
  - notesmith
  - verification
  - workiq
  - transcripts
  - handoff
status: complete
---

# Work laptop bootstrap and transcript access-boundary results

Related:

- [[work-laptop-bootstrap-and-access-boundary-handoff]]
- [[transcript-access-boundary-probe]]
- [[transcript-occurrence-matching-findings]]
- [[work-integrations-phase4-verification-results]]

## Environment

- Repository: `surdy/notes-method`, branch based on `main` at `e75e849`
- Verification date: September 5, 2026
- Machine: macOS work laptop
- Notesmith: installed desktop release `0.1.0`
- Work IQ CLI:
  `1.0.0.28144+10c4074955aee0affce923a5fb04d7ed22c5a09e`
- Real vault: `Customer Notes`
- Corporate-data remote: private repository
  `surdy/notesmith-work-vault`

No meeting subjects, customer names, attendee identities, join URLs, tenant
GUIDs, or transcript content are included below.

## Task A: bootstrap the real work vault

| Check | Verdict | Evidence |
|---|---|---|
| Vault registration and indexing | Pass | `Customer Notes` is registered at its real local path. `/api/status` reported it `ready` with 370 indexed notes, zero parse warnings, and a healthy watcher. The existing `work` fixture vault also remained ready. |
| Work Notes kit | Pass | The kit was applied while preserving the existing vault registration and config. Kit folders, templates, routing, fields, prompts, dashboards, scripts, and connectors are present. |
| Connector executability | Pass | `calendar-sync.py`, `email-summary.py`, and `transcript-sync.py` were made executable after kit application. |
| Machine-specific calendar config | Pass | `corp_domains` contains the two authenticated corporate domains, `github.com` and `microsoft.com`. Customer-domain mappings remain in customer notes rather than the connector config. |
| Durable Work IQ executable | Pass | `@microsoft/workiq@1.0.0` is installed globally at `/opt/homebrew/bin/workiq`; the LaunchAgent PATH includes that directory. |
| Job enablement order | Pass | `calendar-sync` was enabled and proved first. `transcript-sync` was then enabled with `after = ["calendar-sync"]`. The daemon reports both jobs valid and their latest runs `succeeded`. |
| Briefing jobs | Pass (intentional choice) | `daily-briefing` and `email-summary` remain disabled, matching the day-one choice made during bootstrap. |
| Historical event population | Pass | A one-time calendar backfill from August 1, 2026 created 71 records, updated 21, and skipped 44 cancelled events. The resulting 92 Calendar files were committed and pushed to the private vault remote. |
| Transcript state | Pass | `transcript-sync` completed after historical events existed and populated the real-vault denial cache used by task B. |
| Git remote and durability | Pass | The vault is a clean Git worktree on `main`; its authorized remote is private. Bootstrap and synchronized Calendar history commits were pushed. Git automation is configured for 5-minute commits and 10-minute pull/push intervals. |
| Daemon login service | Pass with reboot caveat | `com.surdy.notesmith.daemon` is a loaded user LaunchAgent with `RunAtLoad` and `KeepAlive`. A forced launchd restart changed the PID and returned `/api/status = ok` with both vaults ready and both watchers healthy. A full OS reboot was not performed. |

### Launchd permission finding

The first LaunchAgent start remained alive without binding port `27183`.
Process sampling placed it in the initial vault walk, blocked in `opendir`.
macOS TCC logs showed an unresolved
`kTCCServiceSystemPolicyDocumentsFolder` request for the installed Notesmith
sidecar binary.

Granting Notesmith access to the Documents folder unblocked startup. The
permission persisted across a forced launchd restart: the replacement process
started normally, bound the configured local address, indexed both vaults, and
reported healthy watchers. This was a machine permission issue, not a relative
vault path, hidden `.git` traversal, corrupt lockfile, or missing launchd
environment.

### Rebuild runbook

1. Install Notesmith and install the Work IQ CLI globally so
   `/opt/homebrew/bin/workiq` exists.
2. Clone the private work-vault repository to the approved work-laptop path.
   Do not clone or sync it to the homelab.
3. Register the vault as `Customer Notes`, apply the Work Notes kit, and run:

   ```sh
   chmod +x .notesmith/connectors/calendar-sync.py
   chmod +x .notesmith/connectors/email-summary.py
   chmod +x .notesmith/connectors/transcript-sync.py
   ```

4. Set the real corporate domains in
   `.notesmith/connectors/calendar-sync.config.json`. Keep customer-domain
   mappings in customer notes.
5. Enable `calendar-sync`; run it successfully before enabling
   `transcript-sync`. Leave the briefing pair disabled unless explicitly
   requested.
6. Install `~/Library/LaunchAgents/com.surdy.notesmith.daemon.plist` with the
   installed sidecar command, a PATH containing `/opt/homebrew/bin`, and
   `RunAtLoad` plus `KeepAlive`.
7. On the first launch, grant Notesmith Documents-folder access when macOS
   prompts. Verify the service:

   ```sh
   launchctl kickstart -k "gui/$(id -u)/com.surdy.notesmith.daemon"
   curl -fsS http://127.0.0.1:27183/api/status
   notesmith job list --vault "Customer Notes"
   ```

8. For historical transcript coverage on a fresh clone, backfill Calendar
   records first and then run transcript sync:

   ```sh
   .notesmith/connectors/calendar-sync.py --since YYYY-MM-DD
   notesmith job run --vault "Customer Notes" transcript-sync
   ```

## Task B: transcript access boundary

### Cache-only stage

| Check | Verdict | Evidence |
|---|---|---|
| Real event inventory | Pass | The probe found 59 event notes with join URLs, representing 35 distinct meeting series. |
| Denial-cache join | Pass | Two series matched the seven-day denial cache by the connector's hashed join key. No raw join URL was persisted or printed. |
| Cache-only conclusion | Underpowered | Two classified series were insufficient to test whether tenant predicts access, so the documented live read-only stage was required. |

### Live read-only stage

The probe made one `/me/onlineMeetings` lookup for each of the 33 previously
unclassified series.

| Tenant label | Denied | Resolved |
|---|---:|---:|
| `own-tenant` | 21 | 13 |
| `foreign-1` | 0 | 1 |

The table above is the **re-run on `main` at `33bae0d`**, after the own-tenant
defect below was fixed. The original run printed the 34-series group as
`foreign-2`; it is the signed-in user's own tenant.

| Check | Verdict | Evidence |
|---|---|---|
| Tenant-only boundary | Rejected | One redacted tenant label appears in both groups: 21 series were denied while 13 series from that same tenant resolved. |
| Cross-tenant explanation for 40% coverage | Rejected | Meeting home tenant does not fully predict whether `/me/onlineMeetings` resolves a series. Cross-tenant hosting therefore does not explain the observed coverage ceiling. |
| IT escalation warranted | Yes, and not pursued | The 21 denied series sit in the user's OWN tenant, which also produced 13 successful resolutions — the isolated set worth investigating as a Teams policy, meeting-roster, or access-control boundary. Harpreet declined the escalation on 2026-09-05; 40% stands as the accepted ceiling. |
| Connector change indicated | No | The connector correctly classifies and caches Graph/Teams access denials. The remaining distinction is not available in the event metadata tested by the connector. |

**Verdict:** tenant does not fully predict transcript access. The probe found
substantial resolved/denied overlap within one tenant label, so the access
boundary remains a Microsoft policy or meeting-membership question rather than
a join-URL normalization or connector-matching defect.

### Own-tenant identification — defect, fixed and re-verified

As first run, the probe could not identify the signed-in user's own tenant:
`/organization` needs `Organization.Read.All`, which the delegated Work IQ
token lacks. Every group therefore printed as `foreign-N`, and the 34-of-35
group appeared as `foreign-2` — which understated the finding by letting it
read as a cross-tenant split.

`own_tenant_id` gained a scope-free fallback in `df66ec5`: a meeting the
signed-in user organized is hosted in that user's tenant, so `/me`'s id matched
against a join URL's context `Oid` identifies it. The re-run on `33bae0d`
labelled the group `own-tenant`, identified via "inferred from a meeting you
organized" — the fallback, since `/organization` still refuses.

The overlap result was never affected by this; only its presentation was. But
the corrected label is what makes the finding legible: **21 of 34 series in our
own tenant deny transcript lookup while 13 resolve**, which is a materially
stronger statement than the same numbers under an anonymous `foreign-2`.

## Not exercised

- A full macOS logout/login or reboot. LaunchAgent termination and restart under
  launchd was exercised instead.
- Natural observation of the configured 5-minute auto-commit and 10-minute
  auto-pull/auto-push timers. The same private remote was verified with a
  manual commit and push, and the worktree was clean afterward.
- `daily-briefing` and `email-summary`, intentionally disabled for day one.
- An admin-side inspection of Teams meeting policy or application access
  policy. The delegated-token probe establishes that this conversation is
  warranted but cannot perform it.
- A human edit to a real calendar event followed by update-in-place
  verification.
