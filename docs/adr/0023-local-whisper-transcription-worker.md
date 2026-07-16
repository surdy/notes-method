# ADR 0023 — Local Whisper Transcription Worker

## Status

Accepted (2026-07-15). Engine choice **ratified** on 2026-07-15: **Whisper
(whisper.cpp via `whisper-rs`)** is the primary `Transcriber` backend for the
first slice; **Parakeet (ONNX via `transcribe-rs`)** is explicitly preserved as
a future trait-backed backend (§1/§2), added later without caller changes.
Implementation is phased via
[#271](https://github.com/surdy/notes-method/issues/271) (P2a core),
[#270](https://github.com/surdy/notes-method/issues/270) (P2b worker/queue),
[#272](https://github.com/surdy/notes-method/issues/272) (P2c YouTube fallback),
and [#273](https://github.com/surdy/notes-method/issues/273) (P2d agent structuring).

Part of Phase 3 ([#187](https://github.com/surdy/notes-method/issues/187))
(Memory & multimodal). Implements the transcription half of
[ADR 0019](0019-media-ingestion-pipeline.md) §2/§4 and the
[ADR 0020](0020-web-clipper.md) §8.3 "no-captions → Whisper worker" handoff.
Tracks [#204](https://github.com/surdy/notes-method/issues/204) (voice / meeting
transcription) and the audio fallback branch of
[#208](https://github.com/surdy/notes-method/issues/208) (YouTube).

Governed by [ADR 0015](0015-ai-agent-integration-roadmap.md) Option A: Notesmith
runs **no local chat LLM**. The only daemon/worker-side models are embeddings
([ADR 0018](0018-embedding-and-vector-search.md)) and now Whisper transcription.
Structuring a transcript into a summary / action items / decisions is the user's
**ACP agent's** job over MCP — not Notesmith's.

## Context

[ADR 0019](0019-media-ingestion-pipeline.md) fixed the ingestion architecture:
`fetch → extract/clean → (transcribe | fetch transcript) → normalize markdown →
chunk → embed → store`, with **placement B** — a colocated `notesmith` CLI
worker, never the daemon — owning transcription. It deliberately deferred the
concrete transcription engine, model-delivery, and audio-acquisition decisions.
This ADR makes them.

Two consumers now need transcription:

1. **[ADR 0020](0020-web-clipper.md) §8.3 handoff.** A user clips a YouTube URL
   with no published caption track. The daemon returns a non-fatal
   `NoCaptions` result and must hand the video to a worker for Whisper over the
   audio. Today `clip.rs` only leaves a `TODO(#208 P2): enqueue Whisper worker
   handoff`; no worker or queue exists.
2. **[#204](https://github.com/surdy/notes-method/issues/204) local audio.** A
   user drops a meeting/voice recording (or a podcast audio file) and wants a
   timestamped transcript note they (or their agent) can turn into structured
   notes.

Both need the same core: audio in → timestamped transcript → normalized
markdown note with provenance frontmatter → [ADR 0018](0018-embedding-and-vector-search.md)
chunk handoff, with segment timestamps preserved as `media_ts_start` /
`media_ts_end`.

`#204` was backlogged on one concern: **bundling a local model**. Embeddings
already solved the same problem — `bge-small` is bundled via a Tauri resource
(`resources/embed-model/*`), resolved at runtime through
`NOTESMITH_EMBED_MODEL_DIR`, with a download fallback (see ADR 0018 §9.2). This
ADR reuses that precedent verbatim.

## Decision

### 1. A dedicated `notesmith-transcribe` crate behind a `Transcriber` trait

Transcription lives in a new crate `notesmith-transcribe` (not
`notesmith-transcript`, which already owns agent-chat persistence). It exposes a
small engine-agnostic boundary so the engine can change without touching the
worker, note normalization, or the ADR 0018 handoff:

```rust
pub struct AudioInput { /* path or decoded PCM + sample rate */ }

pub struct TranscriptSegment {
    pub start: f64,   // seconds
    pub end: f64,     // seconds
    pub text: String,
}

pub struct Transcript {
    pub language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
}

pub trait Transcriber: Send + Sync {
    fn transcribe(&self, audio: &AudioInput) -> Result<Transcript, TranscribeError>;
}
```

This mirrors the `Embedder` / `VectorStore` trait boundary from ADR 0018 §1: the
architectural line is fixed even though the concrete engine can evolve. The
`TranscriptSegment` shape is intentionally identical to the YouTube caption
segment in `notesmith-clip::youtube`, so both feed the same normalization path.

### 2. Engine: whisper.cpp via `whisper-rs` (with a documented Parakeet alternative)

The engine choice was evaluated between two Rust-viable options. **We choose
whisper.cpp via `whisper-rs`** for the first slice, behind the §1 trait so it
can be swapped later.

**Option A — whisper.cpp via [`whisper-rs`](https://github.com/tazz4843/whisper-rs) (chosen)**

- **Pros**
  - **Licensing is clean end-to-end and bundleable.** `whisper-rs` and
    `whisper.cpp` are MIT; OpenAI's Whisper weights are MIT; ggerganov's GGML
    conversions on Hugging Face are MIT. A bundled `ggml-*.bin` raises no
    redistribution question — the exact property that unblocks the §3 bundle
    decision that stalled #204.
  - **Mirrors the embeddings precedent 1:1** — a single model file resolved from
    a directory, env-var override, download fallback. Low novelty, low risk.
  - **Segment timestamps are first-class.** ADR 0019 §2 makes timestamps
    mandatory; whisper.cpp emits per-segment start/end natively, no extra
    alignment step.
  - **Metal / Core ML acceleration on Apple Silicon** (the primary dev/target
    platform) is mature.
  - Battle-tested, widely deployed, stable bindings.
- **Cons**
  - Adds a **C/C++ build dependency** (cmake/clang) to the workspace build.
  - Whisper weights are larger than Parakeet for comparable English accuracy;
    Parakeet is often faster.

**Option B — Parakeet (ONNX) via [`transcribe-rs`](https://github.com/cjpais/transcribe-rs) (the crate Handy extracted)**

- **Pros**
  - **Reuses the ONNX Runtime already in our tree.** `fastembed` pulls in
    `ort` 2.0 (confirmed in `Cargo.lock`), so an ONNX engine adds no new native
    runtime — a genuine synergy.
  - Parakeet-TDT is **fast and accurate** (v3 is multilingual); `transcribe-rs`
    also offers a whisper.cpp backend behind one interface.
  - No C++ whisper build if the Parakeet-only backend is used.
- **Cons**
  - **Model licensing is messier to bundle.** Parakeet ONNX weights are
    NVIDIA-licensed (CC-BY-style, attribution-bound) rather than MIT — a heavier
    redistribution review than Whisper's MIT, cutting against the whole point of
    the §3 bundle decision.
  - `transcribe-rs` is **young (2025)** and less battle-tested than `whisper-rs`.
  - Larger, multi-engine dependency surface to audit and pin.

**Rationale.** The user chose to **bundle** the model (§3). Whisper's clean MIT
weights make bundling trivially defensible, whereas bundling NVIDIA Parakeet
weights needs a separate license review — so licensing, not raw speed, decides
the first slice. whisper.cpp also gives native timestamps and reuses the exact
bundling machinery embeddings already proved. The `ort` synergy is real and
attractive, so the §1 `Transcriber` trait explicitly preserves the option to add
a `transcribe-rs`/Parakeet backend later (e.g. a faster non-bundled
"download Parakeet" tier) without disturbing callers.

### 3. Model delivery: bundle via Tauri resource, mirror `bge-small`

Per the user decision, the default model is **bundled**, reusing the embeddings
mechanism exactly:

- Bundle a **quantized `ggml` English model** (target `base.en`, q5_1 ≈ ~60 MB)
  under a new Tauri resource `resources/whisper-model/*` → `whisper-model/`,
  gitignored weights + a tracked README, fetched by a
  `fetch-whisper-model.sh` (mirroring `fetch-embed-model.sh`).
- Resolve at runtime from **`NOTESMITH_WHISPER_MODEL_DIR`** (mirroring
  `NOTESMITH_EMBED_MODEL_DIR`): if it points at a directory containing the
  expected `ggml-*.bin`, load it; otherwise fall back to a cached download.
- The desktop sets `NOTESMITH_WHISPER_MODEL_DIR` when spawning the worker, the
  same way `daemon.rs` passes `model_dir` for embeddings.
- A larger/more-accurate model (e.g. `small`, or multilingual) is opt-in via the
  env var or config, never bundled by default (bundle size discipline).

Compiled-in availability is reported through the existing
`/api/capabilities` surface (a `transcription` block mirroring `embeddings`), so
clients can gate UI on real support — reading a `notesmith_transcribe`
compiled-in constant, **not** `cfg!(feature=...)` in `notesmith-http` (the exact
bug fixed for embeddings in commit `a8b8f55`).

### 4. Placement B worker, mirroring the embeddings worker

Transcription runs in the **colocated `notesmith` CLI worker**, never the daemon
(ADR 0019 §4, unamended for audio; ADR 0020's daemon carve-out is captions-only).
It mirrors the `notesmith-embed` worker (`EmbedWorker`, `notesmith embed` CLI,
`notesmith-http/src/embed_scheduler.rs`):

- A **`notesmith transcribe <audio-path>`** CLI subcommand transcribes a local
  audio file and writes a normalized note (the #204 entry point and the manual
  smoke path).
- A worker loop drains a **pending-transcription queue** (see §5) on an interval,
  the same scheduling shape as the embed scheduler.
- Incrementality and idempotency reuse ADR 0019 §5/§6: identity is the canonical
  `source_url` (or, for local drops, the ADR 0022 raw-path + content-hash key);
  a failed item stays unmatched and is retried next tick.

### 5. The daemon enqueues; it never transcribes

The daemon's only role is to **record intent**, never to run Whisper or fetch
audio. The ADR 0020 §8.3 `NoCaptions` branch (currently a TODO in `clip.rs`)
writes a **pending-transcription queue entry** keyed by canonical `source_url`
with the already-known `YoutubeMeta` provenance, then returns the non-fatal
result to the caller. The queue is a small SQLite-backed table owned by the
worker's domain (not the daemon's note index — preserving the ADR 0012
sole-index-owner invariant); the daemon only appends intent rows. The worker
later: acquires the audio (§6), transcribes (§1–§3), and writes/updates the note.

### 6. Audio acquisition is worker-only and source-specific

- **Local files (#204):** the worker reads the audio directly (decode to PCM for
  the engine). No network.
- **YouTube fallback:** the worker downloads the video's **audio-only adaptive
  stream** (obtained from the same InnerTube player response the caption path
  already uses — see ADR 0020 §8.3.1), under the SSRF guard and bounded-fetch
  limits, then transcribes. This is the only network fetch this ADR adds, and it
  is strictly worker-side and bounded. `yt-dlp`/`ffmpeg` shell-outs are **not**
  required for the first slice and are explicitly out of scope; if a future
  source needs container demuxing, it is a new source module, not a rewrite.

### 7. Normalize to a note; structuring is the agent's job

The worker produces a **timestamped transcript note** with ADR 0019 §3
provenance frontmatter (`source_type: youtube|podcast|audio`, `title`,
`channel`/`author`, `published`, `duration`, `source_url` or raw path,
`ingested_at`) and a body of `H:MM:SS` timestamped segments — identical in shape
to the YouTube caption note from `notesmith-clip::youtube`, so both share the
renderer. It then hands the note to the ADR 0018 chunk → embed → store path with
`media_ts_start`/`media_ts_end` preserved.

Per ADR 0015 Option A, Notesmith does **not** summarize or extract action items
itself. The "structured note (summary + action items + decisions)" acceptance
criterion of #204 is satisfied by the user's ACP agent calling MCP tools over
the resulting transcript note — not by a Notesmith-side LLM.

### 8. Per-item resilience for untrusted audio (ADR 0009 / ADR 0019 §5)

Audio, downloaded streams, and engine output are untrusted. Every worker item is
isolated: a decode failure, a corrupt/zero-length audio file, an engine crash,
or a pathological duration logs
`WARN item=<id> stage=<acquire|decode|transcribe|normalize> reason=<...>` and
skips that item. It must never abort the batch, panic the worker, corrupt a
generated note, or roll back siblings. `unwrap`/`expect` are forbidden on
audio-derived values (ADR 0009). Long/huge inputs are bounded (max duration /
max bytes) so one file cannot wedge the worker.

## Consequences

- The ADR 0020 §8.3 handoff becomes real: no-caption YouTube clips and local
  audio both reach a timestamped, searchable, citable markdown note.
- The daemon stays lean and index-only; all CPU/GPU-heavy, bursty transcription
  is worker-side, consistent with ADR 0012 and ADR 0019 §4.
- Adds a C/C++ build dependency (whisper.cpp) and a bundled model to the desktop
  build; bundle size grows by the chosen quantized model (~60 MB target).
- The `Transcriber` trait keeps a clean path to a later `transcribe-rs`/Parakeet
  ONNX backend that reuses the existing `ort` runtime, without reworking
  callers.
- Structuring/summarization remains agent-side (ADR 0015 Option A); Notesmith
  ships transcripts, not summaries.

## Suggested phasing

1. **P2a — transcription core. ✅ Realized (#271, this commit).**
   `notesmith-transcribe` crate: engine-agnostic `Transcriber` trait +
   `AudioInput`/`Transcript`/`TranscriptSegment`/`TranscribeError` data model,
   `StubTranscriber` (lean placeholder) and feature-gated `LocalWhisper`
   (`whisper-rs`) engine, `LOCAL_WHISPER_COMPILED` capability constant,
   bundled-model resolution via `NOTESMITH_WHISPER_MODEL_DIR` mirroring
   `bge-small`, and the shared transcript → note renderer now owned here and
   consumed (re-exporting `TranscriptSegment`) by `notesmith-clip`. Ships the
   `notesmith transcribe <audio>` CLI. TDD, resilience + no-panic tests. No
   network. Satisfies #204's core (transcript → note).
2. **P2b — worker + queue + capabilities. ✅ Realized (#270, this commit).**
   Per-vault pending-transcription queue (`transcribe.db`, worker-domain SQLite
   keyed by canonical `source_url`, version-guarded, WAL), `TranscribeWorker`
   draining it into notes (per-item isolation, retry under an attempt cap),
   `notesmith transcribe --drain` CLI, a daemon-supervised `transcribe_scheduler`
   shelling out to that CLI per vault on an interval (gated by `[transcribe]
   enabled`, mirroring the ingest scheduler — heavy inference never runs in the
   daemon), the `[transcribe]` vault config, the `/api/capabilities`
   transcription block, and the `clip.rs` `NoCaptions` branch now enqueuing a
   `youtube` intent (daemon records intent only). TDD throughout.
3. **P2c — YouTube audio fallback.** The `clip.rs` `NoCaptions` branch already
   enqueues a `youtube` queue row (P2b); P2c is the worker side: download the
   InnerTube audio-only stream (bounded, SSRF-guarded) for those rows and
   transcribe. Closes the ADR 0020 §8.3 loop.
4. **P2d — agent structuring (docs only).** Confirm the MCP surface an agent uses
   to turn a transcript note into summary/action-items/decisions; no new
   Notesmith-side model.
