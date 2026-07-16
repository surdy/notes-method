# Notesmith CLI Reference

Notesmith ships a single user-facing binary: **`notesmith`**. The workspace also includes a developer-only helper binary, **`theme-gen`**, for precomputing theme CSS from the catalog JSON.

```
notesmith [--vault <name|path>] [--url <daemon-url>] [--format text|json] <command>
```

**Global flags:**

| Flag | Description | Default |
|------|-------------|---------|
| `--vault <name\|path>` | Override vault detection (name from config or path) | auto-detect |
| `--url <daemon-url>` | Target a daemon base URL (e.g. `https://host:8443`) instead of the local daemon. Also settable via the `NOTESMITH_URL` env var (the flag wins). A remote daemon is used as-is and never auto-started; terminate TLS with a reverse proxy. Reverse-proxy subpaths are supported (`https://host/notesmith`). | local daemon |
| `--format text\|json` | Output format | `text` (JSON when piped) |

`--url` / `NOTESMITH_URL` applies to all daemon-backed commands (query, note, search, reindex, task, capture, template, route, daily, periodic, url-open, ai, and `mcp start`). The `daemon` lifecycle subcommands always manage the **local** daemon and ignore the override.

---

## theme-gen (developer tool)

Generate ramp-based theme CSS from `ui/app/src/styles/theme-catalog.json` into `ui/app/src/styles/themes/`.

```bash
cargo run --bin theme-gen -- --catalog ui/app/src/styles/theme-catalog.json --output ui/app/src/styles/themes/
```

| Flag | Description |
|------|-------------|
| `--catalog <path>` | Theme catalog JSON file to read |
| `--output <dir>` | Directory where generated `*.css` files are written |

On success the binary prints `Generated N theme files`.

---

## daemon

### `daemon start`

Start the Notesmith HTTP daemon. Loads all configured vaults, builds caches, and starts the Axum server.

```bash
notesmith daemon start [--bind 127.0.0.1:27183]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--bind <addr>` | Bind address for the HTTP server | `127.0.0.1:27183` |

The daemon indexes all registered vaults on startup and watches for file changes.
It also writes daily-rotated daemon logs to the platform log directory (`~/Library/Logs/Notesmith/` on macOS, `$XDG_STATE_HOME/notesmith/` or `~/.local/state/notesmith/` on Linux) and retains the last 7 days.
On startup it also runs SQLite/Tantivy integrity checks and automatically rebuilds corrupted cache artifacts from markdown files.

Daemon-backed CLI commands auto-start the daemon when `[daemon].auto_start = true` (the default). Set `auto_start = false` to require manual `notesmith daemon start`.

### `daemon stop`

Gracefully stop the running daemon.

```bash
notesmith daemon stop
```

Sends a shutdown request to the daemon via `POST /admin/shutdown`. If the daemon is not running, prints a message and exits cleanly.

### `daemon restart`

Stop the running daemon and start a new one.

```bash
notesmith daemon restart [--bind 127.0.0.1:27183]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--bind <addr>` | Bind address for the restarted daemon | `127.0.0.1:27183` |

Useful after code updates or config changes that require a fresh daemon.
Any unapplied fact-memory `preview_token`s become invalid after restart and
must be regenerated from a new preview.

### `daemon status`

Show daemon status as JSON.

```bash
notesmith daemon status
```

If the daemon is running, prints the full status response (vaults, uptime, version). If not, prints "Daemon is not running."

---

## Desktop app

The Notesmith desktop shell wraps the daemon-served web UI in a native Tauri window. It auto-starts the daemon if needed, discovers the live daemon via the Notesmith lockfile, and exposes tray/menu actions for opening the app, quick capture, and quitting.

```bash
# Run the desktop shell during development
cd crates/notesmith-tauri && cargo tauri dev

# Compile the Rust side without the Tauri CLI
cd crates/notesmith-tauri && cargo check

# Build a distributable app bundle
cd crates/notesmith-tauri && cargo tauri build
```

> Building with `cargo tauri ...` requires the Tauri CLI (`cargo install tauri-cli`) and platform tooling such as Xcode command line tools on macOS.

### Desktop launch flags

The desktop binary accepts a small set of flags before any Tauri-specific arguments:

| Flag | Description |
|------|-------------|
| `--no-restore` | Skip the per-vault window restore on launch. Opens a single window on the default vault instead of replaying `windows.json`. The file is **not deleted** — only ignored for this launch. Useful when a saved window position has become problematic (e.g. a stale monitor layout). |

