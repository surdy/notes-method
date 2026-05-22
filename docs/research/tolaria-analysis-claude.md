---

## Tolaria Design/UX Analysis for Notesmith

### Summary

Tolaria (by Luca Rossi / refactoringhq) is a Tauri + React + TypeScript desktop notes app for macOS/Windows/Linux built around a filesystem-as-source-of-truth philosophy. It has 110+ Architecture Decision Records documenting every major design choice. Both apps share the same Tauri desktop shell and local-first Markdown ideology, but they diverge sharply on editor approach (Tolaria: BlockNote block editor; Notesmith: CodeMirror 6), navigation model (Tolaria: single-note with back/forward; Notesmith: multi-tab), and organization model (Tolaria: flat vault + type docs; Notesmith: folder tree + SQL views). There are numerous high-quality UX patterns Notesmith can adopt directly.

---

## 1. Repositories & Sources Discovered

| Source | URL | What It Contains |
|---|---|---|
| Website | https://tolaria.md/ | Feature descriptions, screenshots, design philosophy |
| GitHub | https://github.com/refactoringhq/tolaria | Source structure, 110+ ADRs, full docs |
| ADR index | `github.com/refactoringhq/tolaria/tree/main/docs/adr` | All architectural decisions with rationale |
| Concept docs | `tolaria.md/concepts/*` | Vaults, notes, types, relationships, editor, AI, git |
| User guides | `tolaria.md/guides/*` | Capture, inbox, organize workflows |

---

## 2. Tolaria Design Philosophy (Executive Summary)

Tolaria is built on **8 core principles**, documented in the README:

1. **Files-first** — Every note is a plain Markdown file; no export step ever needed
2. **Git-first** — Every vault is a git repo by default; history/sync via the tool engineers already know
3. **Offline-first, zero lock-in** — No accounts, no cloud dependencies, no subscriptions
4. **Open source** — AGPL-3.0, built in public
5. **Standards-based** — YAML frontmatter, no proprietary formats
6. **Types as lenses, not schemas** — Types are navigation aids, not enforcement; no required fields
7. **AI-first but not AI-only** — Deep AI integration but works without it
8. **Keyboard-first** — Every feature reachable via keyboard, with full macOS menu bar parity (ADR-0020)

The overall feel is: **Bear Notes meets Notion meets Linear**, with Git and AI woven in. The UI is described as a "four-panel layout inspired by Bear Notes."

---

## 3. Feature-by-Feature Analysis

### 3.1 Application Layout

**Tolaria:**
- **Four-panel layout** (per ARCHITECTURE.md): left sidebar (nav, types, favorites, views) | note list panel | editor | right panel (properties/AI/ToC)
- **Status bar** at bottom for git status (changed files badge), pulse/history, commit, vault switcher
- Git-related actions deliberately moved _out_ of the sidebar into the status bar (ADR-0032): "mixes navigation and action concerns; sidebar becomes harder to scan"
- The sidebar is navigation-only (types, favorites, saved views, folders)

**Notesmith:**
- Three-pane layout: `aside.sidebar` (280px, file tree + views) | optional `MiddlePane` (draggable) | `main.content-area` (tabs + toolbar + editor) | `aside.right-rail-shell` (260px, collapsible)
- No status bar currently
- Settings accessed via gear button ⚙ in sidebar header → navigates to `/settings` route

**Gap/Opportunity:**
- **No status bar in Notesmith** — Notesmith should add a bottom status bar for: active vault name, word count, git status (pending changes badge), save indicator, cursor position. This declutters the sidebar and is a natural home for persistent-but-secondary info.
- The **sidebar header** in Notesmith is just "📝 Notesmith" + gear button (`+page.svelte:109`). Tolaria's approach of a vault menu in the bottom-left corner is cleaner.

---

### 3.2 Editor Experience

