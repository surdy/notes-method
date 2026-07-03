# Providers and Subscriptions

Context: [ADR 0018](../adr/0018-embedding-and-vector-search.md) defines the embedding architecture; [ADR 0019](../adr/0019-media-ingestion-pipeline.md) defines the media ingestion pipeline that feeds it.

## Do Claude/Codex subscriptions give me embeddings?

**No.**

| Subscription/API | Embeddings included? | Practical meaning |
|------------------|----------------------|-------------------|
| Claude Pro/Max | No | Anthropic ships no embeddings product. It points users to Voyage AI, which it acquired. A Claude subscription is irrelevant for Notesmith embeddings. |
| ChatGPT | No | ChatGPT subscriptions cover the chat UI, not the embeddings API. |
| Codex | No | Codex subscription/access covers the Codex agent experience, not an embeddings endpoint. |
| OpenAI API | Yes, metered | `text-embedding-3-small` and `text-embedding-3-large` are API-only on `platform.openai.com`, billed per token on a separate API account. |

`text-embedding-3-small` is roughly `$0.02 / million tokens`, but it is still metered cloud usage. To avoid metered pricing, self-host a local embedder.

## Local, no second service

`fastembed-rs` is the default local path from ADR 0018:

```text
Notesmith worker or daemon
  -> fastembed-rs
  -> bundled ONNX Runtime
  -> auto-downloaded embedding model
```

That runs in-process in the daemon or CLI worker. There is no required Ollama, LM Studio, TEI, Docker service, or GPU server.

`candle` is the pure-Rust/Metal alternative. It is attractive for a deeper native stack, but it requires more model/runtime work than `fastembed-rs`.

Sidecars such as Ollama, LM Studio, or TEI remain optional. They are useful if the user already runs them, not a Notesmith requirement.

## The `Embedder` trait

ADR 0018 uses one embedding abstraction:

```rust
trait Embedder {
    fn id(&self) -> &str;
    fn dim(&self) -> usize;
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}
```

| Implementation | Use |
|----------------|-----|
| `LocalFastEmbed` | Default local/offline implementation using `fastembed-rs`. |
| `OpenAiCompatible { base_url, model, api_key }` | One HTTP implementation for any provider that speaks the OpenAI embeddings wire format. |

Because the OpenAI wire format is a de-facto standard, one HTTP client can cover OpenAI, Voyage, Cohere-compatible providers, SiliconFlow, and local Ollama/LM Studio/TEI servers by changing only `base_url` and `model`.

## Model choices

| Model | Dim | Approx size | Notes |
|-------|-----|-------------|-------|
| `bge-small-en-v1.5` | 384 | about 130MB | Default local model: small, fast, good enough for English notes. |
| `nomic-embed-text` | 768 | about 500MB | Strong local general-purpose model; common in Ollama setups. |
| `bge-m3` | 1024 | about 2GB | Multilingual and stronger retrieval; heavier local footprint. |
| `mxbai-embed-large` | 1024 | about 600MB | Strong local English retrieval; larger vectors and model. |
| `text-embedding-3-small` | 1536 | cloud | OpenAI API-only; cheap per token, not part of ChatGPT/Codex subscriptions. |
| `text-embedding-3-large` | 3072 | cloud | Higher-dimensional OpenAI option; larger storage and cost. |

## Privacy callout

A cloud embedder sends note content to a third party. Notesmith is self-hosted for privacy, so cloud embedding is strictly opt-in, per-vault, and default off/local. Local embedding keeps note content on the machine running Notesmith.
