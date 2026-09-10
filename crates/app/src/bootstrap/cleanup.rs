//! Storage cleanup: snippet/search-cache clearing and full catalog reset.

use orbok::runtime_storage::ProfileCache;
use orbok_core::OrbokResult;
use orbok_db::Catalog;

/// Clear the snippet cache (safe, rebuilds on demand).
pub fn clean_snippets(catalog: &Catalog, cache: &ProfileCache) -> OrbokResult<()> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ClearSnippetCache, 0);
    cache.run_safe_cleanup(catalog, &plan)?;
    Ok(())
}

/// Clear expired search cache (safe, rebuilds on demand).
pub fn clean_search_cache(catalog: &Catalog, cache: &ProfileCache) -> OrbokResult<()> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ClearExpiredSearchCache, 0);
    cache.run_safe_cleanup(catalog, &plan)?;
    Ok(())
}

/// Clear expired extraction-cache entries (safe, re-extracted on demand
/// -- RFC-059 §8 Slice 4: implemented since M10, reachable from no UI
/// until this).
pub fn clean_temporary_extraction(catalog: &Catalog, cache: &ProfileCache) -> OrbokResult<()> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ClearTemporaryExtraction, 0);
    cache.run_safe_cleanup(catalog, &plan)?;
    Ok(())
}

/// Remove index rows already superseded by a re-index (safe -- RFC-059
/// §8 Slice 4, after Slice 2: only frees real bytes because Slice 2 fixed
/// this action's own FTS-row leak; exposed here, not before).
pub fn remove_replaced_stale_indexes(catalog: &Catalog, cache: &ProfileCache) -> OrbokResult<()> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::RemoveReplacedStaleIndexes, 0);
    cache.run_safe_cleanup(catalog, &plan)?;
    Ok(())
}

/// Full catalog reset (destructive — caller must have confirmed).
pub fn reset_catalog(catalog: &Catalog, cache: &ProfileCache) -> OrbokResult<()> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ResetCatalog, 0);
    cache.run_reset(catalog, &plan, true)?;
    Ok(())
}
