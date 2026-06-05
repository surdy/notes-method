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

## Image tags

| Tag | Meaning |
|-----|---------|
| `latest` | Most recent build from `main` |
| `edge` | Same as `latest`; signals active development |
| `sha-<7chars>` | Immutable — pinned to a specific git commit |
| `YYYY.MM.DD` | Date the image was built |

Use `sha-*` tags in production (Compose file, Quadlet unit) so an accidental
`latest` pull never surprises you mid-week.  Update deliberately by picking
the new SHA from the [packages page](https://github.com/surdy/notes-method/pkgs/container/notesmith).

---

## 1. Build the image

The image is automatically built and pushed to GHCR by CI on every push to
`main`. You can pull it directly on your server:

```bash
docker pull ghcr.io/surdy/notesmith:latest

# Or pin to a specific immutable SHA tag (recommended for production):
docker pull ghcr.io/surdy/notesmith:sha-a1b2c3d
```

To build locally from source instead:

```bash
# Docker
docker build -f Containerfile -t notesmith:latest .

# Podman
podman build -f Containerfile -t notesmith:latest .
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

The Tauri desktop app reads `NOTESMITH_DESKTOP_DAEMON_URL` to find the daemon.

### macOS (launchd)

Create `~/Library/LaunchAgents/com.notesmith.env.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.notesmith.env</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/launchctl</string>
    <string>setenv</string>
    <string>NOTESMITH_DESKTOP_DAEMON_URL</string>
    <string>http://100.x.x.x:27183</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
```

```bash
# Load it (or log out and back in)
launchctl load ~/Library/LaunchAgents/com.notesmith.env.plist
```

### Or set it in your shell profile

```bash
# ~/.zshrc or ~/.bashrc — only needed if launching Notesmith from terminal
export NOTESMITH_DESKTOP_DAEMON_URL="http://100.x.x.x:27183"
```

> Replace `100.x.x.x` with your server's Tailscale IP in both cases.

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
| `/config` | Global config (`notesmith/config.toml`) | `/etc/notesmith` |
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
