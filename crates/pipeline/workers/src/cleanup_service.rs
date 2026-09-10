//! End-to-end cleanup service (M10, RFC-011 §11): combines catalog-side
//! cleanup (via [`CleanupExecutor`]) with cache-side cleanup (via
//! [`CacheService`]), driven by a validated [`CleanupPlan`].
//!
//! Call `CleanupService::run_safe` for ordinary cleanup; it will never
//! touch persistent source settings. For destructive operations use
//! `run_reset` with an explicit confirmation token.

use orbok_cache::CacheService;
use orbok_core::{CleanupAction, CleanupPlan, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::CleanupExecutor;
use std::path::Path;
use tracing::info;

/// Combined cleanup outcome (catalog + cache sides).
#[derive(Debug, Default)]
pub struct FullCleanupOutcome {
    pub catalog_rows_deleted: u64,
    /// Approximate cache bytes freed (0 if cache cleanup is not applicable).
    pub cache_bytes_freed: u64,
    /// Approximate catalog-side bytes reclaimed -- currently only non-zero
    /// for `RemoveReplacedStaleIndexes` (RFC-059 §10 criterion 6): see
    /// `orbok_db::repo::CleanupOutcome::bytes_reclaimed`'s own doc comment
    /// for why this is an estimate, not a file-size diff.
    pub catalog_bytes_reclaimed: u64,
}

/// Orchestrates catalog and cache cleanup (RFC-011 §8).
pub struct CleanupService<'a> {
    catalog: &'a Catalog,
    cache: &'a CacheService,
    cache_db_path: &'a Path,
}

impl<'a> CleanupService<'a> {
    pub fn new(catalog: &'a Catalog, cache: &'a CacheService, cache_db_path: &'a Path) -> Self {
        Self {
            catalog,
            cache,
            cache_db_path,
        }
    }

    /// Safe cleanup: validates the plan cannot touch persistent data, then
    /// runs catalog-side and cache-side operations atomically in intent
    /// (RFC-011 §8 "lifecycle-aware cleanup").
    pub fn run_safe(&self, plan: &CleanupPlan) -> OrbokResult<FullCleanupOutcome> {
        // Catalog side.
        let catalog_outcome = CleanupExecutor::new(self.catalog).run_safe(plan)?;
        info!(
            action = ?plan.action,
            rows = catalog_outcome.deleted_rows,
            "catalog cleanup completed"
        );

        // Cache side: map CleanupAction to cache namespace operations.
        let cache_bytes_freed = self.run_cache_side(plan)?;
        if cache_bytes_freed > 0 {
            info!(bytes = cache_bytes_freed, "cache cleanup freed space");
        }

        Ok(FullCleanupOutcome {
            catalog_rows_deleted: catalog_outcome.deleted_rows,
            cache_bytes_freed,
            catalog_bytes_reclaimed: catalog_outcome.bytes_reclaimed,
        })
    }

    /// Destructive catalog reset (requires confirmed ResetCatalog plan).
    pub fn run_reset(
        &self,
        plan: &CleanupPlan,
        keep_settings: bool,
    ) -> OrbokResult<FullCleanupOutcome> {
        // Catalog reset.
        let catalog_outcome =
            CleanupExecutor::new(self.catalog).run_reset_catalog(plan, keep_settings)?;

        // Purge all cache namespaces (RFC-011 §13: full reset clears caches).
        let cache_bytes_freed = self.purge_all_cache_namespaces()?;

        info!(
            rows = catalog_outcome.deleted_rows,
            cache_freed = cache_bytes_freed,
            "catalog reset completed"
        );

        Ok(FullCleanupOutcome {
            catalog_rows_deleted: catalog_outcome.deleted_rows,
            cache_bytes_freed,
            catalog_bytes_reclaimed: catalog_outcome.bytes_reclaimed,
        })
    }