`windows.json` lives in the platform app-config directory (`~/Library/Application Support/com.notesmith.desktop/windows.json` on macOS) and stores `{ vault, x, y, w, h }` for every currently-open vault window. It is rewritten atomically on window create, move, resize, real close, and app quit.

---

## mcp

### `mcp start`

Bridge a stdio-only MCP client (such as Claude Desktop) to a daemon's HTTP MCP
endpoint. The command resolves the target vault, resolves a daemon base URL
(the global `--url` / `NOTESMITH_URL` when set, otherwise the local daemon,
auto-started on demand), then runs a transparent stdio↔HTTP proxy. There is no
embedded vault engine; every request is forwarded to the daemon, which owns the
live indexes.

```bash
notesmith [--url <daemon-url>] mcp start [--vault <name>] [--read-only]
```

| Flag | Description |
|------|-------------|
| `--vault <name>` | Target vault. Taken as-is so it can name a vault hosted only on a remote daemon; otherwise detected from the working directory. |
| `--read-only` | Bridge to the daemon's read-only endpoint, where write tools are rejected. |

The bridge connects to `<daemon>/mcp/<vault>` (or `<daemon>/mcp-ro/<vault>` with
`--read-only`), where `<daemon>` is the global `--url` / `NOTESMITH_URL` target
or the local daemon. See [`docs/mcp.md`](mcp.md) for the tool/resource surface.

---

## vault

### `vault list`

List all registered vaults from `~/.config/notesmith/config.toml`.

```bash
notesmith vault list
```

### `vault detect`

Show which vault would be selected for the current directory.

```bash
notesmith vault detect
```

Detection order:
1. Walk upward from `$PWD` looking for `.notesmith/vault.toml`
2. Honor `--vault <name>` flag
3. Fall back to default vault from global config

### `vault info`

Show vault configuration summary (name, root, capture/daily/editor/git settings, including editor line-number visibility).

```bash
notesmith vault info
```

### `vault reindex`

Rebuild the SQLite cache and Tantivy search index from scratch.

```bash
notesmith vault reindex
```

Output: `Reindexed 42 notes for work into ~/.cache/notesmith/work/cache.sqlite`

---

## reindex

### `reindex`

Ask the daemon to rebuild one vault or all registered vaults.

```bash
notesmith reindex [--vault work] [--cache-only | --search-only]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--vault <name>` | Reindex one registered vault | all registered vaults |
| `--cache-only` | Rebuild only the SQLite cache | rebuild both |
| `--search-only` | Rebuild only the Tantivy search index | rebuild both |

The CLI auto-starts the daemon when needed unless `[daemon].auto_start = false`.

---

## query

### `query sql`

Execute read-only SQL against the daemon's SQLite cache. The CLI auto-starts the daemon when needed unless `[daemon].auto_start = false`.

```bash
notesmith query sql "SELECT title, updated_at FROM v_notes LIMIT 10"
```

Text output renders a formatted table. JSON output returns the full `QueryResult` object.

**Examples:**

```bash
# List active customers
notesmith query sql "SELECT n.title, state.value AS state FROM v_notes n JOIN v_fields note_type ON note_type.vault_name = n.vault_name AND note_type.note_path = n.path AND note_type.key = 'type' LEFT JOIN v_fields state ON state.vault_name = n.vault_name AND state.note_path = n.path AND state.key = 'state' WHERE note_type.value = 'customer' AND state.value = 'Active'"

# Find blocked tasks
notesmith query sql "SELECT text, note_path FROM v_tasks WHERE status = 'blocked'" --format json | jq '.'

# Count notes by type
notesmith query sql "SELECT type, COUNT(*) as count FROM v_notes GROUP BY type ORDER BY count DESC"
```

---

## embed

Run the embedding worker or benchmark the vector-search latency curve. The
worker is the sole writer of each vault's `embeddings.db` (ADR 0018 §2); the
daemon also runs it on an interval, but it is fully runnable by hand.

### `embed`

Run one incremental embedding pass over one or all vaults: changed notes are
re-embedded, unchanged notes skipped, deleted notes pruned.

```bash
notesmith embed                 # all registered vaults
notesmith --vault work embed    # a single vault
notesmith embed --format json
```

