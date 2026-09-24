//! Chunk-and-index worker (RFC-006 §12): loads an extraction result
//! from the cache, chunks it, and atomically inserts chunks + FTS index
//! into the catalog (one transaction).

use crate::chunk_adapter::to_chunk_specs;
use orbok_cache::{CacheService, OrbokCacheNamespace};
use orbok_core::{ExtractionId, FileId, FileStatus, JobType, OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::{
    ChunkRepository, ExistingChunks, FileRepository, IndexJobRepository, SourceRepository,
};
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
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )?;
        // Task 056: a miss used to be `parser_error`, which retried against a
        // cache that would not come back and never re-extracted.
        let output = CacheService::get_fresh(&engine, &validated)?
            .ok_or(OrbokError::ExtractionCacheMissing)?;

        // Find the most recent succeeded extraction record for this file.
        let extraction_id = self.latest_extraction_id(file_id)?;

        let file_name = Path::new(&record.display_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| record.display_path.clone());

        let raw = chunk(&output, &file_name);
        let specs = to_chunk_specs(raw);
        if specs.is_empty() || (specs.len() == 1 && specs[0].normalized_text.is_empty()) {
            // Task 080: `chunk()` returns exactly one empty-text chunk when
            // `output.segments` is empty, and only then (`chunker.rs`'s
            // `empty_document_chunk`) -- so this branch is reached exactly
            // when extraction genuinely found no text, not when chunking
            // dropped real content. Confirmed across every extractor
            // (`orbok-extract`): pdf/html/docx derive `char_count` by
            // summing only the characters behind a pushed segment, so
            // `char_count == 0` and `segments.is_empty()` are the same
            // fact; markdown/text assign every non-blank line to some
            // segment (heading, code block or paragraph), so segments can
            // only be empty when the whole normalized document is
            // whitespace. A silent bug that lost real text without a
            // warning would need to defeat that in every extractor at
            // once, which is a different failure than "this document has
            // no text in it" and would need its own fix, not this state.
            //
            // Previously this returned Ok(()) and left the file exactly as
            // `extract` found it -- `discovered`, labelled "Waiting" --
            // forever, even though every job for it had succeeded (Review
            // Request 255 §5). It is now a finished, distinct state.
            FileRepository::new(self.catalog).set_status(file_id, FileStatus::NoTextFound)?;
            return Ok(());
        }

        // Task 102: a `Chunk` job for an extraction whose chunks already
        // exist is an ordinary event (Prepare again on a healthy file, a
        // duplicated job, a keyword rebuild over a fresh extraction cache),
        // not an error. Same chunks: write only the keyword rows they lack,
        // under the same chunk ids -- nothing that hangs off those ids
        // (embeddings) changes, so no `Embedding` job is queued.
        let chunks = ChunkRepository::new(self.catalog);
        match chunks.reuse_existing_chunks(file_id, &extraction_id, &specs)? {
            ExistingChunks::Reused { .. } => return Ok(()),
            ExistingChunks::Different => {
                // The chunker produced different chunks from the same
                // extraction. Make a new generation: evict this file's cache
                // entry so `Extract` re-extracts (a fresh `extraction_id`),
                // and queue it.
                tracing::info!(
                    file_id = file_id.as_str(),
                    "chunks differ from the stored ones: making a new generation"
                );
                CacheService::remove(&engine, &validated)?;
                IndexJobRepository::new(self.catalog).enqueue(
                    JobType::Extract,
                    Some(&record.source_id),
                    Some(file_id),
                )?;
                return Ok(());
            }
            ExistingChunks::None => {}
        }
        chunks.insert_bundle(file_id, &extraction_id, &specs)?;

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
