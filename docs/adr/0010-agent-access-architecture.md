# ADR 0010 — Agent Access Architecture (Daemon-Hosted MCP, Ops Layer)

## Status

Accepted (2026-06-11). Implementation phased; phases 1–2 complete (Ops layer
+ read-only; daemon-hosted MCP over HTTP/SSE). Phases 3–5 (stdio bridge, CLI
remote profile, auth) outstanding.

## Context

Notesmith exposes three parallel ways to operate on a vault, and they have
drifted apart:

1. **HTTP daemon** (`notesmith-http`) — REST + SSE, network-capable, the
   backing store for the desktop app and the hosted UI. No authentication.
2. **MCP server** (`notesmith-mcp`) — the agent surface. It is **stdio-only**
   and embeds its own `NativeVaultEngine` plus an in-memory `VaultCache` /
   `SearchIndex` built from a single local `vault_root`. It re-implements
   operations the daemon already has, so the two surfaces diverge over time.
3. **CLI** (`notesmith-cli`) — talks plain HTTP to a local daemon bind for
   daemon-backed commands; `vault` / `copy-html` / `mcp` are local-filesystem.

The goal is to let **AI agents operate on a vault whether Notesmith runs
locally or on a server**, without maintaining three implementations of the
same operations and without forcing agents to know where the vault physically
lives.

The deployment reality for the current user is a **personal** homelab: the
server instance runs on a trusted LAN and is reached externally over VPN.
There is no requirement (yet) to expose the daemon to the open internet.

## Decision

**The HTTP daemon becomes the single source of truth. Every agent adapter
becomes a thin, transport-agnostic client of a daemon — local or remote.**

The following decisions were locked on 2026-06-11:

1. **Primary agent transport: daemon-hosted MCP over HTTP/SSE.** The agent
   speaks MCP directly to the daemon and it works identically against a local
   or a remote daemon.
2. **Ship a stdio↔HTTP bridge** alongside, because many MCP clients (e.g.
   Claude Desktop) are stdio-only. `notesmith mcp` becomes that bridge
   (`--url <daemon>`).
3. **The embedded-engine MCP is replaced by the bridge.** There is one code
   path; the MCP server no longer embeds an engine or builds its own indexes.
   Local use auto-starts the daemon as today.
4. **Read-only is exposed as a separate endpoint, not a per-caller scope.**
   The daemon serves both `/mcp/<vault>` (full) and `/mcp-ro/<vault>`
   (read-only). The read-only endpoint is backed by a `ReadOnlyOps` wrapper
   that has no write verbs, so **it works without authentication**. Point a
   "safe" agent at the RO path.
5. **The CLI gains a remote profile** — `--url` / `NOTESMITH_URL` — so a
   laptop CLI can drive the server daemon over VPN.
6. **Transport security is handled by a reverse proxy.** The daemon stays
   plain HTTP; TLS is terminated by Caddy/nginx in front of it. (SSE requires
   the proxy to not buffer responses — e.g. nginx `proxy_buffering off`.)
7. **Vaults are addressed by per-vault path** — `/mcp/<vault>` and
   `/mcp-ro/<vault>` — mirroring the hosted `/app/<vault>` URL scheme. One
   connection operates on one vault.
8. **MCP tools reuse the daemon's live indexes** (kept fresh by the file
   watcher) instead of building a per-connection snapshot. No rebuild, no
   drift, always current.
9. **Authentication and per-identity scopes are deferred.** The current trust
   boundary is the LAN + VPN perimeter. The daemon must not be exposed to the
   open internet while unauthenticated.

### The Ops layer

A single canonical operations abstraction is introduced:

- `Ops` — trait defining every vault operation (read and write).
- `LocalOps` — in-process implementation used by the daemon's own handlers and
  by the daemon-hosted MCP endpoint; wraps the existing engines/indexes.
- `RemoteOps` — an HTTP client implementation used by the CLI remote profile
  and the stdio bridge; talks to a daemon over HTTP.
- `ReadOnlyOps` — a decorator over any `Ops` that returns an error for write
  operations; backs the `/mcp-ro/<vault>` endpoint.

