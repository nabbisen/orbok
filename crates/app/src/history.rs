//! Search history orchestration (RFC-042).
//!
//! Thin glue between the iced update loop and `SearchHistoryRepository`.
//! Honors the privacy gate: when history is disabled (setting off or strict
//! privacy mode) nothing is recorded.

use orbok_core::{
    PrivacySettings, SearchHistoryEntry, SearchHistoryId, SearchHistorySettings, StoredSearchFilter,
};
use orbok_db::Catalog;
use orbok_db::repo::SearchHistoryRepository;
use orbok_search::ActiveFilter;

/// Whether history should be recorded, given current privacy settings.
///
/// Mirrors `PrivacySettings::effective_recent_searches()` (RFC-039/§14):
/// disabled by the per-user toggle or by strict privacy mode.
pub fn history_enabled(privacy: &PrivacySettings) -> bool {
    privacy.effective_recent_searches()
}

/// Record a successful search if history is enabled. No-op otherwise.
///
/// Stores instructions only (text + filter labels), never results
/// (RFC-042 §8.2). Empty queries are rejected by the repository.
pub fn record_search(
    catalog: &Catalog,
    privacy: &PrivacySettings,
    settings: &SearchHistorySettings,
    search_text: &str,
    active_filters: &[ActiveFilter],
    result_count: usize,
    locale: &str,
) {
    if !history_enabled(privacy) {
        return;
    }
    if search_text.trim().is_empty() {
        return;
    }
    let stored: Vec<StoredSearchFilter> = active_filters
        .iter()
        .map(StoredSearchFilter::from)
        .collect();
    let repo = SearchHistoryRepository::new(catalog);
    if let Err(e) = repo.upsert(search_text, &stored, Some(result_count), locale, settings) {
        tracing::warn!("record search history failed: {e}");
    }
}

/// Load the current recent-search list (newest first). Returns an empty
/// vector on error so the UI degrades gracefully.
pub fn load_history(catalog: &Catalog) -> Vec<SearchHistoryEntry> {
    SearchHistoryRepository::new(catalog)
        .list()
        .unwrap_or_else(|e| {
            tracing::warn!("load search history failed: {e}");
            Vec::new()
        })
}

/// Fetch one entry by id for restore.
pub fn get_entry(catalog: &Catalog, id: &SearchHistoryId) -> Option<SearchHistoryEntry> {
    SearchHistoryRepository::new(catalog).get(id).ok().flatten()
}

/// Remove a single entry, then return the refreshed list.
pub fn remove_entry(catalog: &Catalog, id: &SearchHistoryId) -> Vec<SearchHistoryEntry> {
    let repo = SearchHistoryRepository::new(catalog);
    if let Err(e) = repo.remove(id) {
        tracing::warn!("remove search history entry failed: {e}");
    }
    repo.list().unwrap_or_default()
}

/// Clear all entries (RFC-042 §13.3).
pub fn clear_history(catalog: &Catalog) {
    if let Err(e) = SearchHistoryRepository::new(catalog).clear() {
        tracing::warn!("clear search history failed: {e}");
    }
}

/// Filters from a stored entry, dropping any folder filter whose source id
/// no longer exists in the catalog (RFC-042 §9 step 3).
///
/// Returns `(valid_filters, dropped_any)` so the caller can show the
/// friendly dropped-filter notice when something was removed.
pub fn restore_valid_filters(
    catalog: &Catalog,
    entry: &SearchHistoryEntry,
) -> (Vec<StoredSearchFilter>, bool) {
    use orbok_core::SourceId;
    use orbok_db::repo::SourceRepository;

    let sources = SourceRepository::new(catalog);
    let mut kept = Vec::new();
    let mut dropped = false;

    for f in &entry.filters {
        if let Some(folder_id) = f.folder_id() {
            let exists = sources
                .get(&SourceId::from_string(folder_id.to_string()))
                .ok()
                .flatten()
                .is_some();
            if exists {
                kept.push(f.clone());
            } else {
                dropped = true;
            }
        } else {
            kept.push(f.clone());
        }
    }

    (kept, dropped)
}
