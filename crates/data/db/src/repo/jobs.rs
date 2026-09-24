//! Index job queue repository (RFC-002 §7.9, RFC-004 §13).

use crate::catalog::{Catalog, db_err};
use orbok_core::{FileId, JobId, JobStatus, JobType, ModelId, OrbokResult, SourceId, now_iso8601};
use rusqlite::{OptionalExtension, params};

/// A queued or running index job.
#[derive(Debug, Clone)]
pub struct JobRecord {
    pub job_id: JobId,
    pub source_id: Option<SourceId>,
    pub file_id: Option<FileId>,
    pub job_type: JobType,
    pub status: JobStatus,
}

/// Task 055: files with active chunks lacking an active embedding under
/// `?1`, and no unfinished embedding job.
///
/// The unary `+` on the status and model columns is load-bearing. Without it
/// SQLite (with no ANALYZE data) looks chunks and embeddings up through their
/// status indexes, where nearly every row matches, and the query goes
/// quadratic: 10.5 s at 20,000 chunks when every chunk is already embedded,
/// which is every ordinary startup. With it, both lookups use
/// `idx_chunks_file_id` and `idx_embeddings_chunk_id` (8.7 ms). Measured by
/// `tests::task055_backfill_cost`; the plan is pinned by
/// `embedding_backfill_uses_the_file_and_chunk_indexes`.
pub(crate) const EMBEDDING_BACKFILL_FILES_SQL: &str = "SELECT f.file_id, f.source_id FROM files f \
     WHERE EXISTS (SELECT 1 FROM chunks c \
        WHERE c.file_id = f.file_id AND +c.chunk_status = 'active' \
        AND NOT EXISTS (SELECT 1 FROM embeddings e \
            WHERE e.chunk_id = c.chunk_id AND +e.model_id = ?1 \
            AND +e.status = 'active')) \
     AND NOT EXISTS (SELECT 1 FROM index_jobs j \
        WHERE j.file_id = f.file_id AND j.job_type = 'embedding' \
        AND j.status IN ('queued', 'running', 'paused', 'blocked', \
                         'waiting_for_dependency'))";

/// Task 099: files with an active chunk and no unfinished extract/chunk
/// job -- the candidates for a keyword-index rebuild. `chunk_fts` is
/// contentless (RFC-007 §8.1: "stores no retrievable source text"), and
/// `normalized_text` is never persisted anywhere else in the catalog
/// (`ChunkSpec`'s own doc comment), so once `keyword_index_records`/
/// `chunk_fts`/`chunk_fts_trigram` are deleted, re-extraction is the only
/// way to regenerate the text those tables need -- `ChunkRepository::insert_bundle`
/// rebuilds the keyword index as a byproduct of chunking a fresh
/// extraction, the same path every ordinary index run already takes. Same
/// `+` prefix trick as `EMBEDDING_BACKFILL_FILES_SQL`, for the same reason
/// (Task 055's own comment on that constant): without it this goes
/// quadratic on an already-fully-indexed catalog, which is every ordinary
/// call here.
pub(crate) const EXTRACTION_BACKFILL_FILES_SQL: &str = "SELECT f.file_id, f.source_id FROM files f \
     WHERE EXISTS (SELECT 1 FROM chunks c WHERE c.file_id = f.file_id AND +c.chunk_status = 'active') \
     AND NOT EXISTS (SELECT 1 FROM index_jobs j \
        WHERE j.file_id = f.file_id AND j.job_type IN ('extract', 'chunk') \
        AND j.status IN ('queued', 'running', 'paused', 'blocked', \
                         'waiting_for_dependency'))";

pub struct IndexJobRepository<'a> {
    catalog: &'a Catalog,
}