**Tolaria:**
- **BlockNote** rich block editor (Notion-like, slash commands, drag-to-reorder blocks) — ADR-0022
- **Raw mode** toggle (`Cmd+\`) shows plain Markdown with CodeMirror for syntax highlighting — ADR-0037
- Only **2 modes**: rich (BlockNote) or raw (CodeMirror); no separate "reading view"
- **H1 is the only title surface** — no title input above the editor body (ADR-0055/0068)
- Title displayed in breadcrumb bar; filename can auto-rename from first H1 (opt-out setting)
- **Per-note width**: normal or wide via `_width: wide` frontmatter (ADR-0049 related); set in toolbar or Settings
- **Table of Contents panel** toggled by `Cmd+Shift+T`; derived from _live editor state_, not saved Markdown
- Slash commands: `/whiteboard` inserts tldraw board; `/` opens block type picker
- **tldraw whiteboard blocks** embedded in notes, stored as fenced `tldraw` JSON in `.md` files — ADR-0107
- **Mermaid diagrams** rendered inline in rich editor — ADR-0088
- **Math (LaTeX)** support in notes — ADR-0082
- **Autosave** with low-end-safe idle window (debounced, but not on every keystroke on slow machines) — ADR-0102
- **Spellcheck**: not explicitly mentioned but likely off (like Notesmith)

**Notesmith:**
- **CodeMirror 6** for all editing; 3 view modes: `source` (raw CM6), `live-preview` (CM6 + OFM decorations), `reading` (HTML renderer)
- Autosave with 1000ms debounce (`NoteEditor.svelte:55`)
- No rich block editor; stays in source/Markdown paradigm
- No H1-as-title enforcement; editor shows frontmatter `---` block + body
- No per-note width control
- No Table of Contents panel
- No whiteboard support
- Conflict detection with "Reload / Keep mine" banner (`NoteEditor.svelte:385`)

**Gap/Opportunity:**
- **Table of Contents panel** (`Cmd+Shift+T`): High-value for long notes. Tolaria derives it from live editor state. In Notesmith, parsing headings from the CM6 doc on the fly would be straightforward.
- **Per-note width** (`_width: wide` frontmatter): Simple but useful. Notesmith's editor is already full-width; a "focused writing mode" (centered, ~700px max-width) would be valuable.
- **H1-as-title** philosophy: Notesmith shows the full frontmatter including `---` delimiters in source mode. Tolaria hides frontmatter in the rich editor and the breadcrumb shows the filename. This is a philosophy call — Notesmith's approach of showing frontmatter directly is fine for power users.
- **Autosave semantics**: Notesmith's 1000ms delay is reasonable. Tolaria's ADR-0102 describes a "low-end-safe idle window" that pauses autosave if the system is under load.

---

### 3.3 Navigation Model

**Tolaria:**
- **Single note open at a time** — explicitly removed tabs (ADR-0003) as "2000 lines of complexity without proportional benefit"
- **Back/Forward navigation** (`Cmd+[` / `Cmd+]`) replaces tabs for history
- **Cmd+Shift+O**: Open current note in a new window (full App instance per secondary window — ADR-0031)
- **Neighborhood mode** (ADR-0069): Clicking a note normally opens it. Cmd/Ctrl-click _pivots_ the note list to show that note's related notes as a graph neighborhood. The selected note is pinned at top; outgoing relationships appear first, then inverse/backlinks, empty groups shown with count `0`.
- **Quick open**: `Cmd+P` (notes/files); separate from command palette
- **Command palette**: `Cmd+K` (commands)

**Notesmith:**
- **Multi-tab model** with drag-to-reorder, middle-click close, dirty-dot indicator, tab persistence in localStorage
- **Quick Switcher** (separate from command palette, likely `Cmd+O`)
- **Command Palette** (`Cmd+K`) with recent commands, category sections, fuzzy search
- No back/forward navigation history
- No neighborhood/graph browsing mode

**Gap/Opportunity:**
- **Neighborhood mode** is a standout Tolaria feature worth adapting. Rather than removing tabs (which Notesmith users likely value), consider adding a "graph view" mode to the file tree or a dedicated sidebar panel that shows the current note's relationship neighborhood. Cmd+Ctrl-click on a wikilink in the editor could activate it.
- **Back/Forward navigation** (`Cmd+[/]`): Notesmith has tabs, but no back/forward. Adding navigation history (separate from tabs) would be valuable for power users who open many notes sequentially.
- **Open in new window**: Low effort with Tauri. Useful for reference while writing.
- Tolaria's keyboard shortcut split (`Cmd+P` = quick open, `Cmd+K` = commands) is cleaner than Notesmith's current setup. Notesmith uses `Cmd+O` for quick switcher and `Cmd+K` for palette — already good.

---

### 3.4 Sidebar / Navigation Design

**Tolaria:**
- Left sidebar has: **FAVORITES** section (pinned notes with drag-to-reorder) | **VIEWS** section (custom saved views from `.laputa/views/*.yml`) | **TYPES** sections (grouped by type: Projects, People, etc.) | **FOLDERS** tree (optional)
- Section headers are collapsible
- Each type section shows the type's icon + color
- Clicking a type section header navigates to the type document (pinned at top of list)
- **VIEWS section** appears between FAVORITES and TYPES, hidden when no views exist

**Notesmith:**
- Sidebar has a **tab bar** at top (Files | custom view tabs) in a 2-column grid
- When on Files tab: shows folder/file tree
- When on custom view tab: shows sections (recently-viewed, custom-folders, custom-items) with collapsible headers
- Custom items open the `MiddlePane` (a sliding panel between sidebar and editor)
- Badge counts via SQL badge queries

**Gap/Opportunity:**
- **Favorites system**: Tolaria stores favorites as `_favorite: true` + `_favorite_index` in frontmatter (ADR-0038), so they sync via git. Notesmith has no favorites. This is a **high-value, low-effort** feature — add a FAVORITES section to the sidebar, toggled by a keyboard shortcut (`Cmd+D`).
- **Sidebar sections by type**: Tolaria's approach of grouping sidebar by entity type (Projects, People, etc.) is excellent for a personal knowledge base. Notesmith already has SQL-driven sections, which is more powerful, but less discoverable.
- **Sidebar tab bar UX**: Notesmith's current 2-column grid of tabs is awkward if there are 3+ views. Tolaria uses a vertical sidebar navigation instead. Consider switching to icons-only in a narrow strip, or vertical list.
- **Status bar for git**: Following ADR-0032, move git status out of the sidebar to a bottom status bar.

---

### 3.5 Types / Entity Model

**Tolaria:**
- `type:` frontmatter field assigns a note to a type (Project, Person, Topic, etc.)
- **Type documents** are Markdown notes with `type: Type`; they define icon, color, order, sidebar label, template, sort, visibility
- Types appear as sidebar sections, each with icon + color
- Types are **not inferred from folders** — purely from frontmatter (ADR-0006)
- **Flat vault structure**: all notes at vault root, no folder-based type organization (ADR-0006)
- System properties use `_` prefix: `_icon`, `_color`, `_order`, `_sidebar_label`, `_pinned_properties` (ADR-0008)
- `_pinned_properties` on a type document controls which properties appear in the inline editor bar
- Entity type navigable: the `type:` chip in the editor header navigates to the type document
- Instance defaults: creating a new note of a type copies the type document's default field values

**Notesmith:**
- Has `type` field in note frontmatter (referenced in FileTree's `typeIcon()` function)
- Types mapped to hardcoded emoji icons (daily=📅, meeting=🤝, customer=🏢, etc.) in `FileTree.svelte:31-44`
- No type documents, no type-level config
- Right rail shows frontmatter metadata but no type-aware display

**Gap/Opportunity:**
- **Per-note icons via frontmatter** (`_icon`): Notesmith's hardcoded emoji map in FileTree is brittle. Adopting Tolaria's `_icon` convention (emoji | Phosphor name | HTTP URL) gives users control without code changes.
- **Type document system**: Medium-effort, high-value. Allowing users to create `my-type.md` with `type: Type` to define icons, colors, templates for that type would make Notesmith's type system much more powerful without schema enforcement.
- **System property convention** (`_` prefix): Notesmith should adopt this for its own system fields. Hiding `_` prefixed frontmatter keys from the right rail's metadata display would be clean.
- **Pinned properties**: Tolaria shows `_pinned_properties` from a type document as inline chips in the editor header (status, priority, etc.). Notesmith's right rail metadata could be enhanced this way.

---

### 3.6 Relationships & Backlinks

**Tolaria:**
- **Dynamic relationship detection** (ADR-0010): ANY frontmatter field containing `[[wikilinks]]` is treated as a relationship
- Built-in relationships: `belongs_to` (parent), `has` (child, computed inverse of belongs_to), `related_to` (lateral, bidirectional)
- **Inverse relationships computed automatically**: if note A `belongs_to` [[project-B]], then project-B shows note A under its `has` relationship without reverse-linking
- **Neighborhood mode**: pivoting the note list to show all relationships around a note (outgoing groups + inverse/backlinks)
- Properties panel shows relationships as clickable chips with icon + label

**Notesmith:**
- Right rail shows backlinks (notes linking to this note) and outgoing links
- Both derived from SQL queries (`buildBacklinksQuery`, `buildOutgoingLinksQuery` in `right-rail.ts`)
- Metadata section shows frontmatter key-value pairs
- No inverse relationship computation
- No relationship-type grouping

**Gap/Opportunity:**
- **Inverse relationships**: If note A has `belongs_to: [[project-b]]`, Notesmith should show note A under project-b's "Has" section in the right rail. This is a significant navigation improvement.
- **Relationship grouping in right rail**: Currently Notesmith shows flat backlinks and outgoing links. Grouping by relationship type (belongs_to, related_to, etc.) would be more scannable.
- **Neighborhood/graph mode**: As noted in 3.3, a neighborhood view based on the current right rail would be powerful.
- **Custom relationship keys**: Notesmith's SQL-based backlinks work at the wikilink level. Tolaria's dynamic detection of any frontmatter field with wikilinks would require changes to Notesmith's index/query layer.

---

### 3.7 Git Integration

**Tolaria:**
- **Full in-app Git client**: commit, push, pull, diff, per-note history, conflict detection/resolution
- Git status in **status bar** (orange badge for changed file count)
- `Changes` panel shows current vault diff
- `Pulse` panel shows commit history
- **AutoGit** (ADR-0067): idle and inactive checkpoints that auto-commit on activity pause
- Crash-safe note renames via transaction directory (ADR-0075)
- External rename detection via `git diff` on focus regain (ADR-0036)
- Git is a **capability, not a requirement** — non-git vaults work fine (ADR-0085)

**Notesmith:**
- Has `GitSettings` section in settings (remote URL, branch, etc.)
- `notesmith-git` crate exists
- No visible in-app git UI (no diff panel, no commit UI in the main editor)

**Gap/Opportunity:**
- **Status bar git indicator**: At minimum, show a pending-changes badge that navigates to a diff view. Tolaria's ADR-0032 explains the UX rationale clearly.
- **Per-note history**: "View this note's git history" in the NoteToolbar would be valuable.
- **AutoGit checkpoints**: Consider periodic auto-commit when idle. Tolaria's ADR-0067 describes the semantics well.
- **Crash-safe renames**: Tolaria's ADR-0075 describes a `.tolaria-rename-txn/` pattern for atomic renames + wikilink updates — relevant if Notesmith adds rename functionality.

---

### 3.8 Command Palette & Keyboard Navigation

**Tolaria:**
- `Cmd+K`: Command palette (fuzzy-searches all registered commands)
- `Cmd+P`: Quick open (notes/files) — separate from command palette
- Every command must also appear in the macOS menu bar (ADR-0020)
- Central command registry with labels, shortcuts, handlers — `useCommandRegistry`
- All commands registered in a shared manifest (ADR-0106) enabling testable deterministic shortcut routing
- `Cmd+\`: Toggle raw mode
- `Cmd+Shift+T`: Toggle ToC
- `Cmd+Shift+I`: Toggle properties panel
- `Cmd+Shift+L`: Toggle AI panel
- `Cmd+[/]`: Back/Forward
- `Cmd+D`: Toggle favorite
- `Cmd+E`: Mark inbox note organized
- `Cmd+Shift+O`: Open in new window

**Notesmith:**
- `Cmd+K`: Command palette (with category sections, recent commands, fuzzy search) ✅
- `Cmd+O` or similar: Quick switcher (separate)  ✅
- Commands are plain objects in `commands.ts` — no formal registry/manifest
- Templates selected via `window.prompt()` ❌
- Note title entered via `window.prompt()` ❌
- Capture text via `window.prompt()` ❌
- Template list selection via multi-line `window.prompt()` ❌
- Hotkeys registered via `registerHotkeys` in `hotkeys.ts`

**Gap/Opportunity — CRITICAL:**
- **Replace ALL `window.prompt()` calls** in `commands.ts`: Note creation (`promptValue('Note title:')`), folder selection (`promptValue('Folder (optional):', 'Inbox')`), capture text, template selection. These are the most jarring UX issues in Notesmith. Replace with:
  - A proper `<dialog>` / modal component for text inputs
  - A command-palette-style picker for template selection (inline fuzzy list)
  - Ideally integrate folder picker and template selection directly into the command palette as a "sub-palette" (follow-up step after selecting a command)
- **Keyboard shortcut manifest** (ADR-0106): Centralizing all shortcuts in one object enables displaying them everywhere (palette, tooltips, menus) from a single source of truth. Notesmith's current approach of sprinkling shortcuts in `buildCommands()` strings is fragile.
- **Menu bar parity** (ADR-0020): All Notesmith commands should appear in Tauri's native menu — especially File, Edit, View, Note.

---

### 3.9 Settings Page

**Tolaria:**
- Settings page has **Vault state vs. App state** distinction (from concepts/vaults):
  - **Vault state** (synced via git): type icons/colors, saved views, pinned properties, relationship conventions, vault AI guidance files
  - **App state** (installation-local): editor zoom, window size, recent vault list, local cache, AI target selection
- Light/dark theme is app-level (installation-local preference)
- Title/filename auto-rename is app-level

**Notesmith:**
- Settings layout: left nav (220px) + right content ✅ — matches Tolaria's pattern
- Sections: General, Daily Notes, Editor, Sidebar, Git Sync, Hooks | App: Vaults ✅
- "Back to vault" button at top ✅
- Dirty indicator dots (●) on unsaved sections ✅ 
- Shows config file path in footer ✅

**Gap/Opportunity:**
- Notesmith's settings page is already well-structured. The main improvements:
  - **Add "Appearance" section**: light/dark theme toggle, editor font/zoom
  - **Add "Keyboard Shortcuts" section**: view/customize shortcuts
  - **Per-vault vs. app-level clarity**: The vault/app grouping is already in the left nav — maintain this distinction clearly

---

### 3.10 AI Integration

**Tolaria:**
- **Two AI paths** (ADR-0027/0028):
  1. **CLI coding agents** (Claude Code, Codex CLI, OpenCode, Pi, Gemini CLI) streamed through Tolaria's event layer with tool access to vault files
  2. **Direct model chat** (Ollama, LM Studio, OpenAI, Anthropic, Gemini, OpenRouter, custom endpoints) — chat mode with note context, no vault-write tools
- AI panel toggled by `Cmd+Shift+L`
- **AGENTS.md** at vault root: canonical AI guidance file read by coding agents
- **MCP server** exposed for external tools (Claude Code, Cursor, etc.) — ADR-0011/0074/0119
- **Permission modes**: Vault Safe (file/search/edit only) vs. Power User (local shell scoped to vault)
- AI-generated changes are inspectable via Git diff

**Notesmith:**
- Has `notesmith-mcp` crate — MCP server already implemented ✅
- No in-app AI chat panel currently

**Gap/Opportunity:**
- Notesmith already has MCP server. The most accessible next step is:
  - **AGENTS.md**: Auto-generate a vault-level `AGENTS.md` file with Notesmith's schema, conventions, and SQL query syntax — this makes the vault immediately useful to any AI coding agent
  - **AI panel** (`Cmd+Shift+L`): Long-term, add a streaming chat panel backed by a local/API model with vault context
  - **Tolaria's dual-mode AI** (agent with tools vs. chat-only) is the right architecture for notes apps

---

### 3.11 Theming

**Tolaria:**
- Internal light/dark themes via **semantic CSS custom properties** (ADR-0081)
- Previous vault-authored theming system was removed (ADR-0013) — too much maintenance burden
- Theme mode persisted as installation-local app settings
- Avoids "light-mode flash" with pre-paint localStorage mirror in `index.html`
- CSS variables are the public runtime contract for Tailwind v4, shadcn/ui, CodeMirror extensions
- System-follow mode exists (ADR-0112)

**Notesmith:**
- Currently dark theme only (hardcoded `#1e1e1e`, `#252526`, `#2d2d2d` in component styles)
- CSS custom properties defined in components but not centralized: `--sidebar-bg`, `--border-color`, `--text-primary`, `--text-muted`, `--hover-bg`, `--selected-bg`
- No light theme, no system-follow

**Gap/Opportunity:**
- **Centralize CSS variables** into a single `:root { }` block in `app.css` / `index.css` — every component already uses CSS vars, they're just not centralized
- **Add light theme** by adding a `.light` class override block
- **System-follow mode** via `prefers-color-scheme` media query
- Tolaria's ADR-0081 is a direct blueprint: semantic CSS contract → TypeScript helpers for CodeMirror extensions

---

### 3.12 Multi-Vault Support

**Tolaria:**
- **Mounted workspaces** (ADR-0114): multiple registered vaults loaded as one unified graph
- Notes tagged with workspace provenance (`WorkspaceIdentity` on `VaultEntry`)
- Cross-vault wikilinks use stable alias prefix: `[[team/projects/alpha]]`
- Git, folder tree, saved views scoped to "active" vault; search/wikilinks/backlinks span all mounted vaults
- Vault-level badge in note list when disambiguation needed

**Notesmith:**
- Multi-vault support via `VaultSwitcher` component
- Vaults are switched, not merged
- No cross-vault wikilinks

**Gap/Opportunity:**
- **Cross-vault graph merging** is complex but very useful. At minimum: when multiple vaults are open, show vault badge on notes in search results and file tree.
- Notesmith's SQL-based index could be extended to cross-vault queries with a vault column.

---

### 3.13 First-Launch & Onboarding

**Tolaria:**
- First-launch asks: clone Getting Started vault | open existing | create empty
- Getting Started vault cloned from GitHub and then disconnected from remote (safe to edit)
- Optional AI setup prompt after vault opens

**Notesmith:**
- No documented first-launch flow visible in codebase
- `VaultSwitcher` implies vaults are listed but no guided creation

**Gap/Opportunity:**
- **Getting Started vault**: A curated sample vault demonstrating Notesmith's features (SQL views, OFM syntax, templates, routing, hooks) would be extremely valuable for onboarding.

---

## 4. Prioritized Ideas to Incorporate

### 🔴 HIGH IMPACT — Fix These First

| # | Feature | Effort | Why |
|---|---|---|---|
| H1 | **Replace `window.prompt()` with proper modals** | Medium | Most jarring UX issue. Every command that creates/names a note uses native browser prompt. Replace with a proper `<dialog>` component and a sub-palette pattern for multi-step commands. |
| H2 | **Status bar at bottom** | Low-Medium | Git status badge, word count, save indicator, cursor position, vault name/switcher. Follows Tolaria ADR-0032 rationale exactly. |
| H3 | **Table of Contents panel** (`Cmd+Shift+T`) | Low | Parse headings from live CM6 document, show in collapsible right panel. Tolaria derives from live editor; in CM6 this is a syntax tree traversal. |
| H4 | **Favorites system** (`Cmd+D`) | Low-Medium | `_favorite: true` + `_favorite_index` in frontmatter. FAVORITES section at top of sidebar. Portable via git. |
| H5 | **Per-note icons** (`_icon` frontmatter) | Low | Support emoji, named icon (Phosphor/Heroicons), HTTP URL. Replace hardcoded `typeIcon()` emoji map in FileTree. |
| H6 | **Replace hardcoded type emoji map** with config-driven icons | Low | FileTree.svelte:31-44 has a hardcoded `icons` object. Should resolve from note metadata + per-type config. |
| H7 | **Centralize CSS variables + light theme** | Medium | Semantic CSS contract in one place. Light/dark theme toggle. System-follow mode. Prevents "dark mode only" limitation. |

### 🟡 MEDIUM IMPACT — Next Quarter

| # | Feature | Effort | Why |
|---|---|---|---|
| M1 | **Keyboard shortcut manifest** (centralized registry) | Medium | Single source of truth for shortcuts enables display in palette, tooltips, menus. Replaces string literals in `buildCommands()`. |
| M2 | **Back/Forward navigation** (`Cmd+[/]`) | Low-Medium | Navigation history independent of tabs. Complements the tab model. |
| M3 | **Relationship grouping in right rail** | Medium | Group backlinks by relationship type (belongs_to, related_to, custom). Show relationship key as section header. |
| M4 | **Inverse relationship computation** | Medium | If note A has `belongs_to: [[B]]`, show A under B's "Has" section automatically. |
| M5 | **System property convention** (`_` prefix) | Low | Hide `_*` frontmatter fields from right rail metadata display. Reserve for Notesmith's internal fields. |
| M6 | **Neighborhood / graph mode** | High | Cmd+click wikilink or entity to pivot note list/sidebar into that note's relationship neighborhood. |
| M7 | **Per-note width control** | Low | `_width: wide` frontmatter + global default. Editor with max-width constraint for focused writing. |
| M8 | **AGENTS.md auto-generation** | Low | Generate a vault-level AI guidance file explaining Notesmith's SQL schema, OFM conventions, frontmatter fields. |
| M9 | **Type document system** | High | `type: Type` notes defining icons, colors, templates, pinned properties. Replaces hardcoded type config. |
| M10 | **Inbox workflow formalization** | Low | Formalize capture → inbox → organized with `Cmd+E` to mark organized + auto-advance setting. |

### 🟢 LOW IMPACT / DEFERRED

| # | Feature | Effort | Why |
|---|---|---|---|
| L1 | **Open note in new window** (`Cmd+Shift+O`) | Medium | Useful for reference during writing. Tauri multi-window is supported. |
| L2 | **Git diff / history panel** | High | In-app git client; current git settings are already in Notesmith. Full diff UI is a significant undertaking. |
| L3 | **AutoGit checkpoints** | Medium | Auto-commit on idle. Useful but risky without good UX. |
| L4 | **Getting Started vault** | Medium | Onboarding vault demonstrating features. |
| L5 | **AI chat panel** | Very High | Long-term vision. MCP server is already there; in-app streaming chat would require new backend + UI. |
| L6 | **tldraw whiteboards** | Very High | Niche, complex dependency. Low priority for a text-first editor. |
| L7 | **Cross-vault unified graph** | High | Complex graph merging with alias-based wikilinks. |
| L8 | **Canary release channel** | Low | Alpha/stable channels. Engineering quality of life. |

---

## 5. Anti-Patterns / Things to Avoid

### 🚫 Don't Remove Tabs
Tolaria deliberately removed tabs (ADR-0003) as "2000 lines of complexity without proportional benefit." **This was the right call for Tolaria** — they were starting with a fresh UX. But Notesmith's tabs are well-implemented (drag-to-reorder, middle-click close, dirty indicator, persistent state) and Notesmith users likely expect a code-editor-like multi-tab model. **Do not remove tabs** — instead, add back/forward _as a complement_ to tabs.

### 🚫 Don't Mandate Flat Vault
Tolaria mandates a flat vault structure (ADR-0006) and provides a migration command. Notesmith's folder-based hierarchy with recursive scanning is a legitimate choice and aligns with how many users (coming from Obsidian) organize their notes. **Keep folder navigation** and add type-based grouping as an _alternative_ view, not a replacement.

### 🚫 Don't Reintroduce Vault-Authored Theming
Tolaria removed vault-authored themes (ADR-0013) because they created "too much maintenance burden." Notesmith should avoid letting users customize themes via YAML/frontmatter in vault files. Keep themes as app-level settings with a small, well-defined set (light/dark/system).

### 🚫 Don't Use `window.prompt()` for Anything Ever Again
This is the single biggest UX regression in Notesmith's current state. Every `promptValue()` call in `commands.ts` should be replaced before shipping anything else. The pattern of "command executes → native browser dialog → back to app" is jarring in a desktop app.

### 🚫 Don't Over-Couple Git to the Core Model
Tolaria initially required git (ADR-0034, later superseded by ADR-0085 for non-git vault support). Notesmith should treat git as an optional capability from day one — the app must work completely without a git remote.

### ⚠️ Be Careful with BlockNote-style Block Editors
Tolaria's big bet on BlockNote (Notion-like) is a valid choice but means Markdown source becomes less readable in the raw editor (blocks are serialized as Markdown but with some round-trip impedance). Notesmith's CodeMirror-all-the-way-down approach keeps the source cleaner and more portable. **Don't abandon CodeMirror** for a block editor — the OFM decorations, SQL blocks, live preview, and task handling are deep CM6 features that would need complete rewrites in BlockNote.

---

## 6. Visual / UX Design Observations

Based on the website screenshots and code inspection:

**Tolaria's Visual Identity:**
- Clean, minimal UI with generous whitespace
- Type-colored chips (blue for Projects, etc.) in note lists
- Phosphor icons throughout (consistent, not emoji-heavy)
- Status chips for note status (colored by lifecycle stage)
- Properties panel shows inline chips for relationships (clickable wikilinks)
- Breadcrumb bar above editor showing filename + type chip + status chip + URL chip + date badge
- Progress indicator for `goal:` / `result:` fields

**Notesmith's Visual Identity (current):**
- Dark-only VS Code-inspired color scheme (`#1e1e1e`, `#252526`)
- Emoji-based icons (📁📄📅🤝) — functional but inconsistent
- Minimal decorative UI; everything is text/borders
- Right rail uses UPPERCASE section labels with `▾/▸` toggles
- Command palette has good polish (16px rounded corners, proper blur backdrop, shortcut pills)

**Specific Visual Improvements:**
1. **Breadcrumb bar** in the editor: Replace the current `NoteToolbar` (which shows `path/to/note`) with a richer breadcrumb showing note title, type chip (colored), status chip. Clicking the type chip navigates to the type document.
2. **Metadata chips** in the right rail: Replace the current key/value rows with colored chips for status, clickable wikilink chips for relationships.
3. **Note list rows** in custom views: Add type icon + status chip to each row (the `MiddlePane` and `CustomItemsSection` currently show just title + subtitle).
4. **Section headers**: Tolaria uses lowercase labels without `text-transform: uppercase` in some places — less shouty than Notesmith's all-caps labels.

---

## 7. Gaps and Uncertainties

- **Tolaria's actual React component structure** was not accessed (private repo source, only docs/ADRs were readable). The exact UI component implementations are inferred from the ADRs and docs.
- **Website screenshots** were referenced in the HTML but the images are at `/landing/*.png` paths which require the website to load correctly — they couldn't be fetched as images in this research.
- **Tolaria's custom views** use a YAML filter engine (ADR-0040), while Notesmith uses SQL — Notesmith's approach is more powerful and flexible; no change needed there.
- **Tolaria's Properties panel** (the right-side panel with pinned properties, relationships, etc.) is more developed than Notesmith's right rail. The exact interaction model for the Properties panel isn't fully documented in accessible ADRs.
- **Notesmith's `notesmith-hooks`, `notesmith-routing`, `notesmith-tasks`** crates are unique to Notesmith — Tolaria has no equivalent. These are Notesmith advantages that need no changes.

---

## 8. Key Source Citations

| File | Lines | What It Shows |
|---|---|---|
| `ui/app/src/routes/+page.svelte` | 106–145 | Notesmith's three-pane layout, no status bar |
| `ui/app/src/lib/commands.ts` | 87–90, 98–113 | `window.prompt()` calls — must be replaced |
| `ui/app/src/lib/components/FileTree.svelte` | 31–44 | Hardcoded type→emoji mapping |
| `ui/app/src/lib/components/NoteEditor.svelte` | 55–88 | Autosave implementation (1000ms debounce) |
| `ui/app/src/lib/components/RightRail.svelte` | 1–125 | Right rail with backlinks, outgoing links, metadata |
| `ui/app/src/lib/components/TabBar.svelte` | 56–95 | Tab model with drag-and-drop |
| `ui/app/src/routes/settings/+page.svelte` | 101–135 | Settings left nav + right content layout |
| Tolaria ADR-0003 | — | Deliberately removed tabs (no tabs model) |
| Tolaria ADR-0020 | — | Keyboard-first design: every feature via keyboard, menu bar parity |
| Tolaria ADR-0032 | — | Git actions in status bar, not sidebar |
| Tolaria ADR-0038 | — | Frontmatter-backed favorites (`_favorite`) |
| Tolaria ADR-0040 | — | Custom views as `.yml` files |
| Tolaria ADR-0049 | — | Per-note `_icon` property (emoji/Phosphor/URL) |
| Tolaria ADR-0069 | — | Neighborhood mode for graph browsing |
| Tolaria ADR-0081 | — | Internal light/dark theme via semantic CSS variables |
| Tolaria ADR-0107 | — | tldraw whiteboards as fenced blocks |
| Tolaria ABSTRACTIONS.md | — | `VaultEntry` TypeScript interface, semantic field names, system properties |