//! Embedding worker (RFC-008 §14): reads chunk normalized text from the
//! extraction cache, embeds it in batches, and stores vectors in the
//! catalog. Chunk text is consumed and not logged (NFR-014).

use orbok_cache::{CacheService, EngineOptions, OrbokCacheNamespace};
use orbok_core::{FileId, ModelId, OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::{
    ChunkRepository, EmbeddingRepository, FileRepository, NewEmbedding, SourceRepository,
};
use orbok_extract::ExtractOutput;
use orbok_fs::{GuardedSource, PathGuard};
use orbok_models::{EmbeddingModel, MockEmbeddingModel};
use std::path::Path;

/// Embedding worker for one file.
pub struct EmbeddingWorker<'a> {
    catalog: &'a Catalog,
    cache: &'a CacheService,
    model: Box<dyn EmbeddingModel>,
    model_id: ModelId,
}

impl<'a> EmbeddingWorker<'a> {
    /// Use the mock model (tests, or when no real model is installed).
    pub fn with_mock(catalog: &'a Catalog, cache: &'a CacheService) -> Self {
        Self {
            catalog,
            cache,
            model: Box::new(MockEmbeddingModel),
            model_id: ModelId::from_string("mock_mock-v1".to_string()),
        }
    }

    /// Use a specific embedding model (real or mock).
    /// Supply a stable `model_id` string for registry lookup
    /// (e.g. `"mock_mock-v1"` or `"embedding_multilingual-e5-small-v1"`).
    pub fn with_model(
        catalog: &'a Catalog,
        cache: &'a CacheService,
        model: Box<dyn EmbeddingModel>,
        model_id: ModelId,
    ) -> Self {
        Self {
            catalog,
            cache,
            model,
            model_id,
        }
    }

    /// Embed all active chunks of a file and persist vectors.
    pub fn run(&self, file_id: &FileId) -> OrbokResult<()> {
        let files = FileRepository::new(self.catalog);
        let record = files.get_by_id(file_id)?.ok_or(OrbokError::FileNotFound)?;
        let sources = SourceRepository::new(self.catalog);
        let source = sources
            .get(&record.source_id)?
            .ok_or(OrbokError::SourceNotFound)?;

        // Re-use the extraction cache to get chunk texts (contentless FTS
        // stores no text; cache is the source for embedding text, Appendix A §9.3).
        let guard = PathGuard::new(vec![GuardedSource::from_record(&source)]);
        let validated = guard.validate(Path::new(&record.canonical_path))?;
        let engine = self.cache.engine::<ExtractOutput>(
            self.catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )?;
        let Some(extract_output) = CacheService::get_fresh(&engine, &validated)? else {
            return Ok(()); // No extraction cache yet — skip (will retry later).
        };

        // Get active chunks for this file.
        let chunks = ChunkRepository::new(self.catalog).list_for_file(file_id)?;
        if chunks.is_empty() {
            return Ok(());
        }

        // Build chunk texts: combine heading + normalized text from extraction
        // segments aligned to the chunk line range. For now, use the full
        // document text for the parent chunk and per-section text for children.
        let all_text: String = extract_output
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| {
                if let Some(heading) = &chunk.heading_path {
                    format!("{heading}\n{all_text}")
                } else {
                    all_text.clone()
                }
            })
            .collect();

        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let vectors = self.model.embed_batch(&text_refs)?;

        let embeddings = EmbeddingRepository::new(self.catalog);
        for (chunk, vector) in chunks.iter().zip(vectors) {
            embeddings.upsert(&NewEmbedding {
                chunk_id: chunk.chunk_id.clone(),
                model_id: self.model_id.clone(),
                dimension: self.model.dimension(),
                vector,
            })?;
        }
        Ok(())
    }

    pub fn model_id(&self) -> &ModelId {
        &self.model_id
    }

    pub fn model(&self) -> &dyn EmbeddingModel {
        self.model.as_ref()
    }
}
