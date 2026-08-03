//! Storage cleanup: snippet/search-cache clearing and full catalog reset.

use orbok::runtime_storage::ProfileCache;
use orbok_db::Catalog;

/// Clear the snippet cache (safe, rebuilds on demand).
pub fn clean_snippets(
    catalog: &Catalog,
    cache: &ProfileCache,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ClearSnippetCache, 0);
    cache.run_safe_cleanup(catalog, &plan)?;
    Ok(())
}

/// Clear expired search cache (safe, rebuilds on demand).
pub fn clean_search_cache(
    catalog: &Catalog,
    cache: &ProfileCache,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ClearExpiredSearchCache, 0);
    cache.run_safe_cleanup(catalog, &plan)?;
    Ok(())
}

/// Full catalog reset (destructive — caller must have confirmed).
pub fn reset_catalog(
    catalog: &Catalog,
    cache: &ProfileCache,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::{CleanupAction, CleanupPlan};
    let plan = CleanupPlan::for_action(CleanupAction::ResetCatalog, 0);
    cache.run_reset(catalog, &plan, true)?;
    Ok(())
}
