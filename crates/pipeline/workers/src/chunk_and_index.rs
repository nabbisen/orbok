//! Chunk-and-index worker (RFC-006 §12): loads an extraction result
//! from the cache, chunks it, and atomically inserts chunks + FTS index
//! into the catalog (one transaction).

use crate::chunk_adapter::to_chunk_specs;
use orbok_cache::{CacheService, EngineOptions, OrbokCacheNamespace};
use orbok_core::{ErrorCategory, ExtractionId, FileId, JobType, OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::{ChunkRepository, FileRepository, IndexJobRepository, SourceRepository};
use orbok_extract::{ExtractOutput, chunk};
use orbok_fs::{GuardedSource, PathGuard};
use rusqlite::params;
use std::path::Path;

/// Chunk-and-index worker.
pub struct ChunkAndIndexWorker<'a> {
    catalog: &'a Catalog,
    cache: &'a CacheService,
}

impl<'a> ChunkAndIndexWorker<'a> {
    pub fn new(catalog: &'a Catalog, cache: &'a CacheService) -> Self {
        Self { catalog, cache }
    }

    /// Load the extraction cache for a file, chunk, and index.
    pub fn run(&self, file_id: &FileId) -> OrbokResult<()> {
        let files = FileRepository::new(self.catalog);
        let record = files.get_by_id(file_id)?.ok_or(OrbokError::FileNotFound)?;
        let sources = SourceRepository::new(self.catalog);
        let source = sources
            .get(&record.source_id)?
            .ok_or(OrbokError::SourceNotFound)?;

        let guard = PathGuard::new(vec![GuardedSource::from_record(&source)]);
        let validated = guard.validate(Path::new(&record.canonical_path))?;

        let engine = self.cache.engine::<ExtractOutput>(
            self.catalog,
            &OrbokCacheNamespace::ExtractSegments,
            EngineOptions::default(),
        )?;
        let output = CacheService::get_fresh(&engine, &validated)?.ok_or_else(|| {
            OrbokError::Extraction {
                category: ErrorCategory::ParserError,
                message: "extraction cache miss: run extraction first".into(),
            }
        })?;

        // Find the most recent succeeded extraction record for this file.
        let extraction_id = self.latest_extraction_id(file_id)?;

        let file_name = Path::new(&record.display_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| record.display_path.clone());

        let raw = chunk(&output, &file_name);
        let specs = to_chunk_specs(raw);
        if specs.is_empty() || (specs.len() == 1 && specs[0].normalized_text.is_empty()) {
            return Ok(());
        }

        ChunkRepository::new(self.catalog).insert_bundle(file_id, &extraction_id, &specs)?;

        // RFC-008 §19 "Chunk Change Handling": a new embedding job is
        // queued after rechunking. Mirrors extract.rs's JobType::Chunk
        // enqueue -- dispatcher-agnostic: both run_pending and RFC-036's
        // Scheduler consume JobType::Embedding (scheduler/job.rs:96).
        IndexJobRepository::new(self.catalog).enqueue(
            JobType::Embedding,
            Some(&record.source_id),
            Some(file_id),
        )?;
        Ok(())
    }

    fn latest_extraction_id(&self, file_id: &FileId) -> OrbokResult<ExtractionId> {
        let conn = self.catalog.lock();
        let id: String = conn
            .query_row(
                "SELECT extraction_id FROM extraction_records \
                 WHERE file_id = ?1 AND status = 'succeeded' \
                 ORDER BY completed_at DESC LIMIT 1",
                params![file_id.as_str()],
                |row| row.get(0),
            )
            .map_err(|e| OrbokError::Database(format!("no extraction record: {e}")))?;
        Ok(ExtractionId::from_string(id))
    }
}
