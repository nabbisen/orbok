//! # orbok-embed
//!
//! Embedding backend factory (RFC-021). Selects and constructs a local
//! [`EmbeddingModel`] implementation from an [`EmbeddingModelConfig`].
//!
//! ## Backend selection
//!
//! | Backend | Feature flag | Notes |
//! |---|---|---|
//! | `Mock` | always | Deterministic 8-dim, test-only |
//! | `OnnxRuntime` | `tract` | Tract ONNX runtime (pure Rust) |
//!
//! Without the `tract` feature, `create_embedding_model` returns
//! [`OrbokError::Cache`] when called with the `OnnxRuntime` backend.
//! Enable the feature at build time and provide model weights to use
//! real inference:
//!
//! ```sh
//! cargo build --features orbok-embed/tract
//! ```
//!
//! The `CandleCpu`/`CandleCuda` variants of [`InferenceBackend`] are not
//! currently supported (RFC-046): selecting either returns a not-supported
//! error. They are retained in the model layer for API stability; a future
//! RFC may reintroduce a Candle backend with a concrete implementation and
//! CI coverage.
//!
//! ## RFC-021 model comparison
//!
//! Evaluated models for the default recommendation:
//!
//! | Model | Dim | Size | License | Japanese | Notes |
//! |---|---|---|---|---|---|
//! | all-MiniLM-L6-v2 | 384 | ~22 MB | Apache 2.0 | Weak | Fast, widely supported |
//! | nomic-embed-text-v1.5 | 768 | ~137 MB | Apache 2.0 | Moderate | Good multilingual |
//! | multilingual-e5-small | 384 | ~490 MB | MIT | Strong | 94 languages including Japanese |
//!
//! **Recommended default (RFC-021):** `multilingual-e5-small` for
//! orbok's mixed Japanese-English use case (RFC-014). The 384-dim
//! vectors keep storage manageable while providing genuine multilingual
//! recall. Users can override via `EmbeddingModelConfig`.

#[cfg(feature = "tract")]
mod tract_backend;

use orbok_core::{OrbokError, OrbokResult};
use orbok_models::{EmbeddingModel, EmbeddingModelConfig, InferenceBackend, MockEmbeddingModel};
use std::path::{Path, PathBuf};

/// Recommended default model configuration for new installations.
///
/// Based on the RFC-021 evaluation: multilingual-e5-small provides the
/// best balance of Japanese recall, storage cost, and CPU inference
/// speed for orbok's typical corpus.
pub const RECOMMENDED_MODEL_NAME: &str = "multilingual-e5-small";
pub const RECOMMENDED_MODEL_VERSION: &str = "v1";
pub const RECOMMENDED_MODEL_DIMENSION: u32 = 384;
pub const RECOMMENDED_MODEL_MAX_SEQ_LEN: u32 = 512;
/// HuggingFace model ID for manual download reference.
pub const RECOMMENDED_HF_MODEL_ID: &str = "intfloat/multilingual-e5-small";
/// Expected ONNX weights file name once downloaded.
pub const RECOMMENDED_ONNX_FILE: &str = "onnx/model.onnx";
/// Expected tokenizer file name once downloaded.
pub const RECOMMENDED_TOKENIZER_FILE: &str = "tokenizer.json";

/// Construct an embedding model from configuration.
///
/// - `Mock` backend: always works, no model file required.
/// - `OnnxRuntime`: requires `--features tract` and the model file.
/// - `CandleCpu`/`CandleCuda`: not currently supported (RFC-046); returns a
///   not-supported error.
///
/// Returns [`OrbokError::Cache`] with a human-readable message when the
/// requested backend is not compiled in or not supported, so callers can
/// degrade to keyword-only mode.
pub fn create_embedding_model(
    config: &EmbeddingModelConfig,
) -> OrbokResult<Box<dyn EmbeddingModel>> {
    match &config.backend {
        InferenceBackend::Mock => Ok(Box::new(MockEmbeddingModel)),

        InferenceBackend::OnnxRuntime => {
            #[cfg(feature = "tract")]
            {
                tract_backend::create(config)
            }
            #[cfg(not(feature = "tract"))]
            {
                Err(OrbokError::Cache(
                    "ONNX inference is not compiled in. \
                     Rebuild with: --features orbok-embed/tract"
                        .into(),
                ))
            }
        }

        // RFC-046 (B1): the Candle backend is not implemented. The variants
        // are retained in `orbok-models` for API stability and route here.
        InferenceBackend::CandleCpu | InferenceBackend::CandleCuda => Err(OrbokError::Cache(
            "Candle inference is not currently supported. Use the ONNX backend.".into(),
        )),
    }
}

/// Build a default configuration for the recommended model.
///
/// The caller must supply the actual `weights_path` where the model was
/// placed (orbok does not download models without explicit user action,
/// RFC-029).
pub fn recommended_config(weights_path: impl Into<String>) -> EmbeddingModelConfig {
    recommended_config_parts(weights_path.into(), None)
}

