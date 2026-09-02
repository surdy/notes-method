# Work-integrations verification — handoff for the work-laptop agent

**Audience:** the agent session running on Harpreet's work laptop.
**Goal:** verify the work-integrations layer merged into `main` (merge
`b00c5bf`) end-to-end on a real machine: job runner, agent jobs, prompt
rendering, the daily-briefing flow (#288), managed sections, and the Work IQ
email path. Everything ran green in CI-style tests; this pass validates the
parts tests can't reach — a real daemon, a real external agent CLI, real
scheduling/wake behavior, and a real auth-protected MCP server.

**Ground rules**

- Work in a **scratch vault** created for this run. Do not touch the real
  work vault, and do not point any of this at real work data — you create
  dummy data (spec below).
- Harpreet is available for the things only a human can do: exporting the
  Work IQ token, logging in the agent CLI, approving anything destructive.
  Ask; don't work around.
- The one real-data exception is phase F (Work IQ email): the summary the
  agent writes is derived from the live inbox. That's the feature. The check
  is that *only* the summary persists.

## 0. Prerequisites (ask Harpreet if missing)

1. **Repo:** `notes-method` at current `main` (must contain merge `b00c5bf`
   and this file). `git log --oneline -5` should show
   `feat(kit): daily-briefing flow as vault config … (#288)`.
2. **Rust toolchain** to build the workspace.
3. **An external ACP agent CLI** on PATH — `claude`, `copilot`, `codex`,
   `gemini`, or `opencode` — logged in. `notesmith ai` defaults to
   `copilot`; pass `--agent claude` (or whichever is installed). Confirm
   with Harpreet which one to use and that it's authenticated.
4. **Work IQ (phase F only):** Harpreet must export the bearer token in the
   environment the daemon/CLI runs in (e.g. `export WORKIQ_TOKEN=...`) and
   give you the MCP endpoint URL. Entra tokens expire in ~1 hour — do phase
   F in one sitting and ask for a fresh token if sessions start failing auth.

## 1. Setup

```bash
cd <repo>
cargo test --workspace            # baseline: everything green before touching anything
cargo build --release             # or cargo install --path crates/notesmith-cli
mkdir -p ~/vaults/verify-work && cd ~/vaults/verify-work
notesmith kit apply work-notes --path .
notesmith daemon start
notesmith daemon status
```

`kit apply` must install (spot-check these exist):

- `.notesmith/vault.toml` with the `[[jobs]] daily-briefing` entry
  (`enabled = false`, `at = "07:30"`, `weekdays_only = true`,
  `agent = { prompt = "daily-note", allow_writes = true }`)
- `.notesmith/prompts/daily-note.md`
- `.notesmith/templates/daily.md` containing the four
  `<!-- notesmith:section:begin briefing/… -->` marker pairs
  (meetings, email, tasks, attention)
- `.notesmith/skill.md` with the "Managed sections" section

Read `docs/managed-sections.md` and the prompt file before phase D — they
define the contract you're verifying.

## 2. Dummy data

Create these notes (via `notesmith note create` / MCP tools / files +
`notesmith reindex`). Use the **actual run day** for "today" — the prompt's
context queries use `date('now','localtime')`, not `{{ today }}`.
Frontmatter relationship fields are lists of quoted wikilinks
(`streams: ["[[Payments Migration]]"]`); `start`/`due`/`date` values must be
`date()`-parseable (use ISO, e.g. `2026-09-01T09:30`).

| Note | Frontmatter / content | Exercises |
|---|---|---|
| `Meetings/… Standup.md` | `kind: event`, `start: <today>T09:30`, `audience: internal` | todays_meetings |
| `Meetings/… Acme sync.md` | `kind: event`, `start: <today>T14:00`, `audience: external`, `customers: ["[[Acme Corp]]"]` | todays_meetings; **control** for unmatched_events (has customers → must NOT appear there) |
| `Meetings/… Globex intro.md` | `kind: event`, `start: <today−2d>T11:00`, `audience: external`, **no** `customers` key | unmatched_events |
| `Meetings/… Old town hall.md` | `kind: event`, `start: <today−10d>`, `audience: external`, no customers | **control**: outside 7-day window → absent |
| any task note | `- [ ] pay invoice [due:: <today>]`, `- [ ] overdue thing [due:: <today−3d>]` | tasks_due (overdue one must be flagged) |
| same note | `- [ ] prep QBR [due:: <today+2d>]` | tasks_upcoming |
| same note | `- [ ] far-off [due:: <today+5d>]`, `- [x] done thing [due:: <today>]` | **controls**: outside window / closed → absent from both task sections |
| `Streams/Payments Migration.md` | `kind: stream`, `status: blocked` | blocked_streams |
| `Streams/Vendor Review.md` | `kind: stream`, `status: waiting` | blocked_streams |
| `Streams/Data Platform.md` | `kind: stream`, `status: active`, no meeting references it | stale_streams |
| `Streams/Onboarding Revamp.md` | `kind: stream`, `status: active` | **control**: give some meeting note `streams: ["[[Onboarding Revamp]]"]` and `date: <today−5d>` → must NOT be stale |

Sanity-check the data layer before involving the agent: run each of the six
`context_queries` SQL blocks from `.notesmith/prompts/daily-note.md` through
`notesmith query sql "…"` and confirm each returns exactly the rows above
(including the controls being absent). Record any query returning
wrong/empty rows as a finding — don't "fix" the SQL to match.

## 3. Verification phases

Work through in order; each phase assumes the previous passed. For every
check record pass/fail plus the actual output on failure.

### A. Headless read-only safety

`notesmith ai prompt daily-note --agent <id>` (NO `--allow-writes`).

- Exit 0; stdout contains a briefing-shaped result.
- The agent must have been **refused** any write: no daily note created, no
  vault file changed (`git status` in the vault if `[git]` enabled, else
  compare a `cp -r` snapshot).

### B. Full run with writes

`notesmith ai prompt daily-note --agent <id> --allow-writes`

- `Daily/<today>.md` exists (created via template if absent) and all four
  `briefing/*` sections are filled between their markers:
  meetings in start order (`HH:MM–HH:MM [[title]] (audience)`), tasks with
  the overdue one flagged plus an "Upcoming" line, attention listing the two
  blocked/waiting streams + the stale stream + the unmatched external event.
- With no Work IQ attached, `briefing/email` reads exactly
  `Email summary unavailable (Work IQ not connected).` and the run still
  succeeds.
- All four marker pairs still present, balanced, in order.

### C. Managed-section contract (the core of #288)

Before each sub-check snapshot the daily note (`cp Daily/<today>.md /tmp/…`).

1. **Outside is inviolable.** Add distinctive human text under `## Focus`
   and `## Notes` (weird spacing, trailing spaces, a stray `<!--` comment).
   Re-run B. Everything outside the marker pairs must be **byte-identical**
   (`diff` the regions, or strip each section interior and `cmp`).
2. **Idempotent re-runs.** Run B twice more with no data changes. No
   duplicate sections, no duplicate bullets, no drift — successive runs
   converge (byte-identical is ideal; semantically identical is the floor).
3. **Missing pair → append.** Delete the whole `briefing/attention` pair
   from the note. Re-run. The full marked block (markers + content) is
   appended at the **end** of the note; nothing else moved; no markers were
   wrapped around existing human text.
4. **Data change propagates.** Flip `Streams/Payments Migration.md` to
   `status: active`, re-run: it leaves attention. Flip back for later phases.

### D. Job runner integration

Edit the scratch vault's `.notesmith/vault.toml`: set the daily-briefing
job's `enabled = true` (docs say this takes effect **without** a daemon
restart — verify that claim itself).

