# Hardware

Context: [ADR 0018](../adr/0018-embedding-and-vector-search.md) chooses local-first embeddings; [ADR 0019](../adr/0019-media-ingestion-pipeline.md) explains why media transcripts can grow the corpus quickly.

## Thesis

**Embeddings are the cheap part.** They are not a chat LLM. The model is smaller, inference is simpler, and per-edit work is tiny after the first index.

## Memory and indexing cost

| Workload | Rough requirement | Notes |
|----------|-------------------|-------|
| Small local embedding model | 1–2GB RAM/VRAM headroom | Good for `bge-small-en-v1.5` and similar models. |
| Larger local embedding model | about 2–4GB RAM/VRAM headroom | Useful for multilingual or higher-quality models. |
| Full re-index of thousands of notes | Minutes | One-time cost after enabling embeddings or changing model/dimensions. |
| Incremental per-note edit | Near-instant | Re-embed only changed chunks keyed by `content_hash`. |

GPU/Metal mainly speeds the initial index. CPU is fine for ongoing incremental work.

## Vector storage size

Storage is straightforward:

```text
vector_bytes = count × dim × 4 bytes
```

| Chunk count | Dim | Vector bytes | Practical size |
|-------------|-----|--------------|----------------|
| 5k | 768 | 15,360,000 | about 15MB |
| 35k | 768 | 107,520,000 | about 108MB |
| 100k | 768 | 307,200,000 | about 307MB |
| 500k | 768 | 1,536,000,000 | about 1.5GB |

SQLite rows, metadata, indexes, and WAL add overhead, but the vector math sets the baseline.

## Throughput guidance

| Machine | Expected embedding throughput |
|---------|-------------------------------|
| Apple Silicon with Metal | Tens to hundreds of notes/sec, depending on chunk size and model. |
| CPU mini-PC | Single-digit to tens of notes/sec. |
| Raspberry Pi 5 | Slower, but viable for background CPU embedding with smaller models. |

The right question is usually not "can it embed?" but "how long does the first backfill take?"

## Hardware recommendation

Do not spec hardware around embeddings. Use what you already have.

| Option | Recommendation |
|--------|----------------|
| Any M-series Mac, 8GB+ unified memory | Best no-cost option. Metal acceleration is available automatically where supported. |
| Always-on home server | An Intel N100/N150 mini-PC with 16GB RAM, roughly `$150–250`, is fine for 24/7 CPU-only embedding. This fits the self-hosted daemon model. |
| Raspberry Pi 5, 8–16GB | Can run `nomic-embed-text` or similar models on CPU for background indexing. |
| Bigger GPU or bigger Mac | Only needed if you also want a local chat LLM. That is the demanding workload, and Notesmith deliberately does not run one under ADR 0015 Option A. |

For Notesmith semantic search, spend engineering effort on incremental indexing, metadata filtering, and monitoring before buying hardware.
