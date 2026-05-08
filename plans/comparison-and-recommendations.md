# Comparison & Recommendations

Three reasoning models produced independent Obsidian implementation plans for `notes-method.md`:

| File | Model |
|---|---|
| `obsidian-plan-gpt-5.5.md` | GPT-5.5 |
| `obsidian-plan-claude-opus-4.7-high.md` | Claude Opus 4.7 (High reasoning) |
| `obsidian-plan-claude-opus-4.7-xhigh.md` | Claude Opus 4.7 (Extra-High reasoning) |

This document compares them and gives a single recommended synthesis.

---

## 1. Where all three agree (treat as decided)

These are the high-confidence recommendations — adopt them without further debate.

| Decision | Convergence |
|---|---|
| **Customer state lives in frontmatter on the Customer Index note**, *not* on Account Info. | All three. |
| **Customer Index note name = customer folder name** (so `[[Acme]]` resolves to the entity, not an arbitrary file). | All three. |
| **Frontmatter is the source of truth** for type, customer, stream, status, etc.; tags are at most a mirror for sidebar filtering. | All three. |
| **Tasks plugin (Schemar) is canonical for tasks**; Dataview is the dashboard/aggregation engine for non-task data. | All three. |
| **Inline Dataview fields `[customer:: [[X]]]` and `[stream:: [[Y]]]`** on every task, in addition to Tasks plugin syntax for due/priority/recurrence. | All three. |
| **One stream = one note** (under `Customers/<X>/Streams/`), never a sub-folder. | All three. |
| **Daily notes are generated into the Inbox folder** with a Templater-driven template. | All three. |
| **Auto Note Mover alone cannot do customer-aware routing** — a Templater script is needed because destinations interpolate `customer`. | All three. |
| **Required core plugins:** Templater, Tasks, Dataview, QuickAdd, Auto Note Mover, plus Obsidian core Daily Notes (or Periodic Notes). | All three. |
| **Customer state values** match the spec exactly: `Active`, `On Hold`, `Temp`, `Inactive`. | All three. |
| **Each customer's index note doubles as that customer's overview dashboard** (active streams, open tasks, recent meetings). | All three. |
| **Stream status is independent of its tasks' statuses.** | All three. |

---

## 2. Where they disagree (judgment calls)

### 2.1 Folder naming: numeric prefixes or not?

| Model | Choice |
|---|---|
| GPT-5.5 | `00 Inbox/`, `01 Home.md`, `02 Tasks/`, `03 Customers/`, `04 General/`, `05 Assets/`, `99 Archive/` |
| Opus High | `00 Inbox/`, `01 Tasks/`, `02 Customers/`, `03 General/`, `04 Dashboards/`, `05 Assets/`, `99 Archive/` |
| Opus xhigh | `Inbox/`, `Tasks/`, `Customers/`, `General/`, `Dashboards/`, `Assets/` (no prefixes) |

**Recommendation:** **Use numeric prefixes.** Two of three models picked them, and the reasoning is concrete: the file explorer's alphabetical sort means without prefixes, `Customers/` sits between `Assets/` and `Dashboards/` and `Inbox/` floats wherever its first letter lands. Prefixes pin Inbox to the top and Archive to the bottom regardless of what you add later. The cost (looking at `02 Customers/`) is trivial. Use the Opus-High layout: `00 Inbox/`, `01 Tasks/`, `02 Customers/`, `03 General/`, `04 Dashboards/`, `05 Assets/`, `99 Archive/`.

### 2.2 Daily notes plugin

| Model | Choice |
|---|---|
| GPT-5.5 | Periodic Notes + **Auto Periodic Notes** + Calendar |
| Opus High | **Core Daily Notes** (just rely on "Open daily note on startup") |
| Opus xhigh | Periodic Notes + Calendar + optional `launchd` job for a true morning trigger |

**Recommendation:** **Periodic Notes + Calendar + Auto Periodic Notes.** Periodic Notes supersedes core Daily Notes (more flexible templates, supports weekly/monthly later if needed) and Auto Periodic Notes is the cleanest way to guarantee the daily note exists. Skip the `launchd` job initially — it's brittle for a marginal benefit. Add it later only if you need the file to exist before opening Obsidian.

### 2.3 The Inbox auto-move trigger

This is the most important disagreement.