1. `notesmith job list` shows `daily-briefing [at 07:30 weekdays] (agent:
   daily-note)` enabled, `last: never` (or prior manual state).
2. `notesmith job run daily-briefing` → run completes; `job list` shows
   `last: succeeded …`; the note updated as in B. Confirm the daemon
   refuses a second `job run` while one is in flight.
3. **Scheduled fire:** set `at` to 2–3 minutes ahead, wait, confirm it fires
   and succeeds on schedule.
4. **Catch-up on wake:** set `at` to a time a few minutes ahead, stop the
   daemon (`notesmith daemon stop`), let the time pass, start it again → the
   missed `at` run is caught up shortly after start. (Closing the laptop lid
   over the `at` time is the even-more-real variant if convenient.)
5. **weekdays_only:** can't wait for a weekend — instead verify state/docs
   coherence: `job list` renders `weekdays (…)` and, if feasible, set the
   machine-independent check: temporarily add `timezone` of a zone where
   it's currently Saturday and confirm the runner skips. Skip this sub-check
   if it gets fiddly; note it as not-exercised rather than forcing it.
6. Restore `at = "07:30"` when done.

### E. Failure modes

1. Break the prompt (temporarily rename `.notesmith/prompts/daily-note.md`)
   → `job run` records a **failed** run in `job list`; daemon stays up.
2. Malform one context query's SQL → `ai prompt` exits nonzero with a
   sensible error. Restore the file exactly afterwards (it must stay
   byte-identical to the kit copy).

### F. Work IQ email path (needs Harpreet)

Configure the server globally in `~/.config/notesmith/config.toml`
(see `docs/ai-mcp-servers.md`):

```toml
[[mcp.servers]]
id = "workiq"
url = "<endpoint from Harpreet>"
display_name = "Work IQ"
enabled = true

[mcp.servers.headers]
Authorization = "Bearer $WORKIQ_TOKEN"
```

With `WORKIQ_TOKEN` exported in the environment that runs the CLI/daemon:

1. Re-run B. `briefing/email` now contains a short bullet summary —
   sender + subject + at most one clause of gist per item.
2. **Hard boundary:** search the entire vault (and `NOTESMITH_STATE_DIR`)
   for raw email leakage — quoted bodies, `From:`/`Received:` headers,
   message-IDs, long verbatim passages. Only the agent's summary may exist
   on disk. Also confirm the token itself appears nowhere in the vault.
3. Disable the server (`enabled = false`), re-run → the fallback line
   returns and the run still succeeds.
4. If the daily-briefing job runs via the daemon, confirm the daemon's
   environment also had the token (headless jobs attach the same servers).

### G. Baseline regression

`cargo test --workspace` once more at the end — the run must not have
depended on any local edit to tracked files (kit/prompt/template restored).

## 4. Report back

Produce a single results table: phase/sub-check, pass/fail/not-exercised,
and for failures the exact command, output, and the note content involved.
Byte-identity claims should say how they were checked (diff/cmp), not just
"looked unchanged". Leave the scratch vault in place for inspection; list
its path in the report.

Known intentional gaps (don't report as findings): no `after =
["calendar-sync"]` on the job yet (the connector doesn't exist; the runner
rejects `after` names absent from config), and the kit ships the job
`enabled = false` by design.
