# AI Agent Permissions

The Notesmith AI agent can read your notes, search your vault, run queries, and — when you choose read-write mode — propose edits. **Nothing is written to disk until you approve it.** Every write action goes through an explicit approval step, and you can choose exactly how long that approval lasts.

This guide explains the approval flow and how each choice works.

For the chat interface and mode toggle, see [AI Chat](ai-chat.md). For agent-assisted editing in the editor, see [AI Editor](ai-editor.md).

## The basic flow

When the agent wants to perform an action it cannot do silently (any write, or a tool call the agent backend flags as requiring consent):

1. The agent pauses.
2. A **permission card** appears in the chat pane, describing what the agent wants to do.
3. For file edits, the card includes a **diff preview** showing what will change.
4. You choose one of four responses: **Allow once**, **Allow this session**, **Always allow**, or **Deny**.
5. The agent proceeds (or stops) based on your answer.

Until you make a choice, nothing happens. The agent does not time out and retry on its own.

## Tool-call cards

Alongside permission cards, the chat also shows **tool-call cards** for actions the agent runs. A card looks like:

```
▸  write_note                                    RUNNING…
```

Click the card to expand it and see the arguments the agent passed and, once the call completes, the result. The status label reads **running…**, **done**, or **failed**. Tool-call cards are informational — they show what the agent did, not what it is about to do. Only write actions pause for your approval.

## The permission card

When the agent proposes a write, a highlighted card appears in the chat pane:

> **The agent wants to run `write_note` (create).**

The card shows the tool name and, when relevant, the operation kind (for example `create`, `update`, or `append`).

### Diff preview

For edits to existing notes, the card includes a diff preview block. The file path is shown at the top. Below it, the current content of the affected area is shown in **red with a `−` prefix**, and the proposed new content is shown in **green with a `+` prefix**. You can scroll the preview if the change is long.

Example (adding a heading to a note):

```diff
General/My Note.md
- This is the first line of the note.
+ # Project Status
+ This is the first line of the note.
```

The diff renders the full old-text block first, then the full new-text block. This is a clear, accessible before/after view — not a line-level patch — so you can quickly judge whether the change looks right.

Nothing is written until you respond to the card.

## Permission choices

Four buttons appear on every permission card:

| Button | Decision | What it means | How long it lasts |
|---|---|---|---|
| **Allow once** | `allow_once` | Approve this single call. | This call only — you will be asked again next time. |
| **Allow this session** | `allow_session` | Approve and suppress further prompts for this tool for the rest of the current chat session. | Until you close Chat or start a new conversation. |
| **Always allow** | `allow_always` | Approve and save the grant so this tool is never re-prompted, even after a restart. | Permanently, across all future sessions for this vault. |
| **Deny** | `deny` | Refuse this call. The agent is told the action was denied and will respond accordingly. | This call only — does not prevent you from allowing it later. |

### Where decisions are remembered

- **Allow once** and **Deny** persist nothing. The next call from the same tool will prompt again.
- **Allow this session** is held in memory for the duration of the current chat session only. Closing Chat or reopening the app clears it.
- **Always allow** writes a persistent grant to the daemon. When you start a new session, these grants are loaded and seeded in automatically, so you never see a prompt for that tool again. To revoke an Always allow grant, use **Settings → AI → Permissions**.

## Read-only vs read-write

The agent runs in one of two permission scopes set when the session starts:

- **Read-only** (the default): the agent is connected to a read-only MCP endpoint. Write tools are simply not available — no permission card will appear because writes cannot be requested in the first place. This is the safest mode for exploration and Q&A.
- **Read-write**: write tools become available, and the permission flow described in this guide is active.

You toggle between modes using the **read/write switch** in the Chat header. The mode is locked in when the session starts; if you toggle it while a session is active the change takes effect at the next session start.

See [AI Chat](ai-chat.md) for details on enabling read-write mode.

## Walkthrough: adding a heading to a note

Here is a complete example of the approval flow.

**You type in Chat:**

> Add a "## Status" heading at the top of General/Project Alpha.md

**The agent responds** with a plan, then pauses and shows a permission card:

> The agent wants to run **`write_note`** (update).
>
> ```
> General/Project Alpha.md
> - This note covers the current phase of Project Alpha.
> + ## Status
> + This note covers the current phase of Project Alpha.
> ```
>
> [ Allow once ]  [ Allow this session ]  [ Always allow ]  [ Deny ]

**You click Allow once.**

The agent proceeds, the heading is written to disk, and the chat shows the tool-call card flipping from **running…** to **done**. The editor refreshes the note automatically.

Because you chose Allow once, the next edit to any note will prompt again.

## Tips

- **Use Allow this session** when you are in the middle of a focused editing burst — for example, cleaning up headings across several notes in one conversation. You will not be prompted on every individual write, but the grant expires when you close Chat.
- **Use Deny** to stop a specific action you did not intend, without ending the conversation. You can rephrase your request and try again.
- **Use Always allow** only for tools you trust completely for all future work in this vault. A good candidate is a low-risk tool you use repeatedly, like reading or searching.

## Safety summary

- Nothing is written to disk until you click an allow button.
- Read-only mode prevents writes entirely at the protocol level — no card, no approval needed.
- Denying a call does not break the session; the agent can attempt a different approach if you ask.
- Always allow grants are per-vault, not global — granting a tool in one vault does not affect other vaults.
