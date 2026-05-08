# Obsidian Implementation Plan — Claude Opus 4.7 (High Reasoning)

A concrete, opinionated implementation of the Notes Method in Obsidian. The plan is biased toward **few, well-known plugins**, **frontmatter as the source of truth**, and **Tasks plugin syntax** for everything task-shaped. Where the spec leaves room, I make a recommendation and explain it.

---

## 0. Guiding Principles (read this first)

A handful of decisions drive everything else. Stating them up front so the rest of the plan is consistent:

1. **Frontmatter is the source of truth.** Type, status, customer, stream, and lifecycle state all live in YAML — not in the body, not in tags only. This is what makes Dataview / Bases queries reliable and what powers the Inbox auto-move.
2. **Tasks plugin is the task system.** Native checkboxes (`- [ ]`) get superpowers (statuses, due dates, recurring, queries). Dataview is the *dashboard* engine but **does not** own task state.
3. **One file per "thing."** A meeting is a note, a stream is a note, a customer is a folder with an index note. Avoid burying meetings inside long-running notes.
4. **Inbox is sacred.** Anything in `00 Inbox/` is "in flight." A note leaves Inbox automatically the moment it has enough metadata to know where it belongs.
5. **Customer state lives on the Customer Index note**, not in the Account Info note. Reasoning in §7.
6. **Two-digit folder prefixes** force sort order in the file explorer and make the structure instantly readable.

---

## 1. Vault Structure

```
VaultRoot/
├── 00 Inbox/                        ← all new notes land here
├── 01 Tasks/                        ← aggregation dashboards (no source-of-truth tasks)
│   ├── Tasks - Active.md
│   ├── Tasks - Blocked.md
│   ├── Tasks - Awaiting Customer.md
│   └── Tasks - On Hold.md
├── 02 Customers/
│   ├── Acme Corp/
│   │   ├── Acme Corp.md             ← Customer Index (frontmatter holds state)
│   │   ├── Account Info/
│   │   │   ├── Account Info.md
│   │   │   ├── Glossary.md
│   │   │   └── Dates and Milestones.md
│   │   ├── Internal Meetings/
│   │   │   └── 2026-05-08 Acme - Internal sync.md
│   │   ├── External Meetings/
│   │   │   └── 2026-05-07 Acme - QBR.md
│   │   └── Streams/
│   │       ├── Migration to v2.md
│   │       └── SSO rollout.md
│   ├── Globex/
│   │   └── …same shape…
│   └── …
├── 03 General/                      ← non-customer notes (ideas, reading, ops)
├── 04 Dashboards/
│   ├── Home.md
│   ├── Customers - Active.md
│   ├── Customers - On Hold.md
│   ├── Customers - Temp.md
│   ├── Customers - Inactive.md
│   └── Streams - All.md
├── 05 Assets/
│   ├── Templates/                   ← Templater + QuickAdd templates
│   │   ├── T - Daily Note.md
│   │   ├── T - Internal Meeting.md
│   │   ├── T - External Meeting.md
│   │   ├── T - Stream of Work.md
│   │   ├── T - Customer Index.md
│   │   ├── T - Account Info.md
│   │   ├── T - Glossary.md
│   │   └── T - Dates and Milestones.md
│   ├── Data/                        ← CSVs, PDFs, attachments referenced from notes
│   └── Attachments/                 ← Obsidian's default attachment folder
└── 99 Archive/                      ← optional: archived customers / streams
```

Why this shape:

- **Numeric prefixes** keep Inbox at the top, Archive at the bottom, regardless of vault size.
- **`Customer.md` lives at the customer folder root** (same name as folder) so `[[Acme Corp]]` always resolves and auto-completes.
- **`Streams/`** is plural and flat. Streams are notes, not folders, so they can be linked, embedded, and queried trivially.
- **`Account Info/`** is a sub-folder, not a single note, because Glossary and Dates/Milestones grow independently and benefit from being separate files (better backlinks, easier embedding).
- **`05 Assets/Templates/`** matches the spec ("Assets > templates, data") and is what Templater and QuickAdd point at.

---

## 2. Plugins

Recommended set, all from the community store unless marked **(core)**. I deliberately keep the list small; each plugin earns its place.

