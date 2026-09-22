//! Crash recovery (RFC-018): detects and repairs interrupted state
//! left by a previous session that terminated abnormally.
//!
//! Called at startup before any work begins. All repairs are non-destructive:
//! running jobs are reset to queued (not deleted), and the previous active
//! index is preserved (RFC-006 §12 replace-on-success guarantee).

use orbok_core::{FileId, OrbokResult, now_iso8601};
use orbok_db::Catalog;
use orbok_db::repo::IndexJobRepository;
use std::path::Path;

/// Results of the startup recovery scan (RFC-018 §16 requirements).
#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Jobs that were `running` and reset to `queued`.
    pub jobs_reset: u64,
    /// Jobs already `queued` from a prior session (still pending).
    pub jobs_pending: u64,
    /// Whether the cache DB was missing and recreated (empty).
    pub cache_recreated: bool,
    /// Whether the cache DB was detected as corrupt and rebuilt.
    pub cache_rebuilt: bool,
    /// Task 078: `discovered` files with no queued or running extract/chunk
    /// job, re-queued for extraction.
    pub jobs_requeued_discovered: u64,
}

/// Run all startup recovery steps.
///
/// Must be called before any worker processes jobs or any search is run.
pub fn run_startup_recovery(
    catalog: &Catalog,
    cache_db_path: &Path,
) -> OrbokResult<RecoveryReport> {
    let cache_status = ensure_cache_db(cache_db_path)?;
    let report = RecoveryReport {
        jobs_reset: reset_interrupted_jobs(catalog)?,
        jobs_pending: count_pending_jobs(catalog)?,
        cache_recreated: cache_status == CacheDbStatus::Recreated,
        cache_rebuilt: cache_status == CacheDbStatus::Rebuilt,
        jobs_requeued_discovered: requeue_orphaned_discovered_files(catalog)?,
    };
    if report.jobs_reset > 0 {
        tracing::warn!(
            reset = report.jobs_reset,
            "reset interrupted jobs to queued on startup"
        );
    }
    if report.jobs_requeued_discovered > 0 {
        tracing::info!(
            requeued = report.jobs_requeued_discovered,
            "requeued extraction for discovered files left without a pending job"
        );
    }
    Ok(report)
}

/// Task 078: a file can sit in `discovered` with no queued or running
/// extract/chunk job forever, since nothing else revisits it --
/// `Scanner::scan` enqueues `Extract` only for a file that is new or whose
/// content hash changed (RFC-004 §9.1/§9.2), so an unmodified file's folder
/// check leaves it alone, and `reset_interrupted_jobs` above only touches
/// `running` jobs. A file could reach this state through a defect since
/// fixed (Task 077) or by exhausting `MAX_JOB_ATTEMPTS` on a file that can
/// never be extracted (a corrupt document).
///
/// Every `discovered` file is selected, then queued through
/// [`IndexJobRepository::enqueue_extraction_if_idle`], which re-checks
/// idleness for that one file inside its own transaction rather than
/// trusting this snapshot -- so a file that already has a job (the common
/// case: mid-scan, or one this same call already queued) is never
/// duplicated, and running this twice queues nothing the second time.
/// Returns the number of files queued.
fn requeue_orphaned_discovered_files(catalog: &Catalog) -> OrbokResult<u64> {
    let file_ids: Vec<FileId> = {
        let conn = catalog.lock();
        let mut stmt = conn
            .prepare("SELECT file_id FROM files WHERE file_status = 'discovered'")
            .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?
            .into_iter()
            .map(FileId::from_string)
            .collect()
    };
    let jobs = IndexJobRepository::new(catalog);
    let mut queued = 0u64;
    for file_id in &file_ids {
        if jobs.enqueue_extraction_if_idle(file_id)? {
            queued += 1;
        }
    }
    Ok(queued)
}

