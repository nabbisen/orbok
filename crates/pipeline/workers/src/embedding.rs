//! Embedding worker (RFC-008 §14): reads chunk normalized text from the
//! extraction cache, embeds it in batches, and stores vectors in the
//! catalog. Chunk text is consumed and not logged (NFR-014).

use orbok_cache::{CacheService, EngineOptions, OrbokCacheNamespace};
use orbok_core::{FileId, ModelId, OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::ChunkRecord;
use orbok_db::repo::{
    ChunkRepository, EmbeddingRepository, FileRepository, NewEmbedding, SourceRepository,
};
use orbok_extract::ExtractOutput;
use orbok_fs::{GuardedSource, PathGuard};
use orbok_models::{EmbeddingModel, MockEmbeddingModel};
use std::path::Path;

/// One file's active chunks and their built embedding texts, ready to
/// hand to `embed_batch`/`embed_batch_with_stats`. Shared between `run`
/// and `run_with_stats` so both call `embed_batch*` on the exact same
/// batch, built exactly once.
struct PreparedBatch {
    chunks: Vec<ChunkRecord>,
    texts: Vec<String>,
}

impl PreparedBatch {
    fn text_refs(&self) -> Vec<&str> {
        self.texts.iter().map(|text| text.as_str()).collect()
    }
}

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
        let Some(batch) = self.prepare_batch(file_id)? else {
            return Ok(());
        };
        let vectors = self
            .model
            .embed_batch(&batch.text_refs())
            .map_err(categorize_inference_error)?;
        self.persist(&batch, vectors)
            .map_err(categorize_write_error)
    }

    /// `run`, plus this file's embedding batch statistics (RFC-048 Task
    /// 011) — `None` when there was nothing to embed (no fresh extraction
    /// cache yet, or no active chunks), matching `run`'s own early-return
    /// cases. Shares `prepare_batch`/`persist` with `run` so the batching
    /// itself is identical either way; the only difference is calling
    /// `embed_batch_with_stats` instead of `embed_batch`, which costs
    /// nothing extra for `run`'s plain path (RFC-048 Task 011 §4 --
    /// `TractEmbeddingModel::embed_batch` still skips the stats
    /// computation entirely).
    pub fn run_with_stats(
        &self,
        file_id: &FileId,
    ) -> OrbokResult<Option<orbok_models::EmbeddingBatchStats>> {
        let Some(batch) = self.prepare_batch(file_id)? else {
            return Ok(None);
        };
        let (vectors, stats) = self
            .model
            .embed_batch_with_stats(&batch.text_refs())
            .map_err(categorize_inference_error)?;
        self.persist(&batch, vectors)
            .map_err(categorize_write_error)?;
        Ok(stats)
    }

    /// Fetch a file's fresh extraction output and active chunks, and build
    /// the per-chunk embedding texts. `None` for either of `run`'s two
    /// legitimate skip cases (no fresh extraction cache yet, or no active
    /// chunks) -- shared so both `run` and `run_with_stats` skip on
    /// exactly the same conditions.
    fn prepare_batch(&self, file_id: &FileId) -> OrbokResult<Option<PreparedBatch>> {
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
            return Ok(None); // No extraction cache yet — skip (will retry later).
        };

        // Get active chunks for this file.
        let chunks = ChunkRepository::new(self.catalog).list_for_file(file_id)?;
        if chunks.is_empty() {
            return Ok(None);
        }

        // Build chunk texts: each chunk's own text (RFC-006 §7.2/§17.2 --
        // "child chunk by default"), reconstructed from the extraction
        // segments whose line range overlaps the chunk's own span, plus
        // optional compact heading context. Not the whole document for
        // every chunk (Task 012 Part A) -- that made every chunk of a
        // document cosine-near-identical, since vector search cannot
        // discriminate which section matches when every candidate carries
        // the same text.
        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| {
                let text = chunk_text(chunk, &extract_output);
                match &chunk.heading_path {
                    Some(heading) => format!("{heading}\n{text}"),
                    None => text,
                }
            })
            .collect();

        Ok(Some(PreparedBatch { chunks, texts }))
    }

    fn persist(&self, batch: &PreparedBatch, vectors: Vec<Vec<f32>>) -> OrbokResult<()> {
        let embeddings = EmbeddingRepository::new(self.catalog);
        for (chunk, vector) in batch.chunks.iter().zip(vectors) {
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

/// Wrap a model's `embed_batch`/`embed_batch_with_stats` failure as RFC-008
/// §15's `inference_error` category (RFC-036 §20.1), unless the model
/// already returned a categorized `OrbokError::Embedding` itself -- a
/// future backend with finer-grained detection (e.g. a real `out_of_memory`
/// signal) must not be overwritten here. Every current backend
/// (`MockEmbeddingModel`, `TractEmbeddingModel`) returns uncategorized
/// errors today, so this is the only place that classification happens in
/// practice.
fn categorize_inference_error(error: OrbokError) -> OrbokError {
    match error {
        already @ OrbokError::Embedding { .. } => already,
        other => OrbokError::Embedding {
            category: "inference_error",
            message: other.to_string(),
        },
    }
}

/// Wrap a vector-persist failure as RFC-008 §15's `write_error` category --
/// the `WritingVector` phase of the job lifecycle (RFC-008 §15), distinct
/// from the `Embedding` phase `categorize_inference_error` covers.
fn categorize_write_error(error: OrbokError) -> OrbokError {
    match error {
        already @ OrbokError::Embedding { .. } => already,
        other => OrbokError::Embedding {
            category: "write_error",
            message: other.to_string(),
        },
    }
}

/// A chunk's own text, reconstructed from the extraction segments whose
/// line range overlaps the chunk's own `[line_start, line_end]` (RFC-006
/// Task 012 Part A). Chunk text is not itself persisted -- contentless
/// FTS indexing discards it (`ChunkSpec::normalized_text`'s own doc
/// comment, RFC-007 §8.1) -- so this is how the text the chunker
/// originally computed for this chunk (`orbok_extract::chunker::chunk`)
/// is reconstructed later, at embedding time, from the two things that
/// *are* persisted: the chunk's line range and the cached extraction
/// segments.
///
/// Overlap, not containment: a fallback/windowed chunk's line range is a
/// fractional interpolation within its section's real segment boundaries
/// (`append_text_windows`), not itself a real segment boundary, so no
/// segment is ever fully *contained* in a narrow window -- containment
/// would return empty text for every windowed chunk. Overlap always
/// includes at least the segment(s) the window was cut from.
fn chunk_text(chunk: &ChunkRecord, extract_output: &ExtractOutput) -> String {
    extract_output
        .segments
        .iter()
        .filter(|segment| {
            segment.line_start <= chunk.line_end && segment.line_end >= chunk.line_start
        })
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
