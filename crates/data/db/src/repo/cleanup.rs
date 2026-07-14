//! Cleanup execution against the catalog (RFC-001 §9, RFC-011).
//!
//! Every entry point takes an [`orbok_core::CleanupPlan`]; safe (ordinary)
//! cleanup re-validates that the plan cannot touch persistent catalog
//! data before any row is deleted. Source files on disk are never
//! touched by any path in this module.

use crate::catalog::{Catalog, db_err};
use orbok_core::{CleanupAction, CleanupPlan, OrbokError, OrbokResult, now_iso8601};
use rusqlite::params;

/// Outcome of a cleanup run.
#[derive(Debug, Clone, Default)]
pub struct CleanupOutcome {
    pub deleted_rows: u64,
}

/// Executes catalog-side cleanup. Cache-engine payload cleanup is the
/// responsibility of `orbok-cache` (Appendix A §12), driven by the same
/// plan at the service layer.
pub struct CleanupExecutor<'a> {
    catalog: &'a Catalog,
}

impl<'a> CleanupExecutor<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Run a *safe* cleanup action. Rejects any plan that includes the
    /// persistent catalog class (RFC-001: "Ordinary cleanup cannot
    /// delete persistent source settings").
    pub fn run_safe(&self, plan: &CleanupPlan) -> OrbokResult<CleanupOutcome> {
        plan.assert_safe_for_ordinary_cleanup()?;
        match plan.action {
            CleanupAction::ClearExpiredSearchCache => self.clear_expired_search_cache(),
            CleanupAction::ClearSnippetCache => self.clear_snippet_cache(),
            CleanupAction::ClearTemporaryExtraction => Ok(CleanupOutcome::default()),
            CleanupAction::RemoveReplacedStaleIndexes => self.remove_replaced_stale_indexes(),
            _ => Err(OrbokError::CleanupWouldTouchPersistentData),
        }
    }

    /// Destructive catalog reset (RFC-001 §8.3). Requires a confirmed
    /// `ResetCatalog` plan. Removes sources, file catalog, chunks,
    /// indexes, caches, jobs, and search history; cascades do most of
    /// the work. Optionally preserves settings (RFC-011/§12.4).
    pub fn run_reset_catalog(
        &self,
        plan: &CleanupPlan,
        keep_settings: bool,
    ) -> OrbokResult<CleanupOutcome> {
        if plan.action != CleanupAction::ResetCatalog {
            return Err(OrbokError::Database(
                "reset requires a ResetCatalog plan".into(),
            ));
        }
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;
        let mut deleted = 0u64;
        // sources cascade to files -> extraction_records -> chunks ->
        // chunk_locations / embeddings / keyword_index_records.
        for table in [
            "sources",
            "index_jobs",
            "search_queries",
            "snippet_cache",
            "app_events",
            "storage_accounting",
            "cache_engines",
            "managed_model_profiles",
            "models",
        ] {
            deleted += tx
                .execute(&format!("DELETE FROM {table}"), [])
                .map_err(db_err)? as u64;
        }
        if !keep_settings {
            deleted += tx.execute("DELETE FROM app_settings", []).map_err(db_err)? as u64;
        }
        // contentless FTS: clear via the special delete-all command.
        tx.execute("INSERT INTO chunk_fts(chunk_fts) VALUES('delete-all')", [])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
        })
    }

    fn clear_expired_search_cache(&self) -> OrbokResult<CleanupOutcome> {
        let now = now_iso8601();
        let conn = self.catalog.lock();
        let mut deleted = conn
            .execute(
                "DELETE FROM search_result_cache WHERE expires_at IS NOT NULL AND expires_at < ?1",
                params![now],
            )
            .map_err(db_err)? as u64;
        deleted += conn
            .execute(
                "DELETE FROM search_queries WHERE expires_at IS NOT NULL AND expires_at < ?1",
                params![now],
            )
            .map_err(db_err)? as u64;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
        })
    }

    fn clear_snippet_cache(&self) -> OrbokResult<CleanupOutcome> {
        let conn = self.catalog.lock();
        let deleted = conn
            .execute("DELETE FROM snippet_cache", [])
            .map_err(db_err)? as u64;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
        })
    }

    /// Remove index records already superseded: chunks whose status is
    /// 'stale' or 'deleted' and that have an active replacement are safe
    /// to drop (RFC-001 §8.1 "obsolete replaced indexes"). v1 removes
    /// stale/deleted chunk rows whose file has at least one active chunk.
    fn remove_replaced_stale_indexes(&self) -> OrbokResult<CleanupOutcome> {
        let conn = self.catalog.lock();
        let deleted = conn
            .execute(
                "DELETE FROM chunks WHERE chunk_status IN ('stale','deleted') AND file_id IN \
                 (SELECT file_id FROM chunks WHERE chunk_status = 'active')",
                [],
            )
            .map_err(db_err)? as u64;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
        })
    }
}
