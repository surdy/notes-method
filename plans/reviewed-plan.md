# Reviewed Implementation Plan

> **How this plan was reviewed:** the user walked through the 12 decision points from `comparison-and-recommendations.md` interactively and answered each one. The decision log in §1 records the user's choice for each. Where the user diverged from the recommendation in `comparison-and-recommendations.md`, this plan reflects the user's choice.

This is the single-document blueprint for implementing the notes method in Obsidian. `notes-method.md` remains the spec; the three model plans remain the raw inputs; `comparison-and-recommendations.md` remains the analysis.

---

## 1. Decision log

| # | Decision point | Choice | vs. recommendation? |
|---|---|---|---|
| 1 | Folder layout | **Plain names** (`Inbox/`, `Tasks/`, `Customers/`, `General/`, `Dashboards/`, `Assets/`, `Archive/`) | Diverges (rec: numeric prefixes) |
| 2 | Daily notes plugin | **Periodic Notes + Calendar + external automation** (e.g. macOS `launchd`) for true morning generation | Extends rec (rec was startup-trigger only) |
| 3 | Inbox auto-move trigger | **Explicit hotkey `⌘⇧A`** → Templater `archive-note.js` | Matches rec |
| 4 | Task status palette | **7 statuses**: `[ ]` To Do, `[/]` In Progress, `[b]` Blocked, `[w]` Awaiting Customer, `[h]` On Hold, `[x]` Done, `[-]` Cancelled | Matches rec (with `[w]` instead of `[a]` for Awaiting) |
| 5 | Task encoding redundancy | **Symbol only** — no `[task_status::]` mirror field | Matches rec |
| 6 | Tabular dashboards | **Dataview only** — skip Bases entirely | Diverges (rec: Bases for Customers/Streams) |
| 7 | Customer state location | **Frontmatter `state:` on Customer Index note only** (no mirror tag) | Diverges (rec included a mirror tag) |
| 8 | Awaiting/On Hold in Active list | **Exclude all three** from Active; **separate aggregation view per status** (Blocked, Awaiting Customer, On Hold) | Extends rec (rec used a single combined "Blocked & Waiting" view) |
| 9 | Daily note retention | **Archive same-day with `⌘⇧A`**. Daily note template includes prev/next day navigation links | Diverges (rec: keep in Inbox during week, sweep weekly) |
| 10 | Stream archival path | **Leave done streams in place** at `Customers/<X>/Streams/`; filter by `status: Done` in dashboards | Diverges (rec: move to `Archive/`) |
| 11 | Meeting note naming | `YYYY-MM-DD - <Customer> - <Internal\|External> - <Topic>.md` | Matches rec |
| 12 | Stream "Next action" field | **Skip** — implicit in top open task | Diverges (rec: adopt `next:` frontmatter) |

---

## 2. Vault structure

```
Inbox/
  Daily/
    2026-05-08.md
Tasks/
  Tasks - Active.md
  Tasks - Blocked.md
  Tasks - Awaiting Customer.md
  Tasks - On Hold.md
  Tasks - By Customer.md
Customers/
  Acme Corp/
    Acme Corp.md                       ← Customer Index (state: lives here)
    Account Info/
      Account Info.md
      Glossary.md
      Dates and Milestones.md
    Internal Meetings/
    External Meetings/
    Streams/
      Migration to v2.md
General/
  Journal/                              ← daily notes after archive
    2026/05/
Dashboards/
  Home.md                               ← Homepage opens this
  Inbox Triage.md
  Customers.md
  Streams.md
Assets/
  templates/
    T - Daily Note.md
    T - Internal Meeting.md
    T - External Meeting.md
    T - Customer Index.md
    T - Account Info.md
    T - Glossary.md
    T - Dates and Milestones.md
    T - Stream of Work.md
    T - Generic Note.md
  scripts/
    archive-note.js
    list-customers.js
  data/                                 ← attachments default location
Archive/
  Customers/                            ← inactive customers (folders moved here)
  Inbox/                                ← Auto Note Mover catch-all
```

Note: with plain folder names, in Obsidian's file explorer the order will be alphabetical: `Archive`, `Assets`, `Customers`, `Dashboards`, `General`, `Inbox`, `Tasks`. To pin Inbox/Home/Tasks at the top of your daily attention, rely on **Bookmarks** (Settings → Core plugins → Bookmarks: pin `Dashboards/Home.md`, `Inbox/`, `Tasks/Tasks - Active.md`) and **Hotkeys for specific files** (`⌘1` … `⌘5`).

