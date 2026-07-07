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

**Addendum (2026-07-06):** §9 adds the **enablement, packaging, and adaptive-settings**
decision — how `local-embed` is turned on per surface (desktop vs server), how the
runtime `embed.enabled` flag decouples "compiled in" from "running", and how the
desktop Settings UI adapts to the *connected* daemon's advertised capabilities
under [ADR 0017](0017-per-window-daemon-connections.md)'s per-window model.

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
- **The cloud embedder (`OpenAiCompatible`) is deferred to a later phase** (see
  §8). P0 ships **local-only**; the `Embedder` trait exists so cloud can be added
  without rework, but no API-key handling ships initially — keeping
  [ADR 0011](0011-embedded-agent-chat.md) Decision 5 ("Notesmith does not manage
  model credentials") intact for now.

### 7. Query-time embedding runs in the daemon

Indexing (§2) runs in the worker, but **search must embed the query string**, and
`vault_search` runs **in the daemon**. Therefore the **daemon hosts an `Embedder`
for query-time embedding only** — it embeds one short query string per search
using the **same `embedder_id`/`dim`** recorded in the store, then runs the k-NN +
metadata JOIN over the ATTACHed read-only `embeddings.db`.

This is a deliberate **refinement of [ADR 0015](0015-ai-agent-integration-roadmap.md)'s
"the daemon runs no model" framing**: the daemon loads the *embeddings* model
(~130MB for `bge-small`) to vectorize queries. It still runs **no chat LLM** and
does no bulk/index embedding — that stays in the worker. If `embedder_id` at query
time does not match the store's, search fails loudly rather than comparing
incompatible vectors.

### 8. Resolved pre-implementation decisions (2026-07-02)

| # | Decision | Choice |
|---|----------|--------|
| 1 | Query-time embedding | **Daemon hosts the embedder** for queries (§7) |
| 2 | `embeddings.db` location | **Persistent `data_dir/<vault>/embeddings.db`** (the `TranscriptStore` precedent, *not* the rebuildable cache dir); daemon `ATTACH`es it read-only |
| 3 | Worker scheduling | **Daemon spawns + supervises** an interval embed worker; `notesmith embed` also runnable manually |
| 4 | Local embedder packaging | **Cargo feature `local-embed`** (cloud/lean builds omit ONNX Runtime) + **download-on-first-run** to `data_dir`, with offline messaging |
| 5 | Cloud embedder + API key | **Deferred** to a later phase; P0 is local-only, no credential handling |
| 6 | Change feed | **Worker does its own filesystem walk + `content_hash`** — fully decoupled from the daemon's (rebuildable) cache index |

Defaults also confirmed: embed **all notes + ingested media**; chunk
heading/paragraph-aware at ~256–512 tokens with ~15% overlap using the model's own
tokenizer; **first slice = P0 over existing notes only** (media ingestion / ADR 0019
deferred).

**Hybrid ranking (P1, [#199](https://github.com/surdy/notes-method/issues/199)):**
combine Tantivy (BM25) + vector results with **Reciprocal Rank Fusion (RRF), `k=60`,
equal weights** — chosen because BM25 and cosine are on incommensurable, per-query
scales, RRF is rank-based (no normalization/calibration), and it cleanly fuses the
**chunk-level** vector list with the **note-level** lexical list. Concretely:
retrieve top ~50 from each, fuse via RRF, dedup chunks→note (keep the best chunk per
note for ranking, return its span for citation), take top-k. `VectorStore::search`
returns **raw distances** (and Tantivy raw BM25) so magnitude is preserved for two
deferred upgrades, in order: **weighted RRF** (`Σ wᵢ/(k+rankᵢ)` — a lexical/semantic
"trust" knob with no normalization, defaulting 1/1) and, only if evaluation shows
RRF leaving quality on the table, a **cross-encoder re-ranker** over the top-N fused
results. A weighted *score blend* was rejected: it needs per-corpus α calibration and
brittle per-query score normalization we have no labeled data to tune.

### 9. Enablement, packaging & adaptive settings (addendum 2026-07-06)

§8.4 settled *how* the local model is compiled in (`local-embed` feature) but left
open *how a user turns embeddings on* — which matters once the desktop connects to
remote daemons (ADR 0017). Decision:

**9.1 Embeddings are off by default and enabled per vault; enabling must not
require a recompile.** The on/off a *user* sees is a **runtime, per-vault flag**, not
a build flag. A new **per-vault `vault.toml` key `[embed] enabled` (default `false`)**
gates both the scheduler (worker passes for that vault) and the query-time path for
that vault, in *every* build. Per-vault (not global) so a user can turn semantic
search on for a large research vault while leaving a throwaway scratch vault
un-embedded — embedding cost (disk, worker CPU, first-run model load) is paid only
where it's wanted, and it pairs naturally with future per-vault cloud embedder keys
(#251). "Compiled in but idle" is a real, cheap state: with no vault enabled nothing
loads, no model downloads, no worker runs — the cost of a capable build sitting idle
is binary size only, not runtime. The model is process-global and loaded lazily on
the first enabled vault, then shared across all enabled vaults.

**9.2 Compile-time vs runtime differ per surface** — because a user can only "enable
it in the app" if the runtime is already in the binary they hold:

| Surface | Packaging (compile-time) | Enablement (runtime) |
|---------|--------------------------|----------------------|
| **Desktop app** | Sidecar **always built with `local-embed`** so the toggle can exist; **bundle the model** with the app so first-enable is offline/instant (no HuggingFace fetch) | **Per-vault Settings toggle**, default off; writes the connected daemon's `vault.toml` `[embed] enabled` for that vault |
| **Server / container** | **Two image flavors along an embed axis**: lean `latest`/`api-latest` (no ONNX) and a `*-embed` tag built with `local-embed`. Keeps the lean image lean (its whole reason to exist) | Per-vault `[embed] enabled` in each `vault.toml`; the lean image can't enable (nothing compiled in) and says so |

Rejected: flipping the **crate default** to on (ships ONNX + a first-run HuggingFace
download to *everyone*, including Pi/tiny-VPS and air-gapped self-hosters, and kills
the lean `api` image). Rejected: **compile-time-only** gating for desktop (would mean
shipping two separate desktop apps — no in-app opt-in).

**9.3 Capabilities are advertised, and the desktop Settings UI is adaptive.**
Embeddings run on **the daemon the window is bound to** (ADR 0017), not the desktop
shell, so the desktop cannot decide this locally — it asks the server.
`GET /api/capabilities` gains an **`embeddings` block** carrying the process-global
facts (whether the runtime is compiled in, and which model it would use):

```json
"embeddings": { "compiled_in": true, "model": "bge-small-en-v1.5", "dim": 384 }
```

The per-vault **`enabled`** state is *not* in this global block — it lives in each
vault's config and is read/written through the per-vault config API alongside the
other `vault.toml` settings. The Settings embed section renders from both: the
capability gates *whether a toggle can exist at all*; the vault config supplies its
*current value*. Because switching connections does a **full webview reload**
(ADR 0017 — `API_BASE` is read once per window), capabilities re-fetch per connection
automatically:

| Connected daemon | Settings shows |
|------------------|----------------|
| Embed-capable (`compiled_in=true`) | Per-vault toggle + model info; reflects/edits that vault's `[embed] enabled` |
| Lean server (`compiled_in=false`) | Section **disabled** with "this server was built without embedding support — use an embed-enabled build / `*-embed` image" |
| Capable but vault config read-only (e.g. read-only remote) | Toggle **read-only**, reflecting the vault's current state |
| Local desktop daemon | Same as embed-capable (bundled sidecar has the feature) |

**9.4 The toggle is server-side, per-vault state, not a desktop preference.**
Enabling it writes the *connected* daemon's `vault.toml` `[embed] enabled` for the
selected vault (through the same config-write path as other per-vault settings, hence
subject to whether that vault's config is editable). This prevents a "silent lie" — a
user connected to a lean server can never flip a switch that does nothing, because
`compiled_in=false` disables it with an explanation.

## Consequences

- Phase 2 retrieval ([#199](https://github.com/surdy/notes-method/issues/199)
  `vault_search`, [#201](https://github.com/surdy/notes-method/issues/201)
  relevant-notes) can proceed on a concrete backend.
- The daemon stays lean; a stale/sleeping worker degrades *freshness of the
  embeddings* but never blocks the daemon or search over already-embedded content.
- Two processes touch `embeddings.db` (worker writes, daemon reads). WAL mode
  makes one-writer/many-readers safe; the daemon's `ATTACH` is read-only. The DB
  is **persistent** (`data_dir`), not in the rebuildable cache dir.
- The daemon loads the **embeddings model** (~130MB) for **query-time** embedding
  only (§7); it still runs no chat LLM and no bulk embedding.
- Choosing sqlite-vec now does **not** lock us in: LanceDB is a store-swap behind
  `VectorStore`, and metadata stays in SQLite either way.
- **(§9)** Embeddings are off by default via a per-vault `[embed] enabled` flag; the
  desktop ships a model-bundled, embed-capable sidecar and adapts its per-vault
  Settings toggle to the *connected* daemon's advertised `embeddings` capability,
  while servers choose a lean or `*-embed` image. "Compiled in but off" costs binary
  size only.

## Implementation status

P0–P2 shipped (2026-07); P3/P4 deferred as planned. Two deliberate de-risking
divergences from the framing above, made during implementation:

- **VectorStore ships as brute-force cosine, not `SqliteVecStore`.** The
  `VectorStore` trait is in place (§5 always allowed brute-force "to begin"), and
  `BruteForceStore` is the current impl — a linear scan over the stored vectors,
  the same asymptotic behavior sqlite-vec would give at this scale, with no
  loadable-extension dependency. sqlite-vec and LanceDB remain store-swaps behind
  the trait; metadata stays in SQLite either way, so nothing about the hybrid
  JOIN or the LanceDB trigger changes.
- **Default embedder is `HashEmbedder` (384-dim, non-semantic); `LocalFastEmbed`
  is behind the `local-embed` Cargo feature.** The default build embeds with a
  deterministic hash so CI, tests, and lean/offline builds need no ONNX runtime
  or first-run model download. `bge-small-en-v1.5` (384-dim) is the real model
  when `local-embed` is enabled. Both defaults are 384-dim, and
  `notesmith_embed::default_embedder()` is the single factory the worker and the
  daemon's query-time path share, so `embedder_id`/`dim` always agree (a mismatch
  fails loud and forces a re-embed, per §7).

Shipped units: `embeddings.db` schema + store scaffolding
([#245](https://github.com/surdy/notes-method/issues/245)), `VectorStore` +
brute-force store ([#246](https://github.com/surdy/notes-method/issues/246)),
`Embedder` + `HashEmbedder`/`LocalFastEmbed`
([#247](https://github.com/surdy/notes-method/issues/247)), `notesmith embed`
worker ([#248](https://github.com/surdy/notes-method/issues/248)), daemon
read-only `ATTACH` + query-time embedding
([#249](https://github.com/surdy/notes-method/issues/249)), benchmark harness
([#250](https://github.com/surdy/notes-method/issues/250)), hybrid `vault_search`
via RRF ([#199](https://github.com/surdy/notes-method/issues/199), §8), and
observability ([#244](https://github.com/surdy/notes-method/issues/244)).
Deferred: `OpenAiCompatible` cloud embedder
([#251](https://github.com/surdy/notes-method/issues/251)) and LanceDB
([#252](https://github.com/surdy/notes-method/issues/252)).

**Implemented (§9 addendum):** the per-vault `[embed] enabled` flag
([#253](https://github.com/surdy/notes-method/issues/253)), the
`/api/capabilities` `embeddings` block
([#254](https://github.com/surdy/notes-method/issues/254)), the desktop per-vault
Settings → Semantic Search toggle + adaptive UI
([#255](https://github.com/surdy/notes-method/issues/255)), the embed-capable
desktop sidecar ([#256](https://github.com/surdy/notes-method/issues/256) Part A),
and the server `*-embed` container image flavor
([#257](https://github.com/surdy/notes-method/issues/257)). Still open: bundling
the model in the desktop app for a fully-offline first-enable
([#256](https://github.com/surdy/notes-method/issues/256) Part B, deferred — the
first enable otherwise downloads the model once).

## Suggested phasing

1. **P0 — store + local embedder (existing notes only).** `chunks` schema,
   `SqliteVecStore`, `LocalFastEmbed` (Cargo feature `local-embed`), daemon-spawned
   `notesmith embed` worker (own fs walk + `content_hash`), persistent
   `data_dir/<vault>/embeddings.db`, daemon `ATTACH` read-only + query-time
   embedding (§7). Benchmark on `golden-vault`. **Media ingestion deferred to
   [ADR 0019](0019-media-ingestion-pipeline.md).**
2. **P1 — hybrid search.** `vault_search` MCP tool: metadata prefilter + vector
   k-NN JOIN, returning chunk citations (char spans / media timestamps).
3. **P2 — monitoring.** `stage=vector_search` spans, `/embeddings/stats`,
   `embed_metrics` daily trend, synthetic benchmark harness ([#244](https://github.com/surdy/notes-method/issues/244)) — the LanceDB early-warning system.
4. **P3 (deferred) — cloud embedder + config.** `OpenAiCompatible`, per-vault
   config, dimension-mismatch re-embed, credential handling.
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
