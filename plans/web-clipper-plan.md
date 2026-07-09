# Web Clipper — Feature Plan

Status: **implemented** (2026-07-09) across P1–P4. Tracking issue:
[#261](https://github.com/surdy/notes-method/issues/261). Architecture:
[ADR 0020](../docs/adr/0020-web-clipper.md),
which amends [ADR 0019](../docs/adr/0019-media-ingestion-pipeline.md) §4 and
reuses its article-ingestion pipeline.

## Goal

Obsidian Web Clipper–style capture: turn a web page into a clean markdown note in
the vault. Extraction runs **server-side from a URL**; the browser/CLI only hands
over the URL. Reuse Obsidian's *concepts* (per-domain templates), not its
extension.

## Locked decisions

| Question | Decision |
|---|---|
| Extraction locus | **Server-side from URL** (daemon fetches + extracts) |
| Reuse of Obsidian extension | No — reuse concepts only; build our own thin trigger |
| Triggers | **HTTP endpoint** (foundation) → **CLI** → **minimal browser extension** |
| Destination | **Inbox** (tagged `inbox`), then existing routing engine files it |
| Templating | **Per-domain templates** (default fallback + per-site) |
| Images | **Config toggle, default = download** into vault |
| Re-clip same URL | **Detect + warn** (return existing note, don't overwrite) |
| Daemon target | **Configurable base URL** (local or remote) |
| Auth | **None** for v1 (explicit accepted risk) |
| SSRF guard | **Mandatory** (block loopback/private/link-local; timeout/size/concurrency caps) |
| Daemon-fetch reconciliation | **Interactive fetch in daemon** + narrow ADR-0019 amendment + shared extraction crate |

## How it fits the existing codebase

- Mirrors the existing capture path: `crates/notesmith-http/src/routes/capture.rs`
  + `CaptureConfig { folder }` in `crates/notesmith-config/src/vault.rs` +
  `NoteCaptured` event. The clipper is "capture, but from a URL."
- `notesmith-templates` already uses **minijinja** → per-domain clip templates
  reuse it.
- `notesmith-routing` already routes on the `inbox` tag → clips slot in with no
  routing changes.
- `notesmith-html` does Markdown→HTML (comrak); the clipper needs the reverse
  (HTML→Markdown) plus readability-style extraction.
- Reuses [ADR 0019](../docs/adr/0019-media-ingestion-pipeline.md) provenance
  frontmatter (`source_url`, `source_type: article`, `title`, `author`,
  `published`, `ingested_at`), canonical-URL dedup, and per-item resilience.

## New / changed pieces

1. **`notesmith-clip` crate (shared extraction library).** `fetch(url)` →
   readability-style extraction → HTML→Markdown → domain template selection →
   minijinja render → frontmatter + body. Houses the SSRF guard + bounded fetch.
   Called by **both** the daemon clip endpoint and the ADR-0019 CLI worker (no
   forked logic). Candidate crates: `readability` / `dom_smoothie` for
   extraction, `htmd` / `html2md` for HTML→Markdown (evaluate maturity in P1).
2. **`ClipConfig` in `notesmith-config` (`vault.toml`).** `enabled`, `folder`
   (default inbox/capture), `download_images` (default true), `templates` =
   ordered list of `{ match (domain/regex), frontmatter[], body }` + `*`
   fallback.
3. **`POST /api/v/{vault}/clip`** route. Body `{ url, tags?, note? }`. Canonicalize
   + dedup-check → fetch → extract → template → (download images) → write to
   inbox tagged `inbox` → emit event. Returns note path, or existing note with
   `duplicate: true`.
4. **Dedup** keyed by canonical `source_url` frontmatter field via the index
   (synchronous read in the endpoint).
5. **Image download** into a vault attachments folder, link rewrite, subject to
   fetch limits; skipped when `download_images = false`.
6. **CLI** — `notesmith clip <url> [--vault] [--tag]` hitting the endpoint.
7. **Browser extension** — Manifest V3 toolbar button; config = base URL + vault
   picker (via `GET /api/app/vaults`); POSTs current tab URL. No client-side
   extraction.
8. **Docs** — `docs/http-api.md` (endpoint), `docs/cli.md` (command),
   `docs/sql-views.md` if a clips view is added; update `notes-method.md`,
   `CONTEXT.md`, `README.md`, `plans/notesmith-plan.md`. ADR
   [0020](../docs/adr/0020-web-clipper.md) written; ADR 0019 §4 cross-referenced.

## Security (v1)

Unauthenticated + configurable/remote base URL is an explicit accepted risk
(vault spam + server-side fetch). **SSRF guard is mandatory:** resolve-then-pin
DNS, block loopback/private/link-local ranges, enforce timeout + max size +
redirect cap + concurrency limit. All fetched HTML is untrusted
([ADR 0009](../docs/adr/0009-resilience-to-malformed-content.md)): no panics,
degrade to a minimal note, `WARN note=<path> stage=<...> reason=<...>`. A shared
token (or token-when-remote) is the defined fast follow.

## Phasing

- **P1 — foundation.** ✅ Done. `notesmith-clip` crate + `POST /clip` + default
  rendering + canonical-URL dedup + SSRF guard + bounded fetch. Tests:
  happy-path, malformed-HTML, empty-article, no-panic on pathological input
  (deep nesting), dedup-detect. Docs: `http-api.md`.
- **P2 — templating + images.** ✅ Done. Per-domain `[[clip.templates]]`
  (minijinja frontmatter/body, longest host-suffix match + `*` fallback) +
  image download (`download_images`, `attachments_folder`) with link rewrite.
  Tests for template selection/rendering + image find/rewrite/SSRF-block.
  Docs: `vault-configuration.md`.
- **P3 — CLI.** ✅ Done. `notesmith clip <url> [--tag ...]`. Docs: `cli.md`.
- **P4 — browser extension.** ✅ Done. Manifest V3 in **`ui/extension/`** (chosen
  over a separate repo to keep it colocated with the daemon it targets):
  toolbar popup, live vault picker (`GET /api/app/vaults`), configurable base
  URL, runtime host-permission request. Plain ES modules, no build step. Docs:
  `ui/extension/README.md`.

## Risks

- **No auth + remote + server fetch** — SSRF guard limits internal-network abuse
  but not vault spam; recorded as an accepted v1 risk with a token follow-up.
- **Per-domain templates** are the biggest scope item; ship a strong default
  first (P1) so per-site is additive (P2), not blocking.
- **Rust extraction maturity** is weaker than JS `Defuddle`; expect tuning. This
  is the accepted cost of the single-code-path / server-side choice.