Naming conventions:

- Customer Index note: same name as folder → `Customers/Acme Corp/Acme Corp.md`. Guarantees `[[Acme Corp]]` resolves to the entity.
- Meeting note: `YYYY-MM-DD - <Customer> - <Internal|External> - <Topic>.md`.
- Stream note: human-readable name; prefix with customer only if a name collision is possible.
- Daily note: `YYYY-MM-DD.md`.
- Default attachment location (Settings → Files & Links): `Assets/data/`.
- Default new note location: `Inbox/`.
- New link format: "Shortest path when possible".

---

## 3. Plugins

Install in this order. Required = the system doesn't work without it. Recommended = adopt unless you have a reason not to. Optional = add when you feel the friction.

| Plugin | Author | Status |
|---|---|---|
| Templater | SilentVoid13 | **Required** |
| Tasks | Schemar | **Required** |
| Dataview | blacksmithgu | **Required** |
| QuickAdd | chhoumann | **Required** |
| Auto Note Mover | farux0 | **Required** (fallback router) |
| Periodic Notes | liamcain | **Required** |
| Calendar | liamcain | **Required** |
| Homepage | mirnovov | Recommended |
| Linter | Platers | Recommended |
| Hotkeys for specific files | Vinzent | Recommended |
| Bookmarks (core) | Obsidian | Recommended (pin Inbox/Tasks/Home) |
| Iconize | FlorianWoelki | Optional (cosmetic) |
| Obsidian Git | Vinzent03 | Recommended (later — backup) |
| Metadata Menu | mdelobelle | Optional (only if YAML drift becomes a problem) |
| Bases (core) | Obsidian | **Skipped** (per decision 6) |
| Auto Periodic Notes | Jamie Hurst | **Skipped** (replaced by `launchd` per decision 2) |
| Projects, Kanban, Buttons, Meta Bind, Note Refactor, Advanced URI | various | Skip |

Plugin settings worth flagging:

- **Templater:** template folder `Assets/templates`; user script folder `Assets/scripts`; trigger Templater on new file creation: **on**; folder template mappings per below.
- **Tasks:** custom statuses configured per §5.2.
- **Dataview:** enable inline queries, JS queries, and inline fields.
- **Periodic Notes:** daily folder `Inbox/Daily`, format `YYYY-MM-DD`, template `Assets/templates/T - Daily Note.md`, open daily note on startup. (Auto-generation handled by `launchd` per §7.)
- **Homepage:** open `Dashboards/Home.md` on startup.
- **Linter:** insert created/updated, sort YAML, trim trailing whitespace, run on save.
- **Bookmarks (core):** pin `Dashboards/Home.md`, `Inbox/`, `Tasks/Tasks - Active.md`, `Tasks/Tasks - Blocked.md`, `Tasks/Tasks - Awaiting Customer.md`, `Tasks/Tasks - On Hold.md` for fast top-of-explorer access (compensates for no numeric prefixes).

Templater folder template mappings (Settings → Templater → Folder Templates):

| Folder | Template |
|---|---|
| `Inbox/Daily` | `T - Daily Note.md` |
| `Inbox` | `T - Generic Note.md` |
| `Customers/*/External Meetings` | `T - External Meeting.md` |
| `Customers/*/Internal Meetings` | `T - Internal Meeting.md` |
| `Customers/*/Streams` | `T - Stream of Work.md` |
| `Customers/*/Account Info` | `T - Account Info.md` |

---

## 4. Templates

All templates live in `Assets/templates/`. Frontmatter conventions:

| Key | Type | Allowed values | Notes |
|---|---|---|---|
| `type` | string | `daily`, `meeting`, `stream`, `customer`, `account-info`, `glossary`, `milestones`, `note` | Drives router. |
| `meeting-kind` | string | `internal`, `external` | Only on meeting notes. |
| `customer` | wikilink | `"[[Acme Corp]]"` | Always wikilink, never plain string. |
| `stream` | wikilink \| null | `"[[Migration to v2]]"` | Optional on meetings/notes. |
| `state` | string | `Active`, `On Hold`, `Temp`, `Inactive` | Customer Index notes only. |
| `status` | string | `In Progress`, `Blocked`, `Done`, `Awaiting Customer`, `On Hold` | Stream notes only. |
| `date` | date | `YYYY-MM-DD` | Meeting / daily notes. |
| `archived` | bool | `true` / `false` | Move trigger; set by archive script. |
| `archived-at` | datetime | `YYYY-MM-DD HH:mm` | Stamped by archive script. |
| `created` / `updated` | datetime | `YYYY-MM-DD HH:mm` | Maintained by Linter. |
| `tags` | list | `[meeting, external]` | Light use. |

### 4.1 `T - Daily Note.md`

Includes prev/next day navigation links per decision 9.

```markdown
---
type: daily
date: <% tp.date.now("YYYY-MM-DD") %>
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: [daily]
---

# <% tp.date.now("dddd, MMMM Do YYYY") %>

[← <% tp.date.yesterday("YYYY-MM-DD") %>](<% tp.date.yesterday("YYYY-MM-DD") %>.md)  ·  [Today](<% tp.date.now("YYYY-MM-DD") %>.md)  ·  [<% tp.date.tomorrow("YYYY-MM-DD") %> →](<% tp.date.tomorrow("YYYY-MM-DD") %>.md)

> [!tip] Daily flow
> 1. Triage Inbox → 0
> 2. Top 3 priorities
> 3. Capture loose tasks here; assign customer/stream later

## Top 3
- [ ] 
- [ ] 
- [ ] 

## Notes


## Captured tasks


## Links
- Yesterday: [[<% tp.date.yesterday("YYYY-MM-DD") %>]]
- Tomorrow: [[<% tp.date.tomorrow("YYYY-MM-DD") %>]]
```

### 4.2 `T - External Meeting.md`

```markdown
---
type: meeting
meeting-kind: external
customer: <% await tp.user.list_customers() |> tp.system.suggester(c => c, _) %>
stream: 
date: <% tp.date.now("YYYY-MM-DD") %>
attendees: []
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: [meeting, external]
---

# <% tp.date.now("YYYY-MM-DD") %> — [[<% tp.frontmatter.customer %>]] — External: <topic>

**Customer:** [[<% tp.frontmatter.customer %>]]  
**Stream:** <% tp.frontmatter.stream ? `[[${tp.frontmatter.stream}]]` : "_n/a_" %>

## Agenda

## Notes

## Decisions

## Customer asks

## Action items (ours)
- [ ] Example task [customer:: [[<% tp.frontmatter.customer %>]]] [stream:: [[<% tp.frontmatter.stream %>]]] [owner:: me] 📅 <% tp.date.now("YYYY-MM-DD", 7) %>

## Action items (theirs)
- [w] Awaiting redlines [customer:: [[<% tp.frontmatter.customer %>]]] [owner:: customer] ⏳ <% tp.date.now("YYYY-MM-DD", 7) %>
```

### 4.3 `T - Internal Meeting.md`

Same shape as External, with `meeting-kind: internal`, tags `[meeting, internal]`, no `Customer asks` / `Action items (theirs)` sections, and no required `stream`.

### 4.4 `T - Customer Index.md`

```markdown
---
type: customer
customer: <% tp.file.title %>
state: Active                          # Active | On Hold | Temp | Inactive
tier:                                  # strategic | growth | smb (optional)
ae: 
csm: 
since: <% tp.date.now("YYYY-MM-DD") %>
renewal: 
aliases: []
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
tags: [customer]
---

# <% tp.file.title %>

> **State:** `=this.state`  ·  **Tier:** `=this.tier`  ·  **Renewal:** `=this.renewal`

## Quick links
- [[Account Info]]
- [[Glossary]]
- [[Dates and Milestones]]

## Active streams
```dataview
TABLE status, target
FROM "Customers/<% tp.file.title %>/Streams"
WHERE type = "stream" AND status != "Done"
SORT status ASC, target ASC
```

## Open tasks for this customer
```tasks
not done
status.symbol does not include b
status.symbol does not include w
status.symbol does not include h
(description includes [[<% tp.file.title %>]]) OR (path includes Customers/<% tp.file.title %>)
group by status.name
sort by due
```

## Recent meetings
```dataview
TABLE meeting-kind AS "Kind", date
FROM "Customers/<% tp.file.title %>"
WHERE type = "meeting"
SORT date DESC
LIMIT 10
```
```

