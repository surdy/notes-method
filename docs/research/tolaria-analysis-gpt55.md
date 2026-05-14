# Tolaria → Notesmith Design/UX Research Report (GPT 5.5)

## Executive summary

Tolaria's core philosophy is **local-first, files-first, Git-aware, AI-ready, keyboard-first knowledge work**. Its strongest UX idea is not any one widget, but the way it treats Markdown files, Git history, semantic types, relationships, AI agents, and command-driven navigation as one coherent product language. Tolaria's public positioning emphasizes plain Markdown/YAML files, no lock-in, Git versioning, rich block editing that still persists to Markdown, native relationships, and AI workflows.

Notesmith already shares many foundations: plain Markdown source of truth, SQLite/Tantivy as rebuildable caches, Rust/SvelteKit/Tauri, multi-vault architecture, CodeMirror 6, command palette, quick switcher, capture, SQL dashboards, templates, routing, MCP/agent workflows, SSE, and deep links. The biggest opportunities from Tolaria are therefore **polish and product framing**: better Git UX, semantic relationship navigation, TOC/right-rail modes, onboarding vaults, type customization, AI permission boundaries, and settings clarity.

---

## Sources reviewed

- Tolaria landing page and website docs (VitePress sidebar config)
- Tolaria README and repo structure
- Key docs: vaults, notes, editor, types, relationships, inbox, Git, AI, file layout, custom views, keyboard shortcuts, display preferences
- Key ADRs: keyboard-first design, BlockNote rich editor, AutoGit, custom views, neighborhood mode, direct model AI targets, unified multi-workspace graph, theme runtime, editor responsiveness
- Key source files: command palette, quick open, sidebar, settings panel, raw editor, CodeMirror hook, editor schema, TOC panel, right rail

---

## Tolaria design philosophy

### 1. Files are sacred; app state is derived

Tolaria treats the filesystem as the source of truth: notes are Markdown, YAML frontmatter gives structure, attachments are normal files, type definitions and saved views are also files, and Git is layered on top as a capability. The docs explicitly state Tolaria should never become the only way to read user data.

**Notesmith comparison:** Already aligned. No major change needed. Keep this as a marketing/design pillar.

### 2. Git is visible product UX, not just backend plumbing

Tolaria presents Git as an in-app history/sync/recovery layer: users can review changes, commit, pull, push, inspect whole-vault history, and inspect per-note history/diffs without leaving the app. It also supports conservative AutoGit checkpoints after idle/inactive thresholds, only for Git-backed vaults and only when saved changes exist.

**Notesmith comparison:** Notesmith plans Git as thin, opt-in support for status, pull, push, auto-commit, auto-pull, and auto-push. **High impact opportunity**: add per-note history/diff UX and whole-vault change review.

### 3. Types and relationships are first-class navigation

Tolaria prefers semantic `type:` and relationship frontmatter over folder hierarchy. Types can define sidebar grouping, icons, colors, order, labels, pinned properties, and new-note defaults. Relationships are frontmatter fields containing wikilinks; Tolaria supports default relationships plus custom relationship fields and inverse relationships.

**Notesmith comparison:** Notesmith has typed frontmatter but its domain model is more fixed and customer/folder-oriented. It plans SQL-powered views rather than Tolaria's type-first sidebar model. **High impact, selectively**: adopt type icons/colors/labels, pinned properties, relationship fields as navigable metadata, inverse relationship display, and "related context" views.

---

## Feature-by-feature comparison

