#!/usr/bin/env bash
# Fetch the bundled whisper.cpp model into the Tauri resource directory so the
# desktop app can enable transcription fully offline — no HuggingFace download
# on first enable (ADR 0023 §3).
#
# The default is `ggml-small.en-q5_1.bin` (~181 MB, English, accuracy-first —
# the ratified default per ADR 0023 §3). The weights are large and are NOT
# committed to git; run this once before `cargo tauri build` for a release that
# ships bundled transcription. The daemon-side worker loads the model at runtime
# via NOTESMITH_WHISPER_MODEL_DIR, resolving any `ggml-*.bin` in the directory
# (see notesmith_transcribe::whisper_model_file), so a flat directory holding
# this one file is all that is required.
#
# To bundle a different tier instead, pass --model NAME (e.g. a multilingual
# `ggml-small-q5_1.bin` for non-English audio, or `ggml-base.en-q5_1.bin` for a
# smaller/faster English model). Only one `ggml-*.bin` should live in the
# resource dir at a time (the resolver picks the lexicographically-first match).
#
# Usage: ./fetch-whisper-model.sh [--force] [--model ggml-small.en-q5_1.bin]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST_DIR="$SCRIPT_DIR/resources/whisper-model"

# HuggingFace repo hosting the whisper.cpp GGML models (upstream, canonical).
HF_REPO="ggerganov/whisper.cpp"
BASE_URL="https://huggingface.co/${HF_REPO}/resolve/main"

# Ratified default (ADR 0023 §3): quantized English `small` model.
MODEL="ggml-small.en-q5_1.bin"
FORCE=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --force) FORCE=1; shift ;;
    --model) MODEL="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

case "$MODEL" in
  ggml-*.bin) ;;
  *) echo "Model must be a ggml-*.bin filename (got: $MODEL)"; exit 1 ;;
esac

mkdir -p "$DEST_DIR"

# Keep only one ggml-*.bin in the resource dir so the runtime resolver is
# unambiguous: drop any previously-fetched model that differs from this one.
if [[ $FORCE -eq 1 ]]; then
  find "$DEST_DIR" -maxdepth 1 -name 'ggml-*.bin' -not -name "$MODEL" -delete
fi

dest="$DEST_DIR/$MODEL"
if [[ -f "$dest" && $FORCE -eq 0 ]]; then
  echo "exists, skipping: $MODEL (use --force to re-download)"
else
  echo "downloading $MODEL -> $dest"
  curl -fL --retry 3 --progress-bar -o "$dest" "$BASE_URL/$MODEL"
fi

echo
echo "Bundled whisper model ready in: $DEST_DIR"
ls -lh "$DEST_DIR"
