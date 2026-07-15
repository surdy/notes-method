# ADR 0020 — Web Clipper

## Status

Accepted (2026-07-09). Amended (2026-07-15) to cover **YouTube as an interactive
source type** — see [§8](#8-youtube-is-an-interactive-source-type-on-the-shared-path).

Amends [ADR 0019](0019-media-ingestion-pipeline.md) §4 (placement) for the
narrow case of user-initiated article clips. Reuses ADR 0019's article
extraction, provenance frontmatter, canonical-URL deduplication, and per-item
resilience unchanged, and hands the resulting note to
[ADR 0018](0018-embedding-and-vector-search.md) at the normal note boundary.
Builds on the existing capture path ([`capture.rs`](../../crates/notesmith-http/src/routes/capture.rs),
`CaptureConfig`) and the routing engine (`notesmith-routing`).

Tracking issues: [#261](https://github.com/surdy/notes-method/issues/261)
(article clipper, done), [#208](https://github.com/surdy/notes-method/issues/208)
(YouTube-as-source, §8).

## Context

Users want Obsidian Web Clipper–style functionality: capture a web page into the
vault as a clean markdown note. Obsidian's clipper is a browser extension that
does extraction **client-side** (its `Defuddle` engine) and delivers via the
`obsidian://` URI protocol plus the system clipboard — tightly coupled to the
Obsidian desktop app.

We deliberately diverge on two axes:

- **Extraction runs server-side from a URL.** The browser/CLI hands a URL to the
  daemon; the daemon fetches and extracts. This keeps all logic in one Rust code
  path (testable, resilient per
  [ADR 0009](0009-resilience-to-malformed-content.md)) and lets clips originate
  from a browser extension, the CLI, or raw HTTP. The tradeoff is reduced
  fidelity on JS-rendered / paywalled / bot-blocked pages, which the browser
  would otherwise see with the user's session — an accepted cost for the single
  code path.
- **We reuse Obsidian's *concepts* (per-domain templates), not its extension.**

This overlaps heavily with [ADR 0019](0019-media-ingestion-pipeline.md), which
already owns article ingestion (`fetch HTML → readability extraction →
markdown`), provenance frontmatter, canonical-URL dedup, and per-item
resilience. The web clipper **is** ADR 0019's "article" source, plus an
interactive trigger, per-domain templating, and inbox routing. It must reuse
that pipeline rather than fork it.

### The conflict this ADR resolves

[ADR 0019](0019-media-ingestion-pipeline.md) §4 states the daemon **never
fetches external media**; a colocated CLI worker does, on a timer/queue, as a
batch process. That invariant's real intent is to keep **heavy, bursty,
CPU-bound work (Whisper transcription) and unbounded batch media** out of the
long-running interactive daemon.

A web clipper is fundamentally **interactive**: the user clicks and expects the
note (and a duplicate warning) to appear immediately. Routing that through a
scheduled batch worker means the note appears only after the next worker tick
and requires a worker process running on every target (desktop **and** the
remote server) purely to support an interactive action. A single bounded HTML
`GET` for one article is nothing like Whisper.

## Decision

### 1. Interactive article fetch is allowed in the daemon (amends ADR 0019 §4)

A **user-initiated, single, bounded article clip** may fetch and extract inside
the daemon, synchronously, behind strict limits (see §6). This is a narrow
carve-out from [ADR 0019](0019-media-ingestion-pipeline.md) §4.

Explicitly **unchanged**: audio/video ingestion, Whisper transcription, and all
**batch / scheduled refresh** remain worker-only. The daemon still never runs
Whisper, never fetches media in bulk, and remains the sole owner of the main
note index ([ADR 0012](0012-agent-transport-acp-mcp.md)).

### 2. One shared extraction library

Article fetch + readability extraction + HTML→markdown live in a single crate
(`notesmith-clip`, or a shared `ingestion` library) called by **both**:

- the interactive clip endpoint (this ADR), and
- the [ADR 0019](0019-media-ingestion-pipeline.md) CLI worker (batch article
  refresh, and the article path shared with podcast/YouTube ingestion).

No forked extraction logic. Provenance frontmatter, canonical-URL dedup, and
per-item resilience are defined once and reused.

### 3. Clip endpoint

`POST /api/v/{vault}/clip` with body `{ url, tags?, note? }`:

1. Canonicalize `url` (strip tracking params) → dedup check against the
   `source_url` field via the index. On match, return the existing note with
   `duplicate: true` (**detect + warn**, do not overwrite).
2. Fetch (bounded — see §6), extract the article body, convert to markdown.
3. Select a template by domain (§4), render via `minijinja` (the engine
   `notesmith-templates` already uses).
4. Optionally download images into the vault (§5).
5. Write the note into the **inbox** tagged `inbox`; the existing
   `notesmith-routing` engine files it later. Emit a `NoteClipped` event
   (or reuse `NoteCaptured`).

Mirrors the existing [`capture.rs`](../../crates/notesmith-http/src/routes/capture.rs)
pattern. `GET /api/app/vaults` already exists for the extension's vault picker.

Frontmatter follows [ADR 0019](0019-media-ingestion-pipeline.md) §3:
`source_url`, `source_type: article`, `title`, `author`/`published` when known,
`ingested_at`. Plus `tags: [inbox, ...]`.

### 4. Per-domain templates

`ClipConfig` in `notesmith-config` (`vault.toml`) carries a default template and
an ordered list of per-domain templates:

```toml
[clip]
enabled = true
folder = ""          # default: inbox / capture folder
download_images = true

[[clip.templates]]
match = "news.ycombinator.com"   # domain or regex
frontmatter = ["title", "author", "published", "source_url"]
body = "{{ content }}"

[[clip.templates]]
match = "*"                       # default fallback
frontmatter = ["title", "source_url", "published"]
body = "{{ content }}"
```

First matching template wins; `*` is the fallback. Templates are rendered with
`minijinja` and have access to extracted variables (`title`, `author`,
`published`, `url`, `content`, `tags`, ...).

### 5. Images

Config toggle `download_images` (**default true**). When on, `<img>` sources are
fetched into a vault attachments folder (subject to the §6 fetch limits) and
links rewritten to local paths. When off, remote URLs are kept.

### 6. Security posture (bounded, unauthenticated, SSRF-guarded)

The clip endpoint is **unauthenticated** for v1, per an explicit product
decision, and the target daemon base URL is **configurable** (local or remote).
This is recorded as a conscious risk: anyone who can reach the endpoint can drive
server-side fetches and write to the vault (vault spam), even remotely.

To bound the exposure, an **SSRF guard is mandatory** regardless:

- Block loopback, private (RFC 1918), link-local, and other non-public IP ranges
  after DNS resolution (guard against DNS-rebinding by resolving then pinning).
- Enforce fetch **timeout**, **max response size**, redirect cap, and a
  **concurrency limit** on in-daemon fetches so a slow/large page can't jam the
  daemon.
- Treat all fetched HTML as untrusted per
  [ADR 0009](0009-resilience-to-malformed-content.md): no panics, degrade to a
  minimal note on extraction failure, log
  `WARN note=<path> stage=<fetch|extract|template> reason=<...>`.

Auth (shared token, or token-when-remote) is a fast follow if the endpoint is
exposed beyond a trusted network; the SSRF guard and bounded-fetch design are
prerequisites for that follow-up, not blockers for it.

### 7. Trigger surfaces

- **HTTP endpoint** — the foundation (§3); also enables share sheets/automation.
- **CLI** — `notesmith clip <url> [--vault] [--tag]`, hitting the endpoint.
- **Minimal browser extension** — Manifest V3 toolbar button; config = base URL
  + vault picker (via `GET /api/app/vaults`); POSTs the current tab's URL. No
  client-side extraction.

### 8. YouTube is an interactive source type on the shared path

This section extends the clipper to YouTube, resolving the ambiguity between the
standalone `youtube_transcript` MCP tool ([#208](https://github.com/surdy/notes-method/issues/208),
[ADR 0019](0019-media-ingestion-pipeline.md) P1) and the interactive clipper.
They are **not two implementations**: YouTube is a `source_type` on the same
shared extraction/ingestion library from §2, with two thin entry points.

**8.1 Same shared library, new source module.** Caption-track fetch, segment
normalization, timestamp preservation, canonical-URL dedup, and provenance
frontmatter for YouTube live in the same crate as article extraction
(`notesmith-clip` / shared `ingestion`), behind the ADR 0019 §1 `Source` trait.
No forked YouTube fetch/parse path may exist. This is the §2 rule applied to a
second source type.

**8.2 The clip endpoint detects source type.** `POST /api/v/{vault}/clip` (§3)
classifies the incoming `url`. A YouTube URL is canonicalized (strip `&t=`,
`&list=`, `&index=`, and tracking params; identity is the canonical video URL
plus `source_type: youtube` per [ADR 0019](0019-media-ingestion-pipeline.md) §6),
dedup-checked against the `source_url` field, then routed to the YouTube source
module. All other §3 steps (template render, inbox write, `NoteClipped` event,
routing) are unchanged. Frontmatter follows
[ADR 0019](0019-media-ingestion-pipeline.md) §3 for media:
`source_type: youtube`, `title`, `channel`, `published`, `duration`,
`source_url`, `ingested_at`, plus `tags: [inbox, ...]`. Segment timestamps are
preserved as `media_ts_start` / `media_ts_end` at the chunk handoff so search
results deep-link to the moment.

**8.3 Captions in-daemon; Whisper stays worker-only.** A published caption track
is a single bounded `GET` — the same shape as the §1 article carve-out — so
fetching it interactively inside the daemon is allowed, under the §6 SSRF guard
and bounded-fetch limits. **If no usable caption track exists, the daemon does
not transcribe.** It returns a clear, non-fatal result and hands the video to the
[ADR 0019](0019-media-ingestion-pipeline.md) §4 CLI worker for Whisper fallback.
The worker, its queue, engine, and bundled model are defined by
[ADR 0023](0023-local-whisper-transcription-worker.md).
ADR 0019 §4 is otherwise unchanged: the daemon still never runs Whisper and never
fetches media in bulk.

**8.3.1 Caption tracks come from the InnerTube player API, not the watch page.**
The caption `baseUrl`s embedded in a watch page's `ytInitialPlayerResponse` are
PoToken/session-locked and return empty bodies to a plain server-side scrape
(confirmed by smoke test; the same wall that breaks `youtube-transcript-api`).
To obtain caption tracks that actually serve timedtext, the source module POSTs
to YouTube's unofficial InnerTube endpoint
(`https://www.youtube.com/youtubei/v1/player`) with the **ANDROID client
context** and matching `Origin`/`Referer`/`User-Agent` headers, then reads the
caption `baseUrl`s from that response. This mirrors how Obsidian's clipper
(via its Defuddle extractor) fetches transcripts. The tradeoff is a dependency
on a private API and a pinned client version — an accepted, contained
maintenance surface: the blast radius is a single POST-plus-parse path inside
`notesmith-clip::youtube`, still under the §6 SSRF guard and bounded-fetch
limits, and callers (§8.2 clip endpoint, §8.4 MCP tool) consume `fetch_youtube`
unchanged. The timedtext parser accepts both the **srv3** `<p t="ms" d="ms">`
format (what the InnerTube ANDROID track serves) and the legacy
`<text start="s" dur="s">` format.

**8.4 The MCP tool is a thin wrapper.** The `youtube_transcript` MCP tool (#208)
calls the same source module and returns the transcript text (and, where useful,
the normalized note). It is a second entry point over the shared library, not a
parallel extractor — matching the §2 / "one extraction code path" invariant.
Because it is agent-invoked and may run against a remote daemon, it obeys the
same captions-in-daemon / Whisper-in-worker boundary as §8.3.

**8.5 Per-domain templates cover YouTube.** The §4 `ClipConfig` template list
matches `youtube.com` / `youtu.be` like any other domain, with access to the
media variables (`channel`, `duration`, `published`, timestamped `content`).

## Consequences

- Interactive "click → note appears" clipping without standing up a worker on
  every target; the clipper works identically against a local or remote daemon.
- ADR 0019's ingestion architecture stays intact where it matters: Whisper,
  audio/video, and batch refresh remain worker-only; the daemon takes on only a
  bounded, single-item HTML fetch.
- One extraction code path serves both interactive clips and batch ingestion.
- The daemon gains a network-egress surface. It is contained by the mandatory
  SSRF guard and bounded-fetch limits; the unauthenticated posture is an
  accepted v1 risk with a defined token follow-up.
- Reduced fidelity on JS-heavy / paywalled / bot-blocked pages vs. a client-side
  extractor — the accepted cost of server-side extraction and a single code
  path.

## Alternatives considered

- **Enqueue → CLI worker (async).** Purest reuse of ADR 0019, no amendment, free
  retries. Rejected for v1: not instant, no immediate "saved to *this* note"
  feedback, and requires a scheduled worker running on every target (including
  the remote server) solely to support an interactive action.
- **Daemon spawns the CLI worker inline and waits.** Technically keeps fetch out
  of the daemon process. Rejected: still blocks a daemon request on network I/O
  (same hang/SSRF exposure via the request lifecycle), adds subprocess overhead
  and error-plumbing, and satisfies the letter but not the spirit of the ADR
  0019 invariant.
- **Fork Obsidian's client-side extractor.** Best fidelity on JS/paywalled
  pages. Rejected: a browser extension to build, sign, publish, and maintain
  across browsers and Manifest V3 churn, plus a second (JS) extraction code path
  diverging from ADR 0019's Rust ingestion.
- **Impersonate Obsidian (`obsidian://` handler + clipboard).** Lets the
  unmodified Obsidian extension target Notesmith. Rejected: fragile,
  desktop-only, clipboard-dependent.