### 4.5 `T - Account Info.md`

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
- Glossary: [[Glossary]]
- Milestones: [[Dates and Milestones]]
```

### 4.6 `T - Glossary.md`

```markdown
---
type: glossary
customer: <% tp.file.folder(true).split("/").slice(-2,-1)[0] %>
tags: [glossary]
---

# <% tp.frontmatter.customer %> — Glossary

| Term | Definition | Notes |
|------|------------|-------|
```

### 4.7 `T - Dates and Milestones.md`

```markdown
---
type: milestones
customer: <% tp.file.folder(true).split("/").slice(-2,-1)[0] %>
tags: [milestones]
---

# <% tp.frontmatter.customer %> — Dates and Milestones

| Date | Type | Description | Stream | Status |
|------|------|-------------|--------|--------|

## Manual entries
- [ ] 2026-06-30 — Renewal kickoff 📅 2026-06-30 [customer:: [[<% tp.frontmatter.customer %>]]]
```

### 4.8 `T - Stream of Work.md`

Per decision 12, no `next:` field; the top open task plays that role.

```markdown
---
type: stream
customer: <% await tp.user.list_customers() |> tp.system.suggester(c => c, _) %>
status: In Progress                    # In Progress | Blocked | Done | Awaiting Customer | On Hold
priority: P2                           # P0 | P1 | P2 | P3
owner: me
started: <% tp.date.now("YYYY-MM-DD") %>
target: 
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: [stream]
---

# <% tp.file.title %>

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

## Archive criteria
- [ ] Outcome delivered or explicitly abandoned
- [ ] Open tasks moved or closed
- [ ] Final summary added
```

### 4.9 `T - Generic Note.md` (Inbox default)

```markdown
---
type: note
customer: 
stream: 
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: []
---

# <% tp.file.title %>


