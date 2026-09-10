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
    /// Estimated bytes reclaimed in the keyword index: the number of
    /// `chunk_fts`/`chunk_fts_trigram` rows this call itself deleted,
    /// times 256 (the same per-record approximation
    /// `orbok_workers::storage::update_storage_accounting` uses for its
    /// `KeywordIndex` dashboard row) -- not a raw file-size diff, since
    /// neither this action nor a contentless FTS5 DELETE runs a VACUUM
    /// that would actually shrink the catalog file (RFC-059 §10
    /// criterion 6). Zero for every action but
    /// `RemoveReplacedStaleIndexes`, which is the one this criterion
    /// names -- and commonly zero there too: see that function's own doc
    /// comment for why a normal re-index usually leaves it nothing to
    /// find.
    pub bytes_reclaimed: u64,
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
            // Managed generations are paired with immutable files outside the
            // catalog. Preserve them until reset is integrated with the
            // exclusive model-store guard in a later RFC-050 phase.
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
        // RFC-059 §1(a)/§6: the trigram table has no other deletion path
        // (nothing else in the workspace ever issues `DELETE FROM
        // chunk_fts_trigram`) and was never cleared by Reset, so a full
        // reset used to leave it matching terms from the erased corpus.
        tx.execute("INSERT INTO chunk_fts(chunk_fts) VALUES('delete-all')", [])
            .map_err(db_err)?;
        tx.execute(
            "INSERT INTO chunk_fts_trigram(chunk_fts_trigram) VALUES('delete-all')",
            [],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
            ..Default::default()
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
            ..Default::default()
        })
    }

    fn clear_snippet_cache(&self) -> OrbokResult<CleanupOutcome> {
        let conn = self.catalog.lock();
        let deleted = conn
            .execute("DELETE FROM snippet_cache", [])
            .map_err(db_err)? as u64;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
            ..Default::default()
        })
    }

    /// Remove index records already superseded: chunks whose status is
    /// 'stale' or 'deleted' and that have an active replacement are safe
    /// to drop (RFC-001 §8.1 "obsolete replaced indexes"). v1 removes
    /// stale/deleted chunk rows whose file has at least one active chunk.
    ///
    /// RFC-059 §6/§0(ii): the FTS rows for those chunks must be deleted
    /// **first**, while `keyword_index_records` -- the only chunk_id <->
    /// FTS-rowid link that exists, since both FTS tables are contentless
    /// -- still has their rowids. `keyword_index_records.chunk_id` is
    /// `ON DELETE CASCADE` (`0001_baseline.sql`), so the `chunks` delete
    /// below destroys that mapping before anything could use it if it ran
    /// first: the rows in both `chunk_fts` and `chunk_fts_trigram` would
    /// become permanently unreachable and unreclaimable. All three
    /// statements run in one transaction so a crash between them cannot
    /// leave the mapping gone with the FTS rows still orphaned.
    fn remove_replaced_stale_indexes(&self) -> OrbokResult<CleanupOutcome> {
        let mut conn = self.catalog.lock();
        let tx = conn.transaction().map_err(db_err)?;
        let stale_chunks_subquery = "SELECT c.chunk_id FROM chunks c \
             WHERE c.chunk_status IN ('stale','deleted') AND c.file_id IN \
             (SELECT file_id FROM chunks WHERE chunk_status = 'active')";
        // `fts_rows_deleted` is this call's own contribution to the
        // keyword index actually shrinking -- the two FTS tables, not the
        // `chunks` catalog row deleted further below. In the common case
        // this is 0: `ChunkRepository::insert_bundle`'s own RFC-059 fix
        // deletes these exact rows at replace time, before this cleanup
        // ever runs, so by the time this call runs there is usually
        // nothing left in `chunk_fts`/`chunk_fts_trigram` for it to find.
        // It is non-zero only when that eager cleanup did not run first
        // (defense-in-depth: a chunk reaching 'stale' by some other path,
        // or debt from before this RFC shipped) -- exactly the scenario
        // `remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`
        // (`crates/search/engine/src/tests/rfc059_erasure_invariant.rs`)
        // constructs directly, since no production path leaves one today.
        let mut fts_rows_deleted = tx
            .execute(
                &format!(
                    "DELETE FROM chunk_fts WHERE rowid IN ( \
                         SELECT k.fts_rowid FROM keyword_index_records k \
                         WHERE k.chunk_id IN ({stale_chunks_subquery}) \
                           AND k.fts_rowid IS NOT NULL \
                     )"
                ),
                [],
            )
            .map_err(db_err)? as u64;
        fts_rows_deleted += tx
            .execute(
                &format!(
                    "DELETE FROM chunk_fts_trigram WHERE rowid IN ( \
                         SELECT k.trigram_fts_rowid FROM keyword_index_records k \
                         WHERE k.chunk_id IN ({stale_chunks_subquery}) \
                           AND k.trigram_fts_rowid IS NOT NULL \
                     )"
                ),
                [],
            )
            .map_err(db_err)? as u64;
        let mut deleted = fts_rows_deleted;
        deleted += tx
            .execute(
                "DELETE FROM chunks WHERE chunk_status IN ('stale','deleted') AND file_id IN \
                 (SELECT file_id FROM chunks WHERE chunk_status = 'active')",
                [],
            )
            .map_err(db_err)? as u64;
        tx.commit().map_err(db_err)?;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
            bytes_reclaimed: fts_rows_deleted * 256,
        })
    }
}
