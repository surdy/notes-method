# Obsidian Implementation Plan — Claude Opus 4.7 (Extra High Reasoning)

This plan turns the spec in `notes-method.md` into a concrete, opinionated Obsidian setup. Where the spec leaves a choice open, I pick one and explain why. The plan is sized for a customer-facing professional juggling 5–30 active customer relationships and dozens of streams of work; it should also hold up if the vault grows past several thousand notes.

The headline opinions, up front:

1. **Each customer is an entity, not just a folder.** Every customer folder contains one *Customer Index* note (`Customers/Acme/Acme.md`) that is the source of truth for `state`, owners, links, etc. Account Info is a sub‑document, not the entity itself.
2. **One source of truth for status per object**, always in YAML frontmatter, lower‑case kebab values (`active`, `in-progress`, `awaiting-customer`). Tags mirror state for sidebar filtering; queries use frontmatter.
3. **Tasks plugin (Schemar) is canonical for tasks**, with custom statuses for the five spec states. Dataview is canonical for *aggregating non‑task data* (notes, customers, streams). Bases is canonical for *Customer dashboards* because it gives a real grouped/filterable database view that survives a vault with hundreds of customers.
4. **Auto‑move on done is driven by frontmatter `archived: true` (or tag `#archive`) plus a Templater "Archive note" command bound to a hotkey.** Auto Note Mover handles the simple cases; the Templater command handles type‑aware routing (meeting → `Customers/<X>/External Meetings/`, daily → `General/Journal/YYYY/MM/`, etc.). I prefer the Templater command as the primary mechanism because it can compute destinations from frontmatter; Auto Note Mover is the fallback.
5. **Daily notes are created by Periodic Notes** (better than the core Daily Notes plugin), placed in `Inbox/Daily/YYYY-MM-DD.md`, and auto‑created on first Obsidian launch of the day. For true "07:00 every morning" generation, we add a tiny `launchd` job that touches the file via a CLI; details below.

---

## 1. Vault structure

Concrete folder layout. Names use Title Case for human folders and lower case for `assets/`. Customer folder names are the customer's exact display name; the Customer Index note inside it shares that exact name (so `[[Acme]]` resolves to the entity, not to a generic file).

```
VaultRoot/
├── Inbox/
│   ├── Daily/                              # auto-generated daily notes land here
│   │   └── 2026-05-08.md
│   └── 2026-05-08 - Quick capture about pricing.md
│
├── Tasks/                                  # aggregated dashboards (no source notes)
│   ├── Tasks - Active.md
│   ├── Tasks - Blocked & Waiting.md
│   ├── Tasks - By Customer.md
│   └── Tasks - This Week.md
│
├── Customers/
│   ├── Acme/
│   │   ├── Acme.md                         # Customer Index note (entity)
│   │   ├── Account Info/
│   │   │   ├── Acme - Account Info.md
│   │   │   ├── Acme - Glossary.md
│   │   │   └── Acme - Dates & Milestones.md
│   │   ├── External Meetings/
│   │   │   └── 2026-05-08 - Acme - QBR.md
│   │   ├── Internal Meetings/
│   │   │   └── 2026-05-07 - Acme - Internal sync.md
│   │   └── Streams/
│   │       ├── Acme - Migration to v3.md
│   │       └── Acme - SSO rollout.md
│   ├── Globex/
│   │   └── ... (same structure)
│   └── _Archive/                           # inactive customers moved here
│       └── Initech/
│
├── General/
│   ├── Journal/                            # daily notes after archive
│   │   └── 2026/05/2026-05-08.md
│   ├── Reading/
│   ├── Reference/
│   └── Personal/
│
├── Dashboards/
│   ├── Home.md                             # set as Homepage
│   ├── Customers.md                        # Bases view
│   ├── Streams.md                          # Bases view
│   └── Inbox Triage.md
│
└── Assets/
    ├── templates/
    │   ├── T - Daily Note.md
    │   ├── T - External Meeting.md
    │   ├── T - Internal Meeting.md
    │   ├── T - Customer Index.md
    │   ├── T - Account Info.md
    │   ├── T - Glossary.md
    │   ├── T - Dates & Milestones.md
    │   ├── T - Stream of Work.md
    │   └── T - Generic Note.md
    ├── scripts/                            # Templater user scripts (.js)
    │   ├── archive-note.js
    │   ├── new-customer.js
    │   ├── new-meeting.js
    │   └── new-stream.js
    ├── bases/                              # .base files for Bases plugin
    │   ├── customers.base
    │   └── streams.base
    └── data/                               # PDFs, images, attachments, CSVs
```

Conventions worth committing to early:

| Concern | Convention |
|---|---|
| Customer Index note name | Exactly the customer display name (e.g. `Acme.md`) |
| Meeting note name | `YYYY-MM-DD - <Customer> - <Topic>.md` |
| Stream note name | `<Customer> - <Stream>.md` |
| Daily note name | `YYYY-MM-DD.md` |
| Attachments | Set "Default location for new attachments" → `Assets/data/` |
| Folder for new notes | `Inbox/` (Obsidian setting: Files & Links → Default location for new notes) |
| Internal links | Always wikilinks `[[Acme]]`, never paths. Enable "Use [[Wikilinks]]" |
| New link format | "Shortest path when possible" so `[[Acme]]` keeps working when files move |

---

## 2. Plugins

Install order matters because some plugins depend on others (Tasks reads frontmatter that Templater writes, etc.).

### Required (core experience)

