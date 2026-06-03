#!/usr/bin/env bash
# Build and launch Notesmith in dev mode.
#
# Usage:
#   ./dev.sh          # incremental build + launch
#   ./dev.sh --clean  # full rebuild from scratch
#   ./dev.sh --build  # produce installable .app bundle

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$REPO_ROOT"

CLEAN=false
BUILD=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --clean) CLEAN=true; shift ;;
    --build) BUILD=true; shift ;;
    *) echo "Usage: ./dev.sh [--clean] [--build]"; exit 1 ;;
  esac
done

if $CLEAN; then
  echo "🧹 Cleaning..."
  cargo clean
  (cd ui/app && rm -rf .svelte-kit node_modules/.vite)
fi

# 1. Build the CLI sidecar
echo "🔨 Building notesmith CLI..."
cargo build --release -p notesmith-cli --quiet

# 2. Copy sidecar binary
echo "📦 Copying sidecar..."
crates/notesmith-tauri/copy-sidecar.sh --profile release

# Also place the sidecar next to the Tauri dev binary so resolve_sidecar_path()
# finds it during `cargo tauri dev` (the Tauri debug exe looks for it in target/debug/).
TARGET_TRIPLE="$(rustc --print host-tuple)"
cp "crates/notesmith-tauri/binaries/notesmith-${TARGET_TRIPLE}" \
   "target/debug/notesmith-${TARGET_TRIPLE}" 2>/dev/null || \
  mkdir -p target/debug && cp "crates/notesmith-tauri/binaries/notesmith-${TARGET_TRIPLE}" \
   "target/debug/notesmith-${TARGET_TRIPLE}"
chmod +x "target/debug/notesmith-${TARGET_TRIPLE}"
echo "✅ Dev sidecar staged at target/debug/notesmith-${TARGET_TRIPLE}"

# 3. Build the SvelteKit frontend
echo "🌐 Building frontend..."
(cd ui/app && pnpm install --silent && pnpm build)

if $BUILD; then
  # Produce installable .app bundle
  echo "📱 Building macOS app..."
  (cd crates/notesmith-tauri && cargo tauri build)
  echo ""
  echo "✅ App bundle ready at: target/release/bundle/macos/"
else
  # Launch in dev mode
  echo "🚀 Launching Notesmith..."
  (cd crates/notesmith-tauri && cargo tauri dev)
fi
