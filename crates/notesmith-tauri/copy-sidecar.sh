#!/usr/bin/env bash
# Copy the notesmith CLI binary into the Tauri sidecar binaries/ directory
# with the required target-triple suffix.
#
# Usage: ./copy-sidecar.sh [--profile release|debug]
#
# Defaults to release profile.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BINARIES_DIR="$SCRIPT_DIR/binaries"

PROFILE="release"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) PROFILE="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

TARGET_TRIPLE="$(rustc --print host-tuple)"
EXTENSION=""
if [[ "$TARGET_TRIPLE" == *"windows"* ]]; then
  EXTENSION=".exe"
fi

SOURCE="$REPO_ROOT/target/$PROFILE/notesmith${EXTENSION}"
DEST="$BINARIES_DIR/notesmith-${TARGET_TRIPLE}${EXTENSION}"

if [[ ! -f "$SOURCE" ]]; then
  echo "Error: notesmith binary not found at $SOURCE"
  echo "Build it first (embed-capable, matching the shipped app):"
  echo "  cargo build --release -p notesmith-cli --features local-embed"
  exit 1
fi

mkdir -p "$BINARIES_DIR"
cp "$SOURCE" "$DEST"
chmod +x "$DEST"
echo "Copied sidecar: $DEST"