| Plugin | Author | Why |
|---|---|---|
| **Templater** | SilentVoid13 | Powerful templates + user scripts in JS. Drives "new meeting", "new customer", "archive note" commands. Strictly better than core Templates. |
| **Tasks** | Schemar (Clare Macrae) | Canonical task engine. Custom statuses, due dates, recurrence, query language. The whole task spec maps cleanly onto it. |
| **Dataview** | blacksmithgu | Query language for non‑task aggregation: lists of meetings, streams, tasks‑by‑project, etc. Everything that Tasks doesn't already do. |
| **Bases** | Obsidian core (1.9+) | Database views over frontmatter. The right tool for the Customers dashboard (grouped by state, filterable, sortable). Ship it because it's first‑party and won't rot. |
| **Periodic Notes** | liamcain | Daily/weekly/monthly notes with format strings, auto‑create on launch, separate template per period. Replaces the core Daily Notes plugin. |
| **Calendar** | liamcain | Sidebar calendar that creates/opens daily notes via Periodic Notes. Worth it for navigation alone. |
| **QuickAdd** | chhoumann | Command palette / hotkey scaffolding for "New customer", "New external meeting for…", "New stream for…". Wraps Templater scripts in friendly entry points. |
| **Auto Note Mover** | farux0 | Moves notes by tag or frontmatter rule. Used as the *fallback* archive mechanism for notes that don't pass through the Templater command. |
| **Homepage** | mirnovov | Opens `Dashboards/Home.md` on Obsidian launch. The Inbox‑zero workflow needs a daily landing pad. |
| **Linter** | Platers | Auto‑maintains `updated`, `created`, sorts frontmatter keys, normalises YAML. Critical for keeping queries reliable. |

### Strongly recommended (quality of life)

| Plugin | Author | Why |
|---|---|---|
| **Iconize** (formerly Obsidian Icon Folder) | FlorianWoelki | Per‑folder icons; makes the customer list scannable. |
| **File Explorer Note Count** | ozntel | Shows "(12)" beside `Inbox/`; instant Inbox‑zero feedback. |
| **Style Settings** | mgmeyers | Required by some of the above to expose tweakables. |
| **Advanced Tables** | tgrosinger | Auto‑formats markdown tables in account info / glossary. |

### Optional (consider later)

- **Projects** (marcusolsson) — Kanban/calendar/gallery views over frontmatter. Bases largely subsumes it now; only add if you want a true Kanban for streams.
- **Tag Wrangler** (pjeby) — bulk‑rename tags. Useful if you adopt tag‑based mirrors for state and want to refactor later.
- **Obsidian Git** (Vinzent03) — versioned backup if you don't already use iCloud/Sync; non‑negotiable on desktop‑only setups.
- **Dataview JS** capability — leave on; we'll use it sparingly for the Customers per‑customer overview to keep things tidy.

### What I deliberately *don't* recommend

- Kanban (mgmeyers) — overlaps with Bases/Projects and creates a parallel data model that's hard to query.
- Various "GTD" plugins (e.g. Reminder, Things‑style) — Tasks + Dataview cover the spec without locking you into another schema.

---

## 3. Templates

All templates live in `Assets/templates/` and are wired into Templater (Settings → Templater → Template folder location → `Assets/templates`). Set "Trigger Templater on new file creation" → on, with folder template mappings (below) so templates fire automatically.

Folder template mappings (Templater settings):

| Folder | Template |
|---|---|
| `Inbox/Daily` | `T - Daily Note.md` |
| `Inbox` | `T - Generic Note.md` |
| `Customers/*/External Meetings` | `T - External Meeting.md` |
| `Customers/*/Internal Meetings` | `T - Internal Meeting.md` |
| `Customers/*/Streams` | `T - Stream of Work.md` |
| `Customers/*/Account Info` | `T - Account Info.md` (override per‑file via QuickAdd) |

### 3.1 `T - Daily Note.md`

```markdown
---
type: daily
date: <% tp.date.now("YYYY-MM-DD") %>
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
status: open
archived: false
tags: [daily]
---

# <% tp.date.now("dddd, MMMM Do YYYY") %>

> [!tip] Daily flow
> 1. Triage Inbox → 0
> 2. Plan: 3 most important things
> 3. Capture meeting notes here only if they're not customer‑specific

## Top 3
- [ ] 
- [ ] 
- [ ] 

## Notes


## Captured tasks
<!-- Tasks created here without a [customer::] field stay personal. -->


## Links
- Yesterday: [[<% tp.date.yesterday("YYYY-MM-DD") %>]]
- Tomorrow: [[<% tp.date.tomorrow("YYYY-MM-DD") %>]]
```

### 3.2 `T - External Meeting.md`

```markdown
---
type: meeting
meeting-kind: external
customer: <% await tp.system.suggester(c => c, await tp.user.list_customers()) %>
stream: 
date: <% tp.date.now("YYYY-MM-DD") %>
attendees: []
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
status: open
archived: false
tags: [meeting, external]
---

# <% tp.date.now("YYYY-MM-DD") %> — [[<% tp.frontmatter.customer %>]] — <% tp.file.title.replace(/^\\d{4}-\\d{2}-\\d{2} - .*? - /, "") %>

**Customer:** [[<% tp.frontmatter.customer %>]]
**Stream:** <% tp.frontmatter.stream ? `[[${tp.frontmatter.stream}]]` : "_n/a_" %>

## Agenda

## Notes

## Decisions

## Action items
<!-- Use Tasks plugin syntax. customer:: and stream:: get inherited automatically by the dashboards. -->
- [ ] Example task [customer:: [[<% tp.frontmatter.customer %>]]] [stream:: [[<% tp.frontmatter.stream %>]]] 📅 <% tp.date.now("YYYY-MM-DD", 7) %>
```

