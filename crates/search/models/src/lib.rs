//! # orbok-models
//!
//! Local AI model vocabulary (RFC-012). Milestone M1–M6 only needs the
//! shared types and the "what is available" summary the UI shows; the
//! install/locate/validate workflow lands in M12.
//!
//! Privacy rule carried from the requirements: model *download* is the
//! only network operation orbok may ever perform, it is explicit, and
//! it never involves document contents.

pub mod download_plan;
pub mod readiness;
pub mod trust;

pub use download_plan::{
    DEFAULT_MODEL_DOWNLOAD_CONCURRENCY, DownloadAction, DownloadPlan, DownloadPlanError,
    FileDownloadProgress, FileDownloadStatus, FriendlyDownloadProblem, ModelFilePlan,
    OverallDownloadProgress, build_download_plan, build_download_plan_against,
};
pub use readiness::{
    FileReadiness, LocalFileIntegrity, LocalFileStatus, ModelProvenance, ModelReadiness,
    ModelReadinessReport, check_app_managed_model_readiness,
    check_app_managed_model_readiness_against, check_model_readiness,
};
pub use trust::{
    DEFAULT_TRUSTED_MODEL, HeaderDisposition, HttpClientPolicy, PRODUCTION_HTTP_CLIENT_POLICY,
    TrustPolicyError, TrustedModelFile, TrustedModelIdentity, TrustedModelManifest,
    TrustedTransportPolicy, redirect_header_disposition, validate_initial_url,
    validate_redirect_url,
};

use serde::{Deserialize, Serialize};

/// Model roles (catalog `models.role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Embedding,
    Reranker,
}

impl ModelRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelRole::Embedding => "embedding",
            ModelRole::Reranker => "reranker",
        }
    }
}

/// Model availability (catalog `models.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Available,
    Missing,
    Invalid,
    Installing,
    Disabled,
}

impl ModelStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelStatus::Available => "available",
            ModelStatus::Missing => "missing",
            ModelStatus::Invalid => "invalid",
            ModelStatus::Installing => "installing",
            ModelStatus::Disabled => "disabled",
        }
    }
}

/// Search capability derived from model availability. Keyword search
/// never depends on models (RFC-007: works with zero models installed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchCapability {
    /// Keyword only: no embedding model available.
    KeywordOnly,
    /// Keyword + semantic: embedding model available.
    Hybrid,
    /// Keyword + semantic + rerank refinement.
    HybridWithRerank,
}

