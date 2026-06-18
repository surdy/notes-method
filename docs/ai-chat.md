# AI Chat

The AI Chat panel lets you have a conversation with an AI agent — Copilot, Claude, Codex, Gemini, or a custom agent — right inside Notesmith. The agent can read your notes, answer questions about your vault, and (when you unlock it) make changes on your behalf.

This guide covers everything from opening the panel for the first time to managing multiple conversation threads.

---

## Opening the Chat Panel

The Chat panel lives in the **right dock** — the collapsible column on the right side of the app. See [Desktop App Guide § 10](app-guide.md#10-right-dock-context--chat) for a full description of the dock.

Three ways to open it:

| Action | Result |
|---|---|
| Press **⌘\\** | Toggles the entire right dock open or closed |
| Click the **✦** button in the workspace chrome (shown while the dock is collapsed) | Reopens the dock directly on the Chat tab |
| Click the **Chat** tab in the right dock's tab row | Switches from a Context tab to Chat (or back) |

The dock's tab row reads **Metadata**, **Links**, **TOC**, and **Chat**. Notesmith remembers your last-used tab per vault, so Chat stays selected the next time you open the dock.

> **Tip:** The agent process doesn't start until you open Chat for the first time in a session. Switching back to Context and then to Chat keeps the conversation alive — the agent stays running in the background.

---

## Your First Chat

Here's the quickest way to try the panel:

1. Press **⌘\\** (or, while collapsed, click **✦**) to open the right dock.
2. Click the **Chat** tab if a Context tab is active.
3. In the **Agent** dropdown at the top of the panel, select **Copilot** (or whichever agent is marked available).
4. Type in the message box:

   ```
   Summarize this note
   ```

5. Press **Enter** (or click **Send**).

The agent reads your currently open note via its context tools and replies with a summary. That's it — your first AI-assisted chat in Notesmith.

---

## Picking an Agent

The **Agent** dropdown near the top of the panel lists every agent Notesmith found on your PATH:

- **Copilot** — GitHub Copilot CLI (`gh copilot`)
- **Claude** — Anthropic Claude CLI
- **Codex** — OpenAI Codex CLI
- **Gemini** — Google Gemini CLI
- **OpenCode** — and any other ACP-compatible agent

Agents are detected automatically when the app starts. Available agents appear first; unavailable ones are shown disabled, labelled **(not found)**. If no agents are found at all, the panel shows a message with a link to **Open AI Agent settings** — see [Troubleshooting](#troubleshooting-diagnostics) below.

**Switching agents** resets the active session so the new agent starts fresh. The model picker updates to reflect the new agent's available models.

> **Tip:** If an agent you installed isn't showing up, go to **Settings → AI Agent → Diagnostics** and click **Run diagnostics** to see exactly which directories Notesmith searched and why an agent was not found.

---

## The First-Message Delay ("Connecting…")

When you send your first message in a session, Notesmith spawns the agent process in the background. This takes a few seconds — you'll see a **Connecting…** status message in the conversation while it happens. Subsequent messages in the same session are instant.

Notesmith also establishes the session eagerly when the panel opens (so the model picker populates before you type). If the agent isn't available, no scary error appears — the session simply retries when you send your first real message.

---

## Choosing a Model

Once a session is connected, a **Model** dropdown appears next to the Agent dropdown. It shows the models available for the active agent. Select a different model and the change takes effect immediately for subsequent turns.

The Model picker is only visible after the session connects successfully. If you don't see it yet, wait for the agent to connect or send your first message.

---

## Personas (Vault Customizations)

If your vault defines custom agent personas in its `_prompts/` or customization configuration, a **Persona** dropdown appears alongside the Agent and Model pickers. Selecting a persona applies its custom instructions and, when configured, its preferred backend agent and model. Select **No persona** to clear it.

For how to create personas, see [Vault Configuration — Customizations](vault-configuration.md).

---

## Read-Only vs Read-Write

The panel header shows a **scope toggle** and a **badge** below it:

```
Operating on work · read-only
```

The toggle button switches between two modes:

| Mode | What the agent can do |
|---|---|
| **Read-only** (default) | Read notes, list files, run search and SQL queries. Cannot create, edit, or delete notes. |
| **Read-write** | Full access: create, edit, and delete notes in the vault. |

**Read-only is the safe default.** The agent can answer most questions — "summarize this note", "what tasks are open?", "find notes about Acme" — without needing write access. Only unlock read-write when you specifically want the agent to make changes.

Click the toggle to switch. The badge updates immediately and the running session is reconfigured on the fly.

> **Important:** Enabling read-write lets the agent modify your vault notes. Review the agent's proposed actions before enabling it for a new or unfamiliar task. See [AI Permissions](ai-permissions.md) for the full permission model.

---

## Sending a Message

Type in the composer at the bottom of the panel. Key behaviors:

- **Enter** — sends the message
- **Shift + Enter** — inserts a newline without sending
- **@** — opens an autocomplete for attaching context (a note, folder, tag, or URL) as a reference the agent resolves via its tools

The current note in the editor is automatically included as context. You can toggle this with the **active note pill** shown in the context row above the composer.

---

## Stop and Regenerate

While the agent is replying, the **Send** button becomes a **Stop** button. Click it to cancel the in-flight response immediately.

Once a turn completes, a **↻ Regenerate** link appears below the composer. Click it to re-run the last user message and get a fresh response — useful when the first answer wasn't quite right.

---

## Starting a New Conversation

Click the **☰** (Conversations) button in the panel header to open the conversations list, then click **+ New conversation**. This clears the current conversation and starts a blank thread. The new thread is saved to the vault's transcript store the moment you send your first message.

---

## Managing Conversations

Click **☰** at any time to see all saved conversations for the current vault.

### Switching threads

Click a conversation title to reopen it. The agent session reconnects lazily the next time you send a message.

### Fork a conversation

Click the **⑂** (fork) button next to a thread. Notesmith copies the entire conversation into a new thread titled **\<original title\> (fork)** and switches to it. You can then continue from that branch independently without changing the original.

> **Tip:** Fork is great for exploring "what if" follow-up questions without losing the original thread. Keep the clean thread as a reference and experiment freely in the fork.

### Export a conversation to a vault note

Click the **↗** (export) button next to a thread. Notesmith creates a markdown note in your vault containing the full conversation, formatted with a YAML frontmatter block (agent, model, timestamps) and role-labelled messages. A success toast shows the saved path:

```
Exported to chat-transcripts/Summarize this note.md
```

The exported note has `type: chat-transcript` in its frontmatter and is fully searchable, linkable, and queryable like any other note in your vault.

> **Tip:** Use Export to preserve research threads you want to reference later — an exported transcript becomes a permanent, indexed note you can wikilink from other notes.

### Delete a conversation

Click the **✕** button next to a thread to permanently delete it from the vault transcript store.

---

## Slash Commands

Type `/` in the composer to open the slash command palette. Commands come from your vault's `_prompts/` folder plus built-in defaults. Select a command to expand its template body into the composer for editing before you send.

For the full list of slash commands and how to create custom ones, see [AI Slash Commands](ai-slash-commands.md).

---

## AI Editor Integration

The agent can also operate directly on text in the editor. For inline rewrite, insert, and apply-to-editor commands, see [AI Editor Integration](ai-editor.md).

---

## Troubleshooting (Diagnostics)

If an agent isn't connecting, go to **Settings → AI Agent**.

### Agent detection report

The **Available agents** section lists every agent Notesmith knows about, with a ✓ (available) or ✗ (not found) badge. Click **Run diagnostics** to trigger a detailed probe. The report shows:

- **Resolved PATH** — the directories Notesmith searched
- **Per-agent verdict** — `available`, `not found`, `probe failed`, or `package not installed`
- **Detected version** — the version string from the agent's version probe, plus any version warning
- **Candidate programs** — which executable names were tried and where they were (or weren't) found

Use the **Copy** button to copy the full plain-text report to your clipboard for a bug report.

> **Claude Code shows "package not installed"?** Claude Code runs through an npm
> adapter (`@zed-industries/claude-code-acp`) launched with `npx`. Having Node.js
> (and therefore `npx`) on your machine is **not** enough — the adapter package
> itself must be present. Install it once with
> `npm install -g @zed-industries/claude-code-acp` (or run
> `npx --yes @zed-industries/claude-code-acp --version` once to populate the npx
> cache), then click **Run diagnostics** again. Until then Notesmith correctly
> reports Claude as unavailable rather than letting a chat fail on first use.

### Recent errors & wire log

Below the detection report, the **Recent errors & wire log** section shows a timestamped list of errors from recent agent sessions, newest first. Click an entry to expand its detail.

To capture the full ACP conversation for debugging a misbehaving agent, enable **Verbose ACP wire log**. This records the outgoing prompts, streamed events, and permission/filesystem requests at Notesmith's protocol boundary (note content is truncated). Errors are always recorded; the wire log is off by default.

Use **Refresh** to reload the log and **Clear** to wipe it.

> **Note:** Verbose wire logging has a small overhead. Leave it off for normal use; turn it on only when chasing a specific bug, then clear the log and turn it off again.

---

## Related Documentation

- [Desktop App Guide § 10 — Right Dock](app-guide.md#10-right-dock-context--chat)
- [AI Permissions](ai-permissions.md)
- [AI Slash Commands](ai-slash-commands.md)
- [AI Editor Integration](ai-editor.md)
- [Vault Configuration — Customizations (Personas)](vault-configuration.md)
