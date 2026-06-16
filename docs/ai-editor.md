# AI Commands in the Editor

Notesmith lets you run AI commands directly against a text selection in the editor — no need to open the chat panel or copy and paste. Select some text, right-click, choose a command, and the result lands back in the note automatically.

---

## Prerequisites

Inline commands share the same agent session as the chat panel. Before using them:

1. Open the agent panel and start an agent session.
2. Switch to **Source mode** or **Live Preview** so the editor is editable and text selection is active.

If no agent session is running when you trigger a command, a warning toast — *"Start the agent panel first."* — appears and nothing is sent.

See [docs/ai-chat.md](ai-chat.md) for how to start an agent session.

---

## Invoking the Context Menu

1. Select any span of text in the editor (a word, a sentence, a whole paragraph).
2. **Right-click** the selection.
3. A floating menu appears at the cursor position with the six AI commands listed below.
4. Click a command to run it, or press **Escape** (or click outside) to dismiss without doing anything.

The same six commands are also available from the **command palette** (`⌘K` → type the command name). They appear under the **AI** category.

> The context menu only appears when there is a non-empty selection. Right-clicking with no selection falls through to the native browser context menu.

---

## The Six Commands

| Command | What it does | Apply mode |
|---------|-------------|------------|
| **Rewrite** | Rewrites the selection to improve clarity and flow while preserving meaning | replace |
| **Summarize** | Summarizes the selection concisely | replace |
| **Expand** | Expands the selection with more detail | replace |
| **Fix** | Fixes spelling, grammar, and punctuation without changing meaning | replace |
| **Continue writing** | Continues writing from the selection in the same voice and style | insert |
| **Custom prompt…** | Opens a text input; sends your instruction verbatim against the selection | replace |

---

## Apply Modes

The apply mode controls where the agent's output lands in the document.

### `replace` — swap the selection

The selected text is removed and replaced by the AI output. The cursor ends up immediately after the new text. This is the mode used by **Rewrite**, **Summarize**, **Expand**, **Fix**, and **Custom prompt…**.

### `insert` — add after the selection

The AI output is inserted at the cursor position, immediately after the end of the selection. The original selected text stays in place. This is the mode used by **Continue writing**.

Each apply is a single undoable transaction — `⌘Z` reverses the change in one step.

---

## Walkthrough: Rewriting a Paragraph

1. Open a note in Source mode or Live Preview.
2. Select a paragraph that feels unclear or wordy.
3. Right-click the selection → the AI context menu appears.
4. Click **Rewrite**.
5. The menu closes and the agent processes the selection in the background. The chat panel shows the turn (the instruction and the agent's response) as a normal conversation item.
6. When the agent finishes, the selected paragraph is replaced with the rewritten version in the editor.
7. Read the result. If you prefer the original, press **`⌘Z`** to undo the replacement in one step.

---

## Tips

**Continue writing inserts rather than replaces — your selection stays.**
Select the last sentence or paragraph of what you have written and choose **Continue writing**. Because the apply mode is `insert`, the original text is not touched; the continuation is added immediately after it. This makes it easy to extend a draft without losing what you wrote.

**Custom prompt… sends your own instruction verbatim against the selection.**
When none of the five named commands fits your need, choose **Custom prompt…**. An input box appears with the placeholder *"e.g. Make this more formal"*. Type any instruction and press Enter. The instruction is sent exactly as you typed it, applied to the current selection, and the result replaces that selection.

---

## Permissions and Read-Only Mode

Inline commands run through the same agent session as the chat panel. If the agent is in **read-only mode**, it cannot write to notes — but inline commands write directly to the editor buffer (not through the agent's file-write tools), so they still apply output back to the document regardless of the agent's read/write setting.

For more on read-only mode, permission grants, and the diff-preview flow for agent-initiated file changes, see [docs/ai-permissions.md](ai-permissions.md).