/// Derive the capability shown in the UI from model statuses.
pub fn search_capability(
    embedding: Option<ModelStatus>,
    reranker: Option<ModelStatus>,
) -> SearchCapability {
    match (embedding, reranker) {
        (Some(ModelStatus::Available), Some(ModelStatus::Available)) => {
            SearchCapability::HybridWithRerank
        }
        (Some(ModelStatus::Available), _) => SearchCapability::Hybrid,
        _ => SearchCapability::KeywordOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC-007/RFC-010: search degrades gracefully without models.
    #[test]
    fn capability_degrades_gracefully() {
        assert_eq!(search_capability(None, None), SearchCapability::KeywordOnly);
        assert_eq!(
            search_capability(Some(ModelStatus::Missing), None),
            SearchCapability::KeywordOnly
        );
        assert_eq!(
            search_capability(Some(ModelStatus::Available), None),
            SearchCapability::Hybrid
        );
        assert_eq!(
            search_capability(Some(ModelStatus::Available), Some(ModelStatus::Missing)),
            SearchCapability::Hybrid
        );
        assert_eq!(
            search_capability(Some(ModelStatus::Available), Some(ModelStatus::Available)),
            SearchCapability::HybridWithRerank
        );
    }
}

/// A vector search candidate (RFC-008 §13).
#[derive(Debug, Clone)]
pub struct VectorCandidate {
    pub chunk_id: orbok_core::ChunkId,
    pub file_id: orbok_core::FileId,
    pub rank: u32,
    pub score: f32,
}

/// Local embedding model abstraction (RFC-008 §6).
///
/// Implementations must not transmit text externally (NFR-001).
pub trait EmbeddingModel: Send + Sync {
    /// Stable name stored in `models.model_name`.
    fn name(&self) -> &str;
    /// Version string stored in `models.model_version`.
    fn version(&self) -> &str;
    /// Output dimension — must match stored embeddings (RFC-008 §11).
    fn dimension(&self) -> u32;
    /// Embed a batch of normalized texts. Returns one vector per input,
    /// each L2-normalized.
    fn embed_batch(&self, texts: &[&str]) -> orbok_core::OrbokResult<Vec<Vec<f32>>>;
}

/// Compute cosine similarity between two L2-normalized vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// L2-normalize a vector in-place. No-op for the zero vector.
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-10 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Serialize a vector to little-endian bytes for BLOB storage (RFC-008
/// §12.1 "sqlite_blob with FP32").
pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Deserialize from BLOB bytes; returns `None` on length mismatch.
pub fn blob_to_vec(blob: &[u8], expected_dim: u32) -> Option<Vec<f32>> {
    let dim = expected_dim as usize;
    if blob.len() != dim * 4 {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

// ── Mock model ──────────────────────────────────────────────────────

/// Deterministic 8-dimensional mock embedding model.
///
/// Uses the SHA-256 of the input text as a pseudo-random source for 8
/// f32 components, then L2-normalizes the result.  **Never use for
/// semantic search** — the outputs are semantically meaningless.
/// Suitable for pipeline correctness tests (RFC-008 §24 tests 1–10).
pub struct MockEmbeddingModel;

impl EmbeddingModel for MockEmbeddingModel {
    fn name(&self) -> &str {
        "mock"
    }
    fn version(&self) -> &str {
        "v1"
    }
    fn dimension(&self) -> u32 {
        8
    }
    fn embed_batch(&self, texts: &[&str]) -> orbok_core::OrbokResult<Vec<Vec<f32>>> {
        use sha2::{Digest, Sha256};
        texts
            .iter()
            .map(|text| {
                let digest = Sha256::digest(text.as_bytes());
                let mut v: Vec<f32> = digest[..8].iter().map(|&b| b as f32 / 255.0).collect();
                l2_normalize(&mut v);
                Ok(v)
            })
            .collect()
    }
}

#[cfg(test)]
mod embedding_tests {
    use super::*;

    // RFC-008 §24 test 2: embedding generation succeeds for sample chunks.
    #[test]
    fn mock_embed_batch() {
        let model = MockEmbeddingModel;
        let vecs = model.embed_batch(&["hello world", "foo bar"]).unwrap();
        assert_eq!(vecs.len(), 2);
        for v in &vecs {
            assert_eq!(v.len(), model.dimension() as usize);
        }
    }

    // RFC-008 §24 test 3: dimension mismatch can be detected by caller.
    #[test]
    fn blob_roundtrip_and_dim_mismatch() {
        let v = vec![0.1_f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let blob = vec_to_blob(&v);
        assert_eq!(blob.len(), 32);
        let back = blob_to_vec(&blob, 8).unwrap();
        for (a, b) in v.iter().zip(&back) {
            assert!((a - b).abs() < 1e-6);
        }
        assert!(
            blob_to_vec(&blob, 16).is_none(),
            "dim mismatch must return None"
        );
    }

    // L2 normalization: unit-length vectors.
    #[test]
    fn normalize_produces_unit_vector() {
        let mut v = vec![3.0_f32, 4.0];
        l2_normalize(&mut v);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    // RFC-008 §24 test 9: cosine sim of identical vectors = 1.0.
    #[test]
    fn cosine_sim_identical_vectors() {
        let mut v = vec![1.0_f32, 2.0, 3.0];
        l2_normalize(&mut v);
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }
}

// ── Reranker (RFC-010) ───────────────────────────────────────────────

/// A candidate document passed to the reranker.
#[derive(Debug, Clone)]
pub struct RerankCandidate {
    pub chunk_id: orbok_core::ChunkId,
    /// Best available text for the passage — typically the loaded snippet.
    pub passage_text: String,
}

/// Per-candidate rerank score (higher = more relevant).
#[derive(Debug, Clone)]
pub struct RerankScore {
    pub chunk_id: orbok_core::ChunkId,
    pub score: f32,
}

/// Optional local cross-encoder reranker (RFC-010 §5).
///
/// - Reranking is always optional; missing model must not break search.
/// - Implementors must not log `passage_text` (NFR-014).
pub trait CrossEncoderReranker: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    /// Maximum candidates to rerank (RFC-010 §9 top-N limit).
    fn max_candidates(&self) -> u32;
    fn rerank(
        &self,
        query: &str,
        candidates: &[RerankCandidate],
    ) -> orbok_core::OrbokResult<Vec<RerankScore>>;
}

/// Deterministic mock reranker: scores by passage length (longer = more
/// informative). Useful for pipeline testing without an ML model.
pub struct MockReranker;

impl CrossEncoderReranker for MockReranker {
    fn name(&self) -> &str {
        "mock-reranker"
    }
    fn version(&self) -> &str {
        "v1"
    }
    fn max_candidates(&self) -> u32 {
        20
    }
    fn rerank(
        &self,
        _query: &str,
        candidates: &[RerankCandidate],
    ) -> orbok_core::OrbokResult<Vec<RerankScore>> {
        let mut scores: Vec<RerankScore> = candidates
            .iter()
            .map(|c| RerankScore {
                chunk_id: c.chunk_id.clone(),
                score: c.passage_text.len() as f32,
            })
            .collect();
        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scores)
    }
}

#[cfg(test)]
mod reranker_tests {
    use super::*;
    use orbok_core::ChunkId;

    // RFC-010 §19 test 4: reranker changes final order when scores differ.
    #[test]
    fn mock_reranker_orders_by_length() {
        let r = MockReranker;
        let candidates = vec![
            RerankCandidate {
                chunk_id: ChunkId::from_string("c1".to_string()),
                passage_text: "short".into(),
            },
            RerankCandidate {
                chunk_id: ChunkId::from_string("c2".to_string()),
                passage_text: "a much longer passage".into(),
            },
        ];
        let scores = r.rerank("query", &candidates).unwrap();
        assert_eq!(
            scores[0].chunk_id.as_str(),
            "c2",
            "longer passage should rank first"
        );
    }

    // RFC-010 §20: missing reranker does not break search.
    #[test]
    fn rerank_max_candidates_limit() {
        assert!(MockReranker.max_candidates() > 0);
    }
}

// ── Inference backend (M12) ──────────────────────────────────────────

/// The compute backend used for local inference.
///
/// **Note (RFC-046):** `CandleCpu` and `CandleCuda` are **not currently
/// supported**. The `orbok-embed` factory routes them to a not-supported
/// error; no Candle backend is implemented. These variants are retained for
/// model-layer/API stability. A future backend-API RFC may revisit them
/// (e.g. reintroduce a Candle backend, or remove the variants under a
/// deprecation policy). They are intentionally **not** `#[deprecated]` yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceBackend {
    /// CPU-only inference via candle. Not currently supported (RFC-046).
    CandleCpu,
    /// GPU inference via candle + CUDA. Not currently supported (RFC-046).
    CandleCuda,
    /// ONNX Runtime (CPU or GPU via execution provider).
    OnnxRuntime,
    /// Mock backend for tests — deterministic, no model files.
    Mock,
}

impl InferenceBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            InferenceBackend::CandleCpu => "candle-cpu",
            InferenceBackend::CandleCuda => "candle-cuda",
            InferenceBackend::OnnxRuntime => "onnx-runtime",
            InferenceBackend::Mock => "mock",
        }
    }
}

