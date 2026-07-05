# Embeddings: Operating & Monitoring

Notesmith's optional embeddings pipeline keeps a per-vault vector index for semantic and hybrid search. This guide is for operators and self-hosters running the worker, checking health, and deciding when the current vector store needs to change.

For the end-user explanation of semantic search in chat, see [semantic search](ai-semantic-search.md).

---

## How embedding runs

The colocated embed worker is the **sole writer** of each vault's `embeddings.db`. The daemon starts this worker automatically for every configured vault; no separate service is required.

| Runner | What it does |
|--------|--------------|
| Daemon scheduler | Starts after a 10s initial delay, then runs every 300s by default. |
| `notesmith embed` | Runs one incremental pass by hand. See the [CLI reference](cli.md#embed). |

Each pass is incremental:

- notes whose content hash changed are re-embedded;
- unchanged notes are skipped;
- deleted notes are pruned from the embedding DB;
- malformed notes are skipped with a warning and must not crash the pass, following ADR 0009.

Useful manual commands:

```bash
notesmith embed                 # all registered vaults
notesmith --vault work embed    # one vault
notesmith embed --format json   # machine-readable report
```

---

## Enabling real semantic vectors

By default, Notesmith builds use `HashEmbedder`:

| Mode | Embedder | Notes |
|------|----------|-------|
| Default build | `HashEmbedder`, 384-dim | Deterministic, fully offline, no download; **not** true semantic similarity. The daemon logs a warning that `local-embed` is disabled. |
| `local-embed` feature | `LocalFastEmbed`, `bge-small-en-v1.5`, 384-dim | Real local embeddings via `fastembed`/ONNX. Downloads the model on first construction into `<data_dir>/models/`. |

To get real semantic retrieval, build the binary with `--features local-embed`. If the machine is offline on the first run and the model is not already cached, embedder construction fails and the daemon logs that embedding is disabled for that vault until the model is present.

> **Note:** The worker and query-time embedding use the same embedder factory. Stored `embedder_id` and `dim` should therefore match the query embedder; if they do not, search fails loudly instead of mixing incompatible vectors.

---

## Where data lives

Embedding data is durable application data. It lives outside the vault and outside the rebuildable `cache.sqlite` index.

| Data | Path | Notes |
|------|------|-------|
| Vault embedding DB | `<data_dir>/<vault>/embeddings.db` | `<data_dir>` honors `XDG_DATA_HOME`, falls back to the platform local-data dir, then appends `notesmith/`. On Linux this is commonly `~/.local/share/notesmith/<vault>/embeddings.db`. |
| Local model cache | `<data_dir>/models/` | Used by `LocalFastEmbed` when built with `local-embed`. |

Vault names are sanitized for directory use, so `/`, `\`, and `:` become `_` in the storage path.

---

## Tuning the interval

Set `NOTESMITH_EMBED_INTERVAL_SECS` to a positive integer number of seconds before starting the daemon:

```bash
NOTESMITH_EMBED_INTERVAL_SECS=120 notesmith daemon start
```

Invalid, missing, or non-positive values fall back to the default 300s interval.

---

## Monitoring

Use the stats endpoint for live health and capacity signals. Full reference: [HTTP API](http-api.md).

```bash
curl http://127.0.0.1:27183/api/v/work/embeddings/stats
```

A vault that has never been embedded returns zeros and `null` fields, not an error. An unknown vault returns `404`.

| Field | Meaning |
|-------|---------|
| `vector_count` | Stored chunk vectors for the vault. |
| `db_bytes` | Size of `embeddings.db` on disk. |
| `dim` | Vector dimension, or `null` if never embedded. |
| `embedder_id` | Model/embedder that produced the vectors, or `null` if never embedded. |
| `p50_ms` / `p95_ms` | Rolling search-latency percentiles over the recent in-process query window. |
| `sample_count` | Number of samples backing the percentiles, up to the last 256 searches. |
| `last_ingest_at` | Unix seconds of the last `embeddings.db` write, or `null`. |

Vector searches also emit a tracing span for log-based inspection:

```text
stage=vector_search n_vectors=... k=... filtered=... duration_ms=...
```

After each pass, the scheduler upserts a daily trend row inside `embeddings.db`:

| Column | Meaning |
|--------|---------|
| `date` | UTC date key. |
| `vault_name` | Vault name. |
| `vector_count` | Current stored vector count. |
| `db_bytes` | Current DB size. |
| `p95_ms` | Current rolling p95 search latency. |

This `embed_metrics` table is for trend lines, not one-off diagnosis.

---

## Benchmarking latency

Run `notesmith embed bench` on the host that will serve the vault. It builds a synthetic brute-force k-NN latency curve at increasing vector counts and reports where p95 first crosses 150 ms and 300 ms.

```bash
notesmith embed bench
notesmith embed bench --baseline --format json
```

Common flags include `--dim` (default 384), `--scales`, `--k` (default 10), `--queries` (default 50), `--baseline`, and `--format json`; see [CLI reference](cli.md#embed). Use a release binary for representative numbers.

---

## When to switch the vector store

The current vector store is a brute-force cosine scan, which is fine for tens of thousands of vectors. Keep the sqlite-vec / LanceDB swap data-triggered, not speculative.

Use **p95 latency**, not average latency:

| p95 vector-search latency | Action |
|---------------------------|--------|
| `< 150 ms` | Healthy. |
| `150–300 ms` | Watch and investigate. |
| `> 300 ms` | Switch or aggressively optimize. |

The compound trigger from the [scaling & monitoring](embeddings/05-scaling-and-monitoring.md) design is:

- `vector_count > ~150k` **and** `p95_ms > 200 ms`; or
- `p95_ms > 300 ms` at any vector count.

There are levers before switching, especially metadata prefiltering, quantization, fewer dimensions, and coarser chunking. See the design doc for the full rationale. LanceDB and cloud embedders are deferred/planned capabilities, not currently available operator choices.

---

## Re-embedding after a model change

If `embedder_id` or `dim` no longer matches the query embedder, semantic search fails loudly. Rebuild the vault's embedding DB from scratch:

1. Stop the daemon.
2. Delete that vault's `<data_dir>/<vault>/embeddings.db`.
3. Start the daemon and wait for the scheduled worker, or run `notesmith --vault <name> embed`.

> **Warning:** Delete only `embeddings.db` for the affected vault. Do not delete the vault content or the local model cache unless you intentionally want to re-download the model.

---

## Learn more

- [semantic search](ai-semantic-search.md) — end-user chat and search behavior
- [CLI reference](cli.md#embed) — `notesmith embed` and `notesmith embed bench`
- [HTTP API](http-api.md) — stats endpoint reference
- [scaling & monitoring](embeddings/05-scaling-and-monitoring.md) — thresholds and benchmark rationale
- [ADR 0018](adr/0018-embedding-and-vector-search.md) — embedding and vector-search architecture
