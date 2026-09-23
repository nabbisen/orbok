//! Task 075: backend actions whose result the UI must reflect truthfully.
//!
//! `main.rs`'s `update` closure cannot be driven from a test, so each
//! action's whole sequence -- the backend call, then what the state and
//! notices become -- lives here as a plain function, as `source_removal`
//! and `notice_retry` do.
//!
//! Row 2, reset, moved out (Task 097): its own work runs off the update
//! thread now, on its own connection, so it no longer fits this file's own
//! shape (a plain function taking `&mut AppState` to mutate synchronously).
//! It lives in `main.rs` as `reset_catalog_delete_compact_and_measure` /
//! `reset_outcome_to_messages`; its own real-failure tests moved to
//! `router/tests.rs` alongside Task 096's.

use crate::{bootstrap, history, notice_retry};
use orbok::runtime_context::RuntimeContext;
use orbok::runtime_storage::ProfileCache;
use orbok_core::SearchHistoryId;
use orbok_db::Catalog;
use orbok_ui::AppState;
use orbok_ui::i18n::Locale;
use orbok_ui::notice::UserNotice;
use orbok_ui::state::Message;

/// Row 1: "Clear recent searches" was confirmed. "Cleared" is shown only
/// once the catalog has cleared them.
pub(crate) fn clear_recent_searches(catalog: &Catalog, state: &mut AppState) {
    // The request itself: closes the confirmation, changes nothing else.
    state.update(&Message::ConfirmClearRecentSearches);
    match history::clear_history(catalog) {
        Ok(()) => {
            state.update(&Message::RecentSearchesCleared);
            state.update(&Message::ShowNotice(UserNotice::RecentSearchesCleared));
        }
        Err(e) => {
            tracing::error!("clear search history failed: {e}");
            state.update(&notice_retry::recent_searches_not_cleared());
        }
    }
}

/// Row 7: one recent search was removed. The entry leaves the list only
/// once the catalog has removed it.
pub(crate) fn remove_recent_search(catalog: &Catalog, state: &mut AppState, id: &SearchHistoryId) {
    match history::remove_entry(catalog, id) {
        Ok(()) => state.update(&Message::RecentSearchRemoved(id.clone())),
        Err(e) => {
            tracing::error!("remove search history entry failed: {e}");
            state.update(&notice_retry::recent_search_not_removed(id));
        }
    }
}

/// Row 3: the UI language changed. The new language shows at once, as
/// theme and text size do; a failed save says so.
pub(crate) fn persist_locale(catalog: &Catalog, state: &mut AppState, locale: Locale) {
    let change = Message::PersistLocale(locale);
    if let Err(e) = bootstrap::persist_locale(catalog, &locale) {
        tracing::error!("persist locale failed: {e}");
        state.update(&notice_retry::setting_not_saved(&change));
    }
    state.update(&change);
}

/// Row 4: "Remember recent searches" was toggled. The setting shows the new
/// value at once; a failed save says so. Turning it off clears history,
/// which is reported like any clear.
pub(crate) fn toggle_remember_recent_searches(
    runtime: &RuntimeContext,
    catalog: &Catalog,
    state: &mut AppState,
    on: bool,
) {
    let change = Message::ToggleRememberRecentSearches(on);
    // An unreadable settings file already loads as defaults
    // (`RuntimeStorage::load_settings`); only an unwritable one fails here.
    let mut settings = bootstrap::load_runtime_settings(runtime).unwrap_or_default();
    settings.remember_recent_searches = on;
    if let Err(e) = bootstrap::save_runtime_settings(runtime, &settings) {
        tracing::error!("persist remember recent searches failed: {e}");
        state.update(&notice_retry::setting_not_saved(&change));
    }
    state.update(&change);
    if !on {
        match history::clear_history(catalog) {
            Ok(()) => state.update(&Message::RecentSearchesCleared),
            Err(e) => {
                tracing::error!("clear search history failed: {e}");
                state.update(&notice_retry::recent_searches_not_cleared());
            }
        }
    }
}

/// Row 5: one of the four Safe cleanup actions. Its done-notice is shown
/// only once it has finished; a failure says so, with that cleanup as retry.
pub(crate) fn run_cleanup(
    catalog: &Catalog,
    cache: std::io::Result<ProfileCache>,
    state: &mut AppState,
    cleanup: &Message,
) {
    let cache = match cache {
        Ok(cache) => cache,
        Err(e) => {
            tracing::error!("cache handle unavailable for cleanup: {e}");
            state.update(&notice_retry::cleanup_storage_unavailable(cleanup));
            return;
        }
    };
    let (result, done) = match cleanup {
        Message::CleanSnippets => (
            bootstrap::clean_snippets(catalog, &cache),
            UserNotice::PreviewsCleared,
        ),
        Message::CleanSearchCache => (
            bootstrap::clean_search_cache(catalog, &cache),
            UserNotice::SearchCacheCleared,
        ),
        Message::CleanTemporaryExtraction => (
            bootstrap::clean_temporary_extraction(catalog, &cache),
            UserNotice::ExtractedTextCleared,
        ),
        Message::RemoveReplacedStaleIndexes => (
            bootstrap::remove_replaced_stale_indexes(catalog, &cache),
            UserNotice::ReplacedDataRemoved,
        ),
        _ => return,
    };
    match result {
        Ok(()) => state.update(&Message::ShowNotice(done)),
        Err(e) => {
            tracing::error!("cleanup failed: {e}");
            state.update(&notice_retry::cleanup_did_not_finish(cleanup));
        }
    }
}

/// Row 6: "Check again" / "Prepare again" on a folder. The refreshed list
/// is shown only if it can be read; either failure says so.
pub(crate) fn refresh_source(catalog: &Catalog, state: &mut AppState, source_id: &str) {
    let refreshed = bootstrap::check_and_refresh_source(catalog, source_id)
        .and_then(|health| Ok((health, bootstrap::get_sources(catalog)?)));
    match refreshed {
        Ok((health, cards)) => {
            state.update(&Message::SourcesLoaded(cards));
            state.update(&Message::HealthUpdated(health));
        }
        Err(e) => {
            tracing::error!("source refresh failed: {e}");
            state.update(&notice_retry::folder_not_checked(source_id));
        }
    }
}

#[cfg(test)]
mod tests;