| Area | Tolaria | Notesmith | Recommendation |
|---|---|---|---|
| Vault model | Plain Markdown folder; Git optional; app state derived | Plain Markdown; SQLite/Tantivy caches | Already aligned |
| Multi-vault | Unified graph across mounted workspaces | Multi-vault from day one; per-vault caches | Medium/high: consider cross-vault search later |
| Editor | Rich BlockNote + raw Markdown mode | CodeMirror 6 source, live preview, reading view | Do NOT switch to BlockNote; adopt TOC/width ideas |
| Raw editing | CodeMirror with YAML error banner, wikilink autocomplete | CodeMirror is primary | Copy YAML diagnostics/autocomplete |
| Command palette | Grouped fuzzy commands; footer hints; AI prompt mode | Command palette as primary navigation | Medium: copy grouped results + footer hints |
| Quick open | Separate note/file quick-open overlay | Quick switcher exists | Already aligned |
| Sidebar | Favorites, custom views, types, folders, DnD reorder | Files tab + custom view tabs + middle pane | Notesmith's SQL sidebar is stronger; add polish |
| Custom views | Structured saved filters; nested conditions | SQL-only views/dashboards | Medium: keep SQL, add visual query builder |
| Relationships | Frontmatter wikilinks, inverses, Neighborhood mode | Backlinks/outgoing links planned | High: add relationship neighborhood panel |
| Inbox/capture | Optional Inbox; fast capture first, organize later | Capture folder + routing + backlog-zero | Already strong; copy auto-advance triage UX |
| Settings | Modal with left nav; local-vs-vault clarity | Dedicated /settings page with sections | Copy local-vs-vault settings clarity |
| Theming | Light/Dark/System; no custom themes | Dark theme by default | Medium: add Light/Dark/System |
| AI | CLI agents with safe/power modes + direct chat models | CLI/MCP/agent-first architecture | High: adopt permission boundaries |
| Performance | Lazy/debounced worker indexes; generation checks | Performance targets defined | High: adopt responsiveness contracts |

---

## Highest-value ideas for Notesmith

### High impact

1. **Per-note Git history and diff UI** — Make Git understandable inside the app. Per-note history, current note diff, and whole-vault change review.

2. **Relationship/neighborhood navigation** — Neighborhood mode turns relationships into a browsing mode. Show customer/stream/task relationships around the current note.

3. **Right-rail mode switcher: metadata / backlinks / TOC / AI** — Mutually exclusive focused panels. Adding TOC and mode exclusivity would reduce clutter.

4. **Lazy/debounced side-panel indexes** — Build TOC/backlinks/SQL previews lazily and off-thread. Never block typing.

5. **AI permission boundaries** — Clear "read/chat" vs "write/tool" modes for agent interactions.

6. **Onboarding / getting-started vault** — Ship a sample vault demonstrating capture, routing, SQL dashboards, templates, tasks, and deep links.

### Medium impact

1. **H1 as display title, filename visible separately** — Better note presentation.
2. **Per-note width setting** — Great for dashboards, tables, and meeting notes.
3. **Grouped command palette with footer hints** — Polish for keyboard-first UX.
4. **Visual type customization** — Icons/colors/sidebar labels for note types.
5. **Light/Dark/System instead of custom themes** — Avoid theme marketplace burden.
6. **Search result metadata subtitles** — Type badges, modified age, word count.

### Low impact / nice polish

1. Leading-space AI prompt mode in command palette
2. Status bar AI target switcher
3. Sidebar pluralization settings
4. Phosphor-style icon consistency

---

## Anti-patterns / things to avoid

1. **Do not switch to BlockNote.** Notesmith's core value is OFM fidelity with CodeMirror 6. Rich/block editors create Markdown round-trip complexity.

2. **Do not overbuild custom theming early.** Tolaria removed vault-authored themes due to high maintenance burden. Light/Dark/System is enough.

3. **Do not make Git mandatory.** Tolaria moved toward supporting non-Git folders. Notesmith's opt-in stance is correct.

4. **Do not let side panels compete with typing performance.** Avoid synchronous derived-index work on every keystroke.

5. **Do not blur AI write permissions.** Maintain clear split between tool-capable agents and chat-only targets.

---

## Final recommendation

Notesmith should not copy Tolaria wholesale. Notesmith already has a stronger fit for its user's workflow: SQL dashboards, routing, templates, capture triage, customer folders, OFM compatibility, and agent/CLI-first design. The best Tolaria ideas to incorporate:

1. **Git UX:** per-note history/diffs, whole-vault review, optional AutoGit
2. **Relationship UX:** frontmatter wikilink relationships, inverse links, neighborhood view
3. **Right rail modes:** metadata/backlinks/TOC/AI as mutually exclusive focused panels
4. **Onboarding vault:** sample workspace demonstrating the full method
5. **Settings clarity:** local app preferences vs vault-traveling settings
6. **Performance discipline:** lazy/debounced/off-thread derived panels
7. **AI safety model:** explicit chat-only vs tool/write-capable modes

Tolaria's biggest lesson is that a Markdown notes app feels polished when its technical commitments—files, Git, relationships, AI, keyboard navigation—are visible as simple product affordances, not hidden implementation details.
