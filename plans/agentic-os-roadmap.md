# Agentic-OS Roadmap

> Status: **draft 2026-08-16** — feature-gap analysis, not yet reviewed with
> Harpreet. Prompted by the "Claude Code x Obsidian Agentic OS" walkthrough
> (Chase AI, <https://www.youtube.com/watch?v=njHuj8OxIVI>). This doc records
> where that popular "Claude OS" pattern overlaps with Notesmith, and captures
> the genuine gaps worth building. It does **not** re-decide anything already
> settled in `plans/integrations-control-center-plan.md` (ADR 0025) or
> `plans/ai-integration-roadmap.md`; it references them where they already
> cover a gap.
>
> Tracking epic: **#295**. Sub-issues: #289 (item 1), #290 (item 2),
> #291 (item 3), #292 (item 4), #293 (item 5), #294 (item 6).

## Framing

The video's "Claude OS" has four pillars: (1) an Obsidian command-center UI,
(2) a local voice assistant, (3) a skills/automation backbone, (4) an Obsidian
"memory" layer. Its own stated thesis is **substance over spectacle** — the
automation backbone and a navigable memory map are the value; the pretty UI is
worthless without them.

On the substance pillars, Notesmith is already ahead of the video:

- **Memory layer.** The video is Markdown files + hand-maintained `index.md`
  table-of-contents notes, explicitly *not* RAG ("it's giving it a map, not
  more memory"). Notesmith is that plus a real retrieval engine — hybrid
  lexical+semantic `vault_search` (RRF), `time_query`, `query_sql`,
  `vault_stats`, and provenance-backed fact memory (`memory_*`).
- **Automation backbone.** The video turns manual work into "skills" and
  promotes them to "routines". Notesmith already has the `[[jobs]]` runner
  (`command` + `agent` kinds, `every`/`at`, weekdays, `after` gating, catch-up
  on wake, manual `job run`) plus named vault prompts and headless
  `notesmith ai prompt`.
- **UI.** The video's UI is a hand-built Obsidian community plugin. Notesmith
  ships a full SvelteKit app + Tauri desktop shell with SQL dashboards, command
  palette, and an embedded context/chat dock.

So the roadmap below is the **short list of things the video does that we do
not** — plus the one large pillar (local voice) we have deliberately deferred.

## Item summary

| # | Item | Size | Disposition | Builds on |
|---|------|------|-------------|-----------|
| 1 | Auto-maintained index / MOC notes | S | New — quick win | routing, `vault_stats`, hooks/jobs |
| 2 | Skill/job discovery from Claude Code logs | M | New | `ai prompt`, jobs, prompts |
| 3 | One-click job/skill buttons + skills catalog in UI | S–M | New — surfaces shipped backend | jobs runner, customization skills |
| 4 | Curated command-center dashboard tabs | M | Mostly covered | `integrations-control-center-plan.md` |
| 5 | Local voice mode (STT → router → TTS) | L | Deferred epic | `notesmith-transcribe`, jobs, `ai prompt` |
| 6 | Self-improving automation loops | ? | Speculative / backlog | jobs runner |

---

## 1. Auto-maintained index / MOC notes  *(quick win)*

**Gap.** The video's load-bearing memory mechanism is an `index.markdown`
"table of contents" note at every folder level, nested, so both the human and
the agent get a cheap navigable map instead of scanning the tree. Notesmith
relies on search + `vault_stats` and has no generated index/MOC (map-of-content)
notes.

**Why it's still worth having even though we have search.** An index note is a
zero-token, always-loadable entry point; it complements retrieval rather than
replacing it, and it is human-browsable in any Markdown viewer. It also gives
headless agents a deterministic starting file per folder.

**Proposal.** An opt-in capability that keeps an `index.md` (name configurable)
per folder up to date: a list of child notes (title + wikilink + optional
one-line summary field) and child subfolder indexes, regenerated on note
create/route/archive. Managed-section convention (see `docs/managed-sections.md`)
so human-authored prose above the marker is preserved.

**Shape (to decide in the issue).** Hook-driven vs. a periodic `[[jobs]]` entry
vs. a small core `index` command; managed-section markers; how nesting/rollup
works; config surface (`[index]` in `vault.toml`, folder opt-in/opt-out).

**Acceptance.** Enabling it on the golden vault produces nested `index.md`
notes; adding/moving/archiving a note updates the relevant index within a tick;
human content outside the managed markers is untouched; disabled by default.

## 2. Skill/job discovery from Claude Code logs

**Gap.** The video's best onboarding trick: point Claude Code at its own local
session logs over the last 30/60/90 days and ask "which of these repeated
things should become skills?" — surfacing the reality of your workflow vs. your
mental model. Notesmith has no equivalent.

**Fit.** This maps cleanly onto our headless-agent model: it is an
`agent`-kind analysis that reads logs and *proposes* Notesmith artifacts
(named prompts, `[[jobs]]` entries, customization skills) as draft notes for
review — it never auto-installs.

**Proposal.** A shipped vault prompt + optional job (`skill-discovery`) that:
mines the local coding-agent session logs (path configurable; Claude Code
default), clusters recurring tool-call/command patterns, and writes a review
note under `inbox/` proposing candidate jobs/prompts with rationale and a
ready-to-paste config block. Read-only; human promotes what they want.

**Open questions.** Log location/format portability across agents; privacy
posture (logs may contain secrets — keep everything local, never send raw logs
anywhere); dedupe against already-defined jobs/prompts.

**Acceptance.** Running the prompt on a machine with real CC history produces an
`inbox/` note listing ≥1 concrete, actionable candidate with a copy-pasteable
`[[jobs]]`/prompt stub; no writes outside the inbox note.

## 3. One-click job/skill buttons + skills catalog in the app  *(quick win)*

**Gap.** The video's dashboard turns every skill/automation into a clickable
button and shows a visualization of everything it runs. Notesmith's `[[jobs]]`
runner is real but only reachable via `notesmith job run <name>` /
`POST /api/v/{vault}/jobs/{name}/run`; there's no UI catalog or run button, and
customization skills/prompts aren't surfaced together.

**Proposal.** A "Skills & Jobs" view in the SvelteKit app that lists defined
`[[jobs]]` (kind, schedule, last-run status, validity) and vault
prompts/skills, each with a run button (wired to the existing run endpoint),
last-run status/toast, and enable/disable for jobs. Optionally a compact
dashboard widget of recent job outcomes.

**Note.** Backend already exists (`crates/notesmith-http/src/jobs/`,
`GET /jobs`, `POST /jobs/{name}/run`, `notesmith job list` surfaces
validity+last-run). This is primarily a UI + a couple of read endpoints.

**Acceptance.** The view lists golden-vault jobs with live status; clicking Run
triggers the job and reflects success/failure; disabled jobs are visibly
disabled and not runnable.

## 4. Curated command-center dashboard tabs

**Gap.** The video's headline payoff: at-a-glance metric tiles plus tabbed
report views (morning intel: GitHub/HN/YouTube trends; audience metrics;
calendar schedule; morning headlines) — all just outputs of automations
rendered on a dashboard.

**Status: mostly already planned.** This is the visible surface of
`plans/integrations-control-center-plan.md` (ADR 0025): connector/agent jobs
write notes, SQL views (`notesmith-query`, `views.sql`) render dashboards, and
the daily-briefing flow (#288) is the first such tab. The generic mechanism —
job output → note → view → dashboard — is exactly the video's pattern.

**Remaining, video-specific.** Non-work "intel" connectors (GitHub trending,
Hacker News, YouTube outliers, social metrics) are out of scope for the
work-laptop integrations plan and would be separate, config-declared connectors
feeding the same view mechanism. Track as thin follow-ups, not core work.

**Action.** Confirm the control-center plan is the active priority for the
tabbed dashboard; file intel connectors as individual `enhancement`/`backlog`
tickets only if/when wanted. No new core capability required.

## 5. Local voice mode (STT → router → TTS)  *(deferred epic)*

**Gap.** The video's "Jarvis": a hotkey-triggered local voice loop —
Faster-Whisper (STT) → a small router model (Haiku 4.5, swappable for local) →
three tiers (run a skill / read an existing report / spin up headless Claude
Code) → Kokoro (local TTS) spoken reply. Works even when tabbed out of the app.

**Status: deferred.** `plans/ai-integration-roadmap.md` P3 already defers
voice/Whisper multimodal over bundled-model size/cost. Recording that decision,
not reversing it.

**Why it's tractable if revived.** The substrate exists:
`notesmith-transcribe` (whisper.cpp, feature-gated) can do STT; the three-tier
router maps directly onto what we have — tier 1 = `job run`/skill, tier 2 =
read a note (`get_note`/`vault_search`), tier 3 = headless `notesmith ai
prompt`. Missing pieces: a live capture/hotkey path (vs. our transcript-note
STT), a router prompt, TTS (Kokoro or platform voice), and a bundling/model
strategy — the reason it's deferred.

**Disposition.** Keep as a tracked epic, `backlog`, explicitly not near-term.
Revisit after items 1–3 and the control-center dashboard land.

## 6. Self-improving automation loops

**Gap.** Mentioned in the video (not demoed): give an automation a goal +
success criteria, have it compare each run against past outputs and adjust
until it meets the criteria — a self-improving loop.

**Disposition: speculative / backlog.** Interesting but unproven and easy to
get wrong (drift, cost, silent degradation). Park it as a `backlog` idea
tracker; no design work until a concrete job motivates it. If pursued, it would
layer on the `[[jobs]]` runner (a job that reads its own prior output notes and
proposes a diff to its prompt).

---

## Suggested sequencing

1. **Now (quick wins on shipped infra):** #1 index/MOC notes, #3 skills/jobs UI.
2. **Next:** #2 log-based skill discovery.
3. **Continue as planned:** #4 via `integrations-control-center-plan.md`.
4. **Deferred epics / backlog:** #5 local voice, #6 self-improving loops.
