# Deploying Notesmith on a Server

This directory contains everything needed to run the **Notesmith HTTP daemon**
in a container on a Linux server.  The daemon exposes the full API used by the
CLI, MCP server, and desktop app.

> **Security note:** The daemon has no built-in authentication. These
> instructions assume you expose it **only on your Tailscale private IP**.
> Never bind it to a public interface without adding an authenticating reverse
> proxy in front.

---

## Files

| File | Purpose |
|------|---------|
| `../Containerfile` | Multi-stage image (Rust builder → Debian slim runtime) |
| `docker-compose.yml` | Docker Compose service definition |
| `notesmith.container` | Podman Quadlet unit (systemd-native container management) |
| `config.toml.example` | Template for the Notesmith global config |
| `healthcheck.sh` | Liveness probe script (used by both Docker and Quadlet) |

---

## Image flavors

Four flavors are published from the same `Containerfile`, across two independent
axes — **frontend** (does the daemon serve the browser UI?) and **embeddings**
(is the local semantic-search runtime compiled in?):

| Flavor | Tag prefix | Frontend | Embeddings | Use case |
|--------|------------|----------|------------|----------|
| **app** (default) | `latest`, `sha-*`, `YYYY.MM.DD` | ✅ SvelteKit | ❌ lean | Browser on the same network, or desktop app |
| **api** | `api-latest`, `api-sha-*`, `api-YYYY.MM.DD` | ❌ binary only (smaller) | ❌ lean | CLI / MCP / API-only access, or desktop app with embedded frontend |
| **app-embed** | `latest-embed`, `sha-*-embed`, `YYYY.MM.DD-embed` | ✅ SvelteKit | ✅ `local-embed` | Browser access **and** semantic / hybrid search |
| **api-embed** | `api-latest-embed`, `api-sha-*-embed`, `api-YYYY.MM.DD-embed` | ❌ binary only | ✅ `local-embed` | API-only / desktop-served, **and** semantic / hybrid search |

**Which one do I want?**

1. **Do you need browser access to `/app/`?** Yes → an `app*` flavor. No (CLI/MCP/API
   only, or you drive it from the Tauri desktop app which ships its own frontend) →
   an `api*` flavor (smaller).
2. **Do you want semantic / hybrid search on this server?** No → the lean flavor
   (`app` / `api`). Yes → the matching `*-embed` flavor.

> **Embeddings are opt-in, twice.** The `*-embed` images only *compile in* the
> embedding runtime (ONNX + `bge-small-en-v1.5`, a larger image with a first-run
> model download). Embeddings are then still **off by default per vault** — flip
> `[embed] enabled = true` in each `vault.toml` (or use the desktop **Settings →
> Semantic Search** toggle) to actually turn them on. A **lean** image reports
> `embeddings.compiled_in: false` on `GET /api/capabilities`, and the Settings
> toggle shows a "this server was built without embedding support" note. Prefer a
> lean image on tiny/air-gapped hosts where you don't need vectors. See
> [ADR 0018 §9](../docs/adr/0018-embedding-and-vector-search.md) and
> [Embeddings: Operating & Monitoring](../docs/embeddings-operations.md).

