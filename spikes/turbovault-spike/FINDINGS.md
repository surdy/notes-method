# TurboVault Evaluation Spike — Findings

**Date:** 2026-05-09
**Crate versions tested:** turbovault-core 1.5.0, turbovault-parser 1.5.0, turbovault-vault 1.5.0
**Vault fixture:** `golden-vault/` (16 markdown files covering all note types)

## Executive Summary

TurboVault is a well-engineered Rust crate ecosystem with solid fundamentals (wikilinks, embeds, frontmatter, atomic writes). However, it has **three critical gaps** that make it unsuitable as the primary parser for Notesmith without significant forking:

1. **Extended task statuses not detected** — `[b]`, `[w]`, `[h]`, `[/]`, `[-]` are invisible to the document parser
2. **No inline field extraction** — `[key:: value]` in note body is not parsed
3. **Architectural divergence** — SHA-256 vs blake3, serde_json::Value vs typed enums, GlueSQL vs rusqlite

**Decision: SWAP** — Use a native parser behind the `VaultEngine` trait. Salvage TurboVault's regex patterns and task metadata parser as reference implementations.

---

## Detailed Findings

### 1. YAML Frontmatter Extraction ✅ PASS

All note types parse correctly. All frontmatter keys are preserved, including custom ones:

| Note Type | Keys Detected |
|-----------|--------------|
| daily | type, date, tags, created, updated |
| meeting (internal) | type, meeting-kind, customer, stream, date, tags, created, updated |
| meeting (external) | type, meeting-kind, customer, stream, date, tags, created, updated |
| stream | type, customer, stream, status, priority, owner, started, target, tags, created, updated |
| customer | type, customer, state, tags, created, updated |
| account-info | type, customer, tags, created, updated |
| glossary | type, customer, tags, created, updated |
| milestones | type, customer, tags, created, updated |
| dashboard | type, tags, created, updated |
| note | type, tags, created, updated |

**Issue:** Frontmatter is stored as `HashMap<String, serde_json::Value>`, not as typed enums. Notesmith wants `Frontmatter::Daily(DailyMeta)`, `Frontmatter::Meeting(MeetingMeta)`, etc. This is an impedance mismatch — every consumer would need to do dynamic key lookups and type coercion.

### 2. Wikilink Parsing ✅ PASS

All wikilink forms work correctly:

| Syntax | Result | display_text |
|--------|--------|-------------|
| `[[Acme Corp]]` | target: "Acme Corp" | None |
| `[[Acme Corp#Current Status]]` | target: "Acme Corp#Current Status" | None |
| `[[Widget API#^pricing-block]]` | target: "Widget API#^pricing-block" | None |
| `[[John Smith\|John]]` | target: "John Smith" | Some("John") |

**Note:** The initial research claimed aliases were not supported — this was incorrect. The wikilink parser correctly splits on `|` and populates `display_text`.

**Minor issue:** The wikilink regex parser classifies all wikilinks as `LinkType::WikiLink` regardless of `#Heading` or `#^block` suffixes. The link type differentiation (HeadingRef, BlockRef) only happens in the graph analysis layer, not at parse time. For Notesmith, we'd want type classification at parse time.

### 3. Task Parsing ❌ CRITICAL FAILURE

This is the most significant gap. The notes method requires 7 task statuses. TurboVault detects only 2 in the document parser:

| Checkbox | Status | Detected as Task? | Notes |
|----------|--------|-------------------|-------|
| `- [ ]` | To Do | ✅ Yes | pulldown-cmark recognizes |
| `- [x]` | Done | ✅ Yes | pulldown-cmark recognizes |
| `- [/]` | In Progress | ❌ No | pulldown-cmark ignores |
| `- [-]` | Cancelled | ❌ No | pulldown-cmark ignores |
| `- [b]` | Blocked | ❌ No | pulldown-cmark ignores |
| `- [w]` | Waiting | ❌ No | pulldown-cmark ignores |
| `- [h]` | On Hold | ❌ No | pulldown-cmark ignores |

**Root cause:** TurboVault's document parser uses `pulldown_cmark::Event::TaskListMarker`, which only fires for `- [ ]` and `- [x]`/`- [X]`. Custom checkbox markers are treated as regular list items.

**Evidence from spike run:** The daily note contains 7 task lines but TurboVault reports only 2 tasks.

**TurboVault's standalone parser** (`parse_task_line()`) accepts a wider set: `' '`, `x`, `X`, `-`, `>`, `<`, `/`. But this function is NOT used by the document parser — it's a separate API for callers who already know they have a task line.

**Task emoji metadata:** The `task_parser` module (winnow-based) parses all Obsidian Tasks emoji markers (📅, ⏳, 🛫, ⏫, 🔼, 🔽, ✅, 🔁, etc.) correctly. This is excellent code. However, since most of our tasks aren't detected as tasks in the first place, the emoji parsing never runs on them.

**Impact:** This alone disqualifies TurboVault as a drop-in parser for Notesmith. The 7-status task model is fundamental to the notes method.

### 4. Inline Field Extraction ❌ NOT SUPPORTED

TurboVault does **not** parse `[key:: value]` syntax in note bodies. The `VaultFile` struct has no `inline_fields` field. There is no parser module for inline fields.

TurboVault's `task_parser` module does parse dataview fields within task content (e.g., `- [ ] Task [due:: 2025-01-15]`), but this is limited to task lines and doesn't help with note-body inline fields like `[owner:: me]`, `[sentiment:: positive]`, `[effort:: large]`.

