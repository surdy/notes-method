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
# The shipped desktop app is always embed-capable (ADR 0018 §9.2): the sidecar
# is built with `--features local-embed` so the Settings → Semantic Search toggle
# can turn embeddings on without a rebuild. ONNX is heavy to compile, so
# `--no-embed` opts out for fast dev iteration when you're not touching search.
EMBED_FEATURE=(--features local-embed)
for arg in "$@"; do
  case "$arg" in
    --release) PROFILE="release"; CARGO_PROFILE_FLAG=(--release) ;;
    --no-embed) EMBED_FEATURE=() ;;
  esac
done

echo "[dev-launch] Building notesmith CLI (profile: $PROFILE${EMBED_FEATURE:+, local-embed})..."
(cd "$REPO_ROOT" && cargo build -p notesmith-cli "${CARGO_PROFILE_FLAG[@]}" "${EMBED_FEATURE[@]}")

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

# The daemon resolves its frontend bundle relative to its own binary location:
# <exe_dir>/../../ui/app/build. When the desktop spawns the daemon from
# crates/notesmith-tauri/target/<profile>/, that resolves to
# crates/ui/app/build — which doesn't exist — and the desktop shows a blank
# window. Build the frontend if missing and pin NOTESMITH_APP_DIR so the
# spawned daemon always finds the right bundle regardless of CWD.
APP_BUILD_DIR="$REPO_ROOT/ui/app/build"
if [[ ! -f "$APP_BUILD_DIR/index.html" ]]; then
  echo "[dev-launch] Building SvelteKit frontend (missing $APP_BUILD_DIR/index.html)..."
  if command -v pnpm >/dev/null 2>&1; then
    (cd "$REPO_ROOT/ui/app" && pnpm install --silent && pnpm build)
  else
    (cd "$REPO_ROOT/ui/app" && npm install --silent && npm run build)
  fi
fi
export NOTESMITH_APP_DIR="$APP_BUILD_DIR"
echo "[dev-launch] NOTESMITH_APP_DIR=$NOTESMITH_APP_DIR"

echo "[dev-launch] Launching desktop..."
cd "$SCRIPT_DIR"
exec cargo run --bin notesmith-desktop "${CARGO_PROFILE_FLAG[@]}"