| Model | Trigger | Mechanism |
|---|---|---|
| GPT-5.5 | `inbox_status: ready_to_file` + run a **QuickAdd "Complete Inbox Note"** command | Templater script computes destination and moves the file |
| Opus High | Flip `done: true` in frontmatter (auto-fires on save) | Auto Note Mover for static rules + Templater script (`inbox-router.js`) on file modify for dynamic routing |
| Opus xhigh | Hit hotkey **⌘⇧A** which sets `archived: true` and moves the file | Templater "Archive note" command (`archive-note.js`) bound to the hotkey |

**Recommendation:** **Use the explicit-hotkey model (Opus xhigh's design).**

Reasoning:
- **Auto-fire on `done: true` is too magical.** You'll accidentally move notes mid-edit, especially on mobile when toggling Properties.
- **A two-step flag + separate command (GPT-5.5) is the worst of both worlds:** you change frontmatter *and* run a command, but the frontmatter flag adds nothing the command couldn't decide on its own.
- **One hotkey = one decisive action.** It's the cleanest match for the "Inbox zero" mental model: when a note is done, hit the hotkey, it's gone. The hotkey stamps `archived: true` and `archived-at` automatically, so the audit trail is preserved.
- The Templater script can resolve customer-aware destinations from frontmatter. Auto Note Mover stays as a fallback for tag-only routing.

### 2.4 Task statuses

| Model | Status palette |
|---|---|
| GPT-5.5 | `[ ]` To Do, `[-]` Blocked, `[x]` Done, `[>]` Awaiting Customer, `[/]` On Hold |
| Opus High | `[ ]` To Do, `[/]` In Progress, `[!]` Blocked, `[w]` Awaiting Customer, `[h]` On Hold, `[x]` Done |
| Opus xhigh | `[ ]` To Do, `[/]` In Progress, `[b]` Blocked, `[a]` Awaiting Customer, `[h]` On Hold, `[x]` Done, `[-]` Cancelled |

**Recommendation:** **Adopt Opus xhigh's seven statuses**, which extend the spec with two universally useful additions: `[/]` In Progress and `[-]` Cancelled. Use mnemonic letters (`[b]`, `[a]`, `[h]`) rather than punctuation (`[!]`, `[>]`) — they survive longer and are easier to type. Configure all non-Done statuses as Tasks plugin type `TODO` so they're caught by `not done` queries; partition Active vs Blocked/Waiting in dashboards by filtering on `status.symbol`.

Final palette:

```
[ ]  To Do                TODO
[/]  In Progress          IN_PROGRESS
[b]  Blocked              TODO
[a]  Awaiting Customer    TODO
[h]  On Hold              TODO
[x]  Done                 DONE
[-]  Cancelled            CANCELLED
```

The "active" task list filters out `b`, `a`, `h`. A separate "Blocked & Waiting" view groups `b`/`a`/`h` by status. This satisfies the spec's "not done and not blocked" while giving you somewhere to surface waiting items.

### 2.5 Dual-encoding (status checkbox + `[task_status::]` field)?

| Model | Position |
|---|---|
| GPT-5.5 | Use both, redundantly | 
| Opus High | Symbol only |
| Opus xhigh | Symbol only |

**Recommendation:** **Symbol only.** GPT-5.5's redundancy adds friction (two places to keep in sync, two ways to drift). The Tasks plugin's `status.name`/`status.symbol` filters are reliable enough. Skip `[task_status::]`.

### 2.6 Bases vs Dataview for customer/stream dashboards

| Model | Position |
|---|---|
| GPT-5.5 | Mostly Dataview; mentions Bases as optional |
| Opus High | Dataview + Bases (for kanban/grouped views once felt the friction) |
| Opus xhigh | **Bases as canonical** for Customers/Streams dashboards with `.base` files; Dataview for free-form joins |

**Recommendation:** **Use Opus xhigh's split:** Bases for tabular/grouped/kanban views over frontmatter (Customers grouped by state, Streams grouped by status), Dataview for queries that join task-level data with frontmatter, Tasks plugin for everything checkbox-shaped. Bases is now core (Obsidian 1.9+), so it won't rot. The `.base` files keep view definitions out of dashboard notes and reusable.

### 2.7 Plugins to install

Union of all three lists, with my keep/skip call:

| Plugin | Verdict | Why |
|---|---|---|
| Templater | **Required** | All three. Drives templates and the archive script. |
| Tasks (Schemar) | **Required** | All three. Canonical task engine. |
| Dataview | **Required** | All three. Aggregation queries. |
| QuickAdd | **Required** | All three. Capture macros. |
| Auto Note Mover | **Required** | All three. Fallback router for tag-based rules. |
| Periodic Notes | **Required** | 2/3. Better than core Daily Notes. |
| Calendar | **Required** | 2/3. Sidebar navigation. |
| Auto Periodic Notes | **Recommended** | GPT-5.5. Guarantees the daily note exists. |
| Homepage | **Recommended** | 2/3. Open Home.md on startup. |
| Bases (core) | **Recommended** | 2/3. Customer/Stream dashboards. |
| Linter | **Recommended** | Opus xhigh. Keeps frontmatter clean — important for query reliability. |
| Hotkeys for specific files | **Recommended** | Opus High. Bind `⌘1`–`⌘5` to dashboards. |
| Metadata Menu | **Optional** | GPT-5.5. Dropdowns for state fields prevent typos but only worth it if you find yourself drifting. |
| Iconize | **Optional** | 2/3. Cosmetic. Pick up if folder count grows. |
| Obsidian Git | **Recommended (later)** | Opus xhigh. Versioned backup. |
| Advanced URI | **Skip initially** | GPT-5.5. Only needed if you add OS-level scheduling. |
| Projects | **Skip** | All three deprioritized it in favor of Bases. |
| Kanban (mgmeyers) | **Skip** | Bases board view replaces it. |
| Buttons / Meta Bind | **Skip** | Opinionated; not needed. |
| Note Refactor | **Skip** | Doesn't move files based on metadata. |

---

## 3. Unique strengths worth borrowing

### From GPT-5.5
- **Explicit "Inbox zero dashboard"** that lists notes by `inbox_status` — useful even if you don't adopt the `inbox_status` flag itself; reframe it as "Inbox notes oldest-first" by `file.cday`.
- **Stream "Archive criteria" checklist** at the bottom of the Stream of Work template (outcome delivered, open tasks moved, summary added). Adopt verbatim.
- **Sensitivity frontmatter** (`sensitivity: internal | customer-shareable`) — useful if you ever paste from notes into customer-facing communications.
- **Customer aliases** (`aliases: [Acme, Acme Inc]`) on the index note — improves search/linking.
- **Stale-stream weekly review query** (`file.mtime < today - 14d`).

### From Opus High
- **Guiding principles section** at the top of the plan ("Inbox is sacred", "Frontmatter is the source of truth", "One file per thing", "Two-digit folder prefixes"). Adopt the framing.
- **DataviewJS task filter that excludes Inactive customers** — the proof point that putting state in frontmatter is worth it (dashboards across the vault react automatically).
- **Tasks plugin query inside the stream note** that captures both inline tasks AND tasks elsewhere with `[stream:: [[…]]]` — neat one-liner.
- **`[owner:: me]` / `[owner:: customer]`** inline field on external meeting tasks to separate "ours" vs. "theirs."
- **Snooze via `⏳ scheduled-date`** — clean and uses native Tasks plugin syntax.

### From Opus xhigh
- **Hotkey-driven archive** (`⌘⇧A`) — the cleanest auto-move design.
- **`archived: true` + `archived-at` timestamp** stamped before move — preserves audit trail.
- **Mirror tag (`customer/active`)** that follows frontmatter, kept in sync by Linter or Templater hook — gives sidebar tag-pane filtering for free.
- **Bases `.base` files in `Assets/bases/`** — reusable view definitions, embedded into dashboards via `` ```base `` blocks.
- **Templater user functions in `Assets/scripts/`** (e.g., `list-customers.js` powering the suggester) — clean separation of code from templates.
- **`launchd` job** as a documented optional escalation path (don't enable it unless needed).
- **One-hour bootstrap broken into 7 phases** with time estimates per phase — most actionable implementation order of the three.

---

## 4. Recommended synthesis

Build the system from Opus-xhigh's plan as the spine, with these specific borrows:

1. **Vault layout**: Opus High's numbered prefixes (`00 Inbox`, `01 Tasks`, `02 Customers`, `03 General`, `04 Dashboards`, `05 Assets`, `99 Archive`).
2. **Plugins**: the "Required" + "Recommended" set in §2.7 above. Skip the "Skip" set.
3. **Templates**: Opus xhigh's nine templates (Daily, Internal Meeting, External Meeting, Customer Index, Account Info, Glossary, Dates & Milestones, Stream of Work, Generic Note). Adopt:
   - GPT-5.5's "Archive criteria" checklist in the Stream template.
   - Opus High's `[owner:: me]` / `[owner:: customer]` convention in External Meeting tasks.
4. **Tasks**: Opus xhigh's seven statuses (`[ ]`, `[/]`, `[b]`, `[a]`, `[h]`, `[x]`, `[-]`). Inline `[customer::]` and `[stream::]` on every task. Use `📅` (due), `⏳` (scheduled / snooze), `🛫` (start), `🔁` (recurrence), `⏫/🔼/🔽` (priority).
5. **Inbox auto-move**: Opus xhigh's `⌘⇧A` → Templater `archive-note.js` model. Stamps `archived: true` + `archived-at`, computes destination from `type` + `customer`, moves the file. Auto Note Mover for tag-only fallback rules (e.g., `#archive` → `99 Archive/Inbox`).
6. **Customer state**: frontmatter `state:` on the Customer Index note. Mirror as `customer/<state>` tag for sidebar filtering. Use Bases' kanban/group-by-state view as the daily-driver UI.
7. **Daily notes**: Periodic Notes + Auto Periodic Notes + Calendar. Folder `00 Inbox/Daily/`, format `YYYY-MM-DD`, template `T - Daily Note.md`. Open on startup. Skip `launchd` initially.
8. **Streams**: one note per stream under `02 Customers/<X>/Streams/`. Frontmatter holds `status`, `priority`, `started`, `target`. Tasks block surfaces both inline tasks AND tasks elsewhere referencing the stream.
9. **Dashboards**:
   - `04 Dashboards/Home.md` (set as Homepage): Today, Inbox in-flight, Top-of-mind tasks, Active customers, Streams in progress, Waiting on customer.
   - `04 Dashboards/Inbox Triage.md`: Inbox notes oldest-first, unrouted Inbox tasks.
   - `04 Dashboards/Customers.md`: Bases view from `Assets/bases/customers.base`, grouped by `state`.
   - `04 Dashboards/Streams.md`: Bases view from `Assets/bases/streams.base`, grouped by `customer` or `status`.
   - `01 Tasks/Tasks - Active.md`, `Tasks - Blocked & Waiting.md`, `Tasks - By Customer.md`.
10. **Implementation order**: Opus xhigh's 7-phase, time-estimated bootstrap.

---

## 5. Refinements to make to `notes-method.md` itself

All three plans surfaced extensions worth folding back into the spec:

1. **Add `In Progress` and `Cancelled` to task statuses** — the spec has five; seven is more honest.
2. **Define task due-date / priority / recurrence / snooze conventions** — the spec is silent and a real workflow needs them.
3. **Add a "Next action" field on streams** (Opus High) — the single most-looked-at piece of metadata for customer-facing work.
4. **Decide where daily notes go after end-of-day** — leave in Inbox vs. archive to `03 General/Daily/YYYY/MM/`. Recommendation: archive at end-of-week, not end-of-day, so yesterday's note stays one click away.
5. **Decide on stream archival path** — when `status: Done` and archived, route to `99 Archive/02 Customers/<X>/Streams/`.
6. **Resolve the `Awaiting Customer` ambiguity in the active list** — the spec says "not done and not blocked." Treat `Awaiting Customer` and `On Hold` as also-not-active for the main view; show them in a dedicated "Waiting" view. (All three models converged on this.)
7. **Adopt the meeting note naming convention**: `YYYY-MM-DD - <Customer> - <Topic>.md`.
8. **Adopt the customer-folder-name = customer-index-note-name rule** (so `[[Acme]]` always resolves to the entity).
9. **Customer state**: confirm the recommendation — frontmatter `state:` on the Customer Index note, with optional mirror tag.

---

## 6. The single biggest decision

If you only adopt one thing from this comparison, adopt the **explicit-hotkey archive flow** (Opus xhigh's `⌘⇧A` → Templater script). Every other piece (templates, dashboards, queries) can be evolved later without rework. The auto-move mechanism, by contrast, *is* the workflow — get it right once and Inbox-zero becomes a muscle, not a chore.
