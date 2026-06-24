# Manual Validation Checklist — AI Agent Integration Roadmap

Tracks the features delivered under the **AI Agent Integration Roadmap** epic
([#183](https://github.com/surdy/notes-method/issues/183)) and gives you a
hands-on checklist for validating each one manually.

**As of this checklist, 13 of 26 leaf features are implemented** (all of
Phase 0, Phase 1, and the active Phase 4 items). Phases 2 and 3 are fully
backlogged pending the local-embeddings/transcription model decision (ADR 0015),
and two Phase 4 items remain backlogged — these are listed at the end as **Not
yet implemented** so the checklist stays exhaustive.

> **Setup once before validating:** Launch the desktop app, open a vault, and
> make sure an ACP agent (Copilot CLI / Claude Code / Codex / Gemini) is detected
> in the agent picker. Most chat/editor checks happen in the **Right Dock → Chat**
> segment. CLI checks use the `notesmith` binary.

---

## Phase 0 — Foundation polish & verification ([#184](https://github.com/surdy/notes-method/issues/184))

- [ ] **Diff preview before agent writes + tiered permissions** ([#189](https://github.com/surdy/notes-method/issues/189)) — Ref: [docs/ai-permissions.md](ai-permissions.md)
  - Ask the agent (read-write session) to create/update/append/archive a note.
  - **Verify:** a **diff/preview** of the proposed change appears *before* it is applied.
  - **Verify:** the permission prompt offers **Allow Once / Allow This Session / Always Allow**.
  - **Hint — Session scope:** pick *Allow This Session*, ask for a second write → it should apply with **no** re-prompt for the rest of the session.
  - **Hint — Always scope:** pick *Always Allow*, then fully **quit and relaunch the app** → the same write type should still be auto-approved (persisted across restart).

- [ ] **Session history: fork thread + export chat to note** ([#190](https://github.com/surdy/notes-method/issues/190)) — Ref: [docs/ai-chat.md](ai-chat.md)
  - **Verify fork:** open an existing thread, choose **Fork** → a new thread is created that already contains the prior messages and can be continued independently (original is untouched).
  - **Verify export:** choose **Export to note** → a markdown note lands in the vault (inbox/create path) with **role-labelled** messages and metadata (**agent, model, timestamps**).
  - **Hint — regression check:** confirm the existing **list / open / delete / rename** thread actions still work after forking.

- [ ] **Model/mode pickers + Stop / Regenerate / New chat** ([#191](https://github.com/surdy/notes-method/issues/191)) — Ref: [docs/ai-chat.md](ai-chat.md)
  - **Verify model picker:** it lists models actually advertised by the selected agent; switching applies the choice.
  - **Verify mode toggle:** read-only ↔ read-write stays in sync with the session's MCP scope (read-only sessions can't write).
  - **Hint — Stop:** start a long generation, hit **Stop** → it cancels mid-stream and the UI returns to idle.
  - **Hint — Regenerate:** re-runs the **previous user turn** (not a duplicate send). **New chat** opens a fresh empty thread.

- [ ] **Diagnostics: error log + ACP wire log + version check** ([#192](https://github.com/surdy/notes-method/issues/192))
  - Open the agent **Diagnostics** surface (built on the ADR 0013 agent-discovery diagnostics).
  - **Verify:** recent agent **errors** are listed with enough detail to debug.
  - **Verify:** the **ACP wire-protocol** messages are viewable via a verbose/toggle mode.
  - **Hint — version:** the detected **agent binary version** is shown; if you point it at an old/unsupported binary, an **outdated/unsupported warning** appears.

---

## Phase 1 — Out-of-the-box chat magic ([#185](https://github.com/surdy/notes-method/issues/185))

- [ ] **Custom prompts (static): config defaults + vault `_prompts/` overrides** ([#193](https://github.com/surdy/notes-method/issues/193)) — Ref: [docs/ai-slash-commands.md](ai-slash-commands.md)
  - **Verify defaults:** built-in prompts are seeded into the daemon **config dir** on first run and survive a restart.
  - **Verify overrides:** drop a markdown file with `name` + `description` frontmatter into the vault's **`_prompts/`** folder → it is discovered and listed.
  - **Hint — precedence:** give a `_prompts/` file the **same name** as a default → the vault entry **wins**.
  - **Hint:** static text only for now; the file format is forward-compatible with future `{{variables}}`.

- [ ] **Default slash-command set in chat** ([#194](https://github.com/surdy/notes-method/issues/194)) — Ref: [docs/ai-slash-commands.md](ai-slash-commands.md)
  - Type **`/`** in the chat composer → a filterable command palette opens.
  - **Verify defaults present:** `/summarize` `/rewrite` `/outline` `/fix` `/tags` `/links` `/daily` `/new` `/ask`.
  - **Hint:** selecting a command sends/inserts its prompt text; your `_prompts/` entries appear as slash commands **alongside** the defaults. (Depends on #193.)

- [ ] **Inline editor commands on selection** ([#195](https://github.com/surdy/notes-method/issues/195)) — Ref: [docs/ai-editor.md](ai-editor.md)
  - Select text in a note. Via **right-click context menu** *and* the **command palette**, verify all six actions are available: **Rewrite, Summarize, Expand, Fix, Continue writing, Custom prompt**.
  - **Verify:** each sends the selection + instruction to the agent over ACP and the result is applied (replaces the selection by default).
  - **Hint:** this must work **without opening the chat panel**.

- [ ] **Apply agent output to document (insert / replace / append)** ([#196](https://github.com/surdy/notes-method/issues/196)) — Ref: [docs/ai-editor.md](ai-editor.md)
  - On a chat message, verify the actions: **Insert at cursor**, **Replace selection**, **Apply-to-note (append)**.
  - **Verify positioning:** insert lands at the cursor; replace swaps the current selection; append adds to the end of the active note.
  - **Hint — undo:** each apply must be a **single undo step** (one ⌘Z reverts it cleanly).

- [ ] **Context attachment: @-mentions + pills + active-note auto-include** ([#197](https://github.com/surdy/notes-method/issues/197)) — Ref: [docs/ai-chat.md](ai-chat.md)
  - In the composer, verify autocomplete for **@note**, **@folder**, **@tag**, **@url**.
  - **Verify:** the active note is **auto-included** (with a toggle); the current selection is included when present.
  - **Verify:** attached context shows as **removable pills**; removing a pill drops it from context.
  - **Hint:** `@note/@folder/@tag` resolve through existing MCP read/list tools. **`@url` is passed as plain text only** for now — daemon-side fetching is deferred to the backlogged `web_fetch` tool; the UI should note this.

---

## Phase 4 — Scale & CLI edge ([#188](https://github.com/surdy/notes-method/issues/188))

- [ ] **Headless CLI `notes ai` commands** ([#209](https://github.com/surdy/notes-method/issues/209)) — Ref: [docs/cli.md › `ai`](cli.md)
  - **Summarize:**
    ```bash
    notesmith ai summarize today
    notesmith --format json ai summarize Projects/roadmap.md --agent claude
    ```
    **Verify:** prints a summary to stdout; `--format json` wraps it as `{ "summary": "..." }`.
  - **Weekly digest:**
    ```bash
    notesmith ai weekly-digest
    notesmith --format json ai weekly-digest > digest.json
    ```
    **Verify:** a digest of the current week's notes (Mon–Sun) is produced.
  - **Hint — safety:** runs are **read-only by default** (deny-by-default decider, binds `/mcp-ro/<vault>`). Test that writes are refused without `--allow-writes`, and that `--allow-writes` opts into the read-write scope (auto-approves with **no review** — use carefully).

- [ ] **Customization discovery (agents / skills / instructions)** ([#210](https://github.com/surdy/notes-method/issues/210))
  - Place custom agents/skills/instructions in the recognized **project + global** directories (see the documented format).
  - **Verify:** they are **auto-discovered on startup** and surfaced in the UI (e.g. the agent picker).
  - **Hint — resilience:** drop a **malformed** file in a discovery dir → it should **warn-and-skip**, not crash discovery. Relates to the `notesmith skill` CLI surface.

- [ ] **MCP server management UI** ([#211](https://github.com/surdy/notes-method/issues/211)) — Ref: [docs/ai-mcp-servers.md](ai-mcp-servers.md)
  - Open the MCP servers settings surface.
  - **Verify:** it lists MCP servers **including the built-in daemon vault tools**, each with a **status** indicator.
  - **Verify:** you can **enable/disable** servers and **add an external** one (command/url, args, env).
  - **Hint — persistence:** add a server, restart the app → the config persists and is handed to the agent session. Check the documented **per-vault vs global** scope behaves as described.

- [ ] **@agent routing in chat** ([#212](https://github.com/surdy/notes-method/issues/212))
  - Type **`@agent-name`** in the composer → the message/turn targets that specific discovered agent; an **agent picker** lets you browse discovered agents.
  - **Hint:** confirm the documented v1 routing behavior — whether it **switches the session's active agent** or does **true per-message routing**. (Depends on #210.)

---

## Not yet implemented (for reference)

These remain **open / backlogged** and are out of scope for this validation pass.

### Phase 2 — Retrieval / second brain ([#186](https://github.com/surdy/notes-method/issues/186)) — *fully backlogged*
- [#198](https://github.com/surdy/notes-method/issues/198) Embedding backend: local model runtime + vector store
- [#199](https://github.com/surdy/notes-method/issues/199) `vault_search` MCP tool (hybrid lexical + semantic)
- [#200](https://github.com/surdy/notes-method/issues/200) `time_query` MCP tool
- [#201](https://github.com/surdy/notes-method/issues/201) Relevant Notes panel (similarity + graph-link scoring)
- [#202](https://github.com/surdy/notes-method/issues/202) `vault_stats` / structure MCP tool

### Phase 3 — Memory & multimodal ([#187](https://github.com/surdy/notes-method/issues/187)) — *fully backlogged*
- [#203](https://github.com/surdy/notes-method/issues/203) `memory` MCP tool (save / recall)
- [#204](https://github.com/surdy/notes-method/issues/204) Voice / meeting transcription → structured note
- [#205](https://github.com/surdy/notes-method/issues/205) PDF / EPUB ingestion as context
- [#206](https://github.com/surdy/notes-method/issues/206) Image / vision input
- [#207](https://github.com/surdy/notes-method/issues/207) `web_fetch` + `web_search` MCP tools
- [#208](https://github.com/surdy/notes-method/issues/208) `youtube_transcript` MCP tool

### Phase 4 — remaining backlog
- [#213](https://github.com/surdy/notes-method/issues/213) Projects / scoped workspaces
- [#214](https://github.com/surdy/notes-method/issues/214) Terminal integration (ACP `terminal/*`)