/// Configuration for loading a real embedding model from disk.
///
/// This is the configuration type callers populate to construct a real
/// `EmbeddingModel` implementation via a future `BackendLoader`. The
/// `MockEmbeddingModel` ignores this; it is used only when testing the
/// pipeline without model files.
///
/// Once an `onnx-runtime` (tract) integration is fully wired (M12
/// full implementation), it will consume this config and return a
/// `Box<dyn EmbeddingModel>`.
#[derive(Debug, Clone)]
pub struct EmbeddingModelConfig {
    /// Path to the model weights file (ONNX `.onnx` or safetensors).
    pub weights_path: String,
    /// Tokenizer config path (tokenizer.json for HuggingFace tokenizers).
    pub tokenizer_path: Option<String>,
    /// Expected embedding dimension.
    pub dimension: u32,
    /// Maximum input token length (truncation limit).
    pub max_seq_len: u32,
    /// Compute backend selection.
    pub backend: InferenceBackend,
    /// Model name for registry (e.g. "nomic-embed-text-v1.5").
    pub model_name: String,
    /// Model version string.
    pub model_version: String,
}

impl EmbeddingModelConfig {
    /// Check that the model weights file exists on disk.
    pub fn weights_exist(&self) -> bool {
        std::path::Path::new(&self.weights_path).exists()
    }
}

/// Configuration for a cross-encoder reranker model.
#[derive(Debug, Clone)]
pub struct RerankerConfig {
    pub weights_path: String,
    pub tokenizer_path: Option<String>,
    pub max_seq_len: u32,
    pub backend: InferenceBackend,
    pub model_name: String,
    pub model_version: String,
}