```

---

## 5. Task model

### 5.1 Syntax

```markdown
- [ ] Send updated SOW [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]] [owner:: me] 🔼 📅 2026-05-15
- [/] Drafting pricing model [customer:: [[Acme Corp]]] [stream:: [[Migration to v2]]] 🛫 2026-05-10 📅 2026-05-12
- [b] Blocked on Acme security review [customer:: [[Acme Corp]]] [stream:: [[SSO rollout]]]
- [w] Awaiting Acme legal redlines [customer:: [[Acme Corp]]] [owner:: customer] ⏳ 2026-05-15
- [h] On hold until next quarter [customer:: [[Globex]]]
- [x] Sent intro email ✅ 2026-05-07 [customer:: [[Acme Corp]]]
- [-] Cancelled — superseded by SOW v3 [customer:: [[Acme Corp]]]
```

Conventions:

- Every customer-related task: `[customer:: [[Customer Name]]]`.
- Every stream-related task: `[stream:: [[Stream Name]]]`.
- External-meeting task ownership: `[owner:: me]` or `[owner:: customer]`.
- Due date: `📅 YYYY-MM-DD`.
- Start / snooze: `🛫 YYYY-MM-DD`.
- Scheduled (waiting until): `⏳ YYYY-MM-DD`.
- Recurrence: `🔁 every week` (etc.).
- Priority: `⏫` highest, `🔼` high, `🔽` low.

### 5.2 Tasks plugin custom statuses

Configure under Settings → Tasks → Status Types. Note `[w]` for Awaiting Customer (per decision 4).

| Symbol | Name | Type | Available next |
|---|---|---|---|
| ` ` | To Do | TODO | `/`, `b`, `w`, `h`, `x` |
| `/` | In Progress | IN_PROGRESS | `x`, `b`, `w`, `h` |
| `b` | Blocked | TODO | ` `, `/`, `x` |
| `w` | Awaiting Customer | TODO | ` `, `/`, `x` |
| `h` | On Hold | TODO | ` `, `/`, `x` |
| `x` | Done | DONE | ` ` |
| `-` | Cancelled | CANCELLED | ` ` |

Blocked / Awaiting / On Hold are typed as `TODO` (not `NON_TASK`) so any "open" query catches them; the dashboards then partition by `status.symbol`.

### 5.3 Dashboards

Per decision 8, **separate aggregation views per status** (not a single combined "Blocked & Waiting" view).

`Tasks/Tasks - Active.md`:

````markdown
# Active tasks

```tasks
not done
status.symbol does not include b
status.symbol does not include w
status.symbol does not include h
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by priority, due
hide backlink
short mode
```
````

`Tasks/Tasks - Blocked.md`:

````markdown
# Blocked tasks

```tasks
not done
status.symbol includes b
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by due
hide backlink
short mode
```
````

`Tasks/Tasks - Awaiting Customer.md`:

````markdown
# Awaiting customer

```tasks
not done
status.symbol includes w
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by due
hide backlink
short mode
```
````

`Tasks/Tasks - On Hold.md`:

````markdown
# On hold

```tasks
not done
status.symbol includes h
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by due
hide backlink
short mode
```
````

`Tasks/Tasks - By Customer.md`:

````markdown
# Tasks by customer

```tasks
not done
status.symbol does not include b
group by function task.file.frontmatter?.customer ?? "(no customer)"
sort by due
```
````

---

## 6. Inbox auto-move

### 6.1 Trigger

A note leaves Inbox when **the user hits `⌘⇧A`** in the active note. The hotkey runs the Templater command `archive-note`, which:

1. Reads frontmatter (`type`, `customer`, `meeting-kind`, `stream`, `date`).
2. Computes the destination folder.
3. Stamps `archived: true`, `archived-at: <now>`, and (if `status: open`) sets `status: done`.
4. Moves the file via `app.fileManager.renameFile`.
5. Shows a `Notice` confirming the new path.

### 6.2 Destination resolution

Per decision 10, **done streams stay in place** — no rule routes them to `Archive/`.

| When the note has… | Destination |
|---|---|
| `type: meeting` AND `meeting-kind: external` AND `customer: X` | `Customers/X/External Meetings/` |
| `type: meeting` AND `meeting-kind: internal` AND `customer: X` | `Customers/X/Internal Meetings/` |
| `type: stream` AND `customer: X` | `Customers/X/Streams/` (regardless of `status`) |
| `type: account-info` AND `customer: X` | `Customers/X/Account Info/` |
| `type: glossary` / `milestones` AND `customer: X` | `Customers/X/Account Info/` |
| `type: customer` | `Customers/X/` (with rename to match folder name) |
| `type: daily` | `General/Journal/YYYY/MM/` (year/month from `date`) |
| `type: note` AND `customer: X` AND `stream: S` | `Customers/X/Streams/` |
| `type: note` AND `customer: X` (no stream) | `Customers/X/` |
| `type: note`, no customer | `General/` |

### 6.3 Archive script

`Assets/scripts/archive-note.js`:

```js
module.exports = async (tp) => {
  const file = tp.config.target_file;
  const fm = app.metadataCache.getFileCache(file)?.frontmatter ?? {};
  const customer = (fm.customer ?? "").replace(/\[\[|\]\]/g, "").trim();
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
    // Done streams stay in place per decision 10
    dest = `Customers/${customer}/Streams`;
  } else if (type === "account-info" && customer) {
    dest = `Customers/${customer}/Account Info`;
  } else if ((type === "glossary" || type === "milestones") && customer) {
    dest = `Customers/${customer}/Account Info`;
  } else if (type === "customer" && customer) {
    dest = `Customers/${customer}`;
  } else if (type === "daily") {
    dest = `General/Journal/${year}/${month}`;
  } else if (customer && fm.stream) {
    dest = `Customers/${customer}/Streams`;
  } else if (customer) {
    dest = `Customers/${customer}`;
  } else {
    dest = `General`;
  }

  if (!app.vault.getAbstractFileByPath(dest)) {
    await app.vault.createFolder(dest).catch(() => {});
  }

  await app.fileManager.processFrontMatter(file, fm2 => {
    fm2.archived = true;
    fm2["archived-at"] = tp.date.now("YYYY-MM-DD HH:mm");
    if (fm2.status === "open") fm2.status = "done";
  });

  await app.fileManager.renameFile(file, `${dest}/${file.name}`);
  new Notice(`Archived → ${dest}/${file.name}`);
};
```

Wire-up:
1. Templater → User Scripts folder → `Assets/scripts`.
2. Templater → User-defined commands → add `archive-note`.
3. Settings → Hotkeys → assign `⌘⇧A` to "Templater: archive-note".

### 6.4 Auto Note Mover fallback

For notes archived via tag (e.g., `#archive` added inline), configure Auto Note Mover rules:

