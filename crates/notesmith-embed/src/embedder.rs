//! Embedding backends.
//!
//! ADR 0018 §1/§6/§8. The [`Embedder`] trait abstracts text → vector so the
//! worker and daemon are model-agnostic. Two implementations ship:
//!
//! * [`HashEmbedder`] — a deterministic, dependency-free hashing embedder used
//!   as the default and in tests. It needs no model download, so `cargo test`
//!   and lean builds work fully offline. Its vectors are *not* semantically
//!   meaningful; it exists so the whole pipeline is exercisable without ONNX.
//! * [`LocalFastEmbed`] — real local embeddings via `fastembed-rs` (ONNX
//!   Runtime), behind the `local-embed` Cargo feature so cloud/lean builds omit
//!   the native runtime. The model is downloaded on first run.
//!
//! Cloud (`OpenAiCompatible`) embedders are deferred (ADR 0018 §6, P3 tracker).

#[cfg(feature = "local-embed")]
use crate::EmbedError;
use crate::Result;

/// Text → dense vector. `id()`/`dim()` identify the model so the store can be
/// stamped and query-time embedding can be validated against it (ADR 0018 §7).
pub trait Embedder: Send + Sync {
    /// Embed a batch of texts, returning one vector per input (same order).
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    /// Stable identifier recorded in `_meta.embedder_id` (e.g. the model name).
    fn id(&self) -> &str;
    /// Vector dimension recorded in `_meta.dim`.
    fn dim(&self) -> usize;
}

/// A deterministic, dependency-free hashing embedder.
///
/// Each whitespace token is hashed into the vector space (signed hashing trick)
/// and the result is L2-normalised. Identical text always yields identical
/// vectors, which is all the pipeline/tests need. It is **not** a semantic
/// model — use [`LocalFastEmbed`] for real retrieval quality.
pub struct HashEmbedder {
    id: String,
    dim: usize,
}

impl HashEmbedder {
    /// A hash embedder with the given dimension (default model uses 384).
    pub fn new(dim: usize) -> Self {
        Self {
            id: format!("hash-emb-v1-{dim}"),
            dim,
        }
    }
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new(384)
    }
}

impl Embedder for HashEmbedder {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| hash_embed(t, self.dim)).collect())
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

