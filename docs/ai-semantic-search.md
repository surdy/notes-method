# AI: Semantic & Hybrid Search

Notesmith's AI agent can search your vault with hybrid search: exact keyword matching plus optional local semantic matching. You use it by chatting normally; the agent decides when to call the vault search tool and can ground its answer with note paths and snippets.

---

## What semantic search adds

Semantic search helps the agent find notes by meaning, not only by exact words. It is useful when you remember the idea but not the phrasing:

- "Find notes about the database migration plan."
- "What have I written about reducing meeting load?"
- "Summarize my notes on long-term product positioning."

It complements keyword search and SQL queries. Use exact keywords when you know the phrase you want; use the agent when you want recall, synthesis, or discovery across many notes.

> **Tip:** Semantic search is especially helpful for "second brain" questions where related notes may use different wording.

---

## Using it in chat

Open the AI chat panel and ask the agent to find or recall something from your vault:

> Search my vault for notes about our database migration strategy and summarize them.

The agent can call `vault_search` and return an answer grounded in matching notes. Results include note paths and snippets, so the agent can cite where a claim came from instead of answering from memory alone.

There is no separate semantic-search button in the app. Hybrid search is a tool the agent uses during chat.

For chat scope, read-only vs read-write mode, and permission behavior, see [AI Chat](ai-chat.md).

---

## Hybrid ranking: lexical + semantic

The `vault_search` tool blends two signals:

| Signal | What it finds |
|--------|---------------|
| Lexical search | Exact words and phrases using full-text ranking |
| Semantic search | Conceptually similar notes when real embeddings are available |

The results are fused into one ranked list using Reciprocal Rank Fusion. Each result includes a `path`, a `snippet`, and rank fields showing whether lexical search, semantic search, or both contributed.

Until your vault has an embedding index, `vault_search` automatically falls back to lexical-only results. It still works and does not error.

For the full tool reference, including how `vault_search` differs from the older lexical-only `search_notes` tool, see [MCP Adapter](mcp.md).

---

## Turning on real semantic vectors

Semantic search is **off by default and enabled per vault**. Set `[embed] enabled = true`
in that vault's `.notesmith/vault.toml` (see [vault-configuration.md](vault-configuration.md))
to have the daemon build and maintain its `embeddings.db` and serve hybrid search. Leaving
it `false` keeps the vault lexical-only, so a large research vault can have semantic search on
while a throwaway scratch vault stays un-embedded. The flag is read fresh each worker pass, so
toggling it takes effect within one interval without a daemon restart.

Enabling the flag only has an effect on a build with embedding support compiled in. By default,
Notesmith ships with a zero-setup placeholder embedder. This keeps the indexing pipeline
available everywhere, but matches are effectively keyword-ish rather than truly semantic.

For real semantic search, run Notesmith with the `local-embed` feature enabled. That mode uses a local fastembed ONNX model (`bge-small-en-v1.5`, 384-dim) and downloads it automatically on first run.

Keep setup, worker operation, and monitoring details in one place: see [running & monitoring](embeddings-operations.md).

---

## Privacy & offline

Embeddings run locally on the same machine as your vault and daemon. By default, note content is not sent to any third party.

Cloud embedders, such as sending text to an external provider for higher-quality retrieval, are planned but not yet available.

---

## Current limitations

- Semantic search must be enabled per vault via `[embed] enabled = true` (off by default).
- Real conceptual similarity requires local embeddings to be enabled.
- The default placeholder mode is useful for exercising the pipeline, but it is not a high-quality semantic search model.
- Embedding coverage depends on the worker finishing an index for the vault; before that, search falls back to lexical-only.
- Cloud embedding providers are planned / not yet available.

---

## Learn more

- [AI Chat](ai-chat.md) — chatting with the agent and choosing read-only vs read-write scope
- [MCP Adapter](mcp.md) — `vault_search`, `search_notes`, and other tool details
- [running & monitoring](embeddings-operations.md) — enabling real local vectors and checking indexing status
- ADR 0018 — background design for optional local embeddings and semantic search
