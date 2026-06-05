#!/usr/bin/env bash
# healthcheck.sh — lightweight liveness probe for the Notesmith daemon
# Used as the Docker/Podman HEALTHCHECK command.
# Exits 0 if the daemon responds to /ping, non-zero otherwise.

set -euo pipefail

# Default to localhost inside the container; override with NOTESMITH_HEALTH_URL
URL="${NOTESMITH_HEALTH_URL:-http://127.0.0.1:27183/ping}"

if command -v curl &>/dev/null; then
    curl --silent --fail --max-time 3 "$URL" > /dev/null
elif command -v wget &>/dev/null; then
    wget --quiet --spider --timeout=3 "$URL"
else
    # Fallback: pure bash TCP probe (no external tool needed)
    # Extract host and port from the URL
    host_port="${URL#http://}"
    host_port="${host_port%%/*}"
    host="${host_port%:*}"
    port="${host_port##*:}"
    exec 3<>/dev/tcp/"$host"/"$port"
    printf 'GET /ping HTTP/1.0\r\nHost: %s\r\n\r\n' "$host" >&3
    read -r response <&3
    exec 3>&-
    [[ "$response" == *"200"* ]]
fi