fn hash_embed(text: &str, dim: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; dim];
    for token in text.split_whitespace() {
        let lower = token.to_lowercase();
        let hash = blake3::hash(lower.as_bytes());
        let bytes = hash.as_bytes();
        let idx = (u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize) % dim;
        // Sign bit from a different byte so collisions can cancel.
        let sign = if bytes[4] & 1 == 0 { 1.0 } else { -1.0 };
        v[idx] += sign;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// Real local embeddings via `fastembed-rs` (ONNX Runtime). Behind the
/// `local-embed` feature. Default model `bge-small-en-v1.5` (384-dim). The model
/// is downloaded to `cache_dir` on first construction; if the machine is offline
/// and the model is not cached, construction returns a clear error.
/// The canonical local embedding model and its vector dimension, advertised via
/// `/api/capabilities` regardless of whether `local-embed` is compiled in. When
/// the feature is off this is the model an embed-capable build *would* use; when
/// on it is the model actually loaded (see [`LocalFastEmbed::DEFAULT_MODEL_ID`]).
pub const CANONICAL_MODEL_ID: &str = "bge-small-en-v1.5";
/// Vector dimension of [`CANONICAL_MODEL_ID`].
pub const CANONICAL_DIM: usize = 384;

#[cfg(feature = "local-embed")]
pub struct LocalFastEmbed {
    model: fastembed::TextEmbedding,
    id: String,
    dim: usize,
}

#[cfg(feature = "local-embed")]
impl LocalFastEmbed {
    /// The default model id used when none is specified.
    pub const DEFAULT_MODEL_ID: &'static str = CANONICAL_MODEL_ID;
    /// The default model's vector dimension.
    pub const DEFAULT_DIM: usize = CANONICAL_DIM;

    /// Construct the default `bge-small-en-v1.5` embedder, downloading the model
    /// into `cache_dir` on first run.
    pub fn bge_small(cache_dir: &std::path::Path) -> Result<Self> {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        std::fs::create_dir_all(cache_dir)?;
        let options = InitOptions::new(EmbeddingModel::BGESmallENV15)
            .with_cache_dir(cache_dir.to_path_buf())
            .with_show_download_progress(false);
        let model = TextEmbedding::try_new(options).map_err(|e| {
            EmbedError::Embed(format!(
                "could not load '{}' (first run downloads the model to {}; \
                 ensure network access or pre-seed the cache): {e}",
                Self::DEFAULT_MODEL_ID,
                cache_dir.display(),
            ))
        })?;
        Ok(Self {
            model,
            id: Self::DEFAULT_MODEL_ID.to_string(),
            dim: Self::DEFAULT_DIM,
        })
    }
}

#[cfg(feature = "local-embed")]
impl Embedder for LocalFastEmbed {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let owned: Vec<String> = texts.to_vec();
        self.model
            .embed(owned, None)
            .map_err(|e| EmbedError::Embed(format!("fastembed inference failed: {e}")))
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// Build the canonical query/worker embedder.
///
/// Both the embed worker and the daemon's query-time embedding must use the
/// *same* model, or the stored `embedder_id`/`dim` will not match and searches
/// will fail loudly ([`EmbeddingSearch`](crate::EmbeddingSearch)). Centralising
/// construction here keeps the worker and the daemon in lockstep.
///
/// With the `local-embed` feature this is a real `fastembed` model; otherwise it
/// is the non-semantic [`HashEmbedder`] placeholder so lean/offline builds and
/// `cargo test` keep working.
#[cfg(feature = "local-embed")]
pub fn default_embedder() -> Result<std::sync::Arc<dyn Embedder>> {
    let cache = crate::data_dir()?.join("models");
    Ok(std::sync::Arc::new(LocalFastEmbed::bge_small(&cache)?))
}

/// See [`default_embedder`]. Placeholder build (no `local-embed`).
#[cfg(not(feature = "local-embed"))]
pub fn default_embedder() -> Result<std::sync::Arc<dyn Embedder>> {
    tracing::warn!(
        "the `local-embed` feature is disabled; using a non-semantic HashEmbedder \
         placeholder. Rebuild with `--features local-embed` for real embeddings."
    );
    Ok(std::sync::Arc::new(HashEmbedder::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_embedder_returns_expected_dim() {
        let emb = HashEmbedder::new(384);
        let out = emb.embed(&["hello world".to_string()]).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 384);
        assert_eq!(emb.dim(), 384);
        assert_eq!(emb.id(), "hash-emb-v1-384");
    }

    #[test]
    fn hash_embedder_is_deterministic() {
        let emb = HashEmbedder::default();
        let a = emb.embed(&["the quick brown fox".to_string()]).unwrap();
        let b = emb.embed(&["the quick brown fox".to_string()]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hash_embedder_normalises_nonempty_text() {
        let emb = HashEmbedder::new(64);
        let v = &emb.embed(&["alpha beta gamma".to_string()]).unwrap()[0];
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn hash_embedder_distinguishes_different_text() {
        let emb = HashEmbedder::default();
        let a = emb.embed(&["database systems".to_string()]).unwrap();
        let b = emb.embed(&["mountain hiking".to_string()]).unwrap();
        assert_ne!(a[0], b[0]);
    }

    // Requires network + model download; run with:
    //   cargo test -p notesmith-embed --features local-embed -- --ignored
    #[cfg(feature = "local-embed")]
    #[test]
    #[ignore = "downloads the bge-small model (network)"]
    fn local_fastembed_embeds_to_384_dims() {
        let dir = tempfile::TempDir::new().unwrap();
        let emb = LocalFastEmbed::bge_small(dir.path()).unwrap();
        assert_eq!(emb.dim(), 384);
        let out = emb
            .embed(&["a note about vector search".to_string()])
            .unwrap();
        assert_eq!(out[0].len(), 384);
    }
}