impl<'a> IndexJobRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Enqueue a job (scanner queues `extract` for new/stale files,
    /// RFC-004 §13).
    pub fn enqueue(
        &self,
        job_type: JobType,
        source_id: Option<&SourceId>,
        file_id: Option<&FileId>,
    ) -> OrbokResult<JobId> {
        let id = JobId::generate();
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, \
             created_at, updated_at) VALUES (?1,?2,?3,?4,'queued',?5,?5)",
            params![
                id.as_str(),
                source_id.map(|s| s.as_str()),
                file_id.map(|f| f.as_str()),
                job_type.as_str(),
                now,
            ],
        )
        .map_err(db_err)?;
        Ok(id)
    }

    /// Task 055: queue one embedding job for every file that has active
    /// chunks with no active embedding under `model_id` -- documents indexed
    /// before a model existed, whose own embedding job failed terminally as
    /// `model_missing` and which nothing else ever re-queues (the scanner
    /// only re-queues files whose content changed).
    ///
    /// Idempotent: a file that already has an unfinished embedding job
    /// (queued, running, paused, blocked or waiting) is skipped, so a second
    /// call enqueues nothing. Old `failed` rows are left as they are; the new
    /// job supersedes them. No priority is written, so the scheduler loads
    /// these at the embedding kind's own default priority, as it does every
    /// other embedding job. Returns how many jobs were queued.
    pub fn enqueue_embedding_backfill(&self, model_id: &ModelId) -> OrbokResult<usize> {
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;
        let files: Vec<(String, String)> = {
            let mut stmt = tx.prepare(EMBEDDING_BACKFILL_FILES_SQL).map_err(db_err)?;
            stmt.query_map(params![model_id.as_str()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(db_err)?
            .collect::<Result<_, _>>()
            .map_err(db_err)?
        };
        let now = now_iso8601();
        for (file_id, source_id) in &files {
            tx.execute(
                "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, \
                 created_at, updated_at) VALUES (?1,?2,?3,?4,'queued',?5,?5)",
                params![
                    JobId::generate().as_str(),
                    source_id,
                    file_id,
                    JobType::Embedding.as_str(),
                    now,
                ],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(files.len())
    }

    /// Task 099: queue one `Extract` job for every file matching
    /// [`EXTRACTION_BACKFILL_FILES_SQL`] -- the state change that makes a
    /// keyword-index rebuild happen: `CleanupExecutor::delete_keyword_index`
    /// calls this right after deleting the index tables, and the scheduler
    /// picks the queued jobs up the same as any other. Idempotent, the same
    /// way [`Self::enqueue_embedding_backfill`] is: a file with an unfinished
    /// extract/chunk job already queued is skipped, so a second call queues
    /// nothing new. Returns how many jobs were queued.
    pub fn enqueue_extraction_backfill(&self) -> OrbokResult<usize> {
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;
        let files: Vec<(String, String)> = {
            let mut stmt = tx.prepare(EXTRACTION_BACKFILL_FILES_SQL).map_err(db_err)?;
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(db_err)?
                .collect::<Result<_, _>>()
                .map_err(db_err)?
        };
        let now = now_iso8601();
        for (file_id, source_id) in &files {
            tx.execute(
                "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, \
                 created_at, updated_at) VALUES (?1,?2,?3,?4,'queued',?5,?5)",
                params![
                    JobId::generate().as_str(),
                    source_id,
                    file_id,
                    JobType::Extract.as_str(),
                    now,
                ],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(files.len())
    }

    /// Task 099: the file ids [`Self::enqueue_extraction_backfill`] would
    /// queue right now. `orbok_workers::CleanupService` reads this
    /// *before* calling the executor (while the predicate's "no unfinished
    /// extract/chunk job" half still matches every candidate) to evict
    /// each file's extraction-cache entry first -- without that, a file
    /// whose content has not changed hits `ExtractionWorker::run`'s own
    /// freshness shortcut, which re-queues a `Chunk` job against the
    /// *same* `extraction_id` its still-active chunks already occupy,
    /// and `insert_bundle` then fails the whole rebuild on a UNIQUE
    /// constraint (`chunks(file_id, extraction_id, chunk_ordinal)`)
    /// rather than reindexing anything.
    pub fn extraction_backfill_candidate_file_ids(&self) -> OrbokResult<Vec<FileId>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(EXTRACTION_BACKFILL_FILES_SQL)
            .map_err(db_err)?;
        stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(db_err)?
            .map(|r| r.map(FileId::from_string).map_err(db_err))
            .collect()
    }

    /// Task 099: how many files [`Self::enqueue_extraction_backfill`] would
    /// queue right now -- the Storage page's "prepare keyword search again"
    /// confirmation counts this fresh when it opens, the same way
    /// [`Self::count_embedding_backfill_candidates`] and
    /// `bootstrap::get_reset_counts` count their own dialogs' lines.
    pub fn count_extraction_backfill_candidates(&self) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM ({EXTRACTION_BACKFILL_FILES_SQL})"),
                [],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// Task 099: how many files [`Self::enqueue_embedding_backfill`] would
    /// queue right now, for `model_id` -- the "prepare search by meaning
    /// again" confirmation's own counted line.
    pub fn count_embedding_backfill_candidates(&self, model_id: &ModelId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM ({EMBEDDING_BACKFILL_FILES_SQL})"),
                params![model_id.as_str()],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// Task 056: queue an `Extract` job for `file_id` so its extracted text
    /// is rebuilt -- unless an extract or chunk job for it is already
    /// unfinished (queued, running, paused, blocked or waiting), which will
    /// rebuild or need it anyway. `current_job` -- the job that found the
    /// text missing, itself `running` -- is not counted. Returns whether a
    /// job was queued. The file's source comes from the catalog; a file no
    /// longer there queues nothing.
    pub fn enqueue_extraction_unless_pending(
        &self,
        file_id: &FileId,
        current_job: &JobId,
    ) -> OrbokResult<bool> {
        self.enqueue_extraction_excluding(file_id, current_job.as_str())
    }

    /// HANDOFF-038 "Prepare again": queue an `Extract` job for `file_id`,
    /// exactly as the scanner does for a changed file, unless an extract or
    /// chunk job for it is already unfinished. No job is running on the
    /// file's behalf here, so none is excluded. Returns whether a job was
    /// queued; `false` also means the file has no row.
    pub fn enqueue_extraction_if_idle(&self, file_id: &FileId) -> OrbokResult<bool> {
        self.enqueue_extraction_excluding(file_id, "")
    }

    fn enqueue_extraction_excluding(
        &self,
        file_id: &FileId,
        excluded_job: &str,
    ) -> OrbokResult<bool> {
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;
        let source_id: Option<String> = tx
            .query_row(
                "SELECT source_id FROM files f WHERE f.file_id = ?1 \
                 AND NOT EXISTS (SELECT 1 FROM index_jobs j \
                    WHERE j.file_id = f.file_id AND j.job_id != ?2 \
                    AND j.job_type IN ('extract', 'chunk') \
                    AND j.status IN ('queued', 'running', 'paused', 'blocked', \
                                     'waiting_for_dependency'))",
                params![file_id.as_str(), excluded_job],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        let Some(source_id) = source_id else {
            return Ok(false);
        };
        let now = now_iso8601();
        tx.execute(
            "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, \
             created_at, updated_at) VALUES (?1,?2,?3,?4,'queued',?5,?5)",
            params![
                JobId::generate().as_str(),
                source_id,
                file_id.as_str(),
                JobType::Extract.as_str(),
                now,
            ],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(true)
    }

    /// Move a job to a new status, recording start/completion times.
    pub fn set_status(&self, id: &JobId, status: JobStatus) -> OrbokResult<()> {
        let now = now_iso8601();
        let (started, completed) = match status {
            JobStatus::Running => (Some(now.clone()), None),
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Canceled => {
                (None, Some(now.clone()))
            }
            _ => (None, None),
        };
        let conn = self.catalog.lock();
        conn.execute(
            "UPDATE index_jobs SET status = ?2, updated_at = ?3, \
             started_at = COALESCE(?4, started_at), \
             completed_at = COALESCE(?5, completed_at) WHERE job_id = ?1",
            params![id.as_str(), status.as_str(), now, started, completed],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Fail a job with a named category (RFC-008 §15 -- e.g. `"model_missing"`
    /// for an `Embedding` job dispatched with no embedding model configured).
    /// Distinct from `set_status(Failed)`: this is for a job the dispatcher
    /// never attempted, not one whose attempt raised an error, so the reason
    /// belongs in `error_category`/`error_message` rather than only in a log
    /// line. `error_category`/`error_message` exist in the schema (RFC-002)
    /// but nothing has written them before this.
    pub fn fail_with_category(
        &self,
        id: &JobId,
        category: &str,
        message: Option<&str>,
    ) -> OrbokResult<()> {
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "UPDATE index_jobs SET status = 'failed', error_category = ?2, \
             error_message = ?3, updated_at = ?4, \
             completed_at = COALESCE(completed_at, ?4) WHERE job_id = ?1",
            params![id.as_str(), category, message, now],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Queued jobs in priority/FIFO order.
    pub fn list_queued(&self, limit: u32) -> OrbokResult<Vec<JobRecord>> {
        self.list_by_status(JobStatus::Queued, limit)
    }

    /// Count of jobs in a given status, via `SELECT COUNT(*)` over
    /// `idx_index_jobs_status` (Task 034 §6, audit P-02) -- for a caller
    /// that only needs the count, not `list_by_status(status,
    /// u32::MAX).len()`, which materializes and sorts every matching row.
    pub fn count_with_status(&self, status: JobStatus) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM index_jobs WHERE status = ?1",
                params![status.as_str()],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// Task 108: jobs of one folder that are queued or running -- the work
    /// that makes its card say "Preparing". A blocked or failed job is not
    /// unfinished work in progress, and is left out. Every job type sets
    /// `source_id` (checked by `every_job_a_folder_creates_carries_that_folder`), so this
    /// misses none; the `idx_index_jobs_source_id` index serves the lookup.
    pub fn count_unfinished_for_source(&self, source_id: &SourceId) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM index_jobs \
                 WHERE source_id = ?1 AND status IN ('queued', 'running')",
                params![source_id.as_str()],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }

    /// `Blocked` jobs in priority/FIFO order (RFC-036 §20.2): a retry whose
    /// in-memory re-queue was skipped under backpressure, recorded honestly
    /// rather than as `queued` with no in-memory copy to match. Rehydration
    /// re-discovers these separately from `list_queued`, since a `known`
    /// job id must not gate a row that -- unlike an ordinary still-tracked
    /// `queued` row -- has no live in-memory copy by construction.
    pub fn list_blocked(&self, limit: u32) -> OrbokResult<Vec<JobRecord>> {
        self.list_by_status(JobStatus::Blocked, limit)
    }

    fn list_by_status(&self, status: JobStatus, limit: u32) -> OrbokResult<Vec<JobRecord>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT job_id, source_id, file_id, job_type, status FROM index_jobs \
                 WHERE status = ?1 ORDER BY priority DESC, created_at LIMIT ?2",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![status.as_str(), limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, src, file, jt, st) = row.map_err(db_err)?;
            out.push(JobRecord {
                job_id: JobId::from_string(id),
                source_id: src.map(SourceId::from_string),
                file_id: file.map(FileId::from_string),
                job_type: JobType::parse(&jt)?,
                status: JobStatus::parse(&st)?,
            });
        }
        Ok(out)
    }

    /// A single job's current status, or `None` if the row no longer
    /// exists (RFC-056 Slice 3: source removal cascade-deletes `index_jobs`
    /// rows via the FK on `sources`, so "gone" is an expected, not
    /// exceptional, outcome here).
    pub fn status_of(&self, id: &JobId) -> OrbokResult<Option<JobStatus>> {
        let conn = self.catalog.lock();
        conn.query_row(
            "SELECT status FROM index_jobs WHERE job_id = ?1",
            params![id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_err)?
        .map(|status| JobStatus::parse(&status))
        .transpose()
    }

    /// Count of jobs per status (Indexing view summary cards).
    pub fn count_by_status(&self) -> OrbokResult<Vec<(JobStatus, u64)>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare("SELECT status, COUNT(*) FROM index_jobs GROUP BY status")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(db_err)?;
        let mut out = Vec::new();
        for row in rows {
            let (status, count) = row.map_err(db_err)?;
            out.push((JobStatus::parse(&status)?, count as u64));
        }
        Ok(out)
    }

    /// Enqueue a job with an explicit priority (RFC-036 §8), under a
    /// caller-supplied `id` rather than generating one (unlike `enqueue`):
    /// the sole caller, `Scheduler::enqueue`, already holds an in-memory
    /// `IndexJob` with its own id and pushes that same job into its queue
    /// right after this call returns. Generating a second, different id
    /// here (the original behaviour) left the catalog row and the
    /// in-memory job permanently out of sync -- every later
    /// `Scheduler::complete`/`fail` call updates by the in-memory job's
    /// id, which would then match zero catalog rows.
    pub fn enqueue_with_priority(
        &self,
        id: &JobId,
        job_type: JobType,
        source_id: Option<&SourceId>,
        file_id: Option<&FileId>,
        priority: i64,
    ) -> OrbokResult<()> {
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO index_jobs \
             (job_id, source_id, file_id, job_type, status, priority, \
              attempt_count, created_at, updated_at) \
             VALUES (?1,?2,?3,?4,'queued',?5,0,?6,?6)",
            params![
                id.as_str(),
                source_id.map(|s| s.as_str()),
                file_id.map(|f| f.as_str()),
                job_type.as_str(),
                priority,
                now,
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Record a failed attempt and its error kind (RFC-036 §11).
    pub fn increment_attempt(&self, id: &JobId, error_kind: &str) -> OrbokResult<()> {
        let now = now_iso8601();
        let conn = self.catalog.lock();
        conn.execute(
            "UPDATE index_jobs \
             SET attempt_count = attempt_count + 1, \
                 last_error_kind = ?2, \
                 updated_at = ?3 \
             WHERE job_id = ?1",
            params![id.as_str(), error_kind, now],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Count of files with `file_status = 'indexed'` (for partial
    /// readiness reporting, RFC-036 §14.2).
    pub fn count_indexed_files(&self) -> OrbokResult<u64> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE file_status = 'indexed'",
                [],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        Ok(n as u64)
    }
}