> **Browser access:** Use an `app` flavor if you want to open
> `http://server:27183/app/` directly in a browser. The `api` flavor has no
> daemon-served frontend, but the Tauri desktop app supplies its own embedded
> frontend when connected to the server (see
> [§4](#4-point-the-desktop-app-at-the-remote-daemon)).

## Image tags

| Tag | Meaning |
|-----|---------|
| `latest` | Most recent `app` build from `main` |
| `edge` | Same as `latest`; signals active development |
| `sha-<7chars>` | Immutable — pinned to a specific git commit |
| `YYYY.MM.DD` | Date the image was built |
| `*-embed` | The embed-capable counterpart of any tag above (`latest-embed`, `api-latest-embed`, `sha-<7chars>-embed`, …) — same build with `--features local-embed` |

Use `sha-*` tags in production (Compose file, Quadlet unit) so an accidental
`latest` pull never surprises you mid-week.  Update deliberately by picking
the new SHA from the [packages page](https://github.com/surdy/notes-method/pkgs/container/notesmith).

---

## 1. Build the image

The image is automatically built and pushed to GHCR by CI on every push to
`main`. You can pull it directly on your server:

```bash
# app flavor (default — includes daemon-served frontend for browsers)
docker pull ghcr.io/surdy/notesmith:latest

# Or pin to a specific immutable SHA tag (recommended for production):
docker pull ghcr.io/surdy/notesmith:sha-a1b2c3d

# api flavor (binary only; still works with the Tauri desktop app)
docker pull ghcr.io/surdy/notesmith:api-latest

# embed-capable flavors (compile in semantic/hybrid search — larger image)
docker pull ghcr.io/surdy/notesmith:latest-embed      # app + embeddings
docker pull ghcr.io/surdy/notesmith:api-latest-embed  # api + embeddings
```

To build locally from source instead:

```bash
# Docker — lean app flavor (default target)
docker build -f Containerfile -t notesmith:latest .

# Podman
podman build -f Containerfile -t notesmith:latest .

# Embed-capable flavor: pick the target + pass the local-embed feature
docker build -f Containerfile --target app-embed \
  --build-arg CARGO_FEATURES=local-embed -t notesmith:latest-embed .
# (use --target api-embed for the binary-only embed flavor)
```

> The Containerfile cross-compiles to `linux/amd64` regardless of the build
> machine architecture (works from Apple Silicon Mac too).

---

## 2. Prepare config on the server

```bash
# Create the config directory
mkdir -p /etc/notesmith/notesmith

# Copy the example config and edit it
cp deploy/config.toml.example /etc/notesmith/notesmith/config.toml
$EDITOR /etc/notesmith/notesmith/config.toml
```

Key things to set in `config.toml`:
- **`[vaults.name]`** — one entry per vault, `path` must match the
  container-side mount point (e.g. `/vaults/notes`)
- **`default_vault`** — the vault used when `--vault` is not specified

The container's `/config` mount must be writable. Notesmith updates
`/config/notesmith/config.toml` when vaults are added, renamed, removed, or when
the default vault changes from the desktop app or API.

---

## 3a. Launch with Docker Compose

```bash
# Copy compose file to your server
scp deploy/docker-compose.yml user@server:~/notesmith/

# Edit it: replace 100.x.x.x with your Tailscale IP
#          replace /home/user/notes with your notes path
$EDITOR ~/notesmith/docker-compose.yml

# Start
cd ~/notesmith
docker compose up -d

# Check health
docker compose ps
docker compose logs -f
```

---

## 3b. Launch with Podman Quadlet (systemd)

Quadlet lets systemd manage Podman containers directly — no compose daemon needed.

### Per-user install (rootless)

```bash
mkdir -p ~/.config/containers/systemd
cp deploy/notesmith.container ~/.config/containers/systemd/

# Edit: replace 100.x.x.x, paths, etc.
$EDITOR ~/.config/containers/systemd/notesmith.container

systemctl --user daemon-reload
systemctl --user start notesmith
systemctl --user status notesmith

# Enable on login (requires lingering)
loginctl enable-linger $USER
systemctl --user enable notesmith
```

### System-wide install (root)

```bash
sudo cp deploy/notesmith.container /etc/containers/systemd/

# Edit the unit file
sudo $EDITOR /etc/containers/systemd/notesmith.container

sudo systemctl daemon-reload
sudo systemctl start notesmith
sudo systemctl status notesmith
sudo systemctl enable notesmith
```

### View logs

```bash
# Quadlet logs go to journald
journalctl --user -u notesmith -f       # per-user
sudo journalctl -u notesmith -f         # system-wide

# Daemon's own log files are in the notesmith-logs volume:
podman volume inspect notesmith-logs
```

---

## 4. Point the desktop app at the remote daemon

The recommended way is **in the app**: open **Settings → Connection**, add a
server with your daemon's URL (e.g. `http://100.x.x.x:27183`) and optional
token, then **Test** it and switch to it. You can also switch between **This
Mac** and any saved server at runtime from the **status-bar pill** (bottom-left)
— no restart needed. The selection persists across launches. Either image flavor
works for desktop access, because the shell serves its embedded frontend locally
and sends only API/SSE traffic to the daemon.

---

## 5. Verify connectivity

```bash
# From your Mac (Tailscale must be active)
curl http://100.x.x.x:27183/ping
# → {"status":"ok"}

curl http://100.x.x.x:27183/api/status
# → JSON with version, vaults, uptime, etc.

# List vaults via CLI
notesmith --vault notes note list
```

---

## Volume layout inside the container

| Mount | Purpose | Recommended host path |
|-------|---------|----------------------|
| `/vaults/<name>` | Vault markdown files | `/home/user/notes` |
| `/config` | Global config (`notesmith/config.toml`); must be writable for vault registry updates | `/etc/notesmith` |
| `/data` | SQLite caches, Tantivy indexes, lockfile | Named volume |
| `/logs` | Daemon log files (7-day rolling) | Named volume |

---

## Updating

```bash
# Rebuild image with latest code
docker build -f Containerfile -t notesmith:latest .

# Restart
docker compose up -d --force-recreate   # Docker Compose
systemctl --user restart notesmith       # Quadlet
```

The SQLite caches and Tantivy indexes are stored in the `notesmith-data`
volume and will be rebuilt automatically from your markdown files on first
startup after an update.
