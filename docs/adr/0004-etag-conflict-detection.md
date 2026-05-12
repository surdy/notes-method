# ADR-0004: ETag / BLAKE3 for Config Conflict Detection

**Status**: Accepted  
**Date**: 2026-05 (issue #44)

## Context

Config files can be edited by hand (in an editor) while the settings panel is open. A naïve PUT would silently overwrite external changes.

## Decision

Use **hash-based optimistic concurrency** with ETags:

- `GET /api/v/{vault}/config` returns an `ETag` header containing the BLAKE3 hash of the raw file content
- `PUT /api/v/{vault}/config` requires `If-Match` header with the ETag from the last GET
- Hash mismatch returns `409 Conflict` with the current config and new hash
- Missing `If-Match` returns `428 Precondition Required`
- Frontend shows conflict UI: "Config was changed externally. Reload or overwrite?"

BLAKE3 chosen for speed and simplicity (no cryptographic requirements here).

## Consequences

- No lock files or advisory locking needed
- Works across concurrent editor + settings panel usage
- TOML round-trip erases comments (accepted trade-off — UI-managed config is normalized via `toml::to_string_pretty`)
- Frontend must track ETags and handle 409 responses
