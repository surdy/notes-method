# Scaling and Monitoring

Context: [ADR 0018](../adr/0018-embedding-and-vector-search.md) makes the LanceDB switch data-triggered; [ADR 0019](../adr/0019-media-ingestion-pipeline.md) is the reason vector growth can become unbounded.

## Scale math for media

Long-form transcripts dominate vector count.

| Corpus | Chunk estimate |
|--------|----------------|
| 1-hour transcript | about 30–40 chunks |
| 1,000 transcripts | about 35k chunks |
| Media-heavy year | 100k–500k+ chunks is plausible |

Vector storage baseline:

```text
bytes = count × dim × 4
500k × 768 × 4 bytes = about 1.5GB
```

That is still manageable, but it is large enough that linear scans must be measured, not assumed.

## Latency budget

| p95 vector-search latency | Meaning |
|---------------------------|---------|
| Less than 150ms | Good. Search feels responsive. |
| 150–300ms | Watch. Users may notice depending on UI and query mix. |
| More than 300ms | Danger. This is the threshold where a store change or aggressive optimization is justified. |

Use p95, not average. Users feel tail latency.

## Three signals

| Signal | Thresholds | Role |
|--------|------------|------|
| `vector_count` | less than 100k green; 100–250k watch; more than 250k plan | Early warning only. Count predicts risk but does not prove pain. |
| p95 latency on unfiltered whole-corpus searches | real trigger | The decisive measurement. Broad searches are what stress sqlite-vec linear scan. |
| Disk and RAM | `bytes = count × dim × 4` plus overhead | Capacity guardrail for local machines and always-on servers. |

Filtered searches can stay fast much longer because metadata prefiltering reduces the candidate set.

## Instrumentation

This should become an implementation issue.

Tracing span:

```text
INFO stage=vector_search n_vectors=... k=... duration_ms=...
```

Stats endpoint:

```http
GET /api/v/{vault}/embeddings/stats
```

Response shape:

```json
{
  "vector_count": 35000,
  "db_bytes": 125000000,
  "p50_ms": 42,
  "p95_ms": 118,
  "last_ingest_at": "2026-07-02T22:58:57Z"
}
```

Daily trend table:

```sql
CREATE TABLE embed_metrics(
  date TEXT PRIMARY KEY,
  count INTEGER NOT NULL,
  bytes INTEGER NOT NULL,
  p95_ms REAL NOT NULL
);
```

A daily job should append one row and run one synthetic benchmark query. That gives a trend line instead of one-off guesses.

## Benchmark harness

Do this once per target box:

1. Insert synthetic vectors at 50k, 100k, 250k, 500k, and 1M rows.
2. Use the real chosen dimension and storage format.
3. Time k-NN on the same machine that will run Notesmith.
4. Record where p95 crosses 150ms and 300ms.

Then production monitoring is simple: watch where the real corpus lands on the curve already measured for that hardware.

## Compound trigger for LanceDB

Switch from sqlite-vec to LanceDB when any of these is true:

- `vector_count > ~150k` **and** p95 is more than 200ms;
- p95 is more than 300ms at any count;
- disk or RAM budget is exceeded.

This avoids switching because the corpus sounds large while still giving a clear escape hatch.

## Levers before switching

Use these in priority order:

| Lever | Why it helps |
|-------|--------------|
| Metadata prefilter | Biggest win. Only broad unfiltered whole-corpus searches force the store switch. Filter first, then vector-search survivors. |
| Int8 quantization | Roughly 4× smaller vectors, often enough to buy months. |
| Fewer dimensions | Use a 384-dim model or Matryoshka-truncate `text-embedding-3-small` to 512 or 256 dimensions. |
| Coarser chunking | Fewer chunks and vectors, with some retrieval precision tradeoff. |

The goal is not to avoid LanceDB forever. The goal is to switch when measured data says SQLite has done its job.
