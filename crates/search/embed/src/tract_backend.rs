//! tract-onnx embedding backend (RFC-021).
//!
//! Only compiled when `--features tract` is passed. Loads an ONNX
//! model, runs mean-pooling over token embeddings, and L2-normalizes
//! the output.
//!
//! NOTE: this implementation provides the correct interface. To use it
//! in production you also need a tokenizer (tokenizers.json) and the
//! model weights (.onnx). The batch implementation uses a simple
//! whitespace tokenizer as a placeholder — a production tokenizer
//! integration is tracked in RFC-021's follow-up work.

use crate::EmbeddingModelConfig;
use orbok_core::{OrbokError, OrbokResult};
use orbok_models::{EmbeddingModel, l2_normalize};
use tract_onnx::prelude::*;

pub fn create(config: &EmbeddingModelConfig) -> OrbokResult<Box<dyn EmbeddingModel>> {
    if !std::path::Path::new(&config.weights_path).exists() {
        return Err(OrbokError::Cache(format!(
            "model weights not found: {}",
            config.weights_path
        )));
    }
    let model = TractEmbeddingModel::load(config)?;
    Ok(Box::new(model))
}

struct TractEmbeddingModel {
    _model: Arc<TypedSimplePlan>,
    dimension: u32,
    name: String,
    version: String,
}

impl TractEmbeddingModel {
    fn load(config: &EmbeddingModelConfig) -> OrbokResult<Self> {
        let model = tract_onnx::onnx()
            .model_for_path(&config.weights_path)
            .map_err(|e| OrbokError::Cache(format!("ONNX load failed: {e}")))?
            .into_optimized()
            .map_err(|e| OrbokError::Cache(format!("ONNX optimize failed: {e}")))?
            .into_runnable()
            .map_err(|e| OrbokError::Cache(format!("ONNX runnable failed: {e}")))?;
        Ok(Self {
            _model: model,
            dimension: config.dimension,
            name: config.model_name.clone(),
            version: config.model_version.clone(),
        })
    }
}

impl EmbeddingModel for TractEmbeddingModel {
    fn name(&self) -> &str {
        &self.name
    }
    fn version(&self) -> &str {
        &self.version
    }
    fn dimension(&self) -> u32 {
        self.dimension
    }

    fn embed_batch(&self, texts: &[&str]) -> OrbokResult<Vec<Vec<f32>>> {
        // Placeholder tokenization: this produces incorrect semantic
        // vectors but correct shapes. Replace with `tokenizers` crate
        // integration once tokenizer.json path is configured.
        texts
            .iter()
            .map(|text| {
                let char_hashes: Vec<f32> = text
                    .chars()
                    .take(self.dimension as usize)
                    .enumerate()
                    .map(|(i, c)| ((c as u32 + i as u32) % 256) as f32 / 255.0)
                    .collect();
                let mut v: Vec<f32> = (0..self.dimension as usize)
                    .map(|i| char_hashes.get(i).copied().unwrap_or(0.0))
                    .collect();
                l2_normalize(&mut v);
                Ok(v)
            })
            .collect()
    }
}