> Real semantic vectors require building with `--features local-embed` (a local
> `fastembed` model). Without it, a non-semantic `HashEmbedder` placeholder is
> used so the pipeline runs offline.

### `embed bench`

Measure this host's brute-force k-NN latency curve so the decision to switch the
vector store (SQLite brute-force → sqlite-vec / LanceDB) stays **data-triggered**
(ADR 0018 §5, feeds the monitoring thresholds in issue 244). Inserts synthetic
vectors at increasing scales, times k-NN, and reports the vector count at which
p95 latency first crosses 150 ms (warn) and 300 ms (switch).

```bash
notesmith embed bench
notesmith embed bench --dim 384 --scales 50000,100000,250000,500000,1000000 --queries 50
notesmith embed bench --baseline --format json
```

| Flag | Description | Default |
|------|-------------|---------|
| `--dim <N>` | Synthetic vector dimension | 384 |
| `--scales <list>` | Comma-separated vector counts to measure | `50000,…,1000000` |
| `--k <N>` | Neighbours retrieved per query | 10 |
| `--queries <N>` | Queries sampled per scale | 50 |
| `--baseline` | Also embed + search the target vault as a real-content baseline | off |

> Run with `cargo run --release` (or an installed release binary) for
> representative numbers — debug builds are several times slower.

---

## ingest