The daemon's REST surface, the MCP endpoints, the CLI, and the bridge all sit
on top of `Ops`, so operation logic exists exactly once.

## Consequences

### Capability without auth

Read-only operation is a property of **which surface** an agent connects to,
not of the caller's identity. This is a deliberate trade-off:

- ✅ Safe read-only agents today, with zero auth, via `/mcp-ro/<vault>`.
- ⚠️ The RO endpoint guards against **agent mistakes**, not a malicious actor:
  anyone who can reach the daemon can also reach the full `/mcp/<vault>`. This
  is acceptable only under the LAN/VPN trust model and must be revisited when
  the daemon is exposed more broadly.
- ✅ Forward-compatible: when auth lands, "read-only" can additionally become
  an identity attribute, but the read-only machinery already exists.

### What deferring auth costs

- No per-agent/per-user capability distinctions on the same endpoint.
- No write attribution in audit logs (we can log *that* a write happened, not
  *who* did it).
- Security is entirely the network perimeter — do not expose the daemon
  publicly until auth exists.

### Resilience policy still applies

All `.md` content remains untrusted input. The Ops layer and the MCP endpoints
must honour [ADR 0009](0009-resilience-to-malformed-content.md): per-note
isolation, no `?`-propagation of content parse errors above the per-note
boundary, structured 4xx (not 500) on malformed request bodies.

## Suggested phasing

1. **Ops layer + read-only.** Define `Ops`, refactor the daemon onto
   `LocalOps`, add `ReadOnlyOps`. Pure consolidation; no external behavior
   change beyond the new capability being available internally. *(Done.)*
2. **Host MCP in the daemon.** Add `/mcp/<vault>` and `/mcp-ro/<vault>`
   HTTP/SSE endpoints (rmcp streamable transport) backed by `LocalOps` /
   `ReadOnlyOps` and the daemon's live per-vault indexes. Endpoints are
   mounted for vaults known at daemon start; new vaults need a restart.
   *(Done.)*
3. **Bridge replaces embedded MCP.** `notesmith mcp` becomes the stdio↔HTTP
   bridge; the embedded-engine path is removed.
4. **CLI remote profile.** `RemoteOps` + `--url` / `NOTESMITH_URL`; route
   daemon-backed commands through `Ops`.
5. **Auth (later).** Bearer tokens + per-identity scopes, integrated with the
   existing origin-based `WriteGuard`; loopback exemption for local use.
   Native HTTPS becomes optional at this point.

## Alternatives considered

- **CLI-first (agents shell out to `notesmith`).** Rejected as the primary
  transport: it bakes a process-spawn per call and makes streaming/SSE awkward;
  MCP-over-HTTP is the cleaner native agent surface. The CLI remote profile is
  still provided for scripting.
- **Keep the embedded-engine MCP as an offline mode.** Rejected: it is the
  source of the index/logic drift this ADR exists to remove. The bridge plus
  auto-start covers the local case.
- **Vault as a per-call tool argument / runtime `select_vault`.** Rejected in
  favour of per-vault paths, which mirror the hosted `/app/<vault>` scheme and
  keep one connection scoped to one vault.
- **Per-connection read-only opt-in (`?mode=ro` / header).** Rejected: without
  auth the daemon cannot trust the client to ask for the restricted mode; a
  separate endpoint is the only honest way to express it.
- **Daemon-native TLS.** Deferred: a reverse proxy already terminates TLS in
  the homelab deployment and keeps TLS code out of the daemon.
- **Build auth first.** Deferred: not required under the LAN/VPN trust model,
  and read-only operation — the immediate concern — does not depend on it.

## References

- Architecture proposal artifact (diagrams, change tables): produced in the
  design session; this ADR is the authoritative summary.
- Related ADRs: [0003 — Origin-based CORS](0003-origin-based-cors.md)
  (`WriteGuard`), [0005 — Capabilities endpoint](0005-capabilities-endpoint.md),
  [0006 — Crate per domain](0006-crate-per-domain.md),
  [0009 — Resilience to malformed content](0009-resilience-to-malformed-content.md).