| Plugin | Author | Purpose |
|---|---|---|
| **Daily Notes** *(core)* | Obsidian | Generates today's note with a template, in a configured folder. |
| **Templates** *(core)* | Obsidian | Fallback template insertion. (Templater supersedes it for advanced cases.) |
| **Templater** | SilentVoid13 | Real templating: prompts, file-creation hooks, Inbox auto-move script, dynamic frontmatter. |
| **QuickAdd** | Christian B. B. Houmann | One-shortcut creation of new meetings/streams/customers with prompts → applies the right Templater template into the right folder. |
| **Tasks** | Schemar (Martin Schenck / @schemar) | Task syntax, statuses (custom statuses for *Awaiting Customer*, *On Hold*, *Blocked*), due/scheduled/recurrence, fast queries. **Source of truth for tasks.** |
| **Dataview** | blacksmithgu | Frontmatter-driven dashboards (customers by state, streams by status, per-customer overview). Also DataviewJS for anything Tasks queries can't express. |
| **Bases** *(core, Obsidian 1.9+)* | Obsidian | Table/board views over frontmatter. Excellent for the Customers-by-state list and a kanban of streams without writing a query. Use alongside Dataview, not instead of it. |
| **Auto Note Mover** | farux | Moves a note to a destination folder when a tag or frontmatter value matches a rule. This is the Inbox auto-move workhorse. |
| **MetaEdit** *(optional)* or **Properties view** *(core)* | chhoumann / Obsidian | Edit frontmatter from a UI; keeps `status:` and `state:` consistent. The core Properties view is usually enough; install MetaEdit only if you want inline editing in Dataview tables. |
| **Calendar** | liamcain | Sidebar calendar that surfaces daily notes; useful for the Inbox/daily workflow. |
| **Periodic Notes** *(optional)* | liamcain | Only if you decide to add weekly/monthly reviews. Not required by the spec. |
| **Hotkeys for specific files** | Vinzent | Bind ⌘1 = Home, ⌘2 = Tasks Active, ⌘3 = Tasks Blocked, etc. Tiny QoL win that pays off daily. |
| **Iconize** *(optional)* | Florian Woelki | Folder icons (📥 Inbox, 👥 Customers, 📊 Dashboards). Cosmetic but improves scanability. |

How they compose:

- **QuickAdd → Templater → file in `00 Inbox/`** is the creation path for *most* notes.
- **Templates set frontmatter** (`type`, `customer`, `stream`, `status`, `done`).
- **Auto Note Mover watches frontmatter** and relocates the file as soon as it has enough info.
- **Tasks plugin** scans the whole vault for `- [ ]` lines and renders the Active/Blocked dashboards.
- **Dataview / Bases** read frontmatter for everything that isn't a task (customers, streams, meetings).

Plugins I deliberately did *not* pick, and why:

- **Projects** (Marcus Olsson) — overlaps heavily with Bases; Bases is now core, so prefer it.
- **Note Refactor** — useful for splitting notes, but doesn't move files based on metadata, so it doesn't replace Auto Note Mover.
- **Kanban** (mgmeyers) — tempting for stream status, but a Bases board view does the same thing without a separate file format. Add Kanban only if you specifically want drag-and-drop columns.
- **Tasks alternatives** (Checklist, Reminder, etc.) — Tasks plugin's status system + queries cover everything in the spec.

---

## 3. Templates

All templates live in `05 Assets/Templates/` and are invoked via **QuickAdd** (which calls **Templater** under the hood). Frontmatter keys are kept short and consistent across templates so queries are uniform.

### 3.1 Conventions

| Key | Type | Allowed values | Notes |
|---|---|---|---|
| `type` | string | `daily`, `meeting-internal`, `meeting-external`, `stream`, `customer`, `account-info`, `glossary`, `milestones`, `general` | Drives Auto Note Mover rules. |
| `customer` | wikilink | `"[[Acme Corp]]"` | Always a link, never a plain string — gives backlinks for free. |
| `stream` | wikilink \| null | `"[[Migration to v2]]"` | Optional on meetings/general notes. |
| `status` | string | `In Progress`, `Blocked`, `Done`, `Awaiting Customer`, `On Hold` | On streams. Mirrors Tasks statuses. |
| `state` | string | `Active`, `On Hold`, `Temp`, `Inactive` | On Customer Index notes only. |
| `date` | date | `YYYY-MM-DD` | On meetings & daily notes. |
| `attendees` | list | `["Jane (Acme)", "me"]` | On meeting notes. |
| `done` | bool | `true` / `false` | The Inbox auto-move trigger. |
| `tags` | list | `[meeting, acme]` | Light use; frontmatter keys preferred. |

### 3.2 Daily Note — `T - Daily Note.md`

```markdown
---
type: daily
date: <% tp.date.now("YYYY-MM-DD") %>
done: false
tags: [daily]
---
# <% tp.date.now("dddd, MMMM Do YYYY") %>

## Focus
- 

## Notes
- 

## Tasks captured today
- [ ] 

## Log
- <% tp.date.now("HH:mm") %> · 
```

Notes:
- `done: false` keeps it in Inbox until you flip it. When the day is wrapped, set `done: true` and Auto Note Mover sends it to `03 General/Daily/YYYY/MM/`.
- Tasks captured here are picked up by Tasks plugin globally; they become "homeless" tasks until you assign a `customer::`/`stream::` inline field, or move the task into a stream note.

