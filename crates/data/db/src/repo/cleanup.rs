//! Cleanup execution against the catalog (RFC-001 §9, RFC-011).
//!
//! Every entry point takes an [`orbok_core::CleanupPlan`]; safe (ordinary)
//! cleanup re-validates that the plan cannot touch persistent catalog
//! data before any row is deleted. Source files on disk are never
//! touched by any path in this module.

use crate::catalog::{Catalog, db_err};
use crate::repo::jobs::IndexJobRepository;
use orbok_core::{CleanupAction, CleanupPlan, ModelId, OrbokError, OrbokResult, now_iso8601};
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
    /// Task 099: how many files `delete_keyword_index`/`delete_vector_index`
    /// queued a job for, to rebuild what was just deleted. Zero for every
    /// other action, and zero for `delete_vector_index` when no model was
    /// available to rebuild against (the index is still deleted; nothing
    /// is marked, since nothing could pick it up).
    pub files_marked_for_rebuild: u64,
}

/// `run_reset_catalog`'s own delete order (sources cascade to files ->
/// extraction_records -> chunks -> chunk_locations / embeddings /
/// keyword_index_records).
///
/// Review 276 §3: `sources` must come before `models`. `embeddings.model_id
/// REFERENCES models(model_id)` with no `ON DELETE` clause -- the only FK
/// in this schema that neither cascades nor nulls
/// (`0001_baseline.sql:173`) -- so if `models` ran first, deleting a row
/// still referenced by a surviving `embeddings` row would fail the FK
/// check and roll this whole transaction back, leaving Reset reporting
/// failure with nothing removed. This order is what makes that never
/// happen: `sources`'s cascade has already emptied `embeddings` by the
/// time `models` is reached. A named constant, not an inline array, so
/// `sources_is_deleted_before_models` can assert the order itself without
/// running a reset at all -- reordering this list is exactly the mistake
/// that test exists to catch.
pub(crate) const RESET_DELETE_ORDER: &[&str] = &[
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
    // Task 094: this doc comment already claimed "and search
    // history" -- it did not, until this line. No FK links this
    // table to anything else deleted here (migration
    // 0004_search_history.sql has no REFERENCES clause at all), so
    // it needs its own name, not a cascade.
    "search_history",
];

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
            // Task 099 (RFC-011 §14 criteria 5/6): these used to fall to
            // the catch-all below, which returned
            // `CleanupWouldTouchPersistentData` -- a misleading error, since
            // neither action's `affected_classes()` includes
            // `PersistentCatalog` (both are `RebuildableIndex` only,
            // `assert_safe_for_ordinary_cleanup` above already lets them
            // through). The arm was simply never written.
            CleanupAction::DeleteKeywordIndex => self.delete_keyword_index(),
            CleanupAction::DeleteVectorIndex => self.delete_vector_index(plan.model_id.as_ref()),
            _ => Err(OrbokError::CleanupWouldTouchPersistentData),
        }
    }

    /// Task 099 (RFC-011 §14 criterion 6): delete the whole keyword index
    /// and mark every file that has one for re-preparation.
    ///
    /// `chunk_fts` is contentless FTS5 -- "stores no retrievable source
    /// text" (this crate's own migration comment) -- and the
    /// `normalized_text` `chunk_fts` is built from is never persisted
    /// anywhere else in the catalog (`ChunkSpec`'s own doc comment in
    /// `chunks.rs`); it exists only transiently, while a chunk job holds
    /// it. So once this deletes `keyword_index_records` and both FTS
    /// tables, the only way to regenerate that text is to re-extract: this
    /// is why the rebuild this queues is `Extract`, not a lighter
    /// keyword-only step -- there is no lighter step the storage layer can
    /// support. `ChunkRepository::reuse_existing_chunks` (the chunks are the
    /// stored ones) or `insert_bundle` (a new generation) rebuilds the
    /// keyword index as a byproduct of chunking the extraction, the same
    /// path an ordinary first-time index already takes.
    ///
    /// Sufficient on its own (Task 102). A file whose content has not
    /// changed still has a fresh entry in the *extraction* cache (a separate
    /// store, outside this crate), so the `Extract` job this queues takes
    /// `ExtractionWorker::run`'s freshness shortcut and queues a `Chunk` job
    /// against the *same* `extraction_id` its still-active chunks occupy.
    /// `ChunkAndIndexWorker` handles that: when the chunks it derives are the
    /// stored ones it writes only the keyword rows they lack, under the same
    /// chunk ids, and queues no `Embedding` job. (Task 099 first avoided the
    /// case by evicting the cache entry, which re-embedded every file.)
    pub fn delete_keyword_index(&self) -> OrbokResult<CleanupOutcome> {
        let deleted = {
            let mut conn = self.catalog.lock();
            let tx = conn.transaction().map_err(db_err)?;
            let deleted = tx
                .execute("DELETE FROM keyword_index_records", [])
                .map_err(db_err)? as u64;
            tx.execute("INSERT INTO chunk_fts(chunk_fts) VALUES('delete-all')", [])
                .map_err(db_err)?;
            tx.execute(
                "INSERT INTO chunk_fts_trigram(chunk_fts_trigram) VALUES('delete-all')",
                [],
            )
            .map_err(db_err)?;
            tx.commit().map_err(db_err)?;
            deleted
        };
        let queued = IndexJobRepository::new(self.catalog).enqueue_extraction_backfill()?;
        Ok(CleanupOutcome {
            deleted_rows: deleted,
            files_marked_for_rebuild: queued as u64,
            ..Default::default()
        })
    }

    /// Task 099 (RFC-011 §14 criterion 5): delete every embedding and, if a
    /// model is available to rebuild against, mark every affected file for
    /// re-embedding. Chunks and the keyword index are untouched -- deleting
    /// this index never forces a re-extraction (§2.7: the two indexes are
    /// independent).
    ///
    /// `model_id` is `None` when no embedding model is currently
    /// configured: the index is still deleted (nothing left over from a
    /// model that was since removed), but nothing is queued, since nothing
    /// exists to embed against and a job with no model to run would never
    /// leave `queued` (RFC-011 review's own bounded-rebuild condition).
    pub fn delete_vector_index(&self, model_id: Option<&ModelId>) -> OrbokResult<CleanupOutcome> {
        let deleted = {
            let conn = self.catalog.lock();
            conn.execute("DELETE FROM embeddings", []).map_err(db_err)? as u64
        };
        let queued = match model_id {
            Some(model_id) => {
                IndexJobRepository::new(self.catalog).enqueue_embedding_backfill(model_id)?
            }
            None => 0,
        };
        Ok(CleanupOutcome {
            deleted_rows: deleted,
            files_marked_for_rebuild: queued as u64,
            ..Default::default()
        })
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
        for table in RESET_DELETE_ORDER {
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
            ..Default::default()
        })
    }
}
