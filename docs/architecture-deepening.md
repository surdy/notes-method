# Architecture Deepening Opportunities

_Generated 2026-05-12. Updated 2026-05-13 after settings redesign (#46–#50) and inbox→capture generalization (#51–#55)._

---

## 1. Split `routes.rs` into domain-scoped route modules

- **Files**: `crates/notesmith-http/src/routes.rs` (2346 lines, ~40 handlers)
- **Problem**: This is a god file. Note CRUD, task mutation, template/daily creation, capture, routing, config I/O, sidebar config, vault management — all inline. ~60% is domain logic, ~40% HTTP plumbing. A change to task toggling requires navigating through note creation, capture workflows, and config handlers. No **locality**.
- **Solution**: Extract into `routes/notes.rs`, `routes/tasks.rs`, `routes/templates.rs`, `routes/config.rs`, `routes/capture.rs`, `routes/routing.rs`, `routes/vaults.rs`. Each sub-module owns its handlers + the types they need. `routes/mod.rs` re-exports the router builder.
- **Benefits**: **Locality** — task bugs stay in `routes/tasks.rs`. **Leverage** — each module has a smaller interface (just its handler fns). Tests can target a single domain area. AI navigability improves dramatically (grep for "task" finds one file, not a 2000-line haystack).

---

## 2. Extract `+page.svelte` orchestration into an AppController

- **Files**: `ui/app/src/routes/+page.svelte` (304 lines), `stores.svelte.ts`, `settings.svelte.ts`, `sse.ts`
- **Problem**: `+page.svelte` is simultaneously: vault bootstrapper, SSE event dispatcher, hotkey registrar, command palette coordinator, middle-pane manager, and child-ref juggler. Its `onMount` does 6 distinct jobs. Tight coupling to every store and child component ref.
- **Solution**: Extract an `app-shell.svelte.ts` composable that owns SSE event classification, vault bootstrap, and panel coordination. `+page.svelte` becomes pure layout + render. Hotkey registration stays in the page but delegates to the shell.
- **Benefits**: **Locality** — SSE dispatch logic testable without component rendering. **Leverage** — shell interface is small (init/teardown + event handlers). The page becomes < 100 lines of markup.

---

## 3. Consolidate `ParsedNote` → `Note` boundary in notesmith-vault

- **Files**: `crates/notesmith-vault/src/parser.rs` (790 lines, `ParsedNote`), `crates/notesmith-core/src/note.rs` (20 lines, `Note`)
- **Problem**: `ParsedNote` mirrors `Note` closely — same fields, same shape. The parser creates `ParsedNote`, then `NativeVaultEngine` maps it to `Note`. This is a shallow adapter: the **deletion test** says removing `ParsedNote` would concentrate complexity (just parse directly into `Note`). Frontmatter extraction is also duplicated between parser and save pipeline.
- **Solution**: Have the parser produce `Note` directly. `ParsedNote` becomes an internal builder/accumulator if needed, but never crosses the module boundary. Deduplicate frontmatter extraction into a shared helper.
- **Benefits**: **Depth** — `NativeVaultEngine` becomes deeper (does more behind `VaultEngine` trait) instead of being a thin mapper. One fewer type at the **seam**. Tests simplify.

---

## 4. Split `api.ts` by domain

- **Files**: `ui/app/src/lib/api.ts` (579 lines, ~29 functions + ~20 types)
- **Problem**: Every API call lives in one file. The settings redesign and capture rename added ~150 lines of types and functions. No grouping — note CRUD sits next to SQL execution sits next to config management sits next to vault CRUD.
- **Solution**: Split into `api/notes.ts`, `api/config.ts`, `api/vaults.ts`, `api/templates.ts`, `api/core.ts` (shared types, `ApiError`, `encodePath`). Re-export from `api/index.ts` for backward compatibility.
- **Benefits**: **Locality** — config API changes don't touch note API. Smaller files are faster to scan. Import paths signal domain intent.

---

## 5. Deepen `VaultStore` by extracting tab and tree concerns

- **Files**: `ui/app/src/lib/stores.svelte.ts` (190 lines), `tab-state.ts`
- **Problem**: `VaultStore` is vault state + tab manager + tree builder + localStorage persistence. The `buildTree` function is pure logic embedded in the store file. Tab operations delegate to `tab-state.ts` but the store still owns all the `_applyTabState` glue. **Interface** is large: 12 public methods + 7 state fields.
- **Solution**: Extract `buildTree` into its own utility (already pure). Move tab glue into a `TabStore` that wraps `tab-state.ts`. `VaultStore` keeps only vault/notes/selection concerns.
- **Benefits**: **Depth** — each store has a focused interface. `TabStore` is independently testable (currently `tab-state.ts` tests exist but `VaultStore` has zero tests). Tree building becomes a tested utility.

---

## 6. `notesmith-query` is a pass-through — absorb or deepen

- **Files**: `crates/notesmith-query/src/lib.rs` (5 lines — just `pub use executor::*`)
- **Problem**: The crate's entire public interface is a re-export of its single internal module. The **deletion test** says: if you deleted this crate and inlined `executor.rs` into the consumer, complexity doesn't change. It's a shallow module that exists for organizational reasons but adds a crate boundary (compile unit, Cargo.toml, version) without earning it.
- **Solution**: Either (a) absorb into `notesmith-index` (its main consumer) or (b) deepen it with query planning, caching, or query validation that currently lives in HTTP handlers.
- **Benefits**: Option (a) reduces workspace noise. Option (b) creates a real **seam** — the query module owns SQL execution end-to-end, and the HTTP layer just passes strings to it.

---

## 7. Extract settings page sections into standalone components

- **Files**: `ui/app/src/routes/settings/+page.svelte` (790 lines), `SidebarSettings.svelte` (1064 lines), `VaultsSettings.svelte` (534 lines)
- **Problem**: The settings page (`+page.svelte`) contains all section templates inline — General, Daily, Editor, Git, Hooks each as `{#if selectedSection === ...}` blocks. `SidebarSettings.svelte` is the largest component at 1064 lines. Settings-related components total ~2388 lines across 3 files.
- **Solution**: Extract each section into its own component: `GeneralSettings.svelte`, `DailySettings.svelte`, `EditorSettings.svelte`, `GitSettings.svelte`, `HooksSettings.svelte`. The page becomes a thin router between tabs and components. Consider splitting `SidebarSettings` into sub-components for each section type (views list, section editor, item editor).
- **Benefits**: **Locality** — editing Git settings doesn't require scrolling past Daily, Editor, and General sections. Each section component can own its own validation and save logic. Easier to add new settings sections.

---

## Codebase Stats (as of 2026-05-13)

### Rust workspace (14 crates)
- Dependency graph: `core ← {vault,tasks,html,config,hooks} ← {index,query,templates,routing,git} ← {http,mcp,cli}`
- No circular dependencies
- `notesmith-http` has the largest dependency fan-in (11 workspace crates)
- `notesmith-core` is the type hub (~28 public items)
- `routes.rs` is the largest file at 2346 lines (~40 handlers)

### Frontend (SvelteKit)
- 18+ components, largest: `SidebarSettings.svelte` (1064), `settings/+page.svelte` (790), `VaultsSettings.svelte` (534), `NoteEditor.svelte` (481), `RightRail.svelte` (425)
- 8 test files covering: api, tab-state, task-markers, fuzzy, note-loading, sql-blocks, right-rail, hotkeys
- Not tested: VaultStore, SettingsStore, SSE orchestration, component rendering
- `api.ts` has grown to 579 lines with ~29 exported functions
