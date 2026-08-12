//! Index job queue repository (RFC-002 §7.9, RFC-004 §13).

use crate::catalog::{Catalog, db_err};
use orbok_core::{FileId, JobId, JobStatus, JobType, OrbokResult, SourceId, now_iso8601};
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