### 3.3 `T - Internal Meeting.md`

Same as External Meeting but with `meeting-kind: internal`, tag `internal`, and no required `stream` (often these meetings span streams).

### 3.4 `T - Customer Index.md`

This is the canonical customer entity. State lives here.

```markdown
---
type: customer
customer: <% tp.file.title %>
state: active                  # active | on-hold | temp | inactive
tier:                          # optional: strategic | growth | smb
ae:                            # account exec
cse:                           # customer success engineer / your role
started: <% tp.date.now("YYYY-MM-DD") %>
renewal:
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
tags: [customer, customer/active]
---

# <% tp.file.title %>

> **State:** `=this.state`  ·  **Tier:** `=this.tier`  ·  **Renewal:** `=this.renewal`

## Account
- [[<% tp.file.title %> - Account Info]]
- [[<% tp.file.title %> - Glossary]]
- [[<% tp.file.title %> - Dates & Milestones]]

## Streams of work
```dataview
TABLE status, updated
FROM "Customers/<% tp.file.title %>/Streams"
WHERE type = "stream"
SORT status ASC, updated DESC
```

## Open tasks for this customer
```tasks
not done
(description includes [[<% tp.file.title %>]]) OR (path includes Customers/<% tp.file.title %>/)
group by status.name
sort by due
```

## Recent meetings
```dataview
TABLE meeting-kind AS "Kind", date, status
FROM "Customers/<% tp.file.title %>"
WHERE type = "meeting"
SORT date DESC
LIMIT 15
```
```

### 3.5 `T - Account Info.md`

```markdown
---
type: account-info
customer: <% tp.file.folder(true).split("/").slice(-2,-1)[0] %>
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
tags: [account-info]
---

# <% tp.frontmatter.customer %> — Account Info

## Stakeholders
| Name | Role | Email | Notes |
|------|------|-------|-------|

## Environment
- Region:
- Plan:
- Auth:

## Commercial
- ARR:
- Renewal date:
- Contract notes:

## Risks / Watchouts

## Links
- Index: [[<% tp.frontmatter.customer %>]]
- Glossary: [[<% tp.frontmatter.customer %> - Glossary]]
- Milestones: [[<% tp.frontmatter.customer %> - Dates & Milestones]]
```

### 3.6 `T - Glossary.md`

```markdown
---
type: glossary
customer: <% tp.file.folder(true).split("/").slice(-2,-1)[0] %>
tags: [glossary]
---

# <% tp.frontmatter.customer %> — Glossary

| Term | Meaning | Notes |
|------|---------|-------|
```

### 3.7 `T - Dates & Milestones.md`

```markdown
---
type: milestones
customer: <% tp.file.folder(true).split("/").slice(-2,-1)[0] %>
tags: [milestones]
---

# <% tp.frontmatter.customer %> — Dates & Milestones

```dataview
TABLE date, kind, status
WHERE customer = this.customer AND (type = "milestone" OR type = "stream")
SORT date ASC
```

## Manual milestones
- [ ] 2026-06-30 — Renewal kickoff 📅 2026-06-30 [customer:: [[<% tp.frontmatter.customer %>]]]
```

### 3.8 `T - Stream of Work.md`

```markdown
---
type: stream
customer: <% await tp.system.suggester(c => c, await tp.user.list_customers()) %>
stream: <% tp.file.title.replace(/^.*? - /, "") %>
status: in-progress            # in-progress | blocked | done | awaiting-customer | on-hold
priority: P2                   # P0 | P1 | P2 | P3
owner:
started: <% tp.date.now("YYYY-MM-DD") %>
target:
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: [stream]
---

# <% tp.frontmatter.customer %> — <% tp.frontmatter.stream %>

> **Customer:** [[<% tp.frontmatter.customer %>]]  ·  **Status:** `=this.status`  ·  **Target:** `=this.target`

## Goal

## Approach

## Open tasks
```tasks
not done
(stream includes [[<% tp.file.title %>]]) OR (path includes <% tp.file.path(true) %>)
group by status.name
sort by priority, due
```

## Decisions log
- 

## Meetings touching this stream
```dataview
LIST FROM "Customers/<% tp.frontmatter.customer %>"
WHERE type = "meeting" AND contains(file.outlinks, this.file.link)
SORT date DESC
```
```

### 3.9 `T - Generic Note.md` (Inbox default)

```markdown
---
type: note
customer:
stream:
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
status: open                 # open | done
archived: false
tags: []
---

# <% tp.file.title %>


```

`status: open → done` plus the archive command (Section 5) is what gets a generic Inbox note moved out.

---

## 4. Task model

### 4.1 Syntax

Use the Tasks plugin's emoji syntax for task‑level metadata, and inline Dataview fields (`[key:: value]`) for *cross‑cutting* metadata Tasks doesn't natively understand (customer, stream). Tasks indexes inline Dataview fields too, so a single line works for both engines.

```markdown
- [ ] Draft migration plan [customer:: [[Acme]]] [stream:: [[Acme - Migration to v3]]] 🔼 📅 2026-05-15 🛫 2026-05-10 🔁 every 2 weeks
```

Reading left to right: status checkbox, description, customer link, stream link, priority (🔼 = high), due date (📅), start date (🛫), recurrence (🔁).

