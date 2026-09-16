//! Exact vector search (RFC-008 §13): loads all active embeddings and
//! computes cosine similarity in a single pass.
//!
//! ANN is deferred until benchmarks show exact scan is insufficient
//! (RFC-008 §26: "Correctness first").

use orbok_core::{ChunkId, FileId, OrbokResult, SearchScope};
use orbok_db::Catalog;
use orbok_db::repo::EmbeddingRepository;
use orbok_models::{VectorCandidate, cosine_similarity};

/// Exact cosine-similarity scan over all active embeddings for a model.
pub struct ExactVectorSearch<'a> {
    pub catalog: &'a Catalog,
    pub model_id: String,
    pub dimension: u32,
    /// RFC-060 §7: the same kind/folder restriction the keyword queries
    /// apply, so a scoped search cannot pick up vector candidates the
    /// keyword half excluded.
    pub scope: SearchScope,
}

impl ExactVectorSearch<'_> {
    pub fn search(&self, query_vec: &[f32], limit: u32) -> OrbokResult<Vec<VectorCandidate>> {
        if query_vec.len() != self.dimension as usize {
            return Ok(Vec::new());
        }
        let repo = EmbeddingRepository::new(self.catalog);
        let records = repo.list_active_for_scan(&self.model_id, self.dimension, &self.scope)?;

        let mut scored: Vec<(f32, ChunkId, FileId)> = records
            .into_iter()
            .filter(|r| r.vector.len() == self.dimension as usize)
            .map(|r| {
                let score = cosine_similarity(query_vec, &r.vector);
                (score, r.chunk_id, r.file_id)
            })
            .collect();

        // Descending by score.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit as usize);

        Ok(scored
            .into_iter()
            .enumerate()
            .map(|(i, (score, chunk_id, file_id))| VectorCandidate {
                chunk_id,
                file_id,
                rank: (i + 1) as u32,
                score,
            })
            .collect())
    }
}
