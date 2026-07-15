#!/usr/bin/env bash
# Fetch the bundled embedding model (bge-small-en-v1.5) into the Tauri resource
# directory so the desktop app can enable embeddings fully offline — no
# HuggingFace download on first enable (ADR 0018 §9.2, #256 Part B).
#
# The weights are large (~133MB) and are NOT committed to git; run this once
# before `cargo tauri build` for a release that ships bundled embeddings. The
# files are loaded at runtime via fastembed's "bring your own model" bytes API
# (see notesmith_embed::LocalFastEmbed::bge_small_from_dir), so a plain flat
# directory of these five files is all that is required.
#
# Usage: ./fetch-embed-model.sh [--force]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST_DIR="$SCRIPT_DIR/resources/embed-model"

# HuggingFace repo hosting the ONNX + tokenizer files for bge-small-en-v1.5.
# Matches fastembed's EmbeddingModel::BGESmallENV15 (Xenova/bge-small-en-v1.5).
HF_REPO="Xenova/bge-small-en-v1.5"
BASE_URL="https://huggingface.co/${HF_REPO}/resolve/main"

FORCE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1; shift ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

mkdir -p "$DEST_DIR"

# Map: remote path  ->  local flat filename expected by bge_small_from_dir.
declare -a FILES=(
  "onnx/model.onnx|model.onnx"
  "tokenizer.json|tokenizer.json"
  "config.json|config.json"
  "special_tokens_map.json|special_tokens_map.json"
  "tokenizer_config.json|tokenizer_config.json"
)

for entry in "${FILES[@]}"; do
  remote="${entry%%|*}"
  local_name="${entry##*|}"
  dest="$DEST_DIR/$local_name"
  if [[ -f "$dest" && $FORCE -eq 0 ]]; then
    echo "exists, skipping: $local_name (use --force to re-download)"
    continue
  fi
  echo "downloading $remote -> $dest"
  curl -fL --retry 3 --progress-bar -o "$dest" "$BASE_URL/$remote"
done

echo
echo "Bundled model ready in: $DEST_DIR"
ls -lh "$DEST_DIR"
