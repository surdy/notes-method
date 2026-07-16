# Bundled transcription model

This directory holds the local whisper.cpp model that the desktop app bundles so
enabling **Transcription** is offline and instant — no HuggingFace download on
first enable (ADR 0023 §3).

The weights are **not committed** (see the repo `.gitignore`). Fetch them before
a release build:

```sh
crates/notesmith-tauri/fetch-whisper-model.sh
```

That downloads one file into this directory:

- `ggml-small.en-q5_1.bin` — the ratified default (~181 MB, English,
  accuracy-first; ADR 0023 §3).

To bundle a different tier instead, pass `--model`:

```sh
# Multilingual (non-English audio):
crates/notesmith-tauri/fetch-whisper-model.sh --model ggml-small-q5_1.bin
# Smaller/faster English:
crates/notesmith-tauri/fetch-whisper-model.sh --model ggml-base.en-q5_1.bin
```

Keep only **one** `ggml-*.bin` here at a time; the runtime resolver picks the
lexicographically-first match.

At runtime the desktop shell resolves this directory (a Tauri bundle resource)
and passes it to the daemon via `NOTESMITH_WHISPER_MODEL_DIR`; the colocated
`notesmith transcribe` worker (spawned by the daemon, inheriting its
environment) loads the model from disk via
`notesmith_transcribe::whisper_model_file`, which accepts any `ggml-*.bin`. If no
model is present (e.g. an unbundled/dev build), the worker falls back to
downloading one on first run.

This `README.md` is committed only so the Tauri resource glob matches when the
weights have not been fetched; it is otherwise unused.
