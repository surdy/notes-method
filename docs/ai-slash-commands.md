# AI Chat Composer: Slash Commands & `@` Context

The chat composer has two power features for driving the agent faster and feeding it the right notes: **slash commands** (type `/`) to invoke saved prompts, and **`@`-mentions** (type `@`) to attach notes, folders, tags, and URLs as context.

See [AI Chat](ai-chat.md) for a general overview of the chat panel.

---

## Slash commands (`/`)

### Opening the palette

Type `/` at the very start of the composer. The palette appears immediately and lists every available command. Keep typing to narrow the list — the palette stays open as long as your input begins with `/` and contains no space. As soon as you add a space or type anything before the `/`, the palette closes and the input is treated as a normal message.

| Input | Palette state |
|-------|---------------|
| `/` | Open, shows all commands |
| `/sum` | Open, filtered to commands matching `sum` |
| `/sum ` | Closed — space ends the command token |
| `hi /sum` | Closed — `/` must be at position 0 |

### How filtering works

Filtering is case-insensitive and uses a two-tier ranking:

1. **Prefix matches** — commands whose name *starts with* your query appear first.
2. **Substring matches** — commands whose name merely *contains* your query appear below.

Within each tier the original order is preserved, so vault prompts that override a built-in of the same name remain in their correct slot.

### Selecting a command

Use **↑ / ↓** to move through the list, then **Enter** or click to select. Selecting a command replaces the `/…` token in the composer with the command's prompt body, ready to review and send.

### Vault badge

Commands loaded from your vault's `_prompts/` folder show a small **vault** badge next to their name. If a vault prompt shares a `name` with a built-in, your vault version wins.

### Built-in commands

Nine commands are available out of the box:

| Command | Description |
|---------|-------------|
| `/ask` | Answer a question using the vault |
| `/daily` | Draft today's daily note |
| `/fix` | Fix spelling and grammar |
| `/links` | Suggest wikilinks to related notes |
| `/new` | Draft a new note from an idea |
| `/outline` | Structured outline of the note |
| `/rewrite` | Rewrite for clarity and flow |
| `/summarize` | Concise summary of the current note |
| `/tags` | Suggest relevant tags |

---

## Custom prompts

Author your own slash commands as `*.md` files in `<vault>/_prompts/`. Each file is picked up automatically the next time the palette loads — no restart required.

### File format

Each prompt file requires YAML frontmatter followed by a Markdown body:

```markdown
---
name: my-command
description: Short label shown in the palette.
---
The prompt body sent verbatim to the agent.
```

**Frontmatter fields:**

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Recommended | The command identifier — appears as `/name` in the palette. Falls back to the file's stem if omitted. |
| `description` | Optional | Short human-readable label shown next to the command name. |

### Complete example

File: `<vault>/_prompts/compare-notes.md`

```markdown
---
name: compare-notes
description: Compare two notes and highlight what each one covers.
---
I have attached two notes as context. Please compare them:

1. What does each note cover?
2. Where do they agree or contradict each other?
3. Which has more detail on each topic?

Suggest which note should be treated as canonical and what content to merge.
```

After saving this file, type `/comp` in the composer — the palette shows `/compare-notes` with a **vault** badge. Press Enter to expand the body into the composer.

> **Tip:** Prompt files in `<vault>/_prompts/` are plain markdown files that live inside your vault directory. When you enable git sync (`[git]` in `vault.toml`), your custom prompts are versioned alongside your notes — shareable with your team and tracked in history. See [Vault Configuration](vault-configuration.md) for git sync settings.

---

## `@`-mention context

### Opening the autocomplete

Type `@` at a word boundary (at the start of the input, or right after a space) to open the autocomplete popup. Continue typing your query to filter candidates. Any whitespace after the `@` closes the popup.

### Attachment kinds

Four kinds of reference can be attached:

| Kind | What it attaches |
|------|-----------------|
| `note` | A single note, referenced by its vault-relative path |
| `folder` | A folder (the agent can list and read its contents) |
| `tag` | A tag, e.g. `#meeting` |
| `url` | A web URL passed as text to the agent |

### Narrowing by kind

Use the `kind:` prefix before your query to search only within a specific kind:

| You type | Searches |
|----------|---------|
| `@Sprint` | All kinds — notes, folders, tags, URLs |
| `@note:Sprint` | Notes only |
| `@folder:Projects` | Folders only |
| `@tag:meeting` | Tags only |
| `@url:github` | URLs only |

### Context pills

Each attached item appears as a small pill above the composer showing the kind label and the item's name. The active note (currently open in the editor) is auto-included as an **active** pill with a checkbox — uncheck it to exclude it from the outgoing message.

To remove a manually added attachment, click the **×** on its pill. Attaching the same item twice has no effect — duplicates are silently deduplicated.

> **Note:** URL attachments are passed to the agent as the raw URL string only; the page is not fetched by Notesmith.

### What gets sent to the agent

When you send a message with attachments, a compact `[Context]` block is prepended to your prompt:

```
[Context]
- note: Projects/Work/Sprint Planning.md
- note: Projects/Work/Retrospective.md
- folder: Projects/Research
- tag: #reading-list
- url: https://example.com/spec
Use your read/list tools to fetch referenced notes/folders/tags.
```

The agent resolves note content, folder listings, and tag matches through its MCP read/list tools — Notesmith never fetches note bodies in the frontend.

---

## Walkthrough: compare two notes

1. Open the chat composer.
2. Type `@Sprint` and select **Projects/Work/Sprint Planning.md** from the autocomplete.
3. Type a space, then `@Retro` and select **Projects/Work/Retrospective.md**.
4. Both notes appear as pills above the composer.
5. Type `/comp` and select `/compare-notes` (vault) to expand the prompt body.
6. Review the prompt, then press **Enter** to send.

The agent receives the `[Context]` block listing both note paths, reads them via MCP, and responds with the comparison.

---

## See also

- [AI Chat](ai-chat.md) — overview of the chat panel, agent selection, and session scope
- [Vault Configuration](vault-configuration.md) — full reference for per-vault `_prompts/` and git sync settings
