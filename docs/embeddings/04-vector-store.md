# Vector Store

Context: [ADR 0018](../adr/0018-embedding-and-vector-search.md) starts with sqlite-vec and defers LanceDB; [ADR 0019](../adr/0019-media-ingestion-pipeline.md) explains why the vector count may later force that switch.

## Start simple: sqlite-vec or brute-force cosine

Notesmith already bundles SQLite through `rusqlite`, so the lowest-friction vector path is SQLite-native:

| Store | Fit |
|-------|-----|
| Brute-force cosine | Acceptable for tiny stores, especially less than 20MB and tens of thousands of vectors. Single-digit ms is realistic on a good local machine. |
| `sqlite-vec` | Small SQLite extension; keeps vectors close to existing metadata and query code. |
| LanceDB | Later store for larger, out-of-core ANN workloads. |

`sqlite-vec` is currently a **linear scan** store, not HNSW/IVF. That is fine below roughly 100k vectors, especially when metadata filters shrink the candidate set first.

## The decisive point: hybrid queries

Notesmith's index is rich. Real search is rarely only "nearest vector". It is usually:

```sql
-- shape, not final syntax
SELECT c.path, c.chunk_id, n.title, distance
FROM vec_chunks AS c
JOIN notes AS n ON n.vault = c.vault AND n.path = c.path
JOIN tags AS t ON t.vault = c.vault AND t.path = c.path
WHERE t.tag = 'project-x'
  AND n.updated_at >= ?
ORDER BY distance
LIMIT 20;
```

That means "top-k nearest where tag/type/date". With vectors in SQLite, this is one JOIN against existing `notes`, `tags`, `fields`, and `v_*` views.

LanceDB cannot join SQLite directly. If vectors move there, Notesmith must either:

1. vector-search in LanceDB, then filter results against SQLite; or
2. duplicate enough metadata into LanceDB to filter before returning.

Therefore **metadata and provenance stay in SQLite regardless of vector backend**. That makes LanceDB a later store swap, not a metadata migration.

## Chunk schema

From ADR 0018:

```sql
chunks(
  vault_name TEXT,
  path TEXT,
  chunk_id INTEGER,
  char_start INTEGER,
  char_end INTEGER,
  media_ts_start REAL NULL,
  media_ts_end REAL NULL,
  content_hash TEXT,
  vector BLOB,
  PRIMARY KEY (vault_name, path, chunk_id)
)
```

| Column | Why it matters |
|--------|----------------|
| `char_start`, `char_end` | Citation offsets: search results can point to the exact source span. |
| `media_ts_start`, `media_ts_end` | Deep links into podcast or YouTube moments. |
| `content_hash` | Incremental re-embed key; unchanged chunks can be skipped. |
| `vector` | Encoded embedding, usually `f32[dim]` at first and possibly quantized later. |

Chunk size should start around 256–512 tokens. One vector per long document loses too much context; tiny chunks create too many rows and weaker retrieval.

## ATTACH DATABASE pattern

Placement B uses two SQLite files:

```sql
ATTACH DATABASE 'embeddings.db' AS embed;

SELECT ...
FROM embed.chunks AS c
JOIN main.notes AS n
  ON n.vault = c.vault_name
 AND n.path = c.path
WHERE ...;
```

The CLI worker owns `embeddings.db` as writer, with WAL enabled. The daemon opens it read-only and attaches it for search. This satisfies ADR 0012 while preserving hybrid filtering.

## When LanceDB earns its keep

LanceDB becomes worth the dependency and operational complexity when Notesmith needs:

- 100k+ vectors with whole-corpus search latency beyond budget;
- out-of-core approximate nearest neighbor indexes such as IVF-PQ or HNSW;
- quantization as a first-class storage/search feature;
- multimodal vectors for images, audio, or richer media later.

The `VectorStore` trait keeps that as a store-swap:

```text
SqliteVecStore now
LanceVectorStore later
same chunk metadata in SQLite always
```