### 3.3 Internal Meeting — `T - Internal Meeting.md`

```markdown
---
type: meeting-internal
customer: "[[<% customerName %>]]"
stream: 
date: <% tp.date.now("YYYY-MM-DD") %>
attendees: []
done: false
tags: [meeting, internal]
---
# <% tp.date.now("YYYY-MM-DD") %> <% customerName %> — Internal: <% topic %>

**Customer:** [[<% customerName %>]]  
**Stream:** <% stream || "—" %>  
**Attendees:** 

## Agenda
- 

## Notes
- 

## Decisions
- 

## Tasks
- [ ] Example task 📅 2026-05-15 #task/internal [stream:: [[<% stream %>]]] [customer:: [[<% customerName %>]]]
```

QuickAdd prompts for `customerName`, `topic`, and optional `stream`, then creates the file as `00 Inbox/<date> <customer> - Internal - <topic>.md`. When `done: true`, Auto Note Mover routes it to `02 Customers/<customer>/Internal Meetings/`.

### 3.4 External Meeting — `T - External Meeting.md`

Identical to internal but with `type: meeting-external`, tags `[meeting, external]`, and an extra section:

```markdown
## Customer asks
- 

## Action items (ours)
- [ ] 

## Action items (theirs)
- [ ] ⏳ 2026-05-15 [owner:: customer] [customer:: [[<% customerName %>]]]
```

`[owner:: customer]` is an inline Dataview field used by the dashboard to separate "ours" vs. "theirs."

### 3.5 Stream of Work — `T - Stream of Work.md`

```markdown
---
type: stream
customer: "[[<% customerName %>]]"
status: In Progress
started: <% tp.date.now("YYYY-MM-DD") %>
target: 
done: false
tags: [stream]
---
# <% streamName %> — <% customerName %>

**Customer:** [[<% customerName %>]]  
**Status:** `= this.status`  
**Started:** `= this.started` · **Target:** `= this.target`

## Goal
> One sentence on what success looks like.

## Context
- 

## Open Tasks
```tasks
not done
(path includes {{query.file.path}}) OR ([stream:: [[{{query.file.basename}}]]])
group by status.name
short mode
```

## Done
```tasks
done
path includes {{query.file.path}}
short mode
```

## Decision log
- 
```

Why a Tasks query inside the stream note: it surfaces both **tasks defined inline in this stream note** *and* **tasks anywhere in the vault that reference this stream** via the `[stream:: [[…]]]` inline field. That means a task captured in a meeting note still shows up on its stream.

### 3.6 Customer Index — `T - Customer Index.md`

This is the single page representing the customer. **State lives here.**

```markdown
---
type: customer
state: Active
tier: 
csm: 
ae: 
since: <% tp.date.now("YYYY-MM-DD") %>
tags: [customer]
---
# <% customerName %>

**State:** `= this.state`

## Quick links
- [[Account Info]]
- [[Glossary]]
- [[Dates and Milestones]]

## Active streams
```dataview
TABLE status, started, target
FROM "02 Customers/<% customerName %>/Streams"
WHERE type = "stream" AND status != "Done"
SORT status ASC
```

## Recent meetings
```dataview
TABLE type, date
FROM "02 Customers/<% customerName %>"
WHERE type = "meeting-internal" OR type = "meeting-external"
SORT date DESC
LIMIT 10
```

## Open tasks for this customer
```tasks
not done
([customer:: [[<% customerName %>]]]) OR (path includes "02 Customers/<% customerName %>")
group by status.name
```
```

### 3.7 Account Info — `T - Account Info.md`

```markdown
---
type: account-info
customer: "[[<% customerName %>]]"
tags: [account]
---
# <% customerName %> — Account Info

**Industry:**  
**Region:**  
**Primary contact:**  
**Renewal date:**  

## Stakeholders
| Name | Role | Notes |
|---|---|---|
|  |  |  |

## Tech stack
- 

## Background
```

### 3.8 Glossary — `T - Glossary.md`

```markdown
---
type: glossary
customer: "[[<% customerName %>]]"
tags: [glossary]
---
# <% customerName %> — Glossary

| Term | Definition |
|---|---|
|  |  |
```

### 3.9 Dates & Milestones — `T - Dates and Milestones.md`

```markdown
---
type: milestones
customer: "[[<% customerName %>]]"
tags: [milestones]
---
# <% customerName %> — Dates & Milestones

```dataview
TABLE date AS "When", note AS "What"
FROM "02 Customers/<% customerName %>"
WHERE milestones
FLATTEN milestones AS m
SORT m.date ASC
```

## Manual entries
- **2026-Q3** — contract renewal
- **2026-08-12** — go-live for [[Migration to v2]]
```

Milestones can also be expressed inline in any note as:

