# ADR-0003: Origin-Based CORS for Write Protection

**Status**: Accepted  
**Date**: 2026-05 (issue #43)

## Context

The Notesmith daemon listens on `127.0.0.1` but is reachable by any local webpage via `fetch()`. Write endpoints (config PUT, note creation) must be protected against cross-origin abuse from malicious local pages.

We considered: auth tokens, session cookies, and Origin header checking.

## Decision

Use **Origin-based CORS enforcement** for write operations:

- Read endpoints (GET): permissive (existing behavior)
- Write endpoints (PUT/POST): require `Origin` header matching allowed origins
- Allowed origins: `tauri://localhost`, `http://localhost`, `http://127.0.0.1`
- No Origin header = allowed (same-origin requests, curl, CLI tools)
- Return `403 Forbidden` for disallowed origins

Implemented as `WriteGuard`, an Axum `FromRequestParts` extractor.

## Consequences

- Lightweight: no tokens, no auth state, no login flow
- Sufficient for single-user local daemon
- Hosted multi-user deployments can layer real auth on top
- Foreign web pages cannot silently write to the local daemon
- CLI and curl access works without configuration