### 4.2 Statuses (configure in Tasks → Settings → Task Statuses)

Map the spec's five statuses onto Tasks symbols. Tasks supports custom statuses with a "type" (`TODO`, `IN_PROGRESS`, `DONE`, `CANCELLED`, `NON_TASK`); we use `TODO` for everything not Done so all open work is queryable, and we filter on the *symbol* to separate the buckets.

| Symbol | Name | Spec status | Type | Next status |
|--------|------|-------------|------|-------------|
| `[ ]` | To Do | To Do | TODO | `/` |
| `[/]` | In Progress | (added — useful) | IN_PROGRESS | `x` |
| `[x]` | Done | Done | DONE | ` ` |
| `[b]` | Blocked | Blocked | TODO | ` ` |
| `[a]` | Awaiting Customer | Awaiting Customer | TODO | ` ` |
| `[h]` | On Hold | On Hold | TODO | ` ` |
| `[-]` | Cancelled | (added — useful) | CANCELLED | ` ` |

Reasoning: keeping Blocked/Awaiting/On Hold as `TODO`‑typed makes them visible in any "open" query you write later; we partition them with `status.symbol` filters in the dashboards.

The spec says active list shows "not done and not blocked". I read "blocked" liberally to also exclude Awaiting and On Hold, because the practical question for Inbox‑zero‑style review is *"what should I work on right now?"*. We surface those three in a separate "Blocked & Waiting" view, grouped by status. Confirm in Refinements (§10) if you'd rather keep Awaiting/On Hold inline.

### 4.3 Aggregated active tasks query

`Tasks/Tasks - Active.md`:

````markdown
# Active tasks

```tasks
not done
status.symbol does not include b
status.symbol does not include a
status.symbol does not include h
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by priority, due
hide backlink
short mode
```
````

To also show the linked stream next to each task (the spec asks for "associated project"), Dataview is more flexible than Tasks for the *display*; here's the same data via Dataview, which lets you render `customer` and `stream` columns:

````markdown
## By stream (Dataview)

```dataview
TASK
WHERE !completed AND !contains(text, "[b]") AND !contains(text, "[a]") AND !contains(text, "[h]")
WHERE status.symbol = " " OR status.symbol = "/"
GROUP BY stream
SORT priority DESC, due ASC
```
````

I recommend keeping the **Tasks** block as the canonical interactive view (you can click checkboxes to advance status) and the **Dataview** block as a read‑only "by stream" cross‑section.

### 4.4 Aggregated blocked / waiting tasks

`Tasks/Tasks - Blocked & Waiting.md`:

````markdown
# Blocked & waiting

```tasks
not done
(status.symbol includes b) OR (status.symbol includes a) OR (status.symbol includes h)
group by status.name
sort by due
```
````

### 4.5 Per‑customer task aggregation

`Tasks/Tasks - By Customer.md`:

````markdown
```tasks
not done
status.symbol does not include b
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by due
```
````

### 4.6 Linking to the stream

The spec requires "aggregated tasks should link to the stream note." Two mechanisms together guarantee this:

1. The task line itself contains `[stream:: [[Stream Name]]]`, which renders as a clickable link in both Tasks and Dataview output.
2. The Tasks plugin's *backlink* (the source file) is shown by default. If the task lives inside the stream note already, the backlink *is* the stream link.

When tasks are captured in a meeting note, item 1 ensures the link survives.

---

## 5. Inbox workflow & auto‑move

The spec wants notes to leave Inbox automatically "once done." This is the trickiest part because "done" semantics differ by note type. My design:

### 5.1 The trigger

A note is considered "done" when **either**:

- `archived: true` is set in frontmatter, **or**
- the tag `#archive` is present anywhere in the note.

This is intentional dual‑trigger: power users prefer frontmatter, but tag is faster to add inline. Both are picked up by Auto Note Mover.

`status: done` alone is *not* the move trigger. Reason: I want a moment between "I closed this thread" and "ship it to long‑term storage" so I can review. `archived: true` is the explicit move signal. Daily notes are an exception (see 5.4).

### 5.2 The destination resolver

Routing rules, in priority order:

| When the note has… | Destination |
|---|---|
| `type: meeting` AND `meeting-kind: external` AND `customer: X` | `Customers/X/External Meetings/` |
| `type: meeting` AND `meeting-kind: internal` AND `customer: X` | `Customers/X/Internal Meetings/` |
| `type: stream` AND `customer: X` | `Customers/X/Streams/` |
| `type: account-info` AND `customer: X` | `Customers/X/Account Info/` |
| `type: daily` | `General/Journal/YYYY/MM/` (year/month from `date`) |
| `type: note` AND `customer: X` AND `stream: S` | `Customers/X/Streams/` (filed alongside `S`) |
| `type: note` AND `customer: X` (no stream) | `Customers/X/` (root of customer folder) |
| `type: note`, no customer | `General/Reference/` |

### 5.3 The mechanism — primary: Templater "Archive note" command

A Templater user script that reads frontmatter, computes the target folder, and uses Obsidian's API to move the file. Bound to `⌘⇧A`.

`Assets/scripts/archive-note.js`:

