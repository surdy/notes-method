# Embeddings & Semantic Search — Design Notes

These notes capture the reasoning journey behind [ADR 0018 — Embedding & Vector Search Architecture](../adr/0018-embedding-and-vector-search.md) and [ADR 0019 — Media Ingestion Pipeline](../adr/0019-media-ingestion-pipeline.md). The ADRs are the crisp decisions; this documentation set preserves the tradeoffs, scale math, and practical guidance that led there.

## TL;DR decisions

- **Local-first embeddings:** run `fastembed-rs` in-process; no Ollama, TEI, or other sidecar is required.
- **Cloud is optional:** a per-vault, opt-in OpenAI-compatible `Embedder` can use OpenAI, Voyage, Cohere-compatible providers, SiliconFlow, or local OpenAI-compatible servers.
- **Placement is B:** a colocated `notesmith embed` worker writes `embeddings.db`; the daemon reads it.
- **Vector store now/later:** start with `sqlite-vec`; move to LanceDB later behind a `VectorStore` trait.
- **Metadata stays in SQLite:** tags, fields, dates, paths, and provenance remain joinable with the live Notesmith index.
- **Chunks carry citations:** store character offsets and media timestamps so results can point back to exact source spans.
- **LanceDB is data-triggered:** switch only when monitoring shows sqlite-vec no longer meets latency or resource budgets.

## Resolved for implementation (2026-07-02)

See [ADR 0018 §8](../adr/0018-embedding-and-vector-search.md). Confirmed: query embedding runs **in the daemon** (§7); `embeddings.db` is **persistent** in `data_dir` (not cache); the embed worker is **daemon-spawned** and does its **own fs walk + `content_hash`**; the local embedder is a **`local-embed` Cargo feature** with download-on-first-run; the **cloud embedder is deferred** (P0 is local-only); first slice embeds **existing notes only** (media ingestion / ADR 0019 later).

## Chapters

| Page | Summary |
|------|---------|
| [01 — Providers and subscriptions](01-providers-and-subscriptions.md) | Why Claude/Codex subscriptions do not include embeddings, and how local/cloud embedders fit behind one trait. |
| [02 — Hardware](02-hardware.md) | Why embeddings are cheap compared with chat LLMs, with memory, storage, and throughput guidance. |
| [03 — Placement](03-placement.md) | The A/B/C placement options and why the colocated worker was chosen for a media-heavy corpus. |
| [04 — Vector store](04-vector-store.md) | Why sqlite-vec is the starting point, why metadata stays in SQLite, and when LanceDB becomes worthwhile. |
| [05 — Scaling and monitoring](05-scaling-and-monitoring.md) | The count, latency, and resource signals that turn vector-store choice into an operational decision. |

## See also

- [ADR 0018 — Embedding & Vector Search Architecture](../adr/0018-embedding-and-vector-search.md)
- [ADR 0019 — Media Ingestion Pipeline](../adr/0019-media-ingestion-pipeline.md)