| Trigger | Folder |
|---|---|
| Tag `#archive` (no other rule) | `Archive/Inbox` |

### 6.5 Daily note archival

Per decision 9, **daily notes are archived same-day with `⌘⇧A`** like any other note. The archive script routes `type: daily` → `General/Journal/YYYY/MM/`. The prev/next day links in the daily note template (per decision 9) make navigation back to recent days a single click even after archive.

---

## 7. Daily notes automation

Per decision 2, daily notes are auto-created via **external automation** (`launchd`) so the file exists before you open Obsidian.

### 7.1 Obsidian-side configuration

- Periodic Notes → Daily: format `YYYY-MM-DD`, folder `Inbox/Daily`, template `Assets/templates/T - Daily Note.md`, open daily note on startup.
- Calendar plugin: pinned to right sidebar for navigation.
- Homepage: open `Dashboards/Home.md` on startup. (When you open Obsidian, Home loads first; the daily note has already been created by `launchd` and is one click away via the prev/next links or the Calendar pane.)

### 7.2 macOS `launchd` job for morning generation

Save as `~/Library/LaunchAgents/com.surdy.daily-note.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.surdy.daily-note</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>-c</string>
    <string>VAULT="$HOME/ObsidianVault"; d=$(date +%Y-%m-%d); f="$VAULT/Inbox/Daily/$d.md"; mkdir -p "$VAULT/Inbox/Daily"; [ -f "$f" ] || touch "$f"</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key><integer>6</integer>
    <key>Minute</key><integer>30</integer>
  </dict>
  <key>RunAtLoad</key>
  <false/>
</dict>
</plist>
```

Replace `$HOME/ObsidianVault` with your actual vault path. Load with:

```bash
launchctl load ~/Library/LaunchAgents/com.surdy.daily-note.plist
```

When you next open Obsidian, the empty file already exists at `Inbox/Daily/<today>.md`. Templater's folder template mapping (`Inbox/Daily` → `T - Daily Note.md`) hydrates it on first open.

To verify the job is loaded:

```bash
launchctl list | grep com.surdy.daily-note
```

To unload:

```bash
launchctl unload ~/Library/LaunchAgents/com.surdy.daily-note.plist
```

> **Note:** the empty file will not yet have the daily-note template applied — Templater applies the folder template the first time the file is *opened* in Obsidian. If you want the file fully templated by 06:30, the cleanest path is an Obsidian Advanced URI call from the `launchd` job invoking the Periodic Notes "open today's note" command. Add that in a later iteration if the empty-file approach feels unfinished.

---

## 8. Customer state

Per decision 7, state lives in **frontmatter `state:` on the Customer Index note only** — no mirror tag. Values: `Active`, `On Hold`, `Temp`, `Inactive`.

Filtering is done entirely via Dataview queries reading the frontmatter. Examples:

Active customers:

```dataview
TABLE state, csm, ae, renewal
FROM "Customers"
WHERE type = "customer" AND state = "Active"
SORT customer ASC
```

All customers grouped by state:

```dataview
TABLE rows.file.link AS "Customers"
FROM "Customers"
WHERE type = "customer"
GROUP BY state
SORT key ASC
```

To change a customer's state: open their Customer Index note, edit the `state:` field via the Properties panel. (Optionally add a `Change customer state` QuickAdd macro for one-click changes.)

---

## 9. Streams of work

Representation: one note per stream at `Customers/<X>/Streams/<Stream>.md`. Frontmatter holds `status`, `priority`, `owner`, `started`, `target`. Stream status is independent of its tasks' statuses.

Per decision 10, when `status: Done`, the stream stays in `Customers/<X>/Streams/`. Dashboards filter by `status: Done` to surface or hide done streams as needed.

Active-streams Dataview view:

```dataview
TABLE customer, status, priority, target, file.mtime AS "Updated"
FROM "Customers"
WHERE type = "stream" AND status != "Done"
SORT priority ASC, target ASC
```

Done streams (for archive/review):

```dataview
TABLE customer, target, file.mtime AS "Closed"
FROM "Customers"
WHERE type = "stream" AND status = "Done"
SORT file.mtime DESC
```

