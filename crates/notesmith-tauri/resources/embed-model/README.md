# Bundled embedding model

This directory holds the local embedding model (`bge-small-en-v1.5`) that the
desktop app bundles so enabling **Semantic Search** is offline and instant — no
HuggingFace download on first enable (ADR 0018 §9.2, #256 Part B).

The weights are **not committed** (see `.gitignore`). Fetch them before a release
build:

```sh
crates/notesmith-tauri/fetch-embed-model.sh
```

That downloads five files into this directory:

- `model.onnx` (the ONNX weights, ~133 MB)
- `tokenizer.json`
- `config.json`
- `special_tokens_map.json`
- `tokenizer_config.json`

At runtime the desktop shell resolves this directory (a Tauri bundle resource)
and passes it to the daemon via `NOTESMITH_EMBED_MODEL_DIR`; the embed worker
loads the model from disk via fastembed's "bring your own model" bytes API
(`notesmith_embed::LocalFastEmbed::bge_small_from_dir`). If `model.onnx` is
absent (e.g. an unbundled/dev build), the daemon falls back to downloading the
model on first run.

This `README.md` is committed only so the Tauri resource glob matches when the
weights have not been fetched; it is otherwise unused.
