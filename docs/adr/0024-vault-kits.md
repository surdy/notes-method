# ADR 0024 — Vault Kits: Shipping the Blessed Configuration

## Status

Accepted (2026-07-27). Realized by the `notesmith-kit` crate, the
`notesmith kit list|show|apply` CLI, `GET /api/app/kits`, and the optional `kit`
field on `POST /api/app/vaults`
(issue [#275](https://github.com/surdy/notes-method/issues/275)).

Depends on the Work Notes schema settled in
[`plans/work-notes-simplification-design.md`](../../plans/work-notes-simplification-design.md)
and realized in the fixture by
[#274](https://github.com/surdy/notes-method/issues/274).

## Context

Notesmith's data model is deliberately generic: unified fields, separate tags,
configurable task statuses, no hardcoded note types. A vault only becomes *useful
for a particular kind of work* through configuration — `fields.toml`,
`routing.yaml`, templates, dashboards, `skill.md`.

The Work Notes schema (meetings, customers, streams, people, tasks) was settled,
documented in [`docs/example-work-notes-kit.md`](../example-work-notes-kit.md),
and proven by making `golden-vault/` use it: routing destinations, template
rendering and YAML-list emission, periodic matching, and every `notesmith sql`
fence are all exercised by the test suite.

It was nonetheless **unreachable**. `notesmith vault` had no init, nothing in the
codebase wrote a default `.notesmith/`, and `POST /api/app/vaults` created the
directory plus an *empty* `.notesmith/` — so a vault made from the desktop app
had no config, no templates, and no routing. Adopting the documented schema meant
hand-authoring roughly sixteen files from a markdown document.

That leaves two failure modes. Transcribing config by hand is error-prone and
tedious enough that the schema goes unused. And a configuration that exists only
as prose in a doc **drifts** from the one the tests actually run — the doc was
already wrong in two places when we checked (a strftime `filename` pattern the
indexer cannot match, and a `field-picker` prompt type that did not exist).

The question this ADR answers is not *what* the blessed configuration should be
(that is the design plan's job) but **how it is delivered, and how it is kept
honest**.

## Decision

### 1. A kit is a set of files embedded in the binary

A **kit** is a vault-relative file set plus a folder skeleton: `.notesmith/`
config (`vault.toml`, `fields.toml`, `routing.yaml`, `skill.md`), templates, and
dashboards. Kit sources live at `kits/<id>/` in the repo and are compiled in with
`include_str!`.

Embedded, not fetched: applying a kit needs no network, works offline and before
any daemon exists, and a kit is versioned with the binary that ships it.

### 2. The shipped kit is byte-identical to the test fixture

This is the load-bearing decision. `kits/work-notes/.notesmith/**` and
`golden-vault/.notesmith/**` must match **byte for byte** — `vault.toml` modulo
the substituted vault name — enforced by
`notesmith-kit/tests/kit_matches_golden_vault.rs`.

The consequence is that `golden-vault`'s entire suite becomes the kit's suite:
routing files a meeting to `Meetings/YYYY/MM/`, the templates render valid
frontmatter lists, `[periodic.*]` filenames match real files, dashboard SQL
executes. **What a user installs is what CI exercises**, and neither copy can be
edited without the other failing.

### 3. Applying is non-destructive and idempotent

An existing file is **reported as skipped and left alone**; `--force` is required
to overwrite. Applying twice is a no-op; applying to a populated vault is safe.
The report distinguishes written / skipped / created-directories so callers can
say what actually happened rather than claiming to have written files that were
already there.

This makes "apply the kit later" a safe operation, which in turn is why the
desktop modal defaults to an **empty vault**: applying afterwards costs one
command, while scaffolding a folder the user only meant to register leaves files
to clean up. Prefer the reversible default.

### 4. A kit configures a vault; it does not populate one

Kits ship configuration, templates, and dashboards — never sample notes. The
folder skeleton (`Inbox/`, `Meetings/`, `Streams/`, …) is created empty.

Dashboards are the deliberate edge case: they are notes by file type, but they
are executable configuration in substance. Kit dashboards therefore carry **no
fixture-specific data** (the fixture's "tasks I owe Acme Corp" section is
fixture-only), which is why they are excluded from §2's byte-equality and
covered instead by a test that every dashboard query executes against a freshly
indexed, empty vault.

### 5. Exactly one substitution: `{{ vault_name }}`

Kit files are copied verbatim apart from the vault name. This is not a template
engine and must not become one: kit files have to stay **diffable against the
fixture** for §2 to be checkable by eye as well as by test.

### 6. Kits are not vault-scoped, and not a migration

`GET /api/app/kits` is deliberately not under `/api/v/{vault}/` — kits ship with
the binary, so they are listable before any vault exists, which is exactly when a
client needs them.

Applying a kit writes configuration where it is absent. It **never rewrites notes
onto the schema**. Migrating an existing vault's content is a separate problem
and is out of scope.

### 7. Two entry points, one endpoint

Scaffolding is available from the CLI (`notesmith kit apply`, no daemon required)
and at vault-creation time via the optional `kit` field on
`POST /api/app/vaults`. The Tauri `open_folder_as_vault` command and remote
registration both already POST to that endpoint, so one field covers local and
remote paths without a second code path.

Applying at creation happens **before** the vault goes live, so the first index
pass already sees its templates and dashboards.

### 8. Per-write resilience

[ADR 0009](0009-resilience-to-malformed-content.md)'s posture applies at the
write boundary: a kit id that does not exist is a structured `422` listing the
valid ids and registers nothing; a target that is not a directory is an error,
not a panic. A daemon that cannot list kits still registers vaults — the desktop
select simply falls back to the empty-vault choice.

## Consequences

- The documented configuration and the tested configuration are **the same
  bytes**, so the kit doc cannot quietly drift from what works.
- Editing the kit means editing both trees or the drift test fails. This friction
  is intentional; it is the mechanism, not a side effect.
- A second kit costs a directory plus a registry entry. The CLI, HTTP surface,
  and modal all enumerate the registry rather than hardcoding `work-notes`.
- Kit assets must be present in any build context. The container build copies
  only `Cargo.*` and `crates/`, so it had to be taught to copy `kits/` as well —
  a constraint any future embedded asset inherits.
- Users can adopt the blessed schema without transcription, and re-run safely as
  the kit evolves (new files land; edited ones are left alone).
- Third-party or user-authored kits are **not** supported: the registry is
  compile-time. Adding user kits would mean reading kit directories at runtime
  and is a separate decision.

## Alternatives considered

- **Documentation only** (the status quo). Rejected: it puts a sixteen-file
  transcription burden on every adopter and lets the doc drift from the tested
  configuration — which had already happened twice before this work.
- **Generate the fixture from the kit at build time** (or the reverse). Rejected:
  the fixture legitimately needs things the kit must not ship — sample notes,
  customer-specific dashboard sections, parser edge-case notes. A test asserting
  equality on the shared subset is simpler than a generator with exceptions, and
  it catches drift in both directions.
- **Copy `golden-vault/` directly at apply time.** Rejected: it is a test
  fixture. Shipping it would install sample customers and meetings into a user's
  vault, and it is not present in a release binary anyway.
- **Fetch kits from a remote registry.** Rejected: scaffolding must work offline
  and before a daemon exists, and a network dependency adds versioning and trust
  problems for what is a handful of text files.
- **Run kit files through the full template engine.** Rejected: §5. Beyond the
  vault name there is nothing to interpolate, and templating would break the
  by-eye diffability that makes the fixture-identity invariant reviewable.
- **A `vault init` command that scaffolds *and* registers.** Rejected:
  registration already exists in two places (global config, `POST /api/app/vaults`)
  and has its own naming/validation rules. Keeping `kit apply` to filesystem
  scaffolding lets it run against any directory, registered or not, new or
  existing.
- **A checkbox rather than a select in the new-vault modal.** Rejected: it bakes
  "there is exactly one kit" into the UI for no saving, where a select makes a
  second kit one more row.

## References

- [ADR 0009 — Resilience to Malformed Content](0009-resilience-to-malformed-content.md)
- [Work Notes Kit](../example-work-notes-kit.md) — the shipped kit's schema and contents
- [`plans/work-notes-simplification-design.md`](../../plans/work-notes-simplification-design.md) — how the schema was settled
- [CLI: `notesmith kit`](../cli.md#kit) · [HTTP: `GET /api/app/kits`](../http-api.md)
- Issues: [#274](https://github.com/surdy/notes-method/issues/274) (fixture on the v2 schema),
  [#275](https://github.com/surdy/notes-method/issues/275) (kit scaffolding),
  [#276](https://github.com/surdy/notes-method/issues/276) (field-picker prompts)
