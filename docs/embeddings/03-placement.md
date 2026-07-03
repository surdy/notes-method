# Placement

Context: [ADR 0018](../adr/0018-embedding-and-vector-search.md) chooses placement B for embeddings; [ADR 0019](../adr/0019-media-ingestion-pipeline.md) makes that choice more important by adding long-form media ingestion.

## Decision frame

The question was where embedding work should run relative to the daemon, watcher, and SQLite index.

| Placement | What it is | Pros | Cons | Verdict |
|-----------|------------|------|------|---------|
| A — in-daemon background task | The daemon starts a Tokio task from the watcher and writes embeddings directly. | Simple for light notes-only use; freshest updates; best with cloud embedder because there is no local model RAM. Respects ADR 0012 when the daemon owns all writes. | Puts model memory, retries, batch work, and failures inside the daemon. Less comfortable for Whisper and large transcript bursts. | Good for light/cloud deployments, not the primary media-heavy design. |
| B — colocated worker | A scheduled `notesmith embed` CLI process writes its own `embeddings.db`; the daemon opens it read-only and `ATTACH`es it. | Isolates heavy/bursty ingestion and Whisper; idempotent and retryable; daemon stays lean; honors ADR 0012 because the daemon never writes the embeddings DB. | Search freshness depends on worker cadence; two processes touch the DB, so WAL/read-only discipline matters. | Chosen primary design. |
| C — CLI to HTTP upsert | A CLI worker embeds content, then calls a daemon HTTP endpoint to upsert vectors. | Keeps one daemon-owned database path. | Adds an endpoint and queue semantics for little benefit once the embedder itself is just HTTP or in-process. Makes the daemon handle write bursts anyway. | Rejected. |

## Why B for the media-heavy corpus

Long-form media changes the shape of the workload:

- transcript extraction and Whisper are bursty;
- one source can produce dozens of chunks;
- cloud calls may 429 or time out;
- local backfills can run for minutes;
- a bad source should be retried later, not destabilize the daemon.

A colocated worker gives that work a separate failure domain while keeping it on the same machine and filesystem as the vault.

## ADR 0012 reconciliation

ADR 0012 protects the live daemon/index path with a single-index-owner invariant. Placement B keeps that spirit by splitting ownership clearly:

| Database | Writer | Reader |
|----------|--------|--------|
| Main Notesmith index | Daemon | Daemon, tools, HTTP/MCP clients |
| `embeddings.db` | `notesmith embed` worker | Daemon via read-only `ATTACH` |

The worker is the sole writer of `embeddings.db`. The daemon is a read-only consumer. That avoids two writers for the same index while still allowing one SQL query to combine search results with live metadata.

## Shared code path

The traits from ADR 0018 keep placement from infecting the core design:

```text
Driver A: daemon watcher task
Driver B: scheduled CLI worker

Both use:
  Chunker -> Embedder -> VectorStore
```

A and B can share chunking, provider configuration, vector-store code, and schema migrations. The only difference is the driver that decides when work runs.
