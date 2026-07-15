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

/// Environment variable pointing at a directory of pre-bundled model files
/// (`model.onnx` + the four tokenizer JSON files). When set and populated, the
/// [`default_embedder`] loads the model from disk via fastembed's
/// "bring your own model" bytes API instead of downloading it from HuggingFace.
/// The desktop app sets this to its bundled model resource so first-enable is
/// offline and instant (ADR 0018 §9.2, #256 Part B). Server/CLI builds leave it
/// unset and keep the download-on-first-run behaviour.
pub const EMBED_MODEL_DIR_ENV: &str = "NOTESMITH_EMBED_MODEL_DIR";

/// The ONNX weight filename expected inside [`EMBED_MODEL_DIR_ENV`].
pub const BUNDLED_ONNX_FILE: &str = "model.onnx";

/// Resolve a usable pre-bundled model directory from [`EMBED_MODEL_DIR_ENV`].
///
/// Returns `Some(dir)` only when the variable is set to a directory that
/// actually contains [`BUNDLED_ONNX_FILE`]; otherwise `None`, so callers cleanly
/// fall back to the network download path. Kept free of the `local-embed`
/// feature gate so the resolution rules are unit-testable in lean builds.
pub fn bundled_model_dir() -> Option<std::path::PathBuf> {
    resolve_bundled_model_dir(std::env::var_os(EMBED_MODEL_DIR_ENV))
}

/// Pure resolution used by [`bundled_model_dir`], separated so the rules are
/// unit-testable without mutating the process environment.
fn resolve_bundled_model_dir(value: Option<std::ffi::OsString>) -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(value?);
    if dir.join(BUNDLED_ONNX_FILE).is_file() {
        Some(dir)
    } else {
        None
    }
}

/// Whether the real local embedding runtime (fastembed/ONNX) is compiled into
/// this build — i.e. whether [`default_embedder`] returns [`LocalFastEmbed`]
/// rather than the [`HashEmbedder`] placeholder. This is the single source of
/// truth `/api/capabilities` advertises as `embeddings.compiled_in`: because it
/// is evaluated in `notesmith-embed` (the crate that owns embedder selection),
/// it stays correct no matter which upstream crate enabled the feature, so a
/// binary that pulls in the real embedder can never mis-report itself as lean
/// (ADR 0018 §9.3).
pub const LOCAL_EMBED_COMPILED: bool = cfg!(feature = "local-embed");

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

    /// Construct the default `bge-small-en-v1.5` embedder from a directory of
    /// pre-bundled model files, with **no network access**.
    ///
    /// `model_dir` must contain [`BUNDLED_ONNX_FILE`](crate::BUNDLED_ONNX_FILE)
    /// plus the four tokenizer JSON files (`tokenizer.json`, `config.json`,
    /// `special_tokens_map.json`, `tokenizer_config.json`). Files are read as
    /// bytes and handed to fastembed's "bring your own model" API, bypassing the
    /// HuggingFace hub cache entirely. This is how the desktop app achieves an
    /// offline first-enable (ADR 0018 §9.2, #256 Part B). The resulting
    /// embedder's [`id`](Embedder::id)/[`dim`](Embedder::dim) match
    /// [`bge_small`](Self::bge_small), so stores embedded either way are
    /// interchangeable.
    pub fn bge_small_from_dir(model_dir: &std::path::Path) -> Result<Self> {
        use fastembed::{
            InitOptionsUserDefined, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
        };

        let read = |name: &str| -> Result<Vec<u8>> {
            let path = model_dir.join(name);
            std::fs::read(&path).map_err(|e| {
                EmbedError::Embed(format!(
                    "could not read bundled model file {}: {e}",
                    path.display()
                ))
            })
        };

        let onnx_file = read(crate::BUNDLED_ONNX_FILE)?;
        let tokenizer_files = TokenizerFiles {
            tokenizer_file: read("tokenizer.json")?,
            config_file: read("config.json")?,
            special_tokens_map_file: read("special_tokens_map.json")?,
            tokenizer_config_file: read("tokenizer_config.json")?,
        };

        let model = UserDefinedEmbeddingModel::new(onnx_file, tokenizer_files);
        let model =
            TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::default())
                .map_err(|e| {
                    EmbedError::Embed(format!(
                        "could not load bundled '{}' from {}: {e}",
                        Self::DEFAULT_MODEL_ID,
                        model_dir.display(),
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
    if let Some(dir) = bundled_model_dir() {
        tracing::info!("loading bundled embedding model from {}", dir.display());
        return Ok(std::sync::Arc::new(LocalFastEmbed::bge_small_from_dir(
            &dir,
        )?));
    }
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

    #[test]
    fn bundled_model_dir_is_none_when_env_unset() {
        assert!(resolve_bundled_model_dir(None).is_none());
    }

    #[test]
    fn bundled_model_dir_is_none_when_onnx_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(resolve_bundled_model_dir(Some(dir.path().as_os_str().to_owned())).is_none());
    }

    #[test]
    fn bundled_model_dir_resolves_when_onnx_present() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(BUNDLED_ONNX_FILE), b"stub").unwrap();
        assert_eq!(
            resolve_bundled_model_dir(Some(dir.path().as_os_str().to_owned())).as_deref(),
            Some(dir.path())
        );
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
