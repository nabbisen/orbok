//! Crash recovery (RFC-018): detects and repairs interrupted state
//! left by a previous session that terminated abnormally.
//!
//! Called at startup before any work begins. All repairs are non-destructive:
//! running jobs are reset to queued (not deleted), and the previous active
//! index is preserved (RFC-006 §12 replace-on-success guarantee).

use orbok_core::{OrbokResult, now_iso8601};
use orbok_db::Catalog;
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
    };
    if report.jobs_reset > 0 {
        tracing::warn!(
            reset = report.jobs_reset,
            "reset interrupted jobs to queued on startup"
        );
    }
    Ok(report)
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