```markdown
> [!milestone] 2026-08-12 — Go-live for Migration v2
```

…and surfaced via Dataview by tagging the note with a `milestones:` frontmatter list, e.g.:

```yaml
milestones:
  - { date: 2026-08-12, note: "Go-live v2" }
  - { date: 2026-09-01, note: "Phase 2 kickoff" }
```

---

## 4. Task Model

### 4.1 Syntax

Use **Tasks plugin emoji syntax** for everything (it parses both emoji and dataview-style inline fields, so emoji is fine and renders compactly). Augment with **inline Dataview fields** for `customer::` and `stream::` so non-Tasks queries can also see them.

```markdown
- [ ] Send updated SOW to Acme legal 📅 2026-05-15 ⏫ [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]]
- [/] In-progress: drafting pricing model 📅 2026-05-12 [customer:: [[Acme Corp]]]
- [!] Blocked on Acme security review [customer:: [[Acme Corp]]] [stream:: [[SSO rollout]]]
- [w] Awaiting Acme to confirm cutover window [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]]
- [h] On hold until next quarter [customer:: [[Globex]]]
- [x] Sent intro email ✅ 2026-05-07 [customer:: [[Acme Corp]]]
```

### 4.2 Custom statuses (Tasks plugin settings)

Configure these under **Settings → Tasks → Status Types**:

| Symbol | Name | Type | Available next |
|---|---|---|---|
| ` ` | To Do | TODO | `/`, `!`, `w`, `h`, `x` |
| `/` | In Progress | IN_PROGRESS | `x`, `!`, `w`, `h` |
| `!` | Blocked | NON_TASK (treated as not-done) | ` `, `/`, `x` |
| `w` | Awaiting Customer | NON_TASK | ` `, `/`, `x` |
| `h` | On Hold | NON_TASK | ` `, `/`, `x` |
| `x` | Done | DONE | ` ` |

Why `!`, `w`, `h` are configured as NON_TASK rather than CANCELLED: it keeps them out of "done" but lets us filter them out of "active" cleanly, and Tasks' `not done` matches them as expected.

### 4.3 Aggregated query — Active tasks (`01 Tasks/Tasks - Active.md`)

```markdown
# Active Tasks

```tasks
not done
status.name includes To Do OR status.name includes In Progress
group by function task.file.frontmatter?.customer ?? "— No customer —"
sort by priority
sort by due
hide backlink
short mode
```
```

### 4.4 Aggregated query — Blocked tasks (`01 Tasks/Tasks - Blocked.md`)

```markdown
# Blocked Tasks

```tasks
not done
status.name includes Blocked
group by function task.file.frontmatter?.customer ?? "— No customer —"
sort by due
hide backlink
```
```

### 4.5 Awaiting / On Hold

Same shape, swapping `status.name includes Awaiting Customer` and `status.name includes On Hold`. These satisfy the spec's distinction that those statuses are *not* "active" and *not* "blocked" but should be visible somewhere.

### 4.6 Per-stream tasks (already shown in §3.5)

Inside a stream note, the Tasks query matches both:
- tasks defined in the stream note's body, and
- tasks elsewhere whose inline `stream:: [[<this stream>]]` points here.

### 4.7 Dataview cross-check

Because we *also* set `[customer:: …]` and `[stream:: …]` inline, Dataview can build alternative views (e.g., a table grouped by stream with task counts) without re-implementing the Tasks engine:

```dataview
TABLE WITHOUT ID
  file.link AS "Stream",
  status AS "Stream status",
  length(filter(file.tasks, (t) => !t.completed)) AS "Open tasks"
FROM "02 Customers"
WHERE type = "stream"
SORT status, file.name
```

---

## 5. Inbox Workflow & Auto-Move

### 5.1 The trigger: `done: true` in frontmatter

The single, uniform signal that a note is "ready to file" is `done: true`. Every template starts with `done: false`. When you finish working on the note you flip the flag (Properties view, hotkey, or a Templater "Mark done" button — see §5.4).

Why frontmatter rather than "all tasks checked" or a tag:

- It's an explicit human decision; doesn't accidentally fire while you're still drafting.
- It works for notes that contain *no* tasks (e.g., glossary updates).
- It's queryable (Dataview can show you "Inbox notes still in flight").

### 5.2 Destination resolution rule

Auto Note Mover decides destination based on `type` (and `customer` when relevant):

| `type` | Destination |
|---|---|
| `daily` | `03 General/Daily/{{YYYY}}/{{MM}}/` |
| `meeting-internal` | `02 Customers/{{customer}}/Internal Meetings/` |
| `meeting-external` | `02 Customers/{{customer}}/External Meetings/` |
| `stream` | `02 Customers/{{customer}}/Streams/` |
| `account-info` | `02 Customers/{{customer}}/Account Info/` |
| `glossary` | `02 Customers/{{customer}}/Account Info/` |
| `milestones` | `02 Customers/{{customer}}/Account Info/` |
| `customer` | `02 Customers/{{customer}}/` (rename to match folder) |
| `general` | `03 General/` |

