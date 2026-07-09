# ADR 0019 — Media Ingestion Pipeline

## Status

Accepted (2026-07-02).

Part of Phase 3 ([#187](https://github.com/surdy/notes-method/issues/187))
(Memory & multimodal) and dependent on the Phase 2 embedding backend
([#198](https://github.com/surdy/notes-method/issues/198)). This ADR defines
the **ingestion side** of the long-form media pipeline that feeds
[ADR 0018](0018-embedding-and-vector-search.md)'s chunking, embedding, and
vector-store design.

Governed by [ADR 0015](0015-ai-agent-integration-roadmap.md) Option A:
Notesmith runs no local chat LLM. The only local models in scope are embeddings
([ADR 0018](0018-embedding-and-vector-search.md)) and Whisper transcription.

## Context

[ADR 0018](0018-embedding-and-vector-search.md) decides how Notesmith chunks,
embeds, stores, and searches text once it exists as notes/chunks. It deliberately
assumes a media-heavy, unbounded corpus, but it does not decide how external
media becomes trustworthy markdown in a vault. This ADR fills that gap.

The first source set is intentionally narrow:

- **Web articles / blogs** — HTML pages that need boilerplate removal before
  they become useful notes.
- **Podcast audio** — audio that needs local transcription and timestamps.
- **YouTube videos** — caption/transcript tracks when available, with local
  transcription as fallback.

The full pipeline is:

```
fetch → extract/clean → (transcribe | fetch transcript) → normalize markdown
  → chunk → embed → store
```

This ADR owns **fetch → extract/clean/transcribe → normalize**. At the chunk
boundary, it hands off to [ADR 0018](0018-embedding-and-vector-search.md), which
owns the chunk schema, character offsets, media timestamps, `content_hash`
incrementality, `Embedder`/`VectorStore` traits, sqlite-vec now, and LanceDB
later.

The scale justifies keeping ingestion as a first-class subsystem, not a helper
inside the vector store:

- A 1-hour podcast or YouTube transcript is roughly **10k words** / **13k
  tokens**, or **30–40 chunks** at ~400 tokens.
- A typical blog article is roughly **7 chunks**.
- 1,000 transcripts is roughly **35k chunks**.
- The corpus grows without a natural upper bound and can plausibly reach
  **100k–500k+ vectors within a year**.

So one vector per source document is not meaningful, and ingestion must preserve
timestamps and character spans so [ADR 0018](0018-embedding-and-vector-search.md)
can produce citable chunks and media deep-links.

## Decision

### 1. Ingestion is its own subsystem

Ingestion is larger than "download a URL" and has a bigger design surface than
the initial vector-store implementation. It includes network fetch, source
authentication or rate limits, HTML cleanup, audio extraction, local Whisper
execution, transcript normalization, provenance, deduplication, retries, and
scheduling.

Model it as source-specific modules behind a small interface, for example a
`Fetcher` / `Source` trait. The exact Rust API can evolve, but the architectural
boundary is fixed: adding PDF, EPUB, newsletters, or future feeds should add a
source module, not rewrite the worker, scheduler, provenance format, or
[ADR 0018](0018-embedding-and-vector-search.md) handoff.

### 2. Source-specific extraction rules

Each source type has a different cheapest reliable path:

- **Articles:** fetch HTML, run readability-style extraction to remove nav,
  ads, comments, cookie banners, and other chrome, then convert the extracted
  article body to markdown.
- **Podcasts:** fetch or locate audio, run **local Whisper transcription**, and
  preserve segment timestamps. Whisper is CPU/GPU-heavy and bursty, which is a
  key reason ingestion is not allowed inside the daemon.
- **YouTube:** prefer the published transcript / caption track when present.
  This is cheap, timestamped, and avoids unnecessary local transcription. If no
  usable caption track exists, fall back to Whisper over the audio and keep
  segment timestamps.

For media, segment timestamps are mandatory. They become
`media_ts_start` / `media_ts_end` on [ADR 0018](0018-embedding-and-vector-search.md)
chunks so search results can deep-link to the moment, not merely cite the note.

### 3. Normalize to markdown notes with provenance frontmatter

Every successfully ingested item becomes a markdown note in the vault. The note
body is cleaned source content; source identity and citation fields live in YAML
frontmatter:

```yaml
---
title: "Example title"
source_url: "https://example.com/post"
source_type: "article" # article | podcast | youtube
author: "Example Author"
channel: "Example Channel"
published: "2026-07-02"
ingested_at: "2026-07-02T22:58:57-07:00"
duration: 3600
---
```

Required fields are `source_url`, `source_type` (`article` | `podcast` |
`youtube`), `title`, `author` and/or `channel` when known, `published` when
known, `ingested_at`, and `duration` for media sources. These fields are indexed
through the existing SQLite metadata pipeline as `fields`, making them joinable
in hybrid search with notes, tags, tasks, and chunks. Provenance is metadata, not
graph structure: wikilinks in arbitrary frontmatter are not backlink-indexed and
must not be relied on for citations or source relationships.

### 4. Placement is B: colocated CLI worker, not in-daemon

Ingestion uses the same placement as [ADR 0018](0018-embedding-and-vector-search.md):
**B, a colocated worker process**. It runs as a `notesmith` CLI worker on an
interval (launchd/systemd timer) or from a queue. It writes normalized markdown
notes into the vault and hands changed content to the embedding worker path,
which writes vectors into `embeddings.db`.

The daemon only reads: it remains the sole owner of the main SQLite/Tantivy note
index from [ADR 0012](0012-agent-transport-acp-mcp.md); the worker is the sole
writer of `embeddings.db` as defined by
[ADR 0018](0018-embedding-and-vector-search.md); and the daemon never fetches
external media, runs Whisper, or writes vectors.

This reconciles media ingestion with [ADR 0012](0012-agent-transport-acp-mcp.md)'s
daemon-is-the-sole-index-owner invariant: there is no second daemon-owned index
writer. Heavy, bursty, network- and CPU-bound work stays out of the long-running
interactive daemon.

> **Amended by [ADR 0020](0020-web-clipper.md):** this placement rule is narrowed
> for one case. A **user-initiated, single, bounded article clip** may fetch and
> extract inside the daemon, synchronously, behind strict timeout/size/concurrency
> limits and an SSRF guard. Audio/video ingestion, Whisper transcription, and all
> batch/scheduled refresh remain worker-only; the daemon still never runs Whisper
> or fetches media in bulk.

### 5. Per-item resilience for untrusted external content

Every fetched article, transcript, caption file, and audio-derived transcript is
untrusted external input. The [ADR 0009](0009-resilience-to-malformed-content.md)
resilience policy applies at the **item** boundary.

A failure in one item logs a warning and skips that item:

```
WARN item=<id> stage=<fetch|extract|transcribe|normalize> reason=<...>
```

It must never abort the batch, panic the worker, corrupt a generated note, or
roll back sibling items. As in [ADR 0018](0018-embedding-and-vector-search.md),
failed items remain unmatched by `content_hash`, so the next worker tick retries
them. This makes transient network failures, caption outages, malformed HTML,
and transcription crashes idempotent and self-healing.

### 6. Deduplication and refresh are keyed by canonical source URL

The canonical, normalized `source_url` is the deduplication key. Re-ingesting the
same article, podcast episode, or YouTube video updates the existing generated
note instead of creating a duplicate.

Canonicalization should remove tracking parameters and normalize equivalent
forms where source-specific rules are safe. The generated note path may be
human-readable, but identity is the canonical source URL plus `source_type`, not
the filename. This lets refresh jobs detect changed source content, update the
note, and trigger [ADR 0018](0018-embedding-and-vector-search.md)'s
`content_hash`-based delete/re-embed flow.

### 7. Hand off at the chunk boundary

Ingestion does not define a second embedding pipeline. After a note is
normalized, the worker hands it to [ADR 0018](0018-embedding-and-vector-search.md)
for:

- chunking at ~256–512 tokens;
- `char_start` / `char_end` citation offsets;
- `media_ts_start` / `media_ts_end` for podcast and YouTube chunks;
- `content_hash` incremental invalidation;
- `Embedder` provider choice;
- `VectorStore` provider choice;
- sqlite-vec now and LanceDB when the data-triggered threshold is crossed.

The media scale above is one of the reasons [ADR 0018](0018-embedding-and-vector-search.md)
chooses chunk-level vectors and a sqlite-vec → LanceDB path. This ADR must not
fork those decisions.

## Consequences

- Long-form external media becomes first-class vault content: searchable,
  filterable, citable markdown notes rather than opaque blobs.
- Search can combine semantic nearest-neighbor results from
  [ADR 0018](0018-embedding-and-vector-search.md) with provenance filters such
  as `source_type`, `author`, `channel`, `published`, and `duration`.
- Media search results can deep-link to source moments because timestamps are
  preserved before chunking.
- The daemon remains lean. If ingestion falls behind, existing notes and already
  embedded chunks still work; freshness degrades without taking down the app.
- Generated notes must be clearly identifiable and safe to update, because
  re-ingestion refreshes existing notes rather than duplicating them.

## Suggested phasing

1. **P0 — article ingestion.** Fetch URLs, reuse the web-fetch direction in
   [#207](https://github.com/surdy/notes-method/issues/207), run
   readability-style extraction, write markdown notes with provenance
   frontmatter, and hand changed notes to [ADR 0018](0018-embedding-and-vector-search.md)'s
   chunk → embed → store path.
2. **P1 — YouTube transcript ingestion.** Implement
   [#208](https://github.com/surdy/notes-method/issues/208): fetch published
   caption/transcript tracks, normalize segments, preserve timestamps, and hand
   timestamped content to [ADR 0018](0018-embedding-and-vector-search.md).
3. **P2 — podcast/audio ingestion via local Whisper.** Add audio fetch and local
   transcription, adjacent to [#204](https://github.com/surdy/notes-method/issues/204)
   and [#205](https://github.com/surdy/notes-method/issues/205). Store segment
   timestamps so chunks support media deep-links.
4. **P3 — dedup, refresh, and scheduling polish.** Canonical URL identity,
   generated-note update policy, retry/backoff, launchd/systemd timer support,
   queue support, and operational metrics for failed stages and stale content.

## Alternatives considered

- **In-daemon ingestion.** Rejected for the same reason
  [ADR 0018](0018-embedding-and-vector-search.md) rejects placement A as the
  primary media-heavy path: Whisper, bursty network fetches, and large batch
  refreshes do not belong in the interactive daemon.
- **Cloud transcription APIs as the default.** Rejected as the default on
  privacy and cost grounds. A cloud transcription provider may become an
  explicit opt-in later, mirroring [ADR 0018](0018-embedding-and-vector-search.md)'s
  optional cloud `Embedder`, but local Whisper is the baseline.
- **One vector per document / no chunking.** Rejected. A 1-hour transcript is
  roughly 13k tokens and 30–40 meaningful chunks; one vector for the whole file
  destroys retrieval precision, citations, and media deep-links.
- **Store transcripts only, with no provenance frontmatter.** Rejected because
  it kills hybrid filtering, source attribution, and reliable citations. The
  SQLite metadata store needs provenance fields to join semantic results with
  source filters.

## References

- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- [ADR 0012 — Agent Transport: ACP + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0015 — AI Agent Integration Roadmap](0015-ai-agent-integration-roadmap.md)
- [ADR 0018 — Embedding & Vector Search Architecture](0018-embedding-and-vector-search.md)
- Phase 3 epic: [#187](https://github.com/surdy/notes-method/issues/187)
  (Memory & multimodal)
- Embedding backend dependency: [#198](https://github.com/surdy/notes-method/issues/198)
- Related ingestion issues: [#204](https://github.com/surdy/notes-method/issues/204)
  (voice/meeting transcription → structured note),
  [#205](https://github.com/surdy/notes-method/issues/205) (PDF/EPUB ingestion),
  [#207](https://github.com/surdy/notes-method/issues/207) (web_fetch/web_search
  MCP tools), [#208](https://github.com/surdy/notes-method/issues/208)
  (youtube_transcript MCP tool)
