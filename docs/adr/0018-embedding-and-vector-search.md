# ADR 0018 — Embedding & Vector Search Architecture

## Status

Accepted (2026-07-02). **Resolves the "embeddings-backend decision"** that keeps
Phase 2 ([#186](https://github.com/surdy/notes-method/issues/186)) and Phase 3
([#187](https://github.com/surdy/notes-method/issues/187)) backlogged in
[ADR 0015](0015-ai-agent-integration-roadmap.md), and supplies the concrete
design for [#198](https://github.com/surdy/notes-method/issues/198)
(embedding backend). Operates within [ADR 0012](0012-agent-transport-acp-mcp.md)'s
**daemon-is-the-sole-index-owner** invariant and [ADR 0009](0009-resilience-to-malformed-content.md)'s
per-item resilience policy.

**Refines** ADR 0015's assumption that the embeddings model runs *in-daemon*
(placement "A"). At the corpus scale we are now targeting (long-form media, not
just hand-written notes), embedding moves to a **colocated worker process**
(placement "B") that writes its own store, which the daemon reads. See
[ADR 0019](0019-media-ingestion-pipeline.md) for the ingestion side that feeds
this pipeline.

## Context

Semantic search / RAG needs an **embeddings** model (text → vector) plus a
**vector store**. Notesmith already ships the hard parts this builds on:

- A daemon-owned **SQLite index** (`notesmith-index`, bundled `rusqlite`) with
  incremental re-indexing keyed on `content_hash` + `mtime_unix`, using
  **per-note savepoints**.
- A per-vault **`notify` file watcher** (`VaultWatcher`) driving incremental
  updates.
- `reqwest` already a workspace dependency.
- Rich structured metadata (`notes`, `tags`, `fields`, `tasks`, `v_*` views)
  that real semantic search must filter against ("top-k nearest **where**
  tag/type/date/vault").

Three facts reshape the original in-daemon assumption:

1. **Neither Claude nor Codex/ChatGPT subscriptions provide embeddings.**
   Anthropic ships **no** embeddings product (it points users at Voyage AI,
   which it acquired); OpenAI embeddings are **API-only** (`platform.openai.com`,
   metered per token), never included in a ChatGPT/Codex *subscription*. So
   "use my subscription for embeddings" is not an option — the realistic choices
   are **self-hosted local** or **metered cloud API**.
2. **Embeddings can run in-process with no second service.** `fastembed-rs`
   bundles ONNX Runtime and auto-downloads a small model, so the daemon *or* a
   CLI worker can embed inside its own binary — no Ollama/TEI sidecar required.
3. **The target corpus is media-heavy and unbounded.** The user intends to
   ingest **web articles, podcast transcripts, and YouTube transcripts** at
   volume. Long-form content is chunk-heavy: a 1-hour transcript ≈ 30–40 chunks;
   1,000 transcripts ≈ 35k chunks, and the corpus **grows forever**. This
   plausibly reaches **100k–500k+ vectors** within a year — squarely past the
   scale at which an in-daemon, whole-corpus design is comfortable.

## Decision

### 1. Two swap points behind traits

- **`Embedder`** — `embed(texts: &[String]) -> Vec<Vec<f32>>`, plus `id()` and
  `dim()`. Impls:
  - `LocalFastEmbed` — in-process ONNX via `fastembed-rs`; offline; $0; **default**.
  - `OpenAiCompatible { base_url, model, api_key }` — one `reqwest` client hitting
    `POST /v1/embeddings`. Because the OpenAI wire format is a de-facto standard,
    this single impl covers **OpenAI, Voyage, Cohere-compat, SiliconFlow, and
    local Ollama / LM Studio / TEI** by changing `base_url` + `model`.
- **`VectorStore`** — `upsert(chunks)`, `search(query_vec, filter, k)`, `delete(path)`.
  Impls: `SqliteVecStore` (now) and `LanceVectorStore` (deferred; see §5).

The store is a **config choice, not an architectural commitment**. Metadata and
chunk provenance stay in SQLite **regardless of vector backend** (see §4), so a
future backend swap is cheap and does not cost hybrid filtering.

### 2. Placement — B (colocated worker), not in-daemon

Embedding runs in a **separate, colocated process** — a `notesmith embed`
CLI subcommand run on an interval (launchd on macOS, systemd timer on Linux) or
as a queued worker. It fetches changed notes, chunks, embeds, and writes to its
**own `embeddings.db`**. The **daemon opens that DB read-only** and `ATTACH`es it
for search.

Rationale, and reconciliation with ADR 0012:

- The daemon **never writes** the embeddings store, so the single-index-owner
  invariant holds; the worker is the sole writer of `embeddings.db`, the daemon a
  reader.
- Ingestion + embedding is **heavy, bursty batch work** (readability extraction,
  Whisper transcription in [ADR 0019](0019-media-ingestion-pipeline.md), embedding
  hundreds of thousands of chunks). Keeping it out of the daemon keeps the daemon
  lean and its blast radius small.
- A periodic sweep is **idempotent and self-healing**: a 429 / network / parse
  failure simply leaves that item's `content_hash` unmatched, so the next tick
  retries it — the same incremental mechanism the note indexer already uses.

Placement **A** (in-daemon background task) remains the right choice for a *light,
notes-only, cloud-embedder* deployment (no model weights in RAM, I/O-bound HTTP).
Placement **C** (CLI → HTTP upsert to the daemon) is rejected: once the embedder
is just an HTTP client it adds an endpoint and moving parts for no gain. The
`Embedder`/`VectorStore` traits mean A and B share all code except the driver, so
supporting both later is cheap.

### 3. Incremental, per-item, resilient

The worker (re)embeds only items whose **`content_hash`** changed — the exact
delete-changed-then-`INSERT OR REPLACE` pattern the note indexer uses. Deletes
cascade like `remove_note`. Each item is embedded inside its **own savepoint**;
one malformed article/transcript logs `WARN item=<id> stage=embed reason=<...>`
and is skipped, never rolling back the batch (ADR 0009).

### 4. Chunk-level schema with citation offsets (mandatory)

One vector per long document is useless; we chunk to **~256–512 tokens** and
store provenance for citations and media deep-links:

```
chunks(
  vault_name TEXT, path TEXT, chunk_id INTEGER,
  char_start INTEGER, char_end INTEGER,      -- cite exact source span
  media_ts_start REAL NULL, media_ts_end REAL NULL,  -- podcast/YouTube deep-link
  content_hash TEXT,                          -- incremental re-embed key
  vector BLOB,                                -- f32[dim] (or int8 if quantized)
  PRIMARY KEY (vault_name, path, chunk_id)
)
```

**Metadata + provenance always live in SQLite**, joinable to the existing
`notes`/`tags`/`fields`/`v_*` views. Semantic search returns chunk IDs; hybrid
filtering ("nearest **where** tag=X / type=Y / date>Z") is a single JOIN. This
split ("vectors + metadata store") is what makes the LanceDB path (§5) cheap:
even after vectors move to Lance, the metadata never leaves SQLite.

Store `embedder_id` (provider + model) and `dim` in `_meta`. On a **mismatch**
(model or dimension changed), trigger a **full re-embed** — vectors of different
models/dims are not comparable.

### 5. Vector store: sqlite-vec now, LanceDB later

**Start on `sqlite-vec`** (a small loadable extension for the `rusqlite` we
already bundle), or even brute-force cosine to begin — tens of thousands of
chunks search in single-digit ms, and metadata prefiltering keeps most searches
scoped. `sqlite-vec` is a **linear scan** (no HNSW/IVF ANN yet), which is fine
below ~100k vectors.

**Move to LanceDB** (behind `VectorStore`) when the corpus crosses the point
where a linear scan stops meeting the latency budget — LanceDB brings disk-based
ANN (IVF-PQ / HNSW), quantization, out-of-core storage, and a natural home for
future multimodal vectors. The switch is **data-triggered, not vibes-triggered**:
see [ADR 0019](0019-media-ingestion-pipeline.md) and the monitoring spec for the
instrumentation (`stage=vector_search` spans, `/embeddings/stats`, `embed_metrics`
trend table, synthetic benchmark harness) and the **compound trigger**:

> Switch when `vector_count > ~150k` **AND** `p95 > 200ms`, **OR** `p95 > 300ms`
> at any count, **OR** disk/RAM budget exceeded.

Latency budget: p95 vector-search **< 150ms** good, **> 300ms** danger. Levers
that push the threshold out *before* switching (in priority order): **metadata
prefilter** (biggest — only *broad, unfiltered, whole-corpus* searches force the
switch), **int8 quantization** (~4×), **fewer dimensions** (384-dim model, or
Matryoshka-truncate `text-embedding-3-small`), **coarser chunking**.

### 6. Cloud key is optional and per-vault; local is the default

Config (per vault):

```
[embedding]
provider = "local"            # "local" | "openai-compatible" | "off"
model    = "bge-small-en-v1.5" # local model name OR remote model id
base_url = ""                  # e.g. https://api.openai.com/v1 (or Voyage/local)
api_key  = "@secret-ref"       # only for cloud; referenced, never stored in index
dim      = 384                 # validated; mismatch => full re-embed
```

- **Default is local/offline**, no API key.
- **Cloud is strictly opt-in** and per-vault. Because a cloud embedder **ships
  note content to a third party**, and Notesmith is self-hosted largely for
  privacy, cloud mode must never be a silent default; some vaults may be local,
  others cloud.
- Recommended defaults: local **`bge-small-en-v1.5`** (384-dim, ~130MB);
  cloud **`text-embedding-3-small`** (1536-dim, ~$0.02/M tokens). Bulk backfill
  of a large media corpus favors **local** ($0, no rate limits).

## Consequences

- Phase 2 retrieval ([#199](https://github.com/surdy/notes-method/issues/199)
  `vault_search`, [#201](https://github.com/surdy/notes-method/issues/201)
  relevant-notes) can proceed on a concrete backend.
- The daemon stays lean; a stale/sleeping worker degrades *freshness of the
  embeddings* but never blocks the daemon or search over already-embedded content.
- Two processes touch `embeddings.db` (worker writes, daemon reads). WAL mode
  makes one-writer/many-readers safe; the daemon's `ATTACH` is read-only.
- A cloud API key, when configured, lives in **config/secret store**, referenced
  from the index — never persisted into the vector DB.
- Choosing sqlite-vec now does **not** lock us in: LanceDB is a store-swap behind
  `VectorStore`, and metadata stays in SQLite either way.

## Suggested phasing

1. **P0 — store + local embedder.** `chunks` schema, `SqliteVecStore`,
   `LocalFastEmbed`, `notesmith embed` worker, incremental via `content_hash`,
   daemon `ATTACH` read-only. Benchmark on `golden-vault`.
2. **P1 — hybrid search.** `vault_search` MCP tool: metadata prefilter + vector
   k-NN JOIN, returning chunk citations (char spans / media timestamps).
3. **P2 — cloud embedder + config.** `OpenAiCompatible`, per-vault config,
   dimension-mismatch re-embed, secret handling.
4. **P3 — monitoring.** `stage=vector_search` spans, `/embeddings/stats`,
   `embed_metrics` daily trend, synthetic benchmark harness (the LanceDB
   early-warning system).
5. **P4 — LanceDB impl.** Only when the compound trigger fires.

## Alternatives considered

- **In-daemon embedding (placement A) as the primary design.** Rejected as
  *primary* for the media-heavy corpus: bursty ingestion + Whisper + 100k+
  vectors do not belong in the daemon. Retained as a supported mode for
  light/cloud deployments.
- **CLI → HTTP upsert (placement C).** Rejected: extra endpoint, no benefit once
  the embedder is an HTTP client.
- **LanceDB from day one.** Rejected at current scale: heavier deps (Arrow /
  Lance / DataFusion), a younger multi-process story, and — decisively — it
  cannot participate in a SQLite JOIN, so hybrid metadata filtering would require
  duplicating metadata or a two-store query. Deferred to the 100k+/multimodal
  regime behind `VectorStore`.
- **Bundling an Ollama/TEI sidecar.** Rejected: `fastembed-rs` gives in-process
  embeddings with no second service to run, supervise, or ship.
- **Relying on a Claude/Codex subscription.** Infeasible: neither exposes an
  embeddings endpoint (see Context).

## References

- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- [ADR 0012 — Agent Transport: ACP + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0015 — AI Agent Integration Roadmap](0015-ai-agent-integration-roadmap.md)
- [ADR 0019 — Media Ingestion Pipeline](0019-media-ingestion-pipeline.md)
- Design discussion: [`docs/embeddings/`](../embeddings/README.md)
- Issues: [#198](https://github.com/surdy/notes-method/issues/198) (embedding backend),
  [#199](https://github.com/surdy/notes-method/issues/199) (vault_search),
  [#201](https://github.com/surdy/notes-method/issues/201) (relevant notes)