/// RFC-018 §16 test 1: any job left in `running` state from a previous
/// session is reset to `queued` so workers will retry it.
fn reset_interrupted_jobs(catalog: &Catalog) -> OrbokResult<u64> {
    let conn = catalog.lock();
    let n = conn
        .execute(
            "UPDATE index_jobs SET status = 'queued', updated_at = ?1 WHERE status = 'running'",
            rusqlite::params![now_iso8601()],
        )
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
    Ok(n as u64)
}

fn count_pending_jobs(catalog: &Catalog) -> OrbokResult<u64> {
    let conn = catalog.lock();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE status = 'queued'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
    Ok(n as u64)
}

#[derive(PartialEq)]
enum CacheDbStatus {
    Ok,
    Recreated,
    Rebuilt,
}

/// RFC-018 §16 test 3/4: ensure the cache DB is accessible.
/// Missing → recreate empty. Corrupt → back up and recreate.
fn ensure_cache_db(path: &Path) -> OrbokResult<CacheDbStatus> {
    if !path.exists() {
        // Missing: localcache will create it on first open; nothing to do.
        return Ok(CacheDbStatus::Recreated);
    }
    // Integrity probe: open and run `PRAGMA integrity_check`.
    match rusqlite::Connection::open(path) {
        Ok(conn) => {
            let result: String = conn
                .query_row("PRAGMA integrity_check", [], |r| r.get(0))
                .unwrap_or_else(|_| "error".to_string());
            if result != "ok" {
                tracing::error!(path = %path.display(), "cache DB corrupt — backing up and removing");
                let backup = path.with_extension("sqlite3.corrupt-backup");
                let _ = std::fs::rename(path, &backup);
                return Ok(CacheDbStatus::Rebuilt);
            }
        }
        Err(e) => {
            tracing::error!(path = %path.display(), error = %e, "cache DB unreadable");
            let backup = path.with_extension("sqlite3.corrupt-backup");
            let _ = std::fs::rename(path, &backup);
            return Ok(CacheDbStatus::Rebuilt);
        }
    }
    Ok(CacheDbStatus::Ok)
}

/// Catalog integrity report (RFC-018 §16 test 7).
#[derive(Debug, Default)]
pub struct IntegrityReport {
    /// Chunks whose parent chunk no longer exists.
    pub orphaned_child_chunks: u64,
    /// Keyword index records without a matching chunk.
    pub orphaned_kw_records: u64,
    /// Embedding records without a matching chunk.
    pub orphaned_embedding_records: u64,
    /// Files without a parent source.
    pub orphaned_files: u64,
}

impl IntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.orphaned_child_chunks == 0
            && self.orphaned_kw_records == 0
            && self.orphaned_embedding_records == 0
            && self.orphaned_files == 0
    }
}

/// Run catalog integrity checks (RFC-018 §16 test 7).
/// Read-only — does not repair, only reports.
pub fn check_catalog_integrity(catalog: &Catalog) -> OrbokResult<IntegrityReport> {
    let conn = catalog.lock();
    let q = |sql: &str| -> OrbokResult<u64> {
        let n: i64 = conn
            .query_row(sql, [], |r| r.get(0))
            .map_err(|e| orbok_core::OrbokError::Database(e.to_string()))?;
        Ok(n as u64)
    };
    Ok(IntegrityReport {
        orphaned_child_chunks: q("SELECT COUNT(*) FROM chunks c \
             WHERE c.parent_chunk_id IS NOT NULL \
             AND NOT EXISTS (SELECT 1 FROM chunks p WHERE p.chunk_id = c.parent_chunk_id)")?,
        orphaned_kw_records: q("SELECT COUNT(*) FROM keyword_index_records k \
             WHERE NOT EXISTS (SELECT 1 FROM chunks c WHERE c.chunk_id = k.chunk_id)")?,
        orphaned_embedding_records: q("SELECT COUNT(*) FROM embeddings e \
             WHERE NOT EXISTS (SELECT 1 FROM chunks c WHERE c.chunk_id = e.chunk_id)")?,
        orphaned_files: q("SELECT COUNT(*) FROM files f \
             WHERE NOT EXISTS (SELECT 1 FROM sources s WHERE s.source_id = f.source_id)")?,
    })
}