Run the drop-folder ingestion worker over one or all vaults (ADR 0022, #263).
Each vault's raw drop folder (`[ingest] raw_dir`, default `raw/`) is scanned for
documents an external tool dropped in; each is extracted into a
provenance-tracked sidecar note under `[ingest] notes_dir` (default
`ingested/`). Raw files are **never moved, renamed, or deleted** (keep-in-place
invariant). Like `embed`, it runs one incremental pass and is fully runnable by
hand.

### `ingest`

```bash
notesmith ingest                 # all registered vaults
notesmith --vault work ingest    # a single vault
notesmith ingest --format json
```

Each pass is incremental and content-hash driven:

- **new** documents are extracted into a fresh sidecar note (`status: ingested`)
- **unchanged** documents (same content hash) are skipped
- **changed** documents are re-extracted (reingested)
- a **renamed** raw file (same content at a new path) moves its note without
  re-extraction
- **failed** extractions (`status: failed`) are recorded and retried next pass
- **unsupported** file types (`status: unsupported`) are recorded once and not
  retried while unchanged
- **orphaned** notes (raw file removed) are reported, never deleted

Configure per vault in `.notesmith/vault.toml`:

```toml
[ingest]
enabled = false      # when true, the daemon auto-runs a pass on an interval
raw_dir = "raw"      # drop folder scanned for documents
notes_dir = "ingested"  # where sidecar notes are written
```

When `[ingest] enabled = true`, the daemon supervises a per-vault scheduler that
shells out to `notesmith ingest --vault <name>` on an interval (default 300s,
override with `NOTESMITH_INGEST_INTERVAL_SECS`) — keeping heavy extraction out of
the interactive daemon process (ADR 0022 §7). The flag is re-read each tick, so
toggling it takes effect within one interval without a daemon restart. The
`notesmith ingest` command remains fully runnable by hand regardless of the flag.

Supported document types match `read_document` (PDF, EPUB; ADR 0019 §8). Ingested
notes are ordinary Markdown, so the embedding worker picks them up automatically
on its next pass.

---

## transcribe

Transcribe a local audio file into a timestamped Markdown note using the local
speech-to-text engine (ADR 0023, #271). The real engine (whisper.cpp via
`whisper-rs`) is compiled in only with `--features local-whisper`; lean builds
fall back to a stub that produces an empty transcript (mirroring how `embed`
falls back without `--features local-embed`).

### `transcribe <AUDIO> [--output FILE] [--tag TAG ...]`

```bash
notesmith transcribe interview.wav                 # print the note to stdout
notesmith transcribe interview.wav --output note.md # write the note to a file
notesmith transcribe interview.wav --tag voice --tag meeting
notesmith transcribe interview.wav --format json
```

- `<AUDIO>` — path to the audio file. WAV is decoded natively (downmixed to
  mono, resampled to 16 kHz). Other containers are out of scope for now (ADR
  0023 §6).
- `--output FILE` — write the rendered note to `FILE` instead of stdout.
- `--tag TAG` — extra tag added after the mandatory `inbox` tag (repeatable).
- `--format json` — emit `{ source, language, segments, compiled_in, output,
  note }` instead of the raw note.

The note carries ADR 0019 §3 media-provenance frontmatter (`title`,
`source_url`, `source_type: audio`, `duration`, detected `language`,
`ingested_at`, `tags`) followed by a `[M:SS] text` timestamped transcript body.

The real engine loads a whisper.cpp GGML model from
`NOTESMITH_WHISPER_MODEL_DIR` (a directory containing a `ggml-*.bin` file); when
unset or empty, the stub is used.

### `transcribe --drain [--vault NAME] [--format json]`

Drain each vault's pending-transcription queue into notes instead of
transcribing a single file (ADR 0023 §4/§5). This is the worker the daemon
spawns on an interval when `[transcribe] enabled = true`, but it is runnable by
hand.

```bash
notesmith transcribe --drain                 # drain every registered vault
notesmith transcribe --drain --vault work    # drain one vault
notesmith transcribe --drain --format json   # machine-readable report
```

- `--vault NAME` — restrict the pass to one vault (default: all registered
  vaults, alphabetically).
- `--format json` — emit `[{ vault, transcribed, failed, skipped, notes }]`
  instead of a text summary.

Each drained item becomes a note under the vault's `[transcribe] notes_dir`
(default `transcribed/`). YouTube items with no captions are acquired by
downloading the audio-only stream and transcribing it — this requires a build
with the `youtube-audio` feature (and `local-whisper` for a real transcript);
lean builds report such items as `failed` with an `Unsupported` reason and retry
them on later passes. A per-item failure is retried under an attempt cap; it
never aborts the pass (resilience policy, ADR 0009).

---

## note

### `note create`

Create a note in the vault root by default.

```bash
notesmith note create "Follow Up" [--folder Customers/Acme] [--content "Body text"]
```

### `note get`

Fetch a note by vault-relative path.

```bash
notesmith note get General/Follow\ Up.md
```

Text output prints just the note body. JSON output prints the full HTTP note payload, including frontmatter, links, tasks, and hash.

### `note put`

Replace a note's content.

```bash
notesmith note put General/Follow\ Up.md --content "# Replaced"
printf '# Replaced from stdin\n' | notesmith note put General/Follow\ Up.md --from-stdin
```

### `note append`

Append content to an existing note.

```bash
notesmith note append General/Follow\ Up.md "Next line"
```

### `note delete`

Delete a note.

```bash
notesmith note delete General/Follow\ Up.md
```

### `note move`

Move a note to a new vault-relative path.

```bash
notesmith note move General/Follow\ Up.md Customers/Acme/Follow\ Up.md
```

All create/put/append writes run through the save pipeline, which trims trailing whitespace, normalizes the trailing newline, and auto-maintains `created`/`updated` frontmatter fields when frontmatter is present.

---

## copy-html

### `copy-html`

Render a vault note to portable HTML with embedded styles, strip frontmatter, and copy the result to the system clipboard.

```bash
notesmith copy-html Customers/Acme/Acme\ Corp.md
```

| Arg | Description |
|-----|-------------|
| `<path>` | Vault-relative note path |

The clipboard entry includes both `text/html` and a plain-text markdown fallback.

---

## capture

Quick-capture commands go through the daemon and auto-start it when needed.

### `capture`

Quick-capture a note to the configured capture folder. Generates a timestamped filename.

```bash
notesmith capture "<text>" [--title <title>]
```

| Arg/Flag | Description |
|----------|-------------|
| `<text>` | Note body content |
| `--title <title>` | Optional title used in filename slug |

**Filename format:** `{capture_folder}/{YYYY-MM-DD HH-MM-SS} - {slug}.md`

If `capture.folder = ""`, the note is created in the vault root. The slug is derived from `--title` if provided, otherwise from the first 40 characters of the text (sanitized to keep alphanumeric, spaces, and hyphens).

**Examples:**

```bash
notesmith capture "Call Sarah about the project"
notesmith capture "Meeting notes from standup" --title "Standup Notes"
notesmith capture "Quick thought" --format json
```

---

## clip

Clip a web page into the vault. Extraction runs **server-side** in the daemon
(fetch → readable-article extraction → Markdown), so the CLI only hands over the
URL. The clip is written to the `[clip].folder` (or the capture folder), tagged
`inbox`, with `source_url`/`source_type: article` provenance frontmatter. See the
[`POST /clip` endpoint](http-api.md#web-clipper) and
[ADR 0020](adr/0020-web-clipper.md).

### `clip`

```bash
notesmith clip <url> [--tag <tag>]...
```

| Arg/Flag | Description |
|----------|-------------|
| `<url>` | URL of the page to clip |
| `--tag <tag>` | Extra tag alongside the mandatory `inbox` tag (repeatable) |

Re-clipping a URL that already exists (matched by canonical `source_url`) prints
`already clipped: <path>` and does not write a duplicate. Images are downloaded
into the vault when `[clip].download_images` is enabled (default).

**Examples:**

```bash
notesmith clip "https://example.com/some-article"
notesmith clip "https://example.com/post" --tag reading --tag ml
notesmith clip "https://example.com/post" --format json
```

---

## task

Task commands go through the daemon and auto-start it when needed.

### `task list`

List tasks from the vault with optional filters.

```bash
notesmith task list [--status <status>] [--field key=value] [--due-before <YYYY-MM-DD>] [--limit N]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--status` | Filter by status (`todo`, `in_progress`, `blocked`, `waiting`, `on_hold`, `done`, `cancelled`) | all |
| `--field key=value` | Filter by an inline field value | none |
| `--due-before` | Only tasks due before this date | none |
| `--limit N` | Maximum results | 200 |

Text output shows `[marker] text  📅 due  (note_path)` for each task.

**Examples:**

```bash
notesmith task list
notesmith task list --status todo --field customer=Acme
notesmith task list --due-before 2025-02-01 --format json | jq '.[].text'
```

### `task add`

Add a new To Do task to an existing note. Inline fields are written as `[key:: value]` on the task line.

```bash
notesmith task add <note_path> <description> [--field key=value ...] [--status-char <char>]
```

| Arg/Flag | Description |
|----------|-------------|
| `note_path` | Vault-relative path to the note |
| `description` | Task text |
| `-f`, `--field key=value` | Inline field (repeatable) |
| `--status-char` | Checkbox character (default: space = todo) |

**Examples:**

```bash
notesmith task add "Projects/migration.md" "Follow up on SLA requirements" -f customer=Acme -f due=2025-02-01
notesmith task add Daily/2025-01-15.md "Review pull requests" -f priority=high
notesmith task add Inbox/tasks.md "Send quarterly report" -f owner=me -f stream="Q2 Review"
```

### `task toggle`

Toggle a task to a new status using its content hash. The hash uniquely identifies the task line and is returned by `note get` or `task list`.

```bash
notesmith task toggle <note_path> <task_hash> <new_status>
```

**Status transitions** (from the notes method):

| From | Allowed next states |
|------|---------------------|
| `todo` | `in_progress`, `blocked`, `waiting`, `on_hold`, `done` |
| `in_progress` | `done`, `blocked`, `waiting`, `on_hold` |
| `blocked` | `todo`, `in_progress`, `done` |
| `waiting` | `todo`, `in_progress`, `done` |
| `on_hold` | `todo`, `in_progress`, `done` |
| `done` | `todo` |
| `cancelled` | `todo` |

Returns `404` if the hash is not found, `409` if it matches more than one task, `422` if the transition is not allowed.

### `task set-status`

Alias for `task toggle` — explicitly set a task's status.

```bash
notesmith task set-status <note_path> <task_hash> <new_status>
```

---

## search

Full-text search across note titles and body content. The CLI auto-starts the daemon when needed unless `[daemon].auto_start = false`.

```bash
notesmith search <terms...> [--limit N]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--limit <N>` | Maximum results | 20 |

**Examples:**

```bash
notesmith search Acme onboarding
notesmith search SSO --limit 5 --format json
```

Text output shows path, title, score, and a context snippet for each result.

---

## template

### `template list`

List available templates with their prompt schemas.

```bash
notesmith template list
```

Text output shows template name and description. JSON output returns the full template metadata including prompts.

### `template render <name> [--prompt KEY=VALUE ...]`

Render a template to stdout without creating a file.

```bash
notesmith template render generic-note --prompt title="Hello World"
```

Text output prints just the rendered content. JSON output returns `{ path, content }`.

### `template instantiate <name> [--prompt KEY=VALUE ...]`

Render and create the note at the computed output path.

```bash
notesmith template instantiate external-meeting --prompt customer=Acme --prompt title="Q2 Check-in"
```

| Flag | Description |
|------|-------------|
| `--prompt KEY=VALUE` | Supply a prompt value (repeatable) |

**Available templates:**

| Name | Description |
|------|-------------|
| `generic-note` | A generic blank note |
| `daily-note` | Daily note for today |
| `external-meeting` | External customer meeting note |
| `internal-meeting` | Internal team meeting about a customer |
| `account-info` | Account information for a customer |
| `customer-index` | Top-level customer index note |
| `glossary` | Glossary of terms for a customer |
| `milestones` | Dates and milestones for a customer |
| `stream` | Customer stream or initiative |

**Examples:**

```bash
notesmith template list
notesmith template render daily-note
notesmith template instantiate stream --prompt customer=Acme --prompt title="Migration to v2"
notesmith template instantiate account-info --prompt customer="Globex Industries" --format json
```

---

## route

### `route preview`

Preview where a note would be routed without moving it.

```bash
notesmith route preview <path>
```

| Arg | Description |
|-----|-------------|
| `path` | Vault-relative path to the note |

Text output shows `source -> destination (rule: rule_id)`.

**Examples:**

```bash
notesmith route preview "Drafts/standup.md"
notesmith route preview "Drafts/idea.md" --format json
```

### `route apply`

Apply routing to move note(s) to their destination folder. Routing applies configured field/tag mutations, stamps `archived: true` and `archived-at`, then moves the note.

```bash
notesmith route apply <path>
```

| Arg | Description |
|-----|-------------|
| `path` | Route a single note by vault-relative path |

**Examples:**

```bash
# Route a single note
notesmith route apply "Drafts/standup.md"

# Route with JSON output
notesmith route apply "Drafts/standup.md" --format json
```

---

## daily

### `daily ensure [--date YYYY-MM-DD]`

Create a daily note for the given date (defaults to today) if it doesn't exist. Uses the configured `daily-note` template.

```bash
notesmith daily ensure
notesmith daily ensure --date 2025-06-15
```

| Flag | Description | Default |
|------|-------------|---------|
| `--date <YYYY-MM-DD>` | Date for the daily note | today |

**Examples:**

```bash
notesmith daily ensure
notesmith daily ensure --date 2025-01-15
notesmith daily ensure --format json
```

### `daily open [--date YYYY-MM-DD]`

Open a daily note for the given date (defaults to today). Creates it if missing, then displays the content.

```bash
notesmith daily open
notesmith daily open --date 2025-06-15
```

| Flag | Description | Default |
|------|-------------|---------|
| `--date <YYYY-MM-DD>` | Date for the daily note | today |

**Examples:**

```bash
notesmith daily open
notesmith daily open --date 2025-01-15 --format json
```

### `daily agent-create [--date YYYY-MM-DD] [--content "..."]`

Agent-oriented daily note workflow. Without `--content`, the daemon assembles and returns the saved prompt template from `.notesmith/prompts/daily-note.md`. With `--content`, the daemon writes that pre-generated content as the day's daily note and rejects conflicts if the note already exists. The CLI auto-starts the daemon when needed.

```bash
notesmith daily agent-create
notesmith daily agent-create --date 2025-06-15
notesmith daily agent-create --date 2025-06-15 --content "---\ntype: daily\ndate: 2025-06-15\n---\n# 2025-06-15"
```

| Flag | Description | Default |
|------|-------------|---------|
| `--date <YYYY-MM-DD>` | Date for the daily note or prompt assembly | today |
| `--content <markdown>` | Write pre-generated content instead of printing a prompt | prompt mode |

---

## periodic

### `periodic open <kind> [--offset N]`

Open the current periodic note for a kind, creating it if missing. `--offset -1` opens the previous period; `--offset 1` opens the next period.

```bash
notesmith periodic open daily
notesmith periodic open weekly
notesmith periodic open monthly --offset -1
```

| Argument / Flag | Description | Default |
|-----------------|-------------|---------|
| `<kind>` | `daily`, `weekly`, `monthly`, `quarterly`, or `yearly` | required |
| `--offset <N>` | Period offset from the current period | `0` |

---

## skill

### `skill print`

Print the detected vault's `.notesmith/skill.md` file so agents can load vault-specific operating instructions.

```bash
notesmith skill print
notesmith --format json skill print
```

---

## url-open

### `url-open <URL>`

Handle a `notesmith://` deep-link URL by translating it into daemon API calls. The CLI auto-starts the daemon when the selected URL route needs it.

```bash
notesmith url-open "notesmith://app/open/main/hello.md"
notesmith url-open "notesmith://app/daily/main"
notesmith url-open "notesmith://app/search/main?q=meeting+notes"
notesmith url-open "notesmith://app/capture/main?text=Remember+to+buy+milk"
notesmith url-open "notesmith://app/new/main?template=meeting&folder=General"
notesmith url-open "notesmith://app/task/main/todo.md?line_hash=abc123&status=done"
notesmith url-open "notesmith://user/standup?date=2026-05-10"
```

**URL scheme:**

| Route | Description |
|-------|-------------|
| `notesmith://app/open/{vault}/{path}` | Open a note |
| `notesmith://app/daily/{vault}` | Create/open today's daily note |
| `notesmith://app/search/{vault}?q={query}` | Full-text search |
| `notesmith://app/new/{vault}?template={name}&folder={path}` | Create note from template |
| `notesmith://app/capture/{vault}?text={text}` | Quick capture to the configured capture folder |
| `notesmith://app/task/{vault}/{path}?line_hash={h}&status={s}` | Toggle a task |
| `notesmith://app/command/{name}?args…` | Trigger a built-in command |
| `notesmith://user/{action}?params…` | Run a user-defined action from `.notesmith/url-actions.yaml` |

---

## ai

Headless, non-interactive commands that drive your external ACP agent (Copilot/Claude/Codex/Gemini/OpenCode) for scripting and cron. Notesmith never runs its own chat LLM: it starts the agent over ACP and the agent reaches vault content through Notesmith's MCP tools via the local `notesmith mcp start` stdio bridge. The daemon is auto-started because the bridge forwards to it.

> For **interactive** AI chat inside the desktop app (with a model picker, edit approvals, slash commands, and more), see the [AI Chat Panel guide](ai-chat.md). The `ai` commands below are the scriptable, no-UI counterpart.

**Shared flags (all `ai` subcommands):**

| Flag | Description | Default |
|------|-------------|---------|
| `--agent <id>` | Built-in agent to drive: `copilot`, `claude`, `codex`, `gemini`, `opencode` | `copilot` |
| `--agent-bin <path>` | Override the agent binary path (otherwise resolved from PATH) | auto |
| `--allow-writes` | Permit the agent to write to the vault (see safety note below) | off |

### Headless permission safety

There is no human present to answer the agent's per-write permission prompts, so a headless run is **read-only by default**: the agent binds the daemon's read-only MCP scope (`/mcp-ro/<vault>`) and a deny-by-default decider refuses every write or permission request. Pass `--allow-writes` only to explicitly opt in — it flips the bridge to the read-write scope and auto-approves every action the agent requests **without review**. Granting writes to an unattended agent is dangerous; use it sparingly and only against trusted prompts.

### `ai summarize <path|today>`

Summarize a single note. `<target>` is either a vault-relative note path or the literal `today` (the agent fetches today's daily note). The summary is printed to stdout; with `--format json` it is wrapped as `{ "summary": "..." }`.

```bash
notesmith ai summarize Projects/roadmap.md
notesmith ai summarize today
notesmith --format json ai summarize today --agent claude
```

### `ai weekly-digest`

Produce a digest of the current week's notes (Monday–Sunday of the week containing today). The agent gathers the period's notes via the MCP search/periodic tools. The digest is printed to stdout; with `--format json` it is wrapped as `{ "digest": "..." }`.

```bash
notesmith ai weekly-digest
notesmith --format json ai weekly-digest > digest.json
```

---

## git

Git integration commands for version history and sync. Requires the vault to be a git repository.

### `git status`

Show working tree status (staged, changed, untracked files).

```bash
notesmith git status
```

### `git pull`

Pull from remote (fast-forward only). Aborts on conflict.

```bash
notesmith git pull
```

### `git push`

Push current branch to remote.

```bash
notesmith git push
```

### `git sync`

Pull then push in one step. Skips push if pull has conflicts.

```bash
notesmith git sync
```

### `git log`

Show recent commits.

```bash
notesmith git log [--count 10]
```

| Flag | Description | Default |
|------|-------------|---------|
| `--count <N>` | Number of commits to show | 10 |

**Examples:**

```bash
notesmith git status
notesmith git log --count 5 --format json
notesmith git sync
```