### 5.3 Implementing it with Auto Note Mover

Auto Note Mover natively supports rules like *"if frontmatter `type` = X and `done` = true, move to folder Y."* For destinations that need to interpolate `customer` (which Auto Note Mover doesn't fully support), use a small **Templater "user function"** wired to a vault-level `on file modify` hook:

```js
// 05 Assets/Templates/scripts/inbox-router.js
module.exports = async (tp) => {
  const f = tp.config.target_file;
  const fm = app.metadataCache.getFileCache(f)?.frontmatter ?? {};
  if (!fm.done) return;
  if (!f.path.startsWith("00 Inbox/")) return;

  const customer = (fm.customer ?? "").replace(/\[\[|\]\]/g, "").trim();
  const map = {
    "daily":             `03 General/Daily/${window.moment(fm.date).format("YYYY/MM")}`,
    "meeting-internal":  `02 Customers/${customer}/Internal Meetings`,
    "meeting-external":  `02 Customers/${customer}/External Meetings`,
    "stream":            `02 Customers/${customer}/Streams`,
    "account-info":      `02 Customers/${customer}/Account Info`,
    "glossary":          `02 Customers/${customer}/Account Info`,
    "milestones":        `02 Customers/${customer}/Account Info`,
    "customer":          `02 Customers/${customer}`,
    "general":           `03 General`,
  };
  const dest = map[fm.type];
  if (!dest) return;
  await app.vault.adapter.mkdir(dest).catch(() => {});
  await app.fileManager.renameFile(f, `${dest}/${f.name}`);
};
```

Bind it under **Templater → User Scripts** and enable **Trigger Templater on new file creation / on modify**. This is the mechanism that satisfies the spec's "auto-move on done."

If you prefer to avoid scripting, you can get ~80% there with **only Auto Note Mover rules** by hard-coding rules per customer (one rule per `customer` value). The Templater script is preferable because it scales to N customers without N rules.

### 5.4 "Mark done" hotkey

Add a Templater command and bind a hotkey (e.g., ⌘⇧D):

```js
// Toggle done in current file's frontmatter, then save.
await app.fileManager.processFrontMatter(tp.file.find_tfile(tp.file.title), (fm) => {
  fm.done = true;
});
```

The router script then fires on the next save and moves the file out of `00 Inbox/`.

### 5.5 Inbox health dashboard

Sanity check that nothing rots in Inbox:

```dataview
TABLE type, customer, file.mtime AS "Last edited"
FROM "00 Inbox"
WHERE done != true
SORT file.mtime ASC
```

---

## 6. Daily Notes Automation

Use **core Daily Notes** plugin, configured as:

- **Date format:** `YYYY-MM-DD`
- **New file location:** `00 Inbox`
- **Template:** `05 Assets/Templates/T - Daily Note.md`

Generation each morning:

- **macOS:** the easiest reliable trigger is to have Obsidian launch at login (System Settings → Login Items) and enable **Daily Notes → Open daily note on startup**. The first launch of the day creates today's note in Inbox.
- **Belt-and-suspenders:** also enable the **Calendar** plugin so a missing day is one click away.
- **For full automation without opening Obsidian:** run a `launchd` job at, say, 06:00 that touches the file path `Vault/00 Inbox/$(date +%Y-%m-%d).md` with the template body. (Optional; only do this if you genuinely need the file to exist before you open Obsidian.)

The daily note's `done: false` keeps it pinned in Inbox during the day. When wrapping up, flip it to `done: true` and the router moves it to `03 General/Daily/YYYY/MM/`.

---

## 7. Customer State

### 7.1 Recommendation: state lives on the **Customer Index note** (`02 Customers/Acme Corp/Acme Corp.md`), in `state:` frontmatter.

Reasoning:

- **The Customer Index note is the canonical "this customer" object.** It's what `[[Acme Corp]]` resolves to and what every other note in the customer's folder links back to. Putting state there keeps state and identity together.
- **Account Info is a sub-document** that may be edited by templates or shared formats; mixing high-level lifecycle state with reference material muddies it.
- **Tags are the wrong tool for state.** Tags are additive and don't have a closed value set — easy to drift between `#active`, `#Active`, `#customer/active`. Frontmatter with a documented enum is enforceable.
- **Folder-based filtering (e.g., `02a Active/`, `02b On Hold/`) is tempting and rejected.** It forces filesystem moves on every state change, breaks links in some plugins, and entangles structure with state. State changes a lot more than identity does.

### 7.2 Filtering implementations

In a dashboard:

```dataview
TABLE state, csm, file.mtime AS "Last touched"
FROM "02 Customers"
WHERE type = "customer"
SORT state ASC, file.name ASC
```

Per-state pages — `04 Dashboards/Customers - Active.md`:

```dataview
LIST
FROM "02 Customers"
WHERE type = "customer" AND state = "Active"
SORT file.name ASC
```

A **Bases view** (`05 Assets/Bases/Customers.base`) gives a sortable, groupable spreadsheet over the same frontmatter without writing queries — recommended as the daily-driver UI for managing state. Group by `state`, show columns `state, csm, since, file.mtime`. Save board view grouped by `state` for a kanban-style view.

Sidebar filtering: combine the dashboards above with **Hotkeys for specific files** so ⌘⇧A opens "Customers - Active."

### 7.3 What to do with state changes

When you flip `state: Active → Inactive`:

- The customer disappears from the Active dashboard automatically.
- Their open tasks still surface in `01 Tasks/Tasks - Active.md` *unless* you add a filter (recommended):

```tasks
not done
status.name includes To Do OR status.name includes In Progress
filter by function {
  const c = task.file.frontmatter?.customer;
  if (!c) return true;
  const cf = app.metadataCache.getFirstLinkpathDest(c.replace(/\[\[|\]\]/g,""), task.file.path);
  const state = cf && app.metadataCache.getFileCache(cf)?.frontmatter?.state;
  return state !== "Inactive";
}
group by function task.file.frontmatter?.customer ?? "— No customer —"
```

This is the main payoff for putting state in frontmatter: dashboards across the vault can react to it.

---

## 8. Streams of Work

### 8.1 Representation

A stream = one note in `02 Customers/<Customer>/Streams/`, using `T - Stream of Work.md`. Frontmatter:

```yaml
type: stream
customer: "[[Acme Corp]]"
status: In Progress       # In Progress | Blocked | Done | Awaiting Customer | On Hold
started: 2026-04-01
target: 2026-08-12
done: false
tags: [stream]
```

### 8.2 Status tracking

- Edit `status:` from the Properties panel (or Bases board view by drag-and-drop).
- A stream's `status` is **independent** of its tasks' statuses (per the spec). A stream can be `In Progress` while it has only `Awaiting Customer` tasks; the dashboard surfaces that as a useful signal.
- When `status: Done` and you're ready to archive, set `done: true` — the router (§5.3) leaves it in `Streams/` (no rule for `done` on streams) but you can add an extra rule that moves it to `99 Archive/02 Customers/<Customer>/Streams/`.

### 8.3 Tasks surfaced from a stream note

Two complementary mechanisms (already shown in §3.5):

1. **Tasks defined inline** in the stream note — picked up by `path includes {{query.file.path}}`.
2. **Tasks defined elsewhere** that carry `[stream:: [[<this stream>]]]` — picked up by the second clause.

Together this means a meeting note can capture an action item assigned to a stream, and that task appears both in the meeting note and on the stream's "Open Tasks" list.

### 8.4 Cross-stream dashboard (`04 Dashboards/Streams - All.md`)

```dataview
TABLE
  customer AS "Customer",
  status AS "Status",
  started AS "Started",
  target AS "Target",
  length(filter(file.tasks, (t) => !t.completed)) AS "Open"
FROM "02 Customers"
WHERE type = "stream"
SORT status ASC, customer ASC
```

Optional Bases board view grouped by `status` for kanban.

---

## 9. Dashboards

### 9.1 `04 Dashboards/Home.md`

```markdown
# Home

## Today
![[<%+ tp.date.now("YYYY-MM-DD") %>]]

## In flight (Inbox)
```dataview
TABLE type, customer, file.mtime AS "Last edited"
FROM "00 Inbox"
WHERE done != true
SORT file.mtime ASC
```

## Top of mind tasks
```tasks
not done
(status.name includes To Do) OR (status.name includes In Progress)
sort by priority
sort by due
limit 15
short mode
```

## Customers
- 🟢 [[Customers - Active]]
- 🟡 [[Customers - On Hold]]
- 🟤 [[Customers - Temp]]
- ⚪ [[Customers - Inactive]]

## Streams in progress
```dataview
TABLE customer, status, target
FROM "02 Customers"
WHERE type = "stream" AND status = "In Progress"
SORT target ASC
```
```

### 9.2 `01 Tasks/Tasks - Active.md` and `Tasks - Blocked.md`

See §4.3 and §4.4.

### 9.3 `04 Dashboards/Customers - Active.md` (and one per state)

```markdown
# Active Customers

```dataview
TABLE state, csm, file.mtime AS "Last touched"
FROM "02 Customers"
WHERE type = "customer" AND state = "Active"
SORT file.name ASC
```

## Their open tasks
```tasks
not done
filter by function {
  const c = task.file.frontmatter?.customer;
  if (!c) return false;
  const cf = app.metadataCache.getFirstLinkpathDest(c.replace(/\[\[|\]\]/g,""), task.file.path);
  return app.metadataCache.getFileCache(cf)?.frontmatter?.state === "Active";
}
group by function task.file.frontmatter?.customer ?? "— No customer —"
sort by due
```
```

### 9.4 Per-customer overview

Already encoded as the Customer Index note (§3.6). It is itself a dashboard.

### 9.5 `04 Dashboards/Streams - All.md`

See §8.4.

### 9.6 Optional: Awaiting Customer / On Hold dashboards

Useful for weekly reviews. Same shape as the Active/Blocked queries with the appropriate `status.name includes …`.

---

## 10. Refinements & Open Questions

### 10.1 Refinements I recommend you adopt

1. **Adopt a meeting note naming convention:** `YYYY-MM-DD <Customer> - <Internal|External> - <Topic>.md`. Sortable, scannable, and survives Auto Note Mover.
2. **Use one stream-status enum across streams *and* tasks** (the current spec already does this implicitly). It removes mental overhead — a "Blocked" task and a "Blocked" stream mean the same shape of thing.
3. **Add a "Next action" field on streams** (`next: "Send pricing model to legal"` in frontmatter) and surface it on the customer overview. This is the one piece of metadata you will look at most often as a CSM/AE.
4. **Recurring tasks**: the Tasks plugin handles `🔁 every week` natively; use it for QBR prep, weekly check-ins, etc.
5. **Snoozed tasks**: model with `⏳ scheduled-date`. Filter them out of "Active" with `scheduled before in 7 days` if you want a "this week" view, or `is not scheduled OR scheduled before tomorrow` for "today."
6. **Archive policy**: when a customer flips to `state: Inactive` for >90 days, move their entire folder to `99 Archive/02 Customers/<Name>/`. Their backlinks still resolve (Obsidian uses note name, not path), and your dashboards stay fast.
7. **Linking convention**: always link customers and streams as `[[Acme Corp]]` and `[[Migration to v2]]` — never `[[02 Customers/Acme Corp/Acme Corp|Acme]]`. Note basenames must therefore be unique across the vault. Prefix stream names with the customer if there's any risk of collision: `Acme - Migration to v2`.
8. **Mobile**: Tasks plugin and Dataview both work on mobile but Templater scripts can be slow on first launch. Keep the router script short (it already is) and avoid heavy Dataview queries on the Home note (limit results, prefer Bases).
9. **Sync**: Obsidian Sync or iCloud both work. Avoid Dropbox for the vault root (`.obsidian/` write contention). If using iCloud, exclude `.obsidian/workspace.json` from sync to prevent device thrash.
10. **Search**: lean on `path:"02 Customers/Acme Corp"` and frontmatter search (`["state":"Active"]`) rather than tag soup.

### 10.2 Gaps in the current spec, with suggested resolutions

| Gap | Suggested resolution |
|---|---|
| Spec doesn't define task **due dates** or **priorities** | Adopt Tasks plugin's `📅 YYYY-MM-DD` (due), `⏳` (scheduled), `⏫/🔼/🔽` (priority). Surface due/priority in the Active dashboard. |
| No **recurring** task model | Tasks plugin `🔁 every week on Monday` handles it. |
| No **task ownership** | Add inline `[owner:: me]` / `[owner:: customer]` (already used in external meetings). |
| No **stream archival** path | When `status: Done` and `done: true`, route to `99 Archive/`. |
| No **glossary aggregation** across customers | Optional `04 Dashboards/Glossary.md` Dataview that lists all glossary entries, grouped by customer. Skip unless needed. |
| **Snoozed** notes (in Inbox but not actionable today) | Add `snooze:` frontmatter date; filter the Inbox dashboard with `WHERE done != true AND (snooze = null OR snooze <= date(today))`. |
| **Conflict** between `done: true` (note state) and a daily note's tasks remaining open | Acceptable: filing the daily note doesn't complete its tasks. Tasks plugin keeps tracking them via `path includes "03 General/Daily"`. |
| **Customer onboarding** is multi-step (folder + 4 notes + index) | Wrap it in a single QuickAdd macro: prompts for name, scaffolds folder + Customer Index + Account Info + Glossary + Dates and Milestones in one shot. |

### 10.3 Direct answer to "Open for Ideas: where customer state lives"

**Put `state:` on the Customer Index note** (`02 Customers/<Customer>/<Customer>.md`), not on Account Info. Three reasons:

1. The Customer Index *is* the customer object in the vault graph; state and identity belong together.
2. Account Info is reference material that you'll often paste/refresh from external sources (CRM exports). You don't want a CRM round-trip to clobber your lifecycle state.
3. Dashboards already need to read Customer Index notes for `csm`, `since`, etc. — one frontmatter read is cheaper and simpler than chasing through to a sub-note.

A useful side effect: a Bases view over `type: customer` becomes your customer pipeline view, with `state` as the kanban column.

---

## 11. Implementation Order

A linear checklist. Each step is independently verifiable.

1. **Create the folder skeleton** exactly as in §1 (empty folders are fine).
2. **Install plugins** from §2 in this order:
   1. Templater
   2. QuickAdd
   3. Tasks (configure custom statuses per §4.2)
   4. Dataview (enable JS queries and inline fields in settings)
   5. Auto Note Mover
   6. Calendar
   7. Hotkeys for specific files
   8. (Optional) Iconize, MetaEdit
3. **Configure core Daily Notes**: format `YYYY-MM-DD`, location `00 Inbox`, template path set after step 4.
4. **Author templates** in `05 Assets/Templates/` (§3). Start with: Daily, Internal Meeting, External Meeting, Stream of Work, Customer Index, Account Info, Glossary, Dates & Milestones.
5. **Add the router script** at `05 Assets/Templates/scripts/inbox-router.js` (§5.3). In Templater settings: enable user scripts, point to that folder, enable "Trigger Templater on new file creation" and on file modify.
6. **Configure Auto Note Mover** with at least the daily-note rule (`type: daily AND done: true → 03 General/Daily`) as a fallback if you skip the router script. Otherwise leave it inactive.
7. **Bind hotkeys** (Hotkeys for specific files): Home, Tasks Active, Tasks Blocked, Customers Active. Plus Templater hotkey for "Mark done" (§5.4) and QuickAdd hotkeys for "New Internal Meeting", "New External Meeting", "New Stream", "New Customer."
8. **Build QuickAdd macros** for the four create flows above. Each prompts for the necessary fields (customer, topic, stream) and instantiates the matching template into `00 Inbox/`.
9. **Create the dashboards** in `04 Dashboards/` and `01 Tasks/` (§9). Verify each query renders.
10. **Onboard your first customer** end-to-end:
    - Run "New Customer" QuickAdd → produces `02 Customers/Acme Corp/` with index, account info, glossary, milestones.
    - Set `state: Active` on the index note.
    - Verify `Customers - Active.md` lists Acme.
11. **Smoke-test the Inbox loop:**
    - Create an Internal Meeting via QuickAdd → confirm it lands in `00 Inbox/`.
    - Add a task `- [ ] Test 📅 tomorrow [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]]`.
    - Confirm the task appears in `01 Tasks/Tasks - Active.md` and on the stream note.
    - Flip `done: true` → confirm the file is moved to `02 Customers/Acme Corp/Internal Meetings/` and the task continues to surface.
12. **One week of usage**, then revisit:
    - Are any rules misfiring?
    - Is anything sitting in Inbox that shouldn't be?
    - Add Bases views for Customers and Streams once you've felt the friction of pure dashboards.
13. **Backup / sync**: enable Obsidian Sync (or your chosen alternative) and verify `.obsidian/plugins/` syncs across devices.
14. **Optional (later):** archival rule for `state: Inactive` customers; weekly review note (Periodic Notes); CRM export pipeline into Account Info.

---

### Appendix A — Example end-to-end file

`00 Inbox/2026-05-08 Acme Corp - External - Pricing review.md` immediately after creation:

```markdown
---
type: meeting-external
customer: "[[Acme Corp]]"
stream: "[[Migration to v2]]"
date: 2026-05-08
attendees: ["Jane Doe (Acme)", "John (Acme legal)", "me"]
done: false
tags: [meeting, external]
---
# 2026-05-08 Acme Corp — External: Pricing review

**Customer:** [[Acme Corp]]  
**Stream:** [[Migration to v2]]  

## Agenda
- Walk through pricing model v3
- Confirm cutover window

## Notes
- Acme prefers fixed annual; we proposed tiered.
- Legal will review redlines by Friday.

## Customer asks
- Send updated SOW reflecting fixed-annual option.

## Action items (ours)
- [ ] Send updated SOW 📅 2026-05-15 ⏫ [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]]
- [ ] Update pricing model to fixed-annual 📅 2026-05-12 [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]]

## Action items (theirs)
- [w] Acme legal to send redlines ⏳ 2026-05-15 [owner:: customer] [customer:: [[Acme Corp]]]
```

When marked `done: true`, the router moves it to `02 Customers/Acme Corp/External Meetings/2026-05-08 Acme Corp - External - Pricing review.md`. The two open tasks are visible on `01 Tasks/Tasks - Active.md`, on `[[Migration to v2]]`'s Open Tasks list, and on `[[Acme Corp]]`'s "Open tasks for this customer" section. The `[w]` task is visible on the Awaiting Customer dashboard but not Active or Blocked.
