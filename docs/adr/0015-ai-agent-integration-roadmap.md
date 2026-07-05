# ADR 0015 — AI Agent Integration Roadmap (MCP-tool architecture)

## Status

Accepted (2026-06-16). Builds on [ADR 0010](0010-agent-access-architecture.md)
(daemon-as-source-of-truth, MCP over HTTP), [ADR 0011](0011-embedded-agent-chat.md)
(embedded ACP chat), [ADR 0012](0012-agent-transport-acp-mcp.md) (ACP + stdio/HTTP
MCP transport), and [ADR 0013](0013-agent-discovery-and-diagnostics.md) (agent
discovery/diagnostics). This ADR does not change the transport; it records **how
new AI capabilities are architected and sequenced**, derived from the feature
research in `research/notes-chat/ai-integration-feature-recommendations.md`.

The roadmap is tracked by umbrella epic **#183** and phase epics **#184–#188**.

## Context

Notesmith already ships the hard foundation: an embedded ACP chat panel, agent
auto-discovery (ADR 0013), 13 MCP vault tools (`notesmith-mcp`), write-with-permission,
endpoint-based read-only/read-write scoping (`/mcp/<vault>` vs `/mcp-ro/<vault>`),
Tantivy lexical search, a rich SQLite index (notes/tags/tasks/links/periodic with
mtime/dates), a file-watcher incremental reindex, and a durable per-vault transcript
store (`notesmith-transcript`).

The greenfield value identified by the research is **retrieval, client-side chat
ergonomics, memory, and multimodal ingestion**. The reference integrations studied
were *AI Tools for Obsidian* (ACP), *Copilot Plus for Obsidian* (LangChain pipeline),
and *cheeragpatel/copilot-obsidian* (GitHub Copilot SDK).

Two confirmed product constraints shape everything:

- **ACP-only / BYO agent.** The user brings their own agent (Copilot, Claude, Codex,
  Gemini, …); Notesmith does not ship or sell model access.
- **Technical-but-lazy users.** North star: zero-config defaults with progressive
  disclosure.

## Decision

### Core architectural principle (Option A)

> **The daemon does the heavy lifting, exposes it as an MCP tool, and the user's
> ACP agent decides when to call it. Notesmith never runs its own chat LLM.**

Concretely, every new capability lives in exactly one of these places:

- 🛠️ **MCP tool** — daemon builds it; the agent calls it (search, time-query,
  memory, web, transcription). This is how all *data/retrieval/ingestion* features
  ship.
- 🖥️ **Client/editor UX** — composes a prompt and talks to the agent **directly over
  ACP** (slash commands, inline edits, insert/replace, context pills). These are
  **not** MCP tools; they are chat/editor surfaces.
- ⚙️ **Daemon infra** — indexing/storage that backs the tools.
- 🧩 **CLI** — a headless/scriptable surface that drives the agent.

We explicitly **reject** re-implementing a Copilot-Plus-style in-app LLM/RAG pipeline
(own model calls, context-window management, provider/key management). The ACP agent
is already a capable orchestrator with its own context management; our job is to give
it **great tools and great context**, which is simpler and benefits any current or
future ACP agent for free.

**The one asterisk:** semantic retrieval needs an **embeddings** model. That is an
embeddings model (text → vector), *not* a chat LLM, and it backs the `vault_search`
MCP tool. It is the only model Notesmith itself runs. **This embeddings-backend
decision is now resolved by [ADR 0018](0018-embedding-and-vector-search.md)**, which
refines the original "daemon-side" assumption: embedding runs in a **colocated
worker process** that owns its own vector store, which the daemon reads (placement
"B"), rather than inside the daemon itself. See [ADR 0019](0019-media-ingestion-pipeline.md)
for the media-ingestion side that motivated that scale-driven refinement.

A secondary principle for the target user: **every feature ships with a working
default and a config escape hatch** — no API key, no setup required by default.

### Phased roadmap

Impact 🟥 high / 🟧 med / 🟦 nice. Effort S/M/L.

