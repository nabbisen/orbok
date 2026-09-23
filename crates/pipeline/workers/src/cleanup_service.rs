//! End-to-end cleanup service (M10, RFC-011 §11): combines catalog-side
//! cleanup (via [`CleanupExecutor`]) with cache-side cleanup (via
//! [`CacheService`]), driven by a validated [`CleanupPlan`].
//!
//! Call `CleanupService::run_safe` for ordinary cleanup; it will never
//! touch persistent source settings. For destructive operations use
//! `run_reset` with an explicit confirmation token.

use localcache::CacheEngine;
use orbok_cache::CacheService;
use orbok_core::{CleanupAction, CleanupPlan, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::CleanupExecutor;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::Path;
use tracing::{info, warn};

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
    /// Task 095: how post-reset compaction reads free space, swappable in
    /// tests so the "not enough room" branch can be exercised without
    /// filling a real disk (`with_available_space_reader`). Production
    /// always uses `fs4::available_space`, the same `statvfs`/
    /// `GetDiskFreeSpaceExW` call `orbok-models` already links for its
    /// model-store locking -- no new dependency.
    available_space: fn(&Path) -> std::io::Result<u64>,
}

impl<'a> CleanupService<'a> {
    pub fn new(catalog: &'a Catalog, cache: &'a CacheService, cache_db_path: &'a Path) -> Self {
        Self {
            catalog,
            cache,
            cache_db_path,
            available_space: real_available_space,
        }
    }

    /// Task 095 test-only seam: override the free-space reader so a test
    /// can force the "not enough room" branch deterministically. Never
    /// used in production, where [`Self::new`]'s `fs4::available_space`
    /// always applies.
    #[cfg(test)]
    pub(crate) fn with_available_space_reader(
        mut self,
        reader: fn(&Path) -> std::io::Result<u64>,
    ) -> Self {
        self.available_space = reader;
        self
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

        // Task 095: give the space back. Reset has already succeeded by
        // this point -- its data is gone either way -- so both files are
        // compacted independently, each only if its own filesystem has
        // room, and neither's failure (or skip) changes the outcome below.
        self.compact_catalog_after_reset();
        self.compact_cache_after_reset();

        Ok(FullCleanupOutcome {
            catalog_rows_deleted: catalog_outcome.deleted_rows,
            cache_bytes_freed,
            catalog_bytes_reclaimed: catalog_outcome.bytes_reclaimed,
        })
    }

    /// Task 095 §1: `VACUUM` needs roughly the file's own size again in
    /// free space while it runs (a fresh copy is built before the
    /// original is replaced) -- exactly the resource a user resetting
    /// *because* the disk is full is short on. Guarded, logged, and never
    /// allowed to affect the reset's own already-decided outcome.
    fn compact_catalog_after_reset(&self) {
        compact_if_room("catalog", self.catalog.path(), self.available_space, || {
            self.catalog.vacuum()
        });
    }

    /// Same guard, the cache file. `CacheService::shrink` already VACUUMs
    /// the whole file via any live namespace's engine handle -- reused
    /// here rather than duplicated.
    fn compact_cache_after_reset(&self) {
        compact_if_room("cache", self.cache_db_path, self.available_space, || {
            self.cache.shrink(self.catalog)
        });
    }