    fn run_cache_side(&self, plan: &CleanupPlan) -> OrbokResult<u64> {
        use orbok_cache::{EngineOptions, OrbokCacheNamespace};

        let size_before = self.cache_db_path.metadata().map(|m| m.len()).unwrap_or(0);

        match plan.action {
            CleanupAction::ClearSnippetCache | CleanupAction::ClearExpiredSearchCache => {
                // Purge the preview-cache namespace.
                let engine = self.cache.engine::<Vec<u8>>(
                    self.catalog,
                    &OrbokCacheNamespace::PreviewCache,
                    EngineOptions::default(),
                )?;
                engine
                    .cleanup_expired()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
                engine
                    .shrink_database()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
            }
            CleanupAction::ClearTemporaryExtraction
            | CleanupAction::RemoveTemporarySourceIndexes => {
                // Purge extract-segments namespace.
                let engine = self.cache.engine::<Vec<u8>>(
                    self.catalog,
                    &OrbokCacheNamespace::ExtractSegments,
                    OrbokCacheNamespace::ExtractSegments.default_engine_options(),
                )?;
                // RFC-059 §7 Slice 3 / §10 criterion 5: this action is the
                // one "Clear temporary extraction" (once Slice 4 wires it
                // into the Storage view) actually runs -- found missing
                // this call while testing criterion 5 for real, not by
                // inspection: `ProfileCache::run_safe_cleanup`
                // (`crates/app/src/runtime_storage.rs`) routes through
                // `CleanupService::run_safe`, i.e. this exact branch, not
                // `orbok_cache::CacheService::run_safe_cleanup` (which
                // already calls `cleanup_expired` but has no production
                // caller). Without it, Slice 3's new TTL was configured
                // but never enforced by the one button meant to enforce
                // it.
                engine
                    .cleanup_expired()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
                engine
                    .purge_stale_versions()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
                engine
                    .cleanup_missing_files()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
                // Without this, deleted rows free pages inside the
                // database file but never shrink it on disk, so
                // size_before/size_after below would report a reclaim of
                // 0 regardless of how many entries were actually removed
                // -- the same gap this branch's own missing
                // `cleanup_expired()` call was, caught by the same test.
                engine
                    .shrink_database()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
            }
            CleanupAction::RemoveReplacedStaleIndexes => {
                // Clean up chunk and embedding bundle caches. Per-namespace
                // options (RFC-059 §7 Slice 3), not a blanket default --
                // see `OrbokCacheNamespace::default_engine_options`'s own
                // doc comment for why a mismatched default here would
                // corrupt ExtractSegments' registered ttl/max_entries.
                for ns in [
                    OrbokCacheNamespace::ChunkBundle,
                    OrbokCacheNamespace::ExtractSegments,
                ] {
                    let engine = self.cache.engine::<Vec<u8>>(
                        self.catalog,
                        &ns,
                        ns.default_engine_options(),
                    )?;
                    engine
                        .cleanup_missing_files()
                        .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
                }
            }
            _ => {}
        }

        let size_after = self.cache_db_path.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(size_before.saturating_sub(size_after))
    }

    /// Erase every payload in every cache namespace (RFC-059 §0(iii)/§1
    /// Slice 1: a real erase, not the maintenance sweeps this used to run).
    ///
    /// `purge_stale_versions`/`cleanup_expired`/`shrink_database` are
    /// maintenance operations, not erasure: the first only matches a
    /// payload version other than the current one, the second needs a TTL
    /// (`ExtractSegments` is opened with `ttl: None` at every site), so on
    /// a full reset neither can match anything an ordinary indexing run
    /// wrote -- confirmed, not assumed (RFC-059 §1's table). A function
    /// named "purge all" that purges nothing and discards every result it
    /// would need to notice that is the same shape as the audit's original
    /// complaint about this RFC's subject.
    ///
    /// `localcache` 0.21.1 already exposes `keys(None)` (every stored path
    /// in the current namespace -- `CacheEngine::engine` scopes every
    /// engine to one namespace, RFC-059 §0(iii)) and `remove(path)`, both
    /// namespace-scoped, so an in-process erase needs no upstream change
    /// and no open-handle sequencing: enumerate, remove each, then
    /// `shrink_database()` to reclaim the freed pages.
    fn purge_all_cache_namespaces(&self) -> OrbokResult<u64> {
        use orbok_cache::OrbokCacheNamespace;
        let size_before = self.cache_db_path.metadata().map(|m| m.len()).unwrap_or(0);
        for ns in [
            OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ChunkBundle,
            OrbokCacheNamespace::PreviewCache,
        ] {
            // Per-namespace options (RFC-059 §7 Slice 3), not a blanket
            // default -- passing the wrong ones here on a purge would
            // still corrupt ExtractSegments' registered ttl/max_entries
            // (`CacheService::register_engine` upserts on every open),
            // even though this call only ever deletes rows.
            let engine =
                self.cache
                    .engine::<Vec<u8>>(self.catalog, &ns, ns.default_engine_options())?;
            let keys = engine
                .keys(None)
                .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
            let mut removed: u64 = 0;
            for key in &keys {
                let did_remove = engine
                    .remove(key)
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
                if did_remove {
                    removed += 1;
                }
            }
            engine
                .shrink_database()
                .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
            info!(
                namespace = ns.as_namespace(),
                entries_removed = removed,
                "cache namespace erased"
            );
        }
        let size_after = self.cache_db_path.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(size_before.saturating_sub(size_after))
    }
}