/// Build a default configuration from the recommended model directory layout.
///
/// The readiness and download flows require:
///
/// ```text
/// tokenizer.json
/// onnx/model.onnx
/// ```
///
/// Use this helper when the caller has the model directory rather than an
/// arbitrary ONNX file path.
pub fn recommended_config_from_model_dir(model_dir: impl AsRef<Path>) -> EmbeddingModelConfig {
    let model_dir = model_dir.as_ref();
    let weights_path = path_to_string(model_dir.join(RECOMMENDED_ONNX_FILE));
    let tokenizer_path = path_to_string(model_dir.join(RECOMMENDED_TOKENIZER_FILE));
    recommended_config_parts(weights_path, Some(tokenizer_path))
}

fn recommended_config_parts(
    weights_path: String,
    tokenizer_path: Option<String>,
) -> EmbeddingModelConfig {
    EmbeddingModelConfig {
        weights_path,
        tokenizer_path,
        dimension: RECOMMENDED_MODEL_DIMENSION,
        max_seq_len: RECOMMENDED_MODEL_MAX_SEQ_LEN,
        backend: InferenceBackend::OnnxRuntime,
        model_name: RECOMMENDED_MODEL_NAME.to_string(),
        model_version: RECOMMENDED_MODEL_VERSION.to_string(),
    }
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC-021: Mock backend is always available.
    #[test]
    fn mock_backend_always_works() {
        let config = EmbeddingModelConfig {
            weights_path: String::new(),
            tokenizer_path: None,
            dimension: 8,
            max_seq_len: 512,
            backend: InferenceBackend::Mock,
            model_name: "mock".into(),
            model_version: "v1".into(),
        };
        let model = create_embedding_model(&config).unwrap();
        let vecs = model.embed_batch(&["hello world"]).unwrap();
        assert_eq!(vecs.len(), 1);
        assert_eq!(vecs[0].len(), model.dimension() as usize);
    }

    // RFC-046 (B1): Candle variants return a stable not-supported error and
    // must NOT instruct the user to rebuild with the (removed) candle feature.
    #[test]
    fn candle_backends_return_not_supported_error() {
        for backend in [InferenceBackend::CandleCpu, InferenceBackend::CandleCuda] {
            let config = EmbeddingModelConfig {
                weights_path: String::new(),
                tokenizer_path: None,
                dimension: 384,
                max_seq_len: 512,
                backend,
                model_name: "test".into(),
                model_version: "v1".into(),
            };
            match create_embedding_model(&config) {
                Err(err) => {
                    let msg = err.to_string();
                    assert!(
                        msg.contains("not currently supported"),
                        "expected not-supported message, got: {msg}"
                    );
                    assert!(
                        !msg.contains("--features"),
                        "must not reference a rebuild feature flag, got: {msg}"
                    );
                }
                Ok(_) => panic!("Candle backend should not construct a model"),
            }
        }
    }

    // RFC-021: Non-compiled backends return an informative error.
    #[cfg(not(feature = "tract"))]
    #[test]
    fn onnx_backend_without_feature_returns_error() {
        let config = EmbeddingModelConfig {
            weights_path: "/nonexistent/model.onnx".into(),
            tokenizer_path: None,
            dimension: 384,
            max_seq_len: 512,
            backend: InferenceBackend::OnnxRuntime,
            model_name: "test".into(),
            model_version: "v1".into(),
        };
        match create_embedding_model(&config) {
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("tract") || msg.contains("compiled"),
                    "error should mention feature flag"
                );
            }
            Ok(_) => panic!("ONNX without tract feature should fail"),
        }
    }

    // RFC-021: recommended_config builds correct defaults.
    #[test]
    fn recommended_config_correct_defaults() {
        let cfg = recommended_config("/models/multilingual-e5-small.onnx");
        assert_eq!(cfg.dimension, RECOMMENDED_MODEL_DIMENSION);
        assert_eq!(cfg.model_name, RECOMMENDED_MODEL_NAME);
        assert_eq!(cfg.max_seq_len, 512);
        assert!(cfg.tokenizer_path.is_none());
    }

    #[test]
    fn recommended_config_from_model_dir_sets_tokenizer_path() {
        let cfg = recommended_config_from_model_dir("/models/multilingual-e5-small");
        assert!(cfg.weights_path.ends_with("onnx/model.onnx"));
        assert!(cfg.tokenizer_path.unwrap().ends_with("tokenizer.json"));
    }

    // RFC-021: storage impact calculation.
    #[test]
    fn storage_impact_per_dimension() {
        // 4 bytes per FP32 component.
        let bytes_384 = 384 * 4; // 1.5 KiB per chunk
        let bytes_768 = 768 * 4; // 3.0 KiB per chunk
        // At 10,000 chunks: 384-dim = ~14 MB, 768-dim = ~29 MB.
        assert_eq!(bytes_384, 1536);
        assert_eq!(bytes_768, 3072);
        // 384-dim is the recommended default for storage efficiency.
        assert!(bytes_384 < bytes_768);
    }
}