    fn run_cache_side(&self, plan: &CleanupPlan) -> OrbokResult<u64> {
        use orbok_cache::OrbokCacheNamespace;

        let size_before = self.cache_db_path.metadata().map(|m| m.len()).unwrap_or(0);

        match plan.action {
            CleanupAction::ClearSnippetCache | CleanupAction::ClearExpiredSearchCache => {
                // Task 093: this arm's former target, `PreviewCache`, was
                // retired -- it never had a producer (Review Request 270
                // §3, Review 270 §3). Both actions' real work is
                // catalog-side (`CleanupExecutor::clear_snippet_cache`/
                // `clear_expired_search_cache`, run unconditionally by
                // `run_safe` before this function, which never touches the
                // cache *file*), so this arm no longer has namespace work
                // to do -- but it still opens a real engine handle, rather
                // than skipping the cache file entirely, because RFC-061
                // criterion 8 depends on this action failing (not silently
                // succeeding) when the cache file itself is unavailable
                // (`criterion_8_clean_snippets_surfaces_an_error_when_the_cache_path_is_unavailable`
                // occupies the cache DB's path with a directory). Without
                // this open, that failure would go unnoticed by either
                // button, since the catalog side never sees it.
                self.cache.engine::<Vec<u8>>(
                    self.catalog,
                    &OrbokCacheNamespace::ExtractSegments,
                    OrbokCacheNamespace::ExtractSegments.default_engine_options(),
                )?;
            }
            CleanupAction::ClearTemporaryExtraction
            | CleanupAction::RemoveTemporarySourceIndexes => {
                // Owner decision 2026-09-12 (Review 214 §4 Q1): "Clear
                // temporary extraction" erases the whole ExtractSegments
                // namespace outright, rather than only expiring entries
                // older than the 90-day TTL -- the label says "clear," and
                // the cache is rebuildable by RFC-059 §7's own argument, so
                // there is no reason to make a user wait out the TTL to get
                // what the button promises. Superseded: this branch used to
                // call `cleanup_expired`/`purge_stale_versions`/
                // `cleanup_missing_files` plus a cleanup-time entry cap
                // (RFC-059 §7 Slice 3 / §10 criterion 5's original design) --
                // an outright erase makes all of those redundant here, since
                // nothing survives to expire, purge, or cap. The cap is
                // enforced at scheduler idle instead (RFC-059 Amendment 2,
                // `trim_extraction_cache_to`).
                //
                // Task 079 (Review Request 255 §6): every namespace this
                // project has retired is purged in the same pass, before
                // the live namespace's own erase, so the one `shrink_database`
                // below (inside `erase_engine_namespace`) reclaims both at
                // once -- the user should not need to know the word
                // "namespace" to get that space back, so this rides the
                // existing button rather than adding one.
                self.cache.purge_retired_namespaces()?;
                let engine = self.cache.engine::<Vec<u8>>(
                    self.catalog,
                    &OrbokCacheNamespace::ExtractSegments,
                    OrbokCacheNamespace::ExtractSegments.default_engine_options(),
                )?;
                // Task 095: this button's own VACUUM is unguarded, same as
                // before -- out of scope (§1 "Not in scope"); only Reset's
                // compaction gained the free-space guard.
                erase_engine_namespace(&engine, true)?;
            }
            CleanupAction::RemoveReplacedStaleIndexes => {
                // Task 093: `ChunkBundle`, this arm's other former target,
                // was retired (never had a producer). The catalog-side half
                // of this action (`CleanupExecutor::remove_replaced_stale_indexes`,
                // the `chunks`/FTS tables) is unaffected and still real
                // work, so this stays a real cache-side action too, just
                // against the one namespace left.
                let ns = OrbokCacheNamespace::ExtractSegments;
                let engine =
                    self.cache
                        .engine::<Vec<u8>>(self.catalog, &ns, ns.default_engine_options())?;
                engine
                    .cleanup_missing_files()
                    .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
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
        // Task 079: Reset claims to clear caches (RFC-011 §13); a namespace
        // this project has retired is still a cache, so it is purged here
        // too, not just by the narrower "Clear temporary extraction" path.
        let retired_removed = self.cache.purge_retired_namespaces()?;
        if retired_removed > 0 {
            info!(
                entries_removed = retired_removed,
                "retired cache namespaces erased"
            );
        }
        // Task 093: `ChunkBundle` and `PreviewCache`, this loop's other
        // former members, were retired -- neither ever had a producer, so
        // a reset never actually erased anything under either name; the
        // retired-namespace purge just above already covers their strings
        // (`RETIRED_NAMESPACES`) for the defensive case that verification
        // is wrong for some profile this project never saw. `ExtractSegments`
        // is the only namespace left, so this is no longer a loop.
        let ns = OrbokCacheNamespace::ExtractSegments;
        let engine =
            self.cache
                .engine::<Vec<u8>>(self.catalog, &ns, ns.default_engine_options())?;
        // Task 095: shrink deferred (`shrink_after: false`) -- `run_reset`
        // compacts the cache file itself afterward, guarded by free space,
        // rather than this unconditional VACUUM every namespace erase used
        // to trigger.
        let removed = erase_engine_namespace(&engine, false)?;
        info!(
            namespace = ns.as_namespace(),
            entries_removed = removed,
            "cache namespace erased"
        );
        let size_after = self.cache_db_path.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(size_before.saturating_sub(size_after))
    }

    /// Trim the extraction cache to `cap` entries, least-recently-accessed
    /// first (RFC-059 Amendment 2 §2b, criterion 10). Meant to be called
    /// only when the indexing pipeline is idle -- no Extract, Chunk or
    /// Embedding job queued or running -- because the chunk and embedding
    /// jobs read this namespace as their only source of text; a trim with
    /// one queued would evict what it still needs (Amendment 1 §2a.2).
    /// The caller supplies `cap`, so the scheduler host stays the one
    /// reader of `OrbokCacheNamespace::cleanup_time_entry_cap`.
    pub fn trim_extraction_cache_to(&self, cap: usize) -> OrbokResult<u64> {
        use orbok_cache::OrbokCacheNamespace;
        let ns = OrbokCacheNamespace::ExtractSegments;
        let engine =
            self.cache
                .engine::<Vec<u8>>(self.catalog, &ns, ns.default_engine_options())?;
        let evicted = trim_engine_namespace_to(&engine, cap)?;
        info!(
            namespace = ns.as_namespace(),
            entries_evicted = evicted,
            cap,
            "extraction cache trimmed at scheduler idle"
        );
        Ok(evicted)
    }
}

/// Trim one engine's namespace to at most `cap` entries, evicting the
/// least recently accessed first (`last_accessed_at`, then `updated_at`),
/// and reclaim freed pages only if something was removed. Public
/// `list_entries()`/`remove()`/`shrink_database()` only -- the raw-SQL
/// version against localcache's private schema is what Review 214 §2 had
/// removed. Returns the number of entries evicted.
pub fn trim_engine_namespace_to<T: Serialize + DeserializeOwned>(
    engine: &CacheEngine<T>,
    cap: usize,
) -> OrbokResult<u64> {
    let mut entries = engine
        .list_entries()
        .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
    if entries.len() <= cap {
        return Ok(0);
    }
    entries.sort_by_key(|e| (e.last_accessed_at, e.updated_at));
    let excess = entries.len() - cap;
    let mut evicted: u64 = 0;
    for entry in &entries[..excess] {
        let did_remove = engine
            .remove(&entry.path)
            .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
        if did_remove {
            evicted += 1;
        }
    }
    if evicted > 0 {
        engine
            .shrink_database()
            .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
    }
    Ok(evicted)
}

/// Remove every entry in one engine's namespace, then optionally reclaim
/// the freed pages. Shared by `purge_all_cache_namespaces` (Reset, every
/// namespace, `shrink_after: false` since Task 095 -- `run_reset` compacts
/// the file itself afterward, guarded by free space) and the
/// `ClearTemporaryExtraction` branch (Review 214 §4 Q1, owner decision
/// 2026-09-12: the button erases `ExtractSegments` outright, rather than
/// only expiring entries older than the TTL -- the label says "clear", and
/// the cache is rebuildable by RFC-059 §7's own argument; `shrink_after:
/// true`, unguarded, out of Task 095's scope). Returns the number of
/// entries actually removed.
fn erase_engine_namespace<T: Serialize + DeserializeOwned>(
    engine: &CacheEngine<T>,
    shrink_after: bool,
) -> OrbokResult<u64> {
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
    if shrink_after {
        engine
            .shrink_database()
            .map_err(|e| orbok_core::OrbokError::Cache(e.to_string()))?;
    }
    Ok(removed)
}

/// Task 095: production `available_space` reader -- a monomorphized
/// wrapper, since `fs4::available_space` is generic over `P: AsRef<Path>`
/// and can't be named directly as the plain `fn(&Path) -> ...` pointer
/// [`CleanupService::available_space`] holds.
fn real_available_space(path: &Path) -> std::io::Result<u64> {
    fs4::available_space(path)
}

/// Task 095 §1.2: the free-space margin required before compacting a file
/// of `file_size` bytes -- 10% of the file's own size, floored at 1 MiB.
/// `VACUUM` needs roughly the file's own size again while it runs (a fresh
/// copy is built before the original is replaced); the 10% is headroom for
/// what that copy needs beyond the plain page count -- SQLite's own
/// temporary rollback journal for the new file, plus filesystem block
/// rounding -- not a number this project measured precisely, but one that
/// scales with the file rather than a fixed constant that would be
/// negligible at 2 GB and wasteful at 1 MB. The 1 MiB floor keeps a tiny
/// catalog from requiring an unmeasurably small margin.
pub(crate) fn compaction_margin(file_size: u64) -> u64 {
    const MARGIN_FLOOR_BYTES: u64 = 1024 * 1024;
    (file_size / 10).max(MARGIN_FLOOR_BYTES)
}

/// Task 095 §1.1/§1.2: true when `available` bytes are enough to compact a
/// file of `file_size` bytes -- the file's own size plus
/// [`compaction_margin`]. A pure function so the guard itself is testable
/// with synthetic numbers, independent of a real disk or a real `VACUUM`.
pub(crate) fn has_room_to_compact(available: u64, file_size: u64) -> bool {
    available >= file_size.saturating_add(compaction_margin(file_size))
}

/// Task 095 §1.3/§1.4: compact `path` via `vacuum`, but only if
/// `available_space` reports enough room for it (§1.1/§1.2), and never let
/// the outcome escape as an error -- by construction, not by care: this
/// returns `()`, so nothing it does can flow into `run_reset`'s `Result`.
/// Not enough room logs at `info!` and skips (an ordinary outcome: the
/// reset's data is gone either way). A `vacuum` failure -- room was there,
/// `VACUUM` itself errored -- logs at `warn!`. Both branches, and the
/// `available_space` read itself, are injectable so tests can exercise
/// each without filling a real disk or corrupting a real file
/// (`CleanupService::with_available_space_reader`, and `vacuum` is a
/// plain closure at each call site).
pub(crate) fn compact_if_room(
    label: &str,
    path: &Path,
    available_space: impl FnOnce(&Path) -> std::io::Result<u64>,
    vacuum: impl FnOnce() -> OrbokResult<()>,
) {
    let file_size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            info!(
                file = %path.display(),
                error = %e,
                "could not read {label} file size after reset; skipping compaction"
            );
            return;
        }
    };
    let available = match available_space(path) {
        Ok(a) => a,
        Err(e) => {
            info!(
                file = %path.display(),
                error = %e,
                "could not read free space after reset; skipping {label} compaction"
            );
            return;
        }
    };
    if !has_room_to_compact(available, file_size) {
        info!(
            file = %path.display(),
            file_size,
            available,
            "not enough free space to compact the {label} after reset; skipping"
        );
        return;
    }
    if let Err(e) = vacuum() {
        warn!(
            file = %path.display(),
            error = %e,
            "{label} compaction failed after reset"
        );
    }
}
