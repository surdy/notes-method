# ADR-0001: Native OFM Parser Instead of TurboVault

**Status**: Accepted  
**Date**: 2025 (early implementation)

## Context

Notesmith needs to parse Obsidian Flavored Markdown (OFM) — wikilinks, embeds, tags, callouts, inline fields, task checkboxes with 7 statuses, and block references.

We spiked TurboVault (tree-sitter markdown + custom queries) in `spikes/turbovault-spike/`. The spike revealed that tree-sitter's markdown grammar doesn't natively handle OFM extensions, requiring extensive custom query patterns that were fragile and hard to maintain.

## Decision

Use a native Rust parser with:
- **comrak** for standard Markdown → HTML rendering
- **Custom regex** for wikilinks, embeds, tags, callouts, and inline fields
- **Custom task parser** for configurable checkbox syntax (default: `[ ]`, `[/]`, `[b]`, `[w]`, `[h]`, `[x]`, `[-]`)

The parser lives in `notesmith-vault::parser` and produces `Note` (via `ParsedNote` intermediate).

## Consequences

- Full control over OFM extensions without fighting tree-sitter's grammar
- Simpler dependency chain (no tree-sitter C bindings)
- Must maintain regex patterns ourselves as OFM evolves
- Spike findings documented in `spikes/turbovault-spike/FINDINGS.md`