| Phase | Theme | Status |
|---|---|---|
| **P0** (#184) | Foundation polish & verification | **Active** |
| **P1** (#185) | Out-of-the-box chat magic (client-side) | **Active** |
| **P2** (#186) | Retrieval / second brain (MCP tools) | **Backlog** |
| **P3** (#187) | Memory & multimodal ingestion | **Backlog** |
| **P4** (#188) | Scale & CLI edge | **Active (mixed)** |

Sequencing rationale: P1 first — almost entirely client-side, no daemon work, and
the biggest visible "it's a notes assistant now" jump. P2 (RAG) is the first *Large*
lift and the "second brain" payoff. Its embeddings-backend blocker is now
**resolved by [ADR 0018](0018-embedding-and-vector-search.md)** (see the asterisk
above); remaining P2 work is implementation, not architecture. P3 is modular; P4
monetises Notesmith's *structural* advantages (CLI, MCP, customization dirs).

### Active vs. backlog (this iteration)

The roadmap was scoped down to keep near-term work free of bundled-model
dependencies:

- **Active (13 issues):** all of **P0** (#189–#192); all of **P1** (#193–#197 — static
  custom prompts, default slash set, inline editor commands, apply-to-document,
  context pills); and the model-free **P4** items (#209 headless CLI, #210
  customization discovery, #211 MCP server management UI, #212 `@agent` routing).
- **Backlog (13 issues):** **all of P2** (#198–#202) and **all of P3** (#203–#208),
  plus **P4** Projects (#213) and Terminal (#214).

**Why P2 and P3 were backlogged:** both lean on a bundled/downloaded local model —
*embeddings* for retrieval (P2) and *Whisper* for voice/meeting transcription (P3) —
which adds binary size and a runtime-selection decision. **The embeddings decision is
now made ([ADR 0018](0018-embedding-and-vector-search.md)); the ingestion/Whisper
side is scoped by [ADR 0019](0019-media-ingestion-pipeline.md)**, so P2/P3 are now
architecture-unblocked and gated only by implementation scheduling.
`time_query` (#200) and `vault_stats` (#202) are embedding-independent and may be
promoted to active without the embeddings backend.

### Static prompts now, variables later

P1 custom prompts ship as **static saved prompt strings** (built-in defaults in the
daemon config dir; user overrides as markdown in a vault `_prompts/` folder; the
daemon serves the merged list, vault winning on name collision). The file format is
designed so `{{selection}}`/`{{title}}`/`{{date}}` variable substitution can be added
later without a breaking change. The existing `notesmith-templates` (Minijinja) engine
renders **note** templates and is **not** reused for prompt variables in this slice.

## Consequences

- **Pros.** Minimal new machinery (no provider/key/context management); every tool
  also serves the CLI and external MCP clients, not just the embedded chat; any ACP
  agent benefits; near-term work has no bundled-model dependency.
- **Cons / risks.** No control over model quality (it is the user's agent). Truly
  autonomous *background* LLM work (e.g. auto-summarize-on-save) is hard under Option
  A — the planned answer is P4's headless CLI (#209) shelling out to an agent for
  cron/automation. Semantic features wait on the embeddings decision.
- **Resilience.** New parsers/ingesters (prompts, PDF/EPUB, discovery) follow ADR
  0009: per-item isolation, warn-and-skip on malformed input, no panics on
  file-derived content.

## Alternatives considered

- **Copilot-Plus-style in-app LLM/RAG pipeline.** Rejected: duplicates the agent,
  requires provider/key/context management, and contradicts the ACP-only constraint.
- **Ship semantic search now with a bundled embeddings model.** Deferred to backlog:
  binary-size and runtime-selection cost not justified this iteration. *(Later
  reversed: [ADR 0018](0018-embedding-and-vector-search.md) designs the embeddings
  backend — shipped local/offline by default, with the ONNX model behind the
  `local-embed` feature so the default build carries no bundled-model cost.)*
- **Bundle Whisper for voice/meeting notes now.** Deferred for the same
  bundled-model reason, despite being the standout differentiator.
- **Make all P2 retrieval active, embeddings-independent parts first.** Considered;
  the user chose to backlog the whole retrieval phase for coherence, leaving a note
  that `time_query`/`vault_stats` can be pulled forward.

## References

- Research: `research/notes-chat/ai-integration-feature-recommendations.md`
- Roadmap plan: [`plans/ai-integration-roadmap.md`](../../plans/ai-integration-roadmap.md)
- [ADR 0010 — Agent Access Architecture](0010-agent-access-architecture.md)
- [ADR 0011 — Embedded Agent Chat](0011-embedded-agent-chat.md)
- [ADR 0012 — Agent Transport: ACP + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0013 — Agent Discovery & Diagnostics](0013-agent-discovery-and-diagnostics.md)
- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- [ADR 0018 — Embedding & Vector Search Architecture](0018-embedding-and-vector-search.md)
- [ADR 0019 — Media Ingestion Pipeline](0019-media-ingestion-pipeline.md)
- Agent Client Protocol: https://agentclientprotocol.com
