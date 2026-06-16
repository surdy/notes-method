# AI Agent Integration Roadmap

The definitive task breakdown for AI features in Notesmith. Architecture and
rationale are recorded in [ADR 0015](../docs/adr/0015-ai-agent-integration-roadmap.md);
source research is `research/notes-chat/ai-integration-feature-recommendations.md`.

Tracked by umbrella epic **#183** and phase epics **#184–#188**.

## Architecture in one line

> The daemon does the heavy lifting → exposes it as an **MCP tool** → the user's
> **ACP agent** decides when to call it. Notesmith never runs its own chat LLM.

- 🛠️ **MCP tool** — data/retrieval/ingestion; the agent calls it.
- 🖥️ **Client/editor UX** — composes a prompt, talks to the agent over **ACP**
  (not an MCP tool).
- ⚙️ **Daemon infra** — indexing/storage backing the tools.
- 🧩 **CLI** — headless surface that drives the agent.

The only daemon-side model is a local **embeddings** model (backlogged with P2).

## Status at a glance

- **Active (13):** all P0, all P1, and P4 model-free items.
- **Backlog (13):** all P2, all P3, plus P4 Projects + Terminal.
- **Why backlog:** P2 needs an embeddings model; P3's headline (voice) needs
  Whisper — both add bundled-model cost we are deferring. `time_query` (#200) and
  `vault_stats` (#202) are embedding-independent and can be promoted.

## Phase 0 — Foundation polish & verification (#184, active)

| Issue | Item |
|---|---|
| #189 | Diff preview before agent writes + Allow Once/Session/Always |
| #190 | Session history: fork thread + export chat to note |
| #191 | Model/mode pickers + Stop/Regenerate/New chat controls |
| #192 | Diagnostics: error log + ACP wire log + agent version check |

## Phase 1 — Out-of-the-box chat magic (#185, active, client-side)

| Issue | Item |
|---|---|
| #193 | Custom prompts (static): config-dir defaults + vault `_prompts/` overrides |
| #194 | Default slash-command set: `/summarize /rewrite /outline /fix /tags /links /daily /new /ask` |
| #195 | Inline editor commands (rewrite, summarize, expand, fix, continue, custom) |
| #196 | Apply agent output to document (insert / replace / append) |
| #197 | Context attachment: `@note/@folder/@tag/@url`, active-note auto-include, removable pills |

Prompts are **static** in this slice (no `{{variables}}` yet); the file format is
forward-compatible with variables. `@url` (#197) passes the URL as text until the
daemon-side `web_fetch` tool (#207, backlog) lands.

## Phase 2 — Retrieval / second brain (#186, BACKLOG)

| Issue | Item | Note |
|---|---|---|
| #198 | Embedding backend: local model runtime + vector store | blocks #199/#201 |
| #199 | `vault_search` MCP tool (hybrid lexical + semantic) | needs #198 |
| #200 | `time_query` MCP tool | **embedding-independent** |
| #201 | Relevant Notes panel (similarity + graph-link scoring) | needs #198 |
| #202 | `vault_stats` / structure MCP tool | **embedding-independent** |

## Phase 3 — Memory & multimodal (#187, BACKLOG)

| Issue | Item |
|---|---|
| #203 | `memory` MCP tool (save / recall) |
| #204 | Voice / meeting transcription → structured note (needs Whisper) |
| #205 | PDF / EPUB ingestion as context |
| #206 | Image / vision input |
| #207 | `web_fetch` + `web_search` MCP tools (unblocks @url) |
| #208 | `youtube_transcript` MCP tool |

## Phase 4 — Scale & CLI edge (#188, mixed)

| Issue | Item | Status |
|---|---|---|
| #209 | Headless CLI `notes ai` commands | active |
| #210 | Customization discovery (agents / skills / instructions) | active |
| #211 | MCP server management UI | active |
| #212 | `@agent` routing in chat | active (needs #210) |
| #213 | Projects / scoped workspaces | backlog |
| #214 | Terminal integration (ACP terminal/*) | backlog |

## Effort-aware quick start

If a tight near-term arc is wanted: **P1** (#193 → #194 → #195/#196 → #197) delivers a
purpose-built notes assistant with no daemon work, then the active **P4** power-user
surfaces (#209–#212) build on existing MCP/agent infrastructure.
