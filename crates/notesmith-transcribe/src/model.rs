//! Bundled-model resolution for the local whisper.cpp engine (ADR 0023 §3).
//!
//! Kept free of the `local-whisper` feature gate so the resolution rules are
//! unit-testable in lean builds — mirrors `notesmith_embed`'s
//! `bundled_model_dir`/`resolve_bundled_model_dir`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Environment variable pointing at a directory containing a whisper.cpp GGML
/// model file (`ggml-*.bin`). When set and populated, the engine loads the
/// model from disk instead of downloading it. The desktop app sets this to its
/// bundled model resource so first-enable is offline (ADR 0023 §3); server/CLI
/// builds may leave it unset and supply a model another way.
pub const WHISPER_MODEL_DIR_ENV: &str = "NOTESMITH_WHISPER_MODEL_DIR";

/// Resolve a usable pre-bundled model directory from [`WHISPER_MODEL_DIR_ENV`].
///
/// Returns `Some(dir)` only when the variable is set to a directory that
/// actually contains a `ggml-*.bin` model file; otherwise `None`, so callers
/// cleanly fall back to a placeholder engine.
pub fn bundled_model_dir() -> Option<PathBuf> {
    resolve_bundled_model_dir(std::env::var_os(WHISPER_MODEL_DIR_ENV))
}

/// Pure resolution used by [`bundled_model_dir`], separated so the rules are
/// unit-testable without mutating the process environment.
fn resolve_bundled_model_dir(value: Option<OsString>) -> Option<PathBuf> {
    let dir = PathBuf::from(value?);
    whisper_model_file(&dir).map(|_| dir)
}

/// The path to a whisper.cpp GGML model file inside `dir`, if one exists.
///
/// Accepts any `ggml-*.bin` filename (e.g. `ggml-base.en.bin`,
/// `ggml-base.en-q5_1.bin`) so quantized variants resolve without a hard-coded
/// filename. When several match, the lexicographically-first is returned for
/// determinism.
pub fn whisper_model_file(dir: &Path) -> Option<PathBuf> {
    let mut matches: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_ggml_model(p))
        .collect();
    matches.sort();
    matches.into_iter().next()
}

/// Whether `path`'s filename looks like a whisper.cpp GGML model (`ggml-*.bin`).
fn is_ggml_model(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.starts_with("ggml-") && name.ends_with(".bin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn none_when_env_unset() {
        assert!(resolve_bundled_model_dir(None).is_none());
    }

    #[test]
    fn none_when_dir_missing_model() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("readme.txt"), b"no model here").unwrap();
        assert!(resolve_bundled_model_dir(Some(dir.path().into())).is_none());
        assert!(whisper_model_file(dir.path()).is_none());
    }

    #[test]
    fn resolves_dir_with_ggml_model() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ggml-base.en.bin"), b"fake").unwrap();
        let resolved = resolve_bundled_model_dir(Some(dir.path().into()));
        assert_eq!(resolved.as_deref(), Some(dir.path()));
        assert_eq!(
            whisper_model_file(dir.path()),
            Some(dir.path().join("ggml-base.en.bin"))
        );
    }

    #[test]
    fn accepts_quantized_variant_and_is_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ggml-base.en-q5_1.bin"), b"fake").unwrap();
        fs::write(dir.path().join("ggml-tiny.en.bin"), b"fake").unwrap();
        // lexicographically-first is "ggml-base.en-q5_1.bin"
        assert_eq!(
            whisper_model_file(dir.path()),
            Some(dir.path().join("ggml-base.en-q5_1.bin"))
        );
    }

    #[test]
    fn ignores_non_ggml_bins() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("model.bin"), b"fake").unwrap();
        fs::write(dir.path().join("ggml-base.en.txt"), b"fake").unwrap();
        assert!(whisper_model_file(dir.path()).is_none());
    }
}
