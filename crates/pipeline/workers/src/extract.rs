//! Extraction worker (RFC-005 §14): reads a queued file, runs the
//! extractor, stores the output in the cache, and writes an
//! extraction_record. On success, queues a Chunk job.

use orbok_cache::{CacheService, OrbokCacheNamespace};
use orbok_core::ExtractionId;
use orbok_core::{FileId, JobType, OrbokError, OrbokResult, now_iso8601};
use orbok_db::Catalog;
use orbok_db::repo::{FileRepository, IndexJobRepository, SourceRepository};
use orbok_extract::{ExtractContext, ExtractOutput, ExtractorRegistry};
use orbok_fs::{GuardedSource, PathGuard};
use std::path::Path;

/// Extraction worker instance, held for the duration of an index run.
pub struct ExtractionWorker<'a> {
    catalog: &'a Catalog,
    cache: &'a CacheService,
    registry: ExtractorRegistry,
}

impl<'a> ExtractionWorker<'a> {
    pub fn new(catalog: &'a Catalog, cache: &'a CacheService) -> Self {
        Self {
            catalog,
            cache,
            registry: ExtractorRegistry::default(),
        }
    }

    /// Run extraction for one file. Fails with a typed error on
    /// unrecoverable cases; the worker coordinator converts failures to
    /// a catalog record.
    pub fn run(&self, file_id: &FileId) -> OrbokResult<()> {
        let files = FileRepository::new(self.catalog);
        let record = files.get_by_id(file_id)?.ok_or(OrbokError::FileNotFound)?;
        let sources = SourceRepository::new(self.catalog);
        let source = sources
            .get(&record.source_id)?
            .ok_or(OrbokError::SourceNotFound)?;

        // Build a single-source path guard (RFC-003 §8 boundary).
        let guard = PathGuard::new(vec![GuardedSource::from_record(&source)]);
        let validated = guard.validate(Path::new(&record.canonical_path))?;

        // Skip if cached extraction is still fresh (Appendix A §8).
        let engine = self.cache.engine::<ExtractOutput>(
            self.catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )?;
        if CacheService::get_fresh(&engine, &validated)?.is_some() {
            // Still fresh — queue the chunk job and return.
            IndexJobRepository::new(self.catalog).enqueue(
                JobType::Chunk,
                Some(&record.source_id),
                Some(file_id),
            )?;
            return Ok(());
        }

        // Run extractor with panic isolation and resource limits (RFC-044).
        let output = self
            .registry
            .extract_safely(&validated, &ExtractContext::default())?;

        // Cache the output (Appendix A §9.1).
        CacheService::put(&engine, &validated, &output)?;

        // Record in catalog.
        let extraction_id = ExtractionId::generate();
        let now = now_iso8601();
        {
            let conn = self.catalog.lock();
            conn.execute(
                "INSERT INTO extraction_records \
                 (extraction_id, file_id, extractor_name, extractor_version, \
                  normalization_version, source_content_hash, status, \
                  extracted_char_count, extracted_byte_count, started_at, completed_at, \
                  created_at, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,'succeeded',?7,?8,?9,?9,?9,?9)",
                rusqlite::params![
                    extraction_id.as_str(),
                    file_id.as_str(),
                    output.extractor_name,
                    output.extractor_version,
                    output.normalization_version,
                    record.content_hash,
                    output.char_count as i64,
                    output.char_count as i64,
                    now,
                ],
            )
            .map_err(|e| OrbokError::Database(e.to_string()))?;
        }

        // Queue chunk job.
        IndexJobRepository::new(self.catalog).enqueue(
            JobType::Chunk,
            Some(&record.source_id),
            Some(file_id),
        )?;
        Ok(())
    }
}