```js
// Templater user script. Bind: a Templater "Archive note" command,
// then assign hotkey ⌘⇧A in Settings → Hotkeys.
module.exports = async (tp) => {
  const file = tp.config.target_file;
  const fm = app.metadataCache.getFileCache(file)?.frontmatter ?? {};
  const customer = fm.customer;
  const type = fm.type;
  const meetingKind = fm["meeting-kind"];
  const date = fm.date ?? tp.date.now("YYYY-MM-DD");
  const [year, month] = date.split("-");

  let dest;
  if (type === "meeting" && meetingKind === "external" && customer) {
    dest = `Customers/${customer}/External Meetings`;
  } else if (type === "meeting" && meetingKind === "internal" && customer) {
    dest = `Customers/${customer}/Internal Meetings`;
  } else if (type === "stream" && customer) {
    dest = `Customers/${customer}/Streams`;
  } else if (type === "account-info" && customer) {
    dest = `Customers/${customer}/Account Info`;
  } else if (type === "daily") {
    dest = `General/Journal/${year}/${month}`;
  } else if (customer && fm.stream) {
    dest = `Customers/${customer}/Streams`;
  } else if (customer) {
    dest = `Customers/${customer}`;
  } else {
    dest = `General/Reference`;
  }

  // Ensure folder exists
  if (!app.vault.getAbstractFileByPath(dest)) {
    await app.vault.createFolder(dest).catch(() => {});
  }

  // Stamp archive metadata before move (preserved across rename)
  await app.fileManager.processFrontMatter(file, fm2 => {
    fm2.archived = true;
    fm2["archived-at"] = tp.date.now("YYYY-MM-DD HH:mm");
    if (fm2.status === "open") fm2.status = "done";
  });

  const newPath = `${dest}/${file.name}`;
  await app.fileManager.renameFile(file, newPath);
  new Notice(`Archived → ${newPath}`);
};
```

Wire it up: Templater Settings → User script files folder → `Assets/scripts`. Then add a "User defined commands" entry pointing to `archive-note`. Then assign hotkey.

### 5.4 The mechanism — fallback: Auto Note Mover

For notes archived via tag (`#archive`) or for daily notes that you want to move automatically the next morning, configure Auto Note Mover rules. Settings → Auto Note Mover → Add rule:

| Trigger | Folder |
|---|---|
| Tag `#archive` AND tag `#meeting` AND tag `#external` | (cannot template per‑customer; route to `_Archive/Meetings/External` and rely on the Templater command for customer routing) |
| Frontmatter `type: daily` AND frontmatter `archived: true` | `General/Journal` |
| Tag `#archive` (no other rule matched) | `_Archive/Inbox` |

Auto Note Mover doesn't support frontmatter‑interpolated destinations, which is exactly why the Templater command is primary. Auto Note Mover catches what the user forgets to archive deliberately.

### 5.5 The "I'm done" muscle memory

The full flow:

1. While in any Inbox note, hit `⌘⇧A`.
2. Templater stamps `archived: true`, sets `status: done`, moves the file to its computed destination.
3. Inbox count (visible via File Explorer Note Count) decreases. Inbox‑zero achieved by repetition.

If you'd rather the trigger be flipping `status: done`, change the script to fire from a frontmatter‑change watcher — but I recommend the explicit hotkey because it makes archival a deliberate act, not an accidental side effect of marking a checkbox.

---

## 6. Daily notes automation

### 6.1 Periodic Notes configuration

Settings → Periodic Notes → Daily Notes:

- Format: `YYYY-MM-DD`
- Folder: `Inbox/Daily`
- Template: `Assets/templates/T - Daily Note.md`
- Open daily note on startup: **on**

This guarantees that the *first* time Obsidian launches each day, the daily note is created in `Inbox/Daily/YYYY-MM-DD.md`. For most CSE/AE workflows that's enough — you'll open Obsidian within a couple of hours of starting the day.

### 6.2 True morning generation (optional)

If you genuinely want the file to exist at, say, 06:30 regardless of whether you've opened Obsidian, add a `launchd` job on macOS that creates the empty file (Obsidian will auto‑apply the folder template when opened). Avoid running Obsidian via the CLI just for this; it's brittle.

`~/Library/LaunchAgents/com.user.daily-note.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.user.daily-note</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>-c</string>
    <string>d=$(date +%Y-%m-%d); f="$HOME/ObsidianVault/Inbox/Daily/$d.md"; [ -f "$f" ] || touch "$f"</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key><integer>6</integer>
    <key>Minute</key><integer>30</integer>
  </dict>
</dict>
</plist>
```

`launchctl load ~/Library/LaunchAgents/com.user.daily-note.plist`. When you next open Obsidian, Periodic Notes (or the folder template mapping in Templater) will hydrate the file with the daily template.

### 6.3 Daily note archival

Daily notes accumulate fast. Two options:

- **Manual:** hit `⌘⇧A` at end of day → moves to `General/Journal/YYYY/MM/`.
- **Automatic:** Auto Note Mover rule on `type: daily` + `archived: true`, plus a small QuickAdd "End the day" macro that sets `archived: true` and runs the linter.

I recommend manual. The friction is what drives the "did I capture everything?" review.

---

## 7. Customer state — recommendation and why

### 7.1 Where state lives

**Recommendation: frontmatter `state:` on the Customer Index note (`Customers/Acme/Acme.md`)** — *not* on the Account Info note.

Reasoning:

- The customer is the *entity*; the index note is its record. Account Info is a sub‑document about commercials, stakeholders, environment — semantically a different thing. If state lives on Account Info, you'll keep needing to remember which note answers "who are my active customers?".
- The index note's name equals the customer name, so `[[Acme]]` resolves to the entity. Bases groups cleanly by `state` from a single source.
- Tasks and Dataview can pull `state` from the index note via `customer = "Acme"` joins, so we get a single source of truth.