Per decision 12, there is no `next:` field. The "next action" for a stream is the top entry in the `## Open tasks` section of the stream note (sorted by priority, then due date).

---

## 10. Dashboards

Per decision 6, all dashboards use Dataview (no Bases).

### 10.1 `Dashboards/Home.md` (Homepage)

````markdown
# Home

## Today
- [[<% tp.date.now("YYYY-MM-DD") %>|Open today's daily note]]
- [[Inbox Triage]]

## Top-of-mind tasks
```tasks
not done
status.symbol does not include b
status.symbol does not include w
status.symbol does not include h
(due before in 7 days) OR (priority is above medium)
limit 15
sort by due
short mode
```

## Active customers
```dataview
LIST FROM "Customers"
WHERE type = "customer" AND state = "Active"
SORT customer ASC
```

## Streams in progress
```dataview
TABLE customer, priority, target
FROM "Customers"
WHERE type = "stream" AND status = "In Progress"
SORT priority ASC, target ASC
```

## Quick links
- [[Tasks - Active]]
- [[Tasks - Blocked]]
- [[Tasks - Awaiting Customer]]
- [[Tasks - On Hold]]
- [[Customers]]
- [[Streams]]
````

### 10.2 `Dashboards/Inbox Triage.md`

````markdown
# Inbox triage

## Inbox notes (oldest first)
```dataview
TABLE type, customer, file.cday AS created
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

### 10.3 `Dashboards/Customers.md`

````markdown
# Customers

## All customers grouped by state
```dataview
TABLE rows.file.link AS "Customers"
FROM "Customers"
WHERE type = "customer"
GROUP BY state
SORT key ASC
```

## Active customers
```dataview
TABLE state, tier, csm, ae, renewal
FROM "Customers"
WHERE type = "customer" AND state = "Active"
SORT customer ASC
```

## Customers needing attention (no edits in 30d)
```dataview
TABLE state, file.mtime AS "Last touched"
FROM "Customers"
WHERE type = "customer" AND state = "Active"
WHERE file.mtime < date(today) - dur(30 days)
SORT file.mtime ASC
```
````

### 10.4 `Dashboards/Streams.md`

````markdown
# Streams of work

## Active streams (grouped by customer)
```dataview
TABLE status, priority, target, file.mtime AS "Updated"
FROM "Customers"
WHERE type = "stream" AND status != "Done"
GROUP BY customer
SORT priority ASC, target ASC
```

## Blocked streams
```dataview
TABLE customer, target, file.mtime AS "Updated"
FROM "Customers"
WHERE type = "stream" AND status = "Blocked"
SORT file.mtime DESC
```

## Awaiting-customer streams
```dataview
TABLE customer, target, file.mtime AS "Updated"
FROM "Customers"
WHERE type = "stream" AND status = "Awaiting Customer"
SORT file.mtime DESC
```

## Done streams (archive review)
```dataview
TABLE customer, target, file.mtime AS "Closed"
FROM "Customers"
WHERE type = "stream" AND status = "Done"
SORT file.mtime DESC
```
````

### 10.5 Per-customer overview

The Customer Index note (`Customers/<X>/<X>.md`) is itself the per-customer dashboard. See §4.4 template.

---

## 11. Implementation order

A one-hour bootstrap, then incremental fill-in.

### Phase 0 — vault & settings (10 min)
1. Create vault.
2. Settings → Files & Links: default new note location → `Inbox`; default attachment location → `Assets/data`; new link format → "Shortest path when possible"; use wikilinks → on; auto-update internal links → on.
3. Create folder skeleton from §2 (plain names).
4. Bookmarks (core plugin): pin `Dashboards/Home.md`, `Inbox/`, `Tasks/Tasks - Active.md`, `Tasks/Tasks - Blocked.md`, `Tasks/Tasks - Awaiting Customer.md`, `Tasks/Tasks - On Hold.md`.

### Phase 1 — core plugins (10 min)
5. Install: Templater, Tasks, Dataview, QuickAdd, Auto Note Mover, Periodic Notes, Calendar, Homepage, Linter, Hotkeys for specific files.
6. Configure plugin settings per §3.

### Phase 2 — templates & script (20 min)
7. Create all nine templates from §4.
8. Create `Assets/scripts/archive-note.js` (§6.3) and `list-customers.js` (helper for the suggester).
9. Templater → User-defined commands → add `archive-note`.
10. Settings → Hotkeys → bind `⌘⇧A` to "Templater: archive-note".

### Phase 3 — Tasks plugin statuses (5 min)
11. Tasks → Status Types → configure the 7 statuses per §5.2 (with `[w]` for Awaiting Customer).

### Phase 4 — QuickAdd macros (10 min)
12. Add macros: `New external meeting`, `New internal meeting`, `New stream`, `New customer` (scaffolds folder + index + 3 account-info notes), `Change customer state`. Each prompts and instantiates its template into `Inbox/`.

### Phase 5 — dashboards (15 min)
13. Create `Dashboards/Home.md`, `Inbox Triage.md`, `Customers.md`, `Streams.md`, plus `Tasks/` views (Active, Blocked, Awaiting Customer, On Hold, By Customer).
14. Set Homepage plugin to open `Dashboards/Home.md`.

### Phase 6 — `launchd` morning job (5 min)
15. Save `~/Library/LaunchAgents/com.surdy.daily-note.plist` per §7.2 (replace vault path).
16. Run `launchctl load ~/Library/LaunchAgents/com.surdy.daily-note.plist`.
17. Verify with `launchctl list | grep com.surdy.daily-note`.
18. Test by manually running the inner `bash -c` command and confirming the file appears.

### Phase 7 — first customer & smoke test (10 min)
19. Run `New customer` macro for one real customer.
20. Confirm folder structure, index note, account info notes are created.
21. Capture one external meeting; add a task with `[customer:: [[…]]] [stream:: [[…]]] 📅 next-week`.
22. Verify the task appears in `Tasks/Tasks - Active.md`, on the Stream note, and on `Home.md`.
23. Hit `⌘⇧A` on the meeting note → confirm it lands in `Customers/<X>/External Meetings/`.

### Phase 8 — daily routine (ongoing)
24. Each morning: `launchd` has already created today's daily note → open Obsidian → Home loads → triage Inbox → top 3 written → work the day.
25. Each evening: hit `⌘⇧A` on every Inbox note that's done, including the daily note.
26. Each Friday: review Blocked / Awaiting Customer / On Hold dashboards, close out finished streams, set state changes on customers.

### Phase 9 — optional (later)
27. Install Iconize for folder icons.
28. Install Obsidian Git for backup.
29. Install Metadata Menu if YAML drift becomes a problem.
30. Add Advanced URI integration to the `launchd` job so the daily note opens fully templated at 06:30.
31. Add an "Archive customer" Templater command for inactivation (moves the entire customer folder to `Archive/Customers/`).

---

## 12. Refinements to fold back into `notes-method.md`

These are spec-level changes derived from the decision log; they should be reflected in `notes-method.md` so the spec stays the source of truth.

1. **Task statuses extended** from 5 to 7: add `In Progress` and `Cancelled`.
2. **Active task list excludes** `Blocked`, `Awaiting Customer`, and `On Hold`. Three separate aggregation views surface each non-active status.
3. **Add task metadata conventions**: due dates (`📅`), scheduled (`⏳`), start/snooze (`🛫`), recurrence (`🔁`), priority (`⏫🔼🔽`), ownership (`[owner:: me|customer]`).
4. **Daily notes are archived same-day** via `⌘⇧A`; the daily note template includes prev/next day navigation.
5. **Done streams stay in place** at `Customers/<X>/Streams/`; status filtering happens in dashboards rather than via folder moves.
6. **Meeting note naming**: `YYYY-MM-DD - <Customer> - <Internal|External> - <Topic>.md`.
7. **Customer index naming**: note name = customer folder name (so `[[Customer]]` resolves).
8. **Customer state location**: frontmatter `state:` on Customer Index note (no mirror tag).
9. **Daily note generation**: external `launchd` job creates the file at 06:30; Periodic Notes + Templater hydrate it when first opened.

---

## 13. The single biggest investment

If your bootstrap budget is tight, prioritize getting **§6 (Inbox auto-move)** working end-to-end first. Templates, dashboards, and queries can all be evolved later without rework. The auto-move mechanism *is* the workflow — get the `⌘⇧A` muscle memory established and Inbox-zero becomes the default mode.