**Impact:** Notesmith's data model includes `inline_fields: Vec<InlineField>` on every note. This is essential for sidebar views, SQL queries, and dashboard rendering. Wrapping TurboVault would require a completely separate inline field parser running on every note anyway.

### 5. Callout Blocks ⚠️ PARTIAL

Callout type and title parsing works. Multi-line content requires a non-default option:

| Feature | Default (`ParseOptions::all()`) | With `full_callouts` |
|---------|--------------------------------|---------------------|
| Type detection | ✅ | ✅ |
| Title extraction | ✅ | ✅ |
| Multi-line content | ❌ (empty string) | ✅ |
| Foldability marker | ✅ | ✅ |

The `VaultManager::parse_file()` path uses `ParseOptions::all()` which has `full_callouts: false`. This means the standard parse path returns empty callout content. Multi-line content requires `ParseOptions::all().with_full_callouts()`.

**Mitigation:** This is fixable by wrapping TurboVault and passing the right options. However, it requires controlling the parse pipeline, which conflicts with using VaultManager as a black box.

### 6. Embed Syntax ✅ PASS

Both image and note embeds are correctly detected:

- `![[meeting-screenshot.png]]` — detected as Embed
- `![[Migration to v2#Phase 1]]` — detected as Embed with section ref

### 7. Block References ✅ PASS

Block ID definitions (`^block-id` at end of paragraphs) are detected in the `blocks` vector. Block references in wikilinks (`[[Note#^block-id]]`) are detected as links.

### 8. Atomic Write Correctness ✅ PASS

TurboVault's `VaultManager::write_file()` uses the atomic rename pattern:
1. Write content to `{file}-{uuid}.tmp`
2. Atomic `rename()` syscall
3. Cache invalidation

Write → read roundtrip preserves content exactly, including frontmatter, wikilinks, and task syntax.

**Note:** `fsync` is not called — durability depends on OS page cache. This is an acceptable trade-off documented by TurboVault.

Optimistic concurrency control via `expected_hash` parameter is available (SHA-256 based).

---

## Architecture Comparison

| Aspect | TurboVault | Notesmith Plan | Conflict? |
|--------|-----------|---------------|-----------|
| Content hash | SHA-256 (`sha2`) | blake3 | Yes — different hash values |
| Frontmatter model | `HashMap<String, serde_json::Value>` | Typed enum (`Frontmatter::Daily(DailyMeta)`, etc.) | Yes — impedance mismatch |
| SQL engine | GlueSQL (no CTEs, no window functions) | rusqlite with SQL views | Yes — different query capabilities |
| Task model | `is_completed: bool` | 7-status enum | Yes — fundamental mismatch |
| Inline fields | Not supported | First-class parsed type | Yes — missing feature |
| Graph analysis | petgraph-based, immutable after build | SQLite-backed backlinks view | Different approach |
| File watching | notify (built-in) | notify (planned) | Compatible |
| MCP | Built-in 47-tool MCP server | Separate MCP adapter crate | Overlap — we'd ignore TurboVault's MCP |
| Async runtime | tokio | tokio | Compatible |

---

## What to Salvage

Even though we're swapping to a native parser, TurboVault provides excellent reference implementations:

1. **Wikilink regex:** `r"\[\[([^\]]+)\]\]"` with pipe splitting for aliases — battle-tested
2. **Task emoji metadata parser:** The winnow-based `task_parser` module is portable and well-tested (handles 📅, ⏳, 🛫, ⏫, 🔼, 🔽, ✅, 🔁, 🏁, 🆔, ⛔ and dataview fields within tasks)
3. **Atomic write pattern:** temp file with UUID naming + rename
4. **Callout regex:** `r"^>\s*\[!(\w+)\]([+-]?)\s*(.*)"` with continuation line matching
5. **Excluded range tracking:** Code block/inline code ranges excluded from OFM regex parsing
6. **Link type classification:** Heading ref vs block ref vs anchor detection logic

---

## Decision

### SWAP to native parser implementation

**Rationale:**

1. **The gaps are foundational, not marginal.** Extended task statuses and inline fields are core to the notes method — every note, every query, every dashboard depends on them. Wrapping TurboVault and supplementing with custom parsers would mean running two parse pipelines on every note, defeating the purpose.

2. **The architectural choices diverge.** SHA-256 vs blake3, untyped vs typed frontmatter, GlueSQL vs rusqlite — these aren't style preferences, they're engineering decisions that would create constant friction at the trait boundary.

3. **The reimplementation cost is modest.** TurboVault's OFM parsing is regex-based (not a complex grammar). The patterns are well-documented and can be reimplemented behind our `VaultEngine` trait. The task emoji parser is the most complex piece and can be adapted directly.

4. **The VaultEngine trait boundary is maintained.** The swap changes the implementation, not the interface. If TurboVault fixes these gaps in the future, switching back is possible.

### Action Items

- [ ] Proceed with Issue #2: scaffold workspace with native parser crate
- [ ] Use comrak for HTML rendering (already planned)
- [ ] Implement OFM parser using regex (wikilinks, embeds, tags, callouts, inline fields) + custom task parser supporting all 7 statuses
- [ ] Reference TurboVault's regex patterns and task_parser module during implementation
- [ ] Close Issue #1 with this decision recorded
