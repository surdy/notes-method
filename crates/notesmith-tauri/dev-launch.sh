#!/usr/bin/env bash
# Launch the Notesmith desktop app in dev mode against a freshly built CLI sidecar.
#
# The notesmith-tauri crate is excluded from the workspace and resolves its
# sidecar from `binaries/notesmith-<target-triple>` (copied into
# `target/debug/notesmith-<triple>` by tauri-build during compilation). If that
# file is stale, the desktop launches an old daemon binary that may not match
# the workspace code under development. This script keeps the dance simple:
#
#   1. cargo build -p notesmith-cli   (in the workspace)
#   2. ./copy-sidecar.sh --profile debug
#   3. cargo run --bin notesmith-desktop
#
# Usage:
#   ./dev-launch.sh           # builds + copies sidecar, then launches desktop
#   ./dev-launch.sh --release # use release profile end-to-end

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PROFILE="debug"
CARGO_PROFILE_FLAG=()
if [[ "${1:-}" == "--release" ]]; then
  PROFILE="release"
  CARGO_PROFILE_FLAG=(--release)
fi

echo "[dev-launch] Building notesmith CLI (profile: $PROFILE)..."
(cd "$REPO_ROOT" && cargo build -p notesmith-cli "${CARGO_PROFILE_FLAG[@]}")

echo "[dev-launch] Copying sidecar..."
"$SCRIPT_DIR/copy-sidecar.sh" --profile "$PROFILE"

# Also refresh the tauri-build output dir so `cargo run` sees the fresh binary.
# tauri-build copies binaries/notesmith-<triple> into the per-crate target/<profile>/
# during compilation; the desktop's resolve_sidecar_path() reads from there.
TARGET_TRIPLE="$(rustc --print host-tuple)"
EXTENSION=""
if [[ "$TARGET_TRIPLE" == *"windows"* ]]; then
  EXTENSION=".exe"
fi
TAURI_TARGET_DIR="$SCRIPT_DIR/target/$PROFILE"
mkdir -p "$TAURI_TARGET_DIR"
cp "$SCRIPT_DIR/binaries/notesmith-${TARGET_TRIPLE}${EXTENSION}" \
   "$TAURI_TARGET_DIR/notesmith-${TARGET_TRIPLE}${EXTENSION}"
chmod +x "$TAURI_TARGET_DIR/notesmith-${TARGET_TRIPLE}${EXTENSION}"
echo "[dev-launch] Refreshed $TAURI_TARGET_DIR/notesmith-${TARGET_TRIPLE}${EXTENSION}"

echo "[dev-launch] Launching desktop..."
cd "$SCRIPT_DIR"
exec cargo run --bin notesmith-desktop "${CARGO_PROFILE_FLAG[@]}"
