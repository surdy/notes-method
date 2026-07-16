# ADR 0022 — Local Drop-Folder Ingestion Source

## Status

Accepted (2026-07-15). P0 realized by the `notesmith-ingest` crate and
`notesmith ingest` CLI command; P1 realized by the daemon-supervised
subprocess scheduler (issue [#263](https://github.com/surdy/notes-method/issues/263)).

Part of Phase 3 ([#187](https://github.com/surdy/notes-method/issues/187))
(Memory & multimodal). **Extends [ADR 0019](0019-media-ingestion-pipeline.md)**
by adding a **local filesystem source** to its ingestion worker, and depends on
the Phase 2 embedding backend ([ADR 0018](0018-embedding-and-vector-search.md))
for the chunk → embed → store handoff.

Governed by [ADR 0015](0015-ai-agent-integration-roadmap.md) Option A:
Notesmith runs no local chat LLM. The only local models in scope are embeddings
([ADR 0018](0018-embedding-and-vector-search.md)) and Whisper transcription
([ADR 0019](0019-media-ingestion-pipeline.md)).

## Context

[ADR 0019](0019-media-ingestion-pipeline.md) designs the media ingestion
pipeline for **URL-sourced** content (web articles, podcasts, YouTube). Its
identity and deduplication key is the canonical `source_url` (ADR 0019 §6), and
its triggers are the web clipper ([ADR 0020](0020-web-clipper.md)) and scheduled
URL refresh jobs.

A distinct, push-based workflow is not yet covered: an **external agent or tool
drops a media / document file into a known folder on the Notesmith server**, and
the pipeline ingests it **unattended**, with these requirements from the use
case:

- Files land in a configured **raw drop folder** (e.g. `raw/`).
- **Raw files stay in place** after processing — the pipeline never moves,
  renames, or deletes them.
- The system **tracks what is already processed and what needs (re)processing**,
  including detecting when a raw file's content has changed.
- There are **no tabs / no frontmatter possible on a binary** (`.pdf`, `.mp3`,
  `.png`), so processed-state must live somewhere else.

Local files have no `source_url`, so ADR 0019's identity/dedup key does not
apply, and there is no folder-scan trigger for local files. This ADR fills those
two gaps — identity/tracking for local files, and the drop-folder trigger —
while reusing the rest of ADR 0019 unchanged (worker placement, provenance
frontmatter, per-item resilience, chunk-boundary handoff).

## Decision

### 1. Add a local filesystem source behind the ADR 0019 source interface

Model the drop folder as a new **source module** behind ADR 0019 §1's
`Fetcher` / `Source` boundary. It reuses the worker, provenance format,
resilience model, and [ADR 0018](0018-embedding-and-vector-search.md) handoff.
Adding it must not rewrite the worker, scheduler, or provenance schema — it adds
a source and a local-file identity rule.

### 2. Configured raw drop folder, keep-in-place semantics

A per-vault configured directory (`[ingest] raw_dir`, default `raw/`) is the
drop target. External agents/tools write files into it by any means (SSH/rsync,
network share, API upload, etc. — out of scope for this ADR).

The pipeline is **read-only with respect to raw files**: it never moves,
renames, rewrites, or deletes them. This is a hard invariant so external tooling
can treat the raw folder as an append-only staging area it owns.

### 3. Identity and dedup by `(path + content_hash)`, not `source_url`

For the local source, the deduplication/identity key is the **canonical
vault-relative raw path plus the content hash of the raw bytes**, replacing ADR
0019 §6's `source_url` key. Consequences:

- Re-dropping identical content at the same path is a no-op (hash unchanged).
- Editing a raw file in place (new hash, same path) triggers **re-ingest** and
  ADR 0018's `content_hash` delete/re-embed for the affected note.
- Renaming/moving a raw file (same hash, new path) is detected as *same content,
  new path* and does not re-extract from scratch.

### 4. The generated sidecar note is the durable processed-state ledger

Every successfully ingested raw file produces a markdown note (ADR 0019 §3). For
the local source, that note is also the **record of what was processed**, since
the binary itself cannot carry metadata. Required provenance frontmatter:

```yaml
---
title: "Talk — Example"
source_type: "pdf" # pdf | epub | audio | image | ...
source_path: "raw/talk.pdf"       # the raw file, kept in place
source_hash: "sha256:…"           # hash of the raw bytes at ingest time
source_mtime: "2026-07-15T10:00:00-07:00"
ingested_at: "2026-07-15T10:02:00-07:00"
status: "ingested"                # ingested | failed | unsupported
# type-specific: duration, page_count, …
---
```

The generated note lives in the normal notes area (searchable, linkable,
citable); the raw file stays in `raw/`. `source_path` links the note back to its
binary. Note **exists** → processed; note **absent** → pending;
`source_hash` **≠** the raw file's current hash → needs re-ingest. These fields
index through the existing SQLite `fields` metadata pipeline, so `source_type`,
`duration`, etc. are joinable in hybrid search (ADR 0019 §3).

### 5. Processed-state is hash-based, not tag-based

The authoritative "is this processed / does it need reprocessing" signal is the
**`source_hash` comparison**, never a tag. Tags (`inbox`, and any routing tags)
remain purely for the routing engine and human use; they cannot detect content
drift and must not gate ingestion. This reuses ADR 0018's `content_hash`
incrementality: changed or failed items stay unmatched and are retried on the
next worker tick, which makes ingestion idempotent and self-healing (ADR 0009).

### 6. Optional worker-owned ledger table for scan efficiency and retries

Scanning every sidecar note each tick is acceptable for small vaults. If scan
cost or retry/backoff bookkeeping demands it, add a worker-owned table keyed by
`(raw_path, content_hash, status, last_error, retries, updated_at)` in a
worker-owned SQLite (not the daemon's index, per ADR 0012). The **sidecar note
remains the source of truth**; the table is a derived cache the worker may
rebuild from the vault. Do not add it speculatively.

### 7. Placement stays a colocated worker; the daemon does not ingest

Placement is unchanged from [ADR 0019](0019-media-ingestion-pipeline.md) §4:
ingestion runs in a **colocated `notesmith` CLI worker**, never inside the
interactive daemon. The daemon never runs Whisper, never performs heavy
extraction, and remains the sole reader/owner of the note index (ADR 0012). This
keeps bursty, CPU/IO-heavy ingestion out of the interactive process.

**Realized triggering (P1).** The colocated worker is triggered two ways, both
running the extraction outside the daemon process:

- **On demand** — `notesmith ingest [--vault <name>]` runs one incremental pass
  by hand (backfills, debugging, or driven by an external launchd/systemd timer).
- **Daemon-supervised subprocess scheduler** — the daemon supervises one
  long-lived task per vault (`ingest_scheduler`, mirroring the embed
  supervisor's runtime add/remove reconciliation) that, on an interval, **shells
  out to `notesmith ingest --vault <name>` as a subprocess**. The heavy
  extraction therefore runs in a child process, honouring the "never inside the
  interactive daemon" invariant while still being automatic. Each pass is gated
  per vault by the `vault.toml` `[ingest] enabled` flag, re-read fresh every tick
  so runtime toggling takes effect within one interval; a failed pass is logged
  and retried next tick. Interval and supervision cadence are overridable via
  `NOTESMITH_INGEST_INTERVAL_SECS` / `NOTESMITH_INGEST_SUPERVISE_SECS`.

### 8. Scope stops at "clean, embedded, provenance-tracked note"

Under [ADR 0015](0015-ai-agent-integration-roadmap.md) Option A, Notesmith runs
no chat LLM, so unattended ingestion delivers **searchable text plus embeddings
and provenance** fully automatically. Agent-authored **structuring** of that text
(summary / action items / decisions, e.g.
[#204](https://github.com/surdy/notes-method/issues/204)) requires an agent turn
and is **out of scope** here: Notesmith has no headless/scheduled agent runner,
and adding one is a separate decision. The drop-folder pipeline produces the
clean note; any further synthesis is a subsequent, explicitly-invoked agent
action.

### 9. Per-item resilience for untrusted dropped files

Every dropped file is untrusted external input; the
[ADR 0009](0009-resilience-to-malformed-content.md) policy applies at the
**item** boundary. A failure logs and skips:

```
WARN item=<raw_path> stage=<detect|extract|transcribe|normalize> reason=<...>
```

It must never abort the batch, panic the worker, corrupt a generated note, or
affect sibling files. **Unsupported file types** (no source module handles them)
are recorded once with `status: unsupported` and **not retried forever**;
**transient failures** (`status: failed`) remain eligible for retry on the next
tick via the hash/ledger mechanism.

### 10. Hand off at the chunk boundary

As in [ADR 0019](0019-media-ingestion-pipeline.md) §7, this ADR does not define a
second embedding pipeline. After a note is normalized, the worker hands it to
[ADR 0018](0018-embedding-and-vector-search.md) for chunking, `char_start` /
`char_end` offsets, media timestamps where applicable, `content_hash`
invalidation, and `Embedder` / `VectorStore` choice.

## Consequences

- External systems get a simple, robust contract: **drop a file in `raw/`, get a
  searchable, citable note**, with the raw file preserved.
- Processed-state and staleness are derived from content hashes, so the pipeline
  is idempotent, self-healing, and safe to re-run — no manual "mark as done".
- Provenance frontmatter makes dropped documents first-class in hybrid search
  (filter by `source_type`, `duration`, `page_count`, …), not opaque blobs.
- The daemon stays lean; if ingestion falls behind, existing notes and vectors
  keep working and freshness degrades gracefully.
- Generated notes are clearly identifiable (`source_path` + `source_hash`) and
  safe to update in place on re-ingest.
- A clear boundary is set: automatic ingestion produces clean text; **any
  agent-authored structuring is a separate, opt-in step**, preserving the ADR
  0015 no-daemon-LLM invariant.

## Suggested phasing

1. **P0 — PDF/EPUB drop folder.** Cheapest, model-free: detect type, extract
   text (reuse [#205](https://github.com/surdy/notes-method/issues/205)'s
   parser), write a provenance note keyed by `path + content_hash`, hand off to
   [ADR 0018](0018-embedding-and-vector-search.md). Establishes the drop-folder
   trigger, sidecar-ledger, and keep-in-place invariants.
2. **P1 — tracking & refresh polish.** Hash-based staleness detection, the
   optional ledger table, retry/backoff, `unsupported` handling, operational
   counters (pending / failed / unsupported), **and daemon-supervised subprocess
   scheduling gated by `[ingest] enabled`**. _Realized (#263)._
3. **P2 — audio drop folder via local Whisper.** Extend to `.mp3`/`.m4a`/`.wav`
   using [#204](https://github.com/surdy/notes-method/issues/204)'s
   transcription, preserving segment timestamps for media deep-links.
4. **P3 — image/OCR (optional).** Dropped images become text only via OCR or a
   vision model; without a bundled vision model this is limited to OCR and is
   lower priority than PDF/audio. Interactive vision stays with
   [#206](https://github.com/surdy/notes-method/issues/206) (chat image input).

## Alternatives considered

- **Move/quarantine files on process.** Rejected: the use case requires raw
  files to stay in place as an append-only staging area owned by external tools.
- **Tag-based processed-state** (`ingested` tag). Rejected: tags cannot detect
  content drift, so a re-dropped/edited file would look "done"; hashes are the
  correct staleness signal.
- **In-daemon watch-and-process.** Rejected per ADR 0019 §4: extraction/Whisper
  are bursty and heavy and must not run in the interactive daemon; a watch may
  only enqueue.
- **No sidecar note; track state only in a database.** Rejected: the generated
  note is the user-visible, portable, git-tracked record and the searchable
  artifact; a DB-only ledger is not portable and duplicates what the note
  already encodes. The DB table, if added, is a derived cache (§6).
- **Reuse `source_url` with a `file://` URL as identity.** Rejected: a
  `file://` path is not stable across renames and encodes no content identity;
  `(path + content_hash)` captures both location and drift correctly.

## References

- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- [ADR 0012 — Agent Transport: ACP + stdio/HTTP MCP](0012-agent-transport-acp-mcp.md)
- [ADR 0015 — AI Agent Integration Roadmap](0015-ai-agent-integration-roadmap.md)
- [ADR 0018 — Embedding & Vector Search Architecture](0018-embedding-and-vector-search.md)
- [ADR 0019 — Media Ingestion Pipeline](0019-media-ingestion-pipeline.md)
- [ADR 0020 — Web Clipper](0020-web-clipper.md)
- Phase 3 epic: [#187](https://github.com/surdy/notes-method/issues/187)
  (Memory & multimodal)
- Related issues: [#204](https://github.com/surdy/notes-method/issues/204)
  (voice/meeting transcription), [#205](https://github.com/surdy/notes-method/issues/205)
  (PDF/EPUB ingestion), [#206](https://github.com/surdy/notes-method/issues/206)
  (image/vision input)