I rejected three alternatives:

| Alternative | Why not |
|---|---|
| `state:` on Account Info note | Semantic mismatch. Multiple sub‑documents per customer; you'll forget which is canonical. |
| Tag `#customer/active` only | Tags can't carry the renewal date, AE, tier; you'll end up needing frontmatter anyway. Renaming tags is harder than editing frontmatter. |
| Folder name prefix (`_active/Acme`) | Wrecks links every time state changes. |

### 7.2 But also use a *mirror* tag

Add `customer/active` to the index note's `tags:` array, kept in sync by the linter or by a Templater hook on state change. This gives you free sidebar filtering via the core Tag pane and the search syntax `tag:#customer/active`. It's redundant with frontmatter, but tags give Obsidian's UI affordances frontmatter doesn't.

### 7.3 Filtering by state — Dataview

```dataview
TABLE state, tier, renewal
FROM "Customers"
WHERE type = "customer" AND state = "active"
SORT customer ASC
```

### 7.4 Filtering by state — Bases (recommended for the Customer dashboard)

`Assets/bases/customers.base`:

```yaml
filters:
  and:
    - file.folder.startsWith("Customers/")
    - type == "customer"
views:
  - type: table
    name: All customers
    order: [customer, state, tier, renewal, ae, updated]
    sort:
      - { property: state, direction: asc }
      - { property: customer, direction: asc }
    group_by: state
  - type: table
    name: Active only
    filters:
      and:
        - state == "active"
    order: [customer, tier, renewal, ae, updated]
  - type: cards
    name: Cards by state
    group_by: state
```

Embed in `Dashboards/Customers.md`:

````markdown
```base
file: Assets/bases/customers.base
view: All customers
```
````

### 7.5 Changing state

Just edit the frontmatter on the index note. Bonus: a QuickAdd macro "Change customer state" that prompts for one of the four values and writes it via Templater — keeps the value in lock‑step with the mirror tag.

---

## 8. Streams of work

### 8.1 Representation

One note per stream at `Customers/<X>/Streams/<X> - <Stream>.md`, created from `T - Stream of Work.md`. Frontmatter:

```yaml
type: stream
customer: Acme
stream: Migration to v3
status: in-progress     # in-progress | blocked | done | awaiting-customer | on-hold
priority: P1
owner: surdy
started: 2026-04-01
target: 2026-07-31
archived: false
```

### 8.2 Status tracking

Same single‑source‑of‑truth principle as customer state: status lives in the stream note's frontmatter. The customer index, the streams Bases view, and any Dataview dashboard all read it from there.

When a stream is `done`, set `archived: true` and use `⌘⇧A` to move it under `Customers/<X>/_Archive/Streams/` (the script appends `/_Archive` if it sees `archived: true` AND `type: stream` AND `status: done` — minor extension, easy to add).

### 8.3 Surfacing tasks from inside the stream note

The `T - Stream of Work.md` template includes a Tasks block that surfaces all tasks tagged with this stream regardless of where they were captured (meeting note, daily note, the stream note itself):

````markdown
```tasks
not done
(stream includes [[Acme - Migration to v3]]) OR (path includes Customers/Acme/Streams/Acme - Migration to v3.md)
group by status.name
sort by priority, due
```
````

The double condition catches tasks linked via inline field *and* tasks written directly inside the stream note (which is common when planning the stream itself).

### 8.4 Streams dashboard

`Assets/bases/streams.base`:

```yaml
filters:
  and:
    - type == "stream"
views:
  - type: table
    name: Active streams
    filters:
      and:
        - status != "done"
        - archived != true
    order: [customer, stream, status, priority, target, owner]
    sort:
      - { property: priority, direction: asc }
      - { property: target, direction: asc }
    group_by: customer
  - type: table
    name: Awaiting customer
    filters:
      and:
        - status == "awaiting-customer"
    order: [customer, stream, target, owner, updated]
```

---

## 9. Dashboards

### 9.1 `Dashboards/Home.md` (set as Homepage)

````markdown
# Home

> [!info] Inbox: `$=dv.pages('"Inbox"').where(p => !p.archived).length` items

