# Reviewed Implementation Plan

> **How this plan was reviewed:** the user requested a 12-question interactive review of the decision points from `comparison-and-recommendations.md`. The user was not available to answer interactively. Per autopilot guidance, the **recommended option for each decision point has been applied as the default**. The decision log in §1 below makes every choice explicit and easy to change. To override any decision, edit §1 and the dependent sections will follow.

This plan is a single-document blueprint for implementing the notes method in Obsidian. It supersedes none of the source files — `notes-method.md` remains the spec, the three model plans remain the raw inputs, and `comparison-and-recommendations.md` remains the analysis.

---

## 1. Decision log

| # | Decision point | Choice (default = recommendation) | Override? |
|---|---|---|---|
| 1 | Folder layout | **Numeric prefixes** (`00 Inbox`, `01 Tasks`, …, `99 Archive`) | — |
| 2 | Daily notes plugin | **Periodic Notes + Auto Periodic Notes + Calendar** (skip core Daily Notes; skip `launchd` initially) | — |
| 3 | Inbox auto-move trigger | **Explicit hotkey `⌘⇧A`** → Templater `archive-note.js` (stamps `archived: true` + `archived-at`, computes destination, moves file) | — |
| 4 | Task status palette | **7 statuses**: `[ ]` To Do, `[/]` In Progress, `[b]` Blocked, `[a]` Awaiting Customer, `[h]` On Hold, `[x]` Done, `[-]` Cancelled | — |
| 5 | Task encoding redundancy | **Symbol only** — no `[task_status::]` mirror field. Customer/stream still use inline `[customer:: [[X]]]` / `[stream:: [[Y]]]`. | — |
| 6 | Tabular dashboards | **Bases (core) for Customers + Streams** dashboards via `.base` files; Dataview for cross-cutting joins; Tasks plugin for checkbox queries | — |
| 7 | Customer state location | **Frontmatter `state:` on the Customer Index note**; mirror tag `customer/<state>` for sidebar filtering | — |
| 8 | Awaiting Customer / On Hold in the "Active" list | **Exclude both** from the Active list; surface them in a dedicated "Blocked & Waiting" view grouped by status | — |
| 9 | Daily note retention | **Keep daily notes in `00 Inbox/Daily/` during the week**; sweep to `03 General/Journal/YYYY/MM/` on a Friday review (don't archive same-day) | — |
| 10 | Stream archival path | **`99 Archive/02 Customers/<X>/Streams/`** when `status: Done` and `archived: true` (script extension to the standard archive flow) | — |
| 11 | Meeting note naming | **`YYYY-MM-DD - <Customer> - <Internal\|External> - <Topic>.md`** | — |
| 12 | Stream "Next action" field | **Adopt** — `next:` frontmatter on stream notes; surface on Customer Index and Home dashboards | — |

---

## 2. Vault structure

```
00 Inbox/
  Daily/
    2026-05-08.md
01 Tasks/
  Tasks - Active.md
  Tasks - Blocked & Waiting.md
  Tasks - By Customer.md
02 Customers/
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
03 General/
  Journal/                              ← daily notes after Friday sweep
    2026/05/
04 Dashboards/
  Home.md                               ← Homepage opens this
  Inbox Triage.md
  Customers.md                          ← Bases view
  Streams.md                            ← Bases view
05 Assets/
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
  bases/
    customers.base
    streams.base
  data/                                 ← attachments (default location)
99 Archive/
  02 Customers/                         ← inactive customers / archived streams
```

Naming conventions:

- Customer Index note: same name as folder → `02 Customers/Acme Corp/Acme Corp.md`. Guarantees `[[Acme Corp]]` resolves to the entity.
- Meeting note: `YYYY-MM-DD - <Customer> - <Internal|External> - <Topic>.md`.
- Stream note: human-readable name; prefix with customer only if a name collision is possible (`Acme - Migration to v2` if `Migration to v2` is ambiguous).
- Daily note: `YYYY-MM-DD.md`.
- Default attachment location (Settings → Files & Links): `05 Assets/data/`.
- Default new note location: `00 Inbox/`.
- New link format: "Shortest path when possible".

---

## 3. Plugins

Install in this order. Required = the system doesn't work without it. Recommended = adopt unless you have a reason not to. Optional = add when you feel the friction.

| Plugin | Author | Status |
|---|---|---|
| Templater | SilentVoid13 | Required |
| Tasks | Schemar | Required |
| Dataview | blacksmithgu | Required |
| QuickAdd | chhoumann | Required |
| Auto Note Mover | farux0 | Required (fallback router) |
| Periodic Notes | liamcain | Required |
| Calendar | liamcain | Required |
| Auto Periodic Notes | Jamie Hurst | Recommended |
| Homepage | mirnovov | Recommended |
| Bases (core) | Obsidian | Recommended |
| Linter | Platers | Recommended |
| Hotkeys for specific files | Vinzent | Recommended |
| Iconize | FlorianWoelki | Optional (cosmetic) |
| Obsidian Git | Vinzent03 | Recommended (later — backup) |
| Metadata Menu | mdelobelle | Optional (only if YAML drift becomes a problem) |
| Advanced URI | Vinzent | Skip initially |
| Projects, Kanban, Buttons, Meta Bind, Note Refactor | various | Skip |

Plugin settings worth flagging:

- **Templater:** template folder `05 Assets/templates`; user script folder `05 Assets/scripts`; trigger Templater on new file creation: **on**; folder template mappings per §4.
- **Tasks:** custom statuses configured per §6.
- **Dataview:** enable inline queries, JS queries, and inline fields.
- **Periodic Notes:** daily folder `00 Inbox/Daily`, format `YYYY-MM-DD`, template `05 Assets/templates/T - Daily Note.md`, open daily note on startup.
- **Auto Periodic Notes:** enable daily auto-creation.
- **Homepage:** open `04 Dashboards/Home.md` on startup.
- **Linter:** insert created/updated, sort YAML, trim trailing whitespace, run on save.

Templater folder template mappings (Settings → Templater → Folder Templates):

| Folder | Template |
|---|---|
| `00 Inbox/Daily` | `T - Daily Note.md` |
| `00 Inbox` | `T - Generic Note.md` |
| `02 Customers/*/External Meetings` | `T - External Meeting.md` |
| `02 Customers/*/Internal Meetings` | `T - Internal Meeting.md` |
| `02 Customers/*/Streams` | `T - Stream of Work.md` |
| `02 Customers/*/Account Info` | `T - Account Info.md` |

---

## 4. Templates

All templates live in `05 Assets/templates/`. Frontmatter conventions:

| Key | Type | Allowed values | Notes |
|---|---|---|---|
| `type` | string | `daily`, `meeting`, `stream`, `customer`, `account-info`, `glossary`, `milestones`, `note` | Drives router. |
| `meeting-kind` | string | `internal`, `external` | Only on meeting notes. |
| `customer` | wikilink | `"[[Acme Corp]]"` | Always wikilink, never plain string. |
| `stream` | wikilink \| null | `"[[Migration to v2]]"` | Optional on meetings/notes. |
| `state` | string | `Active`, `On Hold`, `Temp`, `Inactive` | Customer Index notes only. |
| `status` | string | `In Progress`, `Blocked`, `Done`, `Awaiting Customer`, `On Hold` | Stream notes only. |
| `next` | string | free text | Stream notes — the next concrete action. |
| `date` | date | `YYYY-MM-DD` | Meeting / daily notes. |
| `archived` | bool | `true` / `false` | Move trigger; set by archive script. |
| `archived-at` | datetime | `YYYY-MM-DD HH:mm` | Stamped by archive script. |
| `created` / `updated` | datetime | `YYYY-MM-DD HH:mm` | Maintained by Linter. |
| `tags` | list | `[customer/active, meeting]` | Light use; mirror state for sidebar. |

### 4.1 `T - Daily Note.md`

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
- [a] Awaiting redlines [customer:: [[<% tp.frontmatter.customer %>]]] [owner:: customer] ⏳ <% tp.date.now("YYYY-MM-DD", 7) %>
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
tags: [customer, customer/active]
---

# <% tp.file.title %>

> **State:** `=this.state`  ·  **Tier:** `=this.tier`  ·  **Renewal:** `=this.renewal`

## Quick links
- [[Account Info]]
- [[Glossary]]
- [[Dates and Milestones]]

## Active streams
```dataview
TABLE status, next, target
FROM "02 Customers/<% tp.file.title %>/Streams"
WHERE type = "stream" AND status != "Done"
SORT status ASC, target ASC
```

## Open tasks for this customer
```tasks
not done
status.symbol does not include b
status.symbol does not include a
status.symbol does not include h
(description includes [[<% tp.file.title %>]]) OR (path includes 02 Customers/<% tp.file.title %>)
group by status.name
sort by due
```

## Recent meetings
```dataview
TABLE meeting-kind AS "Kind", date
FROM "02 Customers/<% tp.file.title %>"
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

```markdown
---
type: stream
customer: <% await tp.user.list_customers() |> tp.system.suggester(c => c, _) %>
status: In Progress                    # In Progress | Blocked | Done | Awaiting Customer | On Hold
priority: P2                           # P0 | P1 | P2 | P3
next:                                  # next concrete action — single most important field
owner: me
started: <% tp.date.now("YYYY-MM-DD") %>
target: 
created: <% tp.date.now("YYYY-MM-DD HH:mm") %>
updated: <% tp.date.now("YYYY-MM-DD HH:mm") %>
archived: false
tags: [stream]
---

# <% tp.file.title %>

> **Customer:** [[<% tp.frontmatter.customer %>]]  ·  **Status:** `=this.status`  ·  **Next:** `=this.next`  ·  **Target:** `=this.target`

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
LIST FROM "02 Customers/<% tp.frontmatter.customer %>"
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
- [a] Awaiting Acme legal redlines [customer:: [[Acme Corp]]] [owner:: customer] ⏳ 2026-05-15
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

Configure under Settings → Tasks → Status Types:

| Symbol | Name | Type | Available next |
|---|---|---|---|
| ` ` | To Do | TODO | `/`, `b`, `a`, `h`, `x` |
| `/` | In Progress | IN_PROGRESS | `x`, `b`, `a`, `h` |
| `b` | Blocked | TODO | ` `, `/`, `x` |
| `a` | Awaiting Customer | TODO | ` `, `/`, `x` |
| `h` | On Hold | TODO | ` `, `/`, `x` |
| `x` | Done | DONE | ` ` |
| `-` | Cancelled | CANCELLED | ` ` |

Blocked / Awaiting / On Hold are typed as `TODO` (not `NON_TASK`) so any "open" query catches them; the dashboards then partition by `status.symbol`.

### 5.3 Dashboards

`01 Tasks/Tasks - Active.md`:

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

`01 Tasks/Tasks - Blocked & Waiting.md`:

````markdown
# Blocked & waiting

```tasks
not done
(status.symbol includes b) OR (status.symbol includes a) OR (status.symbol includes h)
group by status.name
sort by due
```
````

`01 Tasks/Tasks - By Customer.md`:

````markdown
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

| When the note has… | Destination |
|---|---|
| `type: meeting` AND `meeting-kind: external` AND `customer: X` | `02 Customers/X/External Meetings/` |
| `type: meeting` AND `meeting-kind: internal` AND `customer: X` | `02 Customers/X/Internal Meetings/` |
| `type: stream` AND `customer: X` AND `status != Done` | `02 Customers/X/Streams/` |
| `type: stream` AND `status: Done` | `99 Archive/02 Customers/X/Streams/` |
| `type: account-info` AND `customer: X` | `02 Customers/X/Account Info/` |
| `type: glossary` / `milestones` AND `customer: X` | `02 Customers/X/Account Info/` |
| `type: customer` | `02 Customers/X/` (with rename to match folder name) |
| `type: daily` | `03 General/Journal/YYYY/MM/` (year/month from `date`) |
| `type: note` AND `customer: X` AND `stream: S` | `02 Customers/X/Streams/` |
| `type: note` AND `customer: X` (no stream) | `02 Customers/X/` |
| `type: note`, no customer | `03 General/` |

### 6.3 Archive script

`05 Assets/scripts/archive-note.js`:

```js
module.exports = async (tp) => {
  const file = tp.config.target_file;
  const fm = app.metadataCache.getFileCache(file)?.frontmatter ?? {};
  const customer = (fm.customer ?? "").replace(/\[\[|\]\]/g, "").trim();
  const type = fm.type;
  const meetingKind = fm["meeting-kind"];
  const status = fm.status;
  const date = fm.date ?? tp.date.now("YYYY-MM-DD");
  const [year, month] = date.split("-");

  let dest;
  if (type === "meeting" && meetingKind === "external" && customer) {
    dest = `02 Customers/${customer}/External Meetings`;
  } else if (type === "meeting" && meetingKind === "internal" && customer) {
    dest = `02 Customers/${customer}/Internal Meetings`;
  } else if (type === "stream" && customer && status === "Done") {
    dest = `99 Archive/02 Customers/${customer}/Streams`;
  } else if (type === "stream" && customer) {
    dest = `02 Customers/${customer}/Streams`;
  } else if (type === "account-info" && customer) {
    dest = `02 Customers/${customer}/Account Info`;
  } else if ((type === "glossary" || type === "milestones") && customer) {
    dest = `02 Customers/${customer}/Account Info`;
  } else if (type === "customer" && customer) {
    dest = `02 Customers/${customer}`;
  } else if (type === "daily") {
    dest = `03 General/Journal/${year}/${month}`;
  } else if (customer && fm.stream) {
    dest = `02 Customers/${customer}/Streams`;
  } else if (customer) {
    dest = `02 Customers/${customer}`;
  } else {
    dest = `03 General`;
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
1. Templater → User Scripts folder → `05 Assets/scripts`.
2. Templater → User-defined commands → add `archive-note`.
3. Settings → Hotkeys → assign `⌘⇧A` to "Templater: archive-note".

### 6.4 Auto Note Mover fallback

For notes archived via tag (e.g., `#archive` added inline) or for daily notes that should auto-sweep on Friday, configure Auto Note Mover rules:

| Trigger | Folder |
|---|---|
| Tag `#archive` (no other rule) | `99 Archive/Inbox` |
| Frontmatter `type: daily` AND `archived: true` | `03 General/Journal` (interpolation not supported; sub-folder via QuickAdd macro) |

### 6.5 Daily note retention

Daily notes are *not* archived same-day. The Friday-review macro:

1. Lists daily notes in `00 Inbox/Daily/` from the past week.
2. For each: confirm "captured everything?" → set `archived: true` and run `archive-note`.

This keeps yesterday's note one click away during the week.

---

## 7. Daily notes automation

Settings:

- Periodic Notes → Daily: format `YYYY-MM-DD`, folder `00 Inbox/Daily`, template `05 Assets/templates/T - Daily Note.md`, open on startup.
- Auto Periodic Notes: enable daily auto-creation (creates the file on Obsidian launch even if you don't open it).
- Calendar plugin: pinned to right sidebar for navigation.
- Homepage: open `04 Dashboards/Home.md` on startup (so first thing you see is Home, with today's daily note linked at the top).

`launchd` job for true 06:30 generation: skip initially. Add only if you find yourself missing the file before opening Obsidian. Reference: §6.2 of `obsidian-plan-claude-opus-4.7-xhigh.md`.

---

## 8. Customer state

**Lives in frontmatter `state:` on the Customer Index note.** Values: `Active`, `On Hold`, `Temp`, `Inactive`.

Mirror tag `customer/<state>` (e.g., `customer/active`) on the same note for sidebar tag-pane filtering. Keep in sync via Linter rule or a "Change customer state" QuickAdd macro that updates both atomically.

`05 Assets/bases/customers.base`:

```yaml
filters:
  and:
    - file.folder.startsWith("02 Customers/")
    - type == "customer"
views:
  - type: table
    name: All customers
    order: [customer, state, tier, csm, ae, renewal, updated]
    sort:
      - { property: state, direction: asc }
      - { property: customer, direction: asc }
    group_by: state
  - type: table
    name: Active only
    filters:
      and:
        - state == "Active"
    order: [customer, tier, csm, ae, renewal, updated]
  - type: cards
    name: Cards by state
    group_by: state
```

---

## 9. Streams of work

Representation: one note per stream at `02 Customers/<X>/Streams/<Stream>.md`. Frontmatter holds `status`, `priority`, `next`, `owner`, `started`, `target`. The `next` field is the single most-looked-at piece of metadata — surface it on Customer Index and Home dashboards.

Status values match the spec: `In Progress`, `Blocked`, `Done`, `Awaiting Customer`, `On Hold`. Stream status is independent of its tasks' statuses.

When `status: Done`: hit `⌘⇧A` → script routes to `99 Archive/02 Customers/<X>/Streams/`.

`05 Assets/bases/streams.base`:

```yaml
filters:
  and:
    - type == "stream"
views:
  - type: table
    name: Active streams
    filters:
      and:
        - status != "Done"
        - archived != true
    order: [customer, file.name, status, priority, next, target, owner]
    sort:
      - { property: priority, direction: asc }
      - { property: target, direction: asc }
    group_by: customer
  - type: table
    name: Awaiting customer
    filters:
      and:
        - status == "Awaiting Customer"
    order: [customer, file.name, target, next, owner, updated]
```

---

## 10. Dashboards

### 10.1 `04 Dashboards/Home.md` (Homepage)

````markdown
# Home

## Today
- [[<% tp.date.now("YYYY-MM-DD") %>|Open today's daily note]]
- [[Inbox Triage]]

## Top-of-mind tasks
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
LIST FROM "02 Customers"
WHERE type = "customer" AND state = "Active"
SORT customer ASC
```

## Streams in progress (Next actions)
```dataview
TABLE customer, next, target
FROM "02 Customers"
WHERE type = "stream" AND status = "In Progress"
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

### 10.2 `04 Dashboards/Inbox Triage.md`

````markdown
# Inbox triage

## Inbox notes (oldest first)
```dataview
TABLE type, customer, file.cday AS created
FROM "00 Inbox"
WHERE !archived
SORT file.cday ASC
```

## Inbox tasks not yet routed
```tasks
not done
path includes 00 Inbox/
group by path
```
````

### 10.3 `04 Dashboards/Customers.md`

````markdown
# Customers

```base
file: 05 Assets/bases/customers.base
view: All customers
```

## Customers needing attention (no meetings in 30d)
```dataview
TABLE state, file.mtime AS "Last touched"
FROM "02 Customers"
WHERE type = "customer" AND state = "Active"
WHERE file.mtime < date(today) - dur(30 days)
SORT file.mtime ASC
```
````

### 10.4 `04 Dashboards/Streams.md`

````markdown
# Streams of work

```base
file: 05 Assets/bases/streams.base
view: Active streams
```

## Blocked streams
```dataview
TABLE customer, next, target, updated
FROM "02 Customers"
WHERE type = "stream" AND status = "Blocked"
SORT updated DESC
```
````

### 10.5 Per-customer overview

The Customer Index note (`02 Customers/<X>/<X>.md`) is itself the per-customer dashboard. See §4.4 template.

---

## 11. Implementation order

A one-hour bootstrap, then incremental fill-in.

### Phase 0 — vault & settings (10 min)
1. Create vault.
2. Settings → Files & Links: default new note location → `00 Inbox`; default attachment location → `05 Assets/data`; new link format → "Shortest path when possible"; use wikilinks → on; auto-update internal links → on.
3. Create folder skeleton from §2.

### Phase 1 — core plugins (10 min)
4. Install: Templater, Tasks, Dataview, QuickAdd, Auto Note Mover, Periodic Notes, Auto Periodic Notes, Calendar, Homepage, Linter, Hotkeys for specific files. Enable Bases (core).
5. Configure plugin settings per §3.

### Phase 2 — templates & script (20 min)
6. Create all nine templates from §4.
7. Create `05 Assets/scripts/archive-note.js` (§6.3) and `list-customers.js` (helper for the suggester).
8. Templater → User-defined commands → add `archive-note`.
9. Settings → Hotkeys → bind `⌘⇧A` to "Templater: archive-note".

### Phase 3 — Tasks plugin statuses (5 min)
10. Tasks → Status Types → configure the 7 statuses per §5.2.

### Phase 4 — QuickAdd macros (10 min)
11. Add macros: `New external meeting`, `New internal meeting`, `New stream`, `New customer` (scaffolds folder + index + 3 account-info notes), `Change customer state`. Each prompts and instantiates its template into `00 Inbox/`.

### Phase 5 — dashboards (15 min)
12. Create `04 Dashboards/Home.md`, `Inbox Triage.md`, `Customers.md`, `Streams.md`, plus `01 Tasks/` views.
13. Create `05 Assets/bases/customers.base` and `streams.base`.
14. Set Homepage plugin to open `04 Dashboards/Home.md`.

### Phase 6 — first customer (5 min)
15. Run `New customer` macro for one real customer.
16. Confirm folder structure, index note, account info notes are created.
17. Capture one external meeting; add a task with `[customer:: [[…]]] [stream:: [[…]]] 📅 next-week`.
18. Verify the task appears in `01 Tasks/Tasks - Active.md`, on the Stream note, and on `Home.md`.
19. Hit `⌘⇧A` on the meeting note → confirm it lands in `02 Customers/<X>/External Meetings/`.

### Phase 7 — daily routine (ongoing)
20. Each morning: open Obsidian → daily note auto-created → triage Inbox → top 3 written → work the day.
21. Each evening: hit `⌘⇧A` on every Inbox note that's done.
22. Each Friday: review Blocked & Waiting, sweep daily notes, close out finished streams, set state changes on customers.

### Phase 8 — optional (later)
23. Install Iconize for folder icons.
24. Install Obsidian Git for backup.
25. Install Metadata Menu if YAML drift becomes a problem.
26. Add `launchd` morning daily-note job if Obsidian-launch creation is insufficient.
27. Add an "Archive customer" Templater command for inactivation (moves entire customer folder to `99 Archive/02 Customers/`).

---

## 12. Refinements to fold back into `notes-method.md`

These are spec-level changes derived from the decision log; they should be reflected in `notes-method.md` so the spec stays the source of truth.

1. **Task statuses extended** from 5 to 7: add `In Progress` and `Cancelled`.
2. **Active task list excludes** `Blocked`, `Awaiting Customer`, and `On Hold` (the spec said only "not done and not blocked"; the practical workflow needs all three excluded from Active and surfaced in a separate Waiting view).
3. **Add task metadata conventions**: due dates (`📅`), scheduled (`⏳`), start/snooze (`🛫`), recurrence (`🔁`), priority (`⏫🔼🔽`), ownership (`[owner:: me|customer]`).
4. **Add stream `next:` field** — the next concrete action; surface on Home and Customer Index dashboards.
5. **Add stream archival path** — `99 Archive/02 Customers/<X>/Streams/` when `status: Done` and archived.
6. **Confirm meeting note naming** — `YYYY-MM-DD - <Customer> - <Internal|External> - <Topic>.md`.
7. **Confirm customer index naming** — note name = customer folder name (so `[[Customer]]` resolves).
8. **Confirm customer state location** — frontmatter `state:` on Customer Index, with mirror tag `customer/<state>`.
9. **Daily note retention** — keep in `00 Inbox/Daily/` during the week; sweep to `03 General/Journal/YYYY/MM/` on Friday review.

---

## 13. The single biggest investment

If your bootstrap budget is tight, prioritize getting **§6 (Inbox auto-move)** working end-to-end first. Templates, dashboards, and queries can all be evolved later without rework. The auto-move mechanism *is* the workflow — get the `⌘⇧A` muscle memory established and Inbox-zero becomes the default mode.