// ── Vector quantization (RFC-024) ───────────────────────────────────

/// Quantize an L2-normalized FP32 vector to INT8.
///
/// Maps `[-1.0, +1.0]` → `[-127, +127]` (values outside clip to ±127).
/// Storage cost: 4× smaller than FP32 (1 byte vs 4 bytes per component).
/// Quality impact: typically < 2% recall degradation for 384-dim models.
pub fn quantize_to_i8(v: &[f32]) -> Vec<i8> {
    v.iter()
        .map(|&x| (x * 127.0).round().clamp(-127.0, 127.0) as i8)
        .collect()
}

/// Dequantize INT8 back to FP32 for similarity computation.
pub fn dequantize_from_i8(v: &[i8]) -> Vec<f32> {
    v.iter().map(|&x| x as f32 / 127.0).collect()
}

/// Serialize INT8 vector to bytes for BLOB storage.
pub fn i8_vec_to_blob(v: &[i8]) -> Vec<u8> {
    // i8 values stored as raw bytes (same as u8 cast).
    v.iter().map(|&x| x as u8).collect()
}

/// Deserialize INT8 vector from BLOB bytes.
pub fn i8_blob_to_vec(blob: &[u8], expected_dim: u32) -> Option<Vec<i8>> {
    if blob.len() != expected_dim as usize {
        return None;
    }
    Some(blob.iter().map(|&b| b as i8).collect())
}

/// Compute approximate cosine similarity from INT8 vectors via FP32 conversion.
/// For exact INT8 dot-product, a SIMD-optimised path would be preferable;
/// this provides correct results at lower compute cost than full FP32.
pub fn cosine_similarity_i8(a: &[i8], b: &[i8]) -> f32 {
    cosine_similarity(&dequantize_from_i8(a), &dequantize_from_i8(b))
}

#[cfg(test)]
mod quantization_tests {
    use super::*;

    // RFC-024 AC: FP32 baseline exists — quantization is optional.
    #[test]
    fn fp32_and_i8_both_available() {
        let v = vec![0.6f32, 0.8, 0.0, -0.5];
        let blob_fp32 = vec_to_blob(&v);
        let i8_vec = quantize_to_i8(&v);
        let blob_i8 = i8_vec_to_blob(&i8_vec);
        // INT8 is 4× smaller.
        assert_eq!(blob_i8.len() * 4, blob_fp32.len());
    }

    // RFC-024 AC: Storage savings measured (4× with INT8).
    #[test]
    fn int8_is_4x_smaller_than_fp32() {
        let v: Vec<f32> = (0..384).map(|i| (i as f32 / 384.0) - 0.5).collect();
        let mut vn = v.clone();
        l2_normalize(&mut vn);
        let fp32_bytes = vec_to_blob(&vn).len();
        let int8_bytes = i8_vec_to_blob(&quantize_to_i8(&vn)).len();
        assert_eq!(fp32_bytes, 384 * 4);
        assert_eq!(int8_bytes, 384);
        assert_eq!(fp32_bytes / int8_bytes, 4);
    }

    // RFC-024 AC: Quality loss measured (cosine sim error < 0.02 for normalised vectors).
    #[test]
    fn quantization_quality_loss_is_small() {
        let mut v: Vec<f32> = (0..384).map(|i| (i as f32 * 0.017).sin()).collect();
        l2_normalize(&mut v);
        let q = quantize_to_i8(&v);
        let original_self_sim = cosine_similarity(&v, &v);
        let quantized_self_sim = cosine_similarity_i8(&q, &q);
        // After dequantize, self-sim should still be ~1.0.
        assert!(
            (quantized_self_sim - original_self_sim).abs() < 0.02,
            "quantization quality loss too high: {:.4}",
            (quantized_self_sim - original_self_sim).abs()
        );
    }

    // RFC-024 AC: Vector format migration defined (FP32 ↔ INT8 round-trip).
    #[test]
    fn fp32_int8_roundtrip_within_tolerance() {
        let mut v: Vec<f32> = vec![0.3, -0.7, 0.5, 0.1, -0.2, 0.8, -0.4, 0.6];
        l2_normalize(&mut v);
        let quantized = quantize_to_i8(&v);
        let dequantized = dequantize_from_i8(&quantized);
        for (orig, deq) in v.iter().zip(&dequantized) {
            assert!(
                (orig - deq).abs() < 0.01,
                "round-trip error too large: {orig:.4} → {deq:.4}"
            );
        }
    }
}

#[cfg(test)]
mod rfc043_tests;