## Today
- [[<% tp.date.now("YYYY-MM-DD") %>|Open today's daily note]]
- [Triage Inbox](Dashboards/Inbox%20Triage.md)

## Top‑of‑mind tasks
```tasks
not done
status.symbol does not include b
status.symbol does not include a
status.symbol does not include h
(due before in 7 days) OR (priority is above medium)
limit 15
sort by due
short mode
```

## Active customers
```dataview
LIST FROM "Customers"
WHERE type = "customer" AND state = "active"
SORT customer ASC
```

## Streams in progress
```dataview
TABLE customer, status, target
FROM "Customers"
WHERE type = "stream" AND status = "in-progress"
SORT target ASC
```

## Waiting on customer
```tasks
not done
status.symbol includes a
group by function task.file.frontmatter?.customer ?? "(none)"
short mode
```
````

### 9.2 `Dashboards/Inbox Triage.md`

````markdown
# Inbox triage

## Inbox notes (oldest first)
```dataview
TABLE type, customer, status, file.cday AS created
FROM "Inbox"
WHERE !archived
SORT file.cday ASC
```

## Inbox tasks not yet routed
```tasks
not done
path includes Inbox/
group by path
```
````

### 9.3 `Dashboards/Customers.md`

````markdown
# Customers

```base
file: Assets/bases/customers.base
view: All customers
```

## Customers needing attention (no meetings in 30d)
```dataview
TABLE customer, state, last_seen
FROM "Customers"
FLATTEN dateformat(file.mtime, "yyyy-MM-dd") AS last_seen
WHERE type = "customer" AND state = "active"
WHERE !any(file.inlinks, l => l.type = "meeting" AND date(l.date) >= date(today) - dur(30 days))
SORT last_seen ASC
```
````

### 9.4 `Dashboards/Streams.md`

````markdown
# Streams of work

```base
file: Assets/bases/streams.base
view: Active streams
```

## Blocked streams
```dataview
TABLE customer, target, updated
FROM "Customers"
WHERE type = "stream" AND status = "blocked"
SORT updated DESC
```
````

### 9.5 Per‑customer overview

Already baked into `T - Customer Index.md` (Section 3.4). Every customer's index note is itself a dashboard.

---

## 10. Refinements & open questions

### 10.1 Direct answer to "where does customer state live?"

See §7. Short version: **frontmatter `state:` on the Customer Index note**, with a `customer/<state>` tag mirrored for sidebar filtering. Don't put it on Account Info.

### 10.2 Refinements I'd push on

1. **Add `In Progress` and `Cancelled` task statuses.** The spec lists five but omits both. "In Progress" is hugely useful for "what am I actually mid‑flight on?" surfacing; "Cancelled" lets you remove a task without losing the audit trail.
2. **Treat Awaiting Customer + On Hold as "waiting" in dashboards.** The spec is silent. Bundle them into the Blocked & Waiting view, grouped by status, rather than letting them clutter Active.
3. **Use `📅 due`, `🛫 start`, `⏳ scheduled` consistently.** Without due dates, the active task list is just a long undifferentiated heap. Make it a habit to set at least a `📅` on anything that should appear in "this week."
4. **Adopt recurring tasks (`🔁`) for standing customer rituals.** QBR prep, weekly status email, renewal kickoff at T‑90.
5. **Snoozing** isn't in the spec but matters. Use Tasks' `🛫 start` date — tasks with a `🛫` in the future are excluded from `is starting` queries. A `Tasks/Tasks - Snoozed.md` view surfaces them.
6. **Linking convention.** Always wikilink the customer (`[[Acme]]`) and the stream (`[[Acme - Migration to v3]]`) the first time they appear in any note. This ensures graph and Dataview joins work without you thinking about them. Enable "Automatically update internal links" so renames don't shred your graph.
7. **Naming convention** for meeting notes (`YYYY-MM-DD - <Customer> - <Topic>.md`) gives chronological sort *and* customer context in any flat list. Enforce via a QuickAdd "New external meeting" prompt that builds the filename for you.
8. **Archiving customers**. When a customer goes `inactive`, move the entire customer folder to `Customers/_Archive/<X>/`. Easiest way: a Templater "Archive customer" command that calls `app.vault.rename(folder, ...)`. The mirror tag becomes `customer/inactive`; queries already filter on it.
9. **Mobile.** The whole stack works on Obsidian Mobile *except*: launchd job (Mac‑only), Auto Note Mover (works), Templater user scripts (work, but slower). Bind the archive command to a sidebar pin so it's reachable on iOS.
10. **Sync.** Use Obsidian Sync if you want end‑to‑end encryption; otherwise iCloud Drive on Mac/iOS is acceptable for personal use. **Don't sync `.obsidian/workspace.json`** — it'll thrash. Set Settings → Sync → Sync workspace → off.
11. **Backup.** Add Obsidian Git on desktop to a private repo. Even with Sync, you want history.
12. **Linter cadence.** Set Linter to run on save AND on file rename, with rules: insert frontmatter created/updated, sort YAML, capitalize headings off (preserve), trim trailing whitespace.
13. **Search.** Build a few saved searches: `path:Inbox -tag:#archive`, `tag:#customer/active`, `["status":"blocked"]`. Pin them to the Bookmarks pane.
14. **Daily note retention.** Don't archive daily notes the same day. Wait for end‑of‑week sweep. Reason: you'll often re‑open yesterday's note.
15. **Naming the customer index note** the same as the folder is non‑negotiable for `[[Acme]]` resolution. If two customers share a name, disambiguate the folder *and* the file (`Acme - US`, `Acme - EU`).

### 10.3 Gaps the spec doesn't address — flag and fill

| Gap | Recommendation |
|---|---|
| Task due dates | Tasks plugin's `📅 YYYY-MM-DD`. Dashboards group/sort by it. |
| Recurring tasks | Tasks plugin's `🔁 every N days/weeks/months`. |
| Snooze / start later | `🛫 YYYY-MM-DD`. Filter out from active with `is starting`. |
| Customer renewal tracking | `renewal:` on Customer Index; surface in Home dashboard at T‑90. |
| Confidential customer info | Don't put secrets in the vault. If you must, use a separate encrypted vault and link via plain text references. |
| Attachments grow large | `Assets/data/`, plus periodic prune. Don't sync via Git. |
| Note lifecycle for "General" notes | Same archive command — `type: note` + no customer routes to `General/Reference/`. |
| What if I forget to set `customer:`? | Templater for meeting/stream templates *requires* the picker (`tp.system.suggester` is awaitable and blocks file creation until selected). For generic notes, the archive command falls back to `General/Reference/`. |
| Customer folder template | A QuickAdd macro "New customer" should `mkdir` the four sub‑folders, drop in the index note + three account info notes, prefilled. Without it, you'll forget one. |
| Multiple roles per customer | `tags: [customer/active]` is single‑state by definition; if you want secondary axes (e.g. `priority/strategic`), add separate frontmatter keys (`tier:`), don't overload `state`. |

### 10.4 Risks

- **Plugin churn.** Bases is core; Tasks/Dataview/Templater/Periodic Notes are mature. Auto Note Mover is smaller — if it's ever orphaned, the Templater archive command alone covers the spec.
- **Dataview vs Bases overlap.** Don't try to express the same view in both. Rule of thumb: tabular customer/stream metadata → Bases; free‑form joins (tasks crossed with frontmatter) → Dataview/Tasks.
- **Frontmatter drift.** The Linter and the templates together prevent it. Don't hand‑edit YAML once the templates exist.

---

## 11. Implementation order

A one‑hour bootstrap, then incremental fill‑in.

### Phase 0 — vault & settings (10 min)
1. Create vault rooted at `~/ObsidianVault` (or wherever).
2. Settings → Files & Links: default new note location → `Inbox`; default attachment location → `Assets/data`; new link format → "Shortest path when possible"; use wikilinks → on; auto‑update internal links → on.
3. Create the folder skeleton from §1 (just the top level + `Assets/templates`, `Assets/scripts`, `Assets/bases`, `Inbox/Daily`).

### Phase 1 — core plugins (10 min)
4. Install: Templater, Tasks, Dataview, Periodic Notes, Calendar, QuickAdd, Auto Note Mover, Homepage, Linter. Enable Bases (core).
5. Templater: template folder `Assets/templates`, user script folder `Assets/scripts`, trigger on file creation → on, folder mappings per §3.
6. Tasks: configure custom statuses per §4.2.
7. Dataview: enable inline queries, JS queries, and `Use JavaScript Queries` (for the Inbox count snippet on Home).
8. Periodic Notes: daily folder `Inbox/Daily`, format `YYYY-MM-DD`, template `Assets/templates/T - Daily Note.md`, open on startup.
9. Linter: enable insert created/updated, YAML sort.

### Phase 2 — templates (20 min)
10. Create all nine templates from §3.
11. Create `Assets/scripts/archive-note.js` (§5.3).
12. Templater → User defined commands → add `archive-note`. Settings → Hotkeys → bind `⌘⇧A`.

### Phase 3 — QuickAdd macros (10 min)
13. Add macros: `New external meeting`, `New internal meeting`, `New stream`, `New customer`, `Archive note` (proxies the Templater command), `Change customer state`. Each prompts for the customer (and stream where relevant), then runs the matching template.

### Phase 4 — dashboards (15 min)
14. Create `Dashboards/Home.md`, `Inbox Triage.md`, `Customers.md`, `Streams.md`, plus the two `Tasks/` views.
15. Create the two `.base` files in `Assets/bases/`.
16. Set Homepage plugin to open `Dashboards/Home.md`.

### Phase 5 — first customer (5 min)
17. Run the `New customer` macro for one real customer. Confirm folder structure, index note, account info notes are created.
18. Capture one external meeting via the `New external meeting` macro. Add a task with `[customer:: [[X]]] [stream:: [[…]]] 📅 next-week`.
19. Verify it appears in `Tasks/Tasks - Active.md` and on `Dashboards/Home.md`.

### Phase 6 — daily routine (ongoing)
20. Each morning: open Obsidian → daily note auto‑created → triage Inbox → top 3 written → work the day.
21. Each evening: hit `⌘⇧A` on every Inbox note that's done.
22. Each Friday: review Blocked & Waiting, close out finished streams, set state changes on customers.

### Phase 7 — optional (later)
23. Add `launchd` morning daily‑note job (§6.2).
24. Install Iconize for customer folder icons.
25. Install Obsidian Git for backup.
26. Add an "Archive customer" Templater command for inactivation.

---

## Appendix A — full Tasks plugin status configuration (paste into settings)

```
- [ ]    To Do                   TODO          → /
- [/]    In Progress             IN_PROGRESS   → x
- [x]    Done                    DONE          →
- [b]    Blocked                 TODO          →
- [a]    Awaiting Customer       TODO          →
- [h]    On Hold                 TODO          →
- [-]    Cancelled               CANCELLED     →
```

## Appendix B — minimal "list_customers" Templater user function (referenced by templates)

`Assets/scripts/list-customers.js`:

```js
module.exports = async () => {
  const folder = app.vault.getAbstractFileByPath("Customers");
  if (!folder || !folder.children) return [];
  return folder.children
    .filter(f => f.children) // directories only
    .filter(f => !f.name.startsWith("_")) // skip _Archive
    .map(f => f.name)
    .sort();
};
```

Register as user function `list_customers` in Templater settings; templates reference it as `await tp.user.list_customers()`.

## Appendix C — daily routine cheatsheet

| When | Action | Hotkey / command |
|---|---|---|
| Morning | Open vault → review daily note | (Homepage opens it) |
| Capture | New thought → `Inbox/` generic note | `⌘N` |
| New meeting | QuickAdd → New external meeting | `⌘P` then "New external meeting" |
| New stream | QuickAdd → New stream | `⌘P` then "New stream" |
| Mark task blocked | Right‑click checkbox → choose status, or type `[b]` | — |
| Done with note | Archive | `⌘⇧A` |
| End of day | Sweep Inbox → 0 | repeated `⌘⇧A` |
| Friday review | Walk Blocked & Waiting + Streams dashboards | bookmarks pane |
