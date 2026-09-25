//! Task 075 §3 (with Review 253's amendments): each backend action against a
//! real on-disk profile, made to fail for real.
//!
//! The usual failure is a write lock: under WAL, `BEGIN EXCLUSIVE` from a
//! second connection blocks orbok's writes but not its reads (Review Request
//! 250 §2), and the catalog's 50 ms busy timeout makes the write fail fast.

use super::*;
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeSelection};
use orbok_core::SearchHistorySettings;
use orbok_db::repo::SearchHistoryRepository;
use std::path::PathBuf;

struct Profile {
    _temp: tempfile::TempDir,
    data: PathBuf,
    context: RuntimeContext,
    catalog: Catalog,
}

impl Profile {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("profile");
        let context = RuntimeContext::resolve(
            RuntimeSelection::resolve(false, Some(data.as_os_str().to_os_string())).unwrap(),
            temp.path(),
            PlatformRuntimePaths {
                standard_data_dir: Some(temp.path()),
                standard_settings_dir: Some(temp.path()),
                home_dir: None,
            },
        )
        .unwrap();
        let catalog = bootstrap::open_catalog(&context).unwrap();
        catalog
            .lock()
            .busy_timeout(std::time::Duration::from_millis(50))
            .unwrap();
        Self {
            _temp: temp,
            data,
            context,
            catalog,
        }
    }

    fn db(&self) -> PathBuf {
        self.data.join(orbok_db::CATALOG_FILE_NAME)
    }

    fn cache(&self) -> std::io::Result<ProfileCache> {
        bootstrap::cache_service(&self.context)
    }

    /// A second connection holding the catalog's write lock.
    fn write_lock(&self) -> rusqlite::Connection {
        let locker = rusqlite::Connection::open(self.db()).unwrap();
        locker.execute_batch("BEGIN EXCLUSIVE;").unwrap();
        locker
    }

    fn add_folder(&self, state: &mut AppState) -> String {
        let folder = self.data.parent().unwrap().join("Docs");
        std::fs::create_dir_all(&folder).unwrap();
        bootstrap::add_source_expect_added(&self.catalog, &folder.to_string_lossy()).unwrap();
        state.update(&Message::SourcesLoaded(
            bootstrap::get_sources(&self.catalog).unwrap(),
        ));
        state.sources[0].source_id.clone()
    }

    fn add_history(&self, state: &mut AppState, texts: &[&str]) {
        let repo = SearchHistoryRepository::new(&self.catalog);
        for text in texts {
            repo.upsert(text, &[], Some(1), "en", &SearchHistorySettings::default())
                .unwrap();
        }
        state.update(&Message::HistoryLoaded(history::load_history(
            &self.catalog,
        )));
    }

    fn history_rows(&self) -> usize {
        SearchHistoryRepository::new(&self.catalog)
            .list()
            .unwrap()
            .len()
    }
}

fn retry(state: &AppState) -> Option<Message> {
    state.notice_action.as_deref().cloned()
}

// ── Row 1: clear recent searches ─────────────────────────────────────────

#[test]
fn a_clear_that_fails_keeps_the_list_and_says_so() {
    let profile = Profile::new();
    let mut state = AppState::default();
    profile.add_history(&mut state, &["alpha", "beta"]);
    state.update(&Message::AskClearRecentSearches);
    let _lock = profile.write_lock();

    clear_recent_searches(&profile.catalog, &mut state);

    assert!(
        !state.confirm_clear_history,
        "the confirmation closes whatever the outcome"
    );
    assert_eq!(
        state.search_ui.history.len(),
        2,
        "the list still shows the entries"
    );
    assert_eq!(state.notice, Some(UserNotice::RecentSearchesNotCleared));
    assert!(matches!(
        retry(&state),
        Some(Message::AskClearRecentSearches)
    ));
    drop(_lock);
    assert_eq!(profile.history_rows(), 2, "control: nothing was cleared");
}

#[test]
fn a_clear_that_succeeds_empties_the_list_and_says_so() {
    let profile = Profile::new();
    let mut state = AppState::default();
    profile.add_history(&mut state, &["alpha"]);

    clear_recent_searches(&profile.catalog, &mut state);

    assert!(state.search_ui.history.is_empty());
    assert_eq!(state.notice, Some(UserNotice::RecentSearchesCleared));
    assert_eq!(profile.history_rows(), 0);
}

// ── Row 7: remove one recent search ──────────────────────────────────────

#[test]
fn a_removal_that_fails_keeps_the_entry_and_offers_the_same_removal() {
    let profile = Profile::new();
    let mut state = AppState::default();
    profile.add_history(&mut state, &["alpha", "beta"]);
    let id = state.search_ui.history[0].id.clone();
    let _lock = profile.write_lock();

    remove_recent_search(&profile.catalog, &mut state, &id);

    assert!(
        state.search_ui.history.iter().any(|e| e.id == id),
        "the entry is still listed"
    );
    assert_eq!(state.notice, Some(UserNotice::RecentSearchNotRemoved));
    assert!(matches!(retry(&state), Some(Message::RemoveRecentSearch(r)) if r == id));
}

#[test]
fn a_removal_that_succeeds_drops_the_entry() {
    let profile = Profile::new();
    let mut state = AppState::default();
    profile.add_history(&mut state, &["alpha", "beta"]);
    let id = state.search_ui.history[0].id.clone();

    remove_recent_search(&profile.catalog, &mut state, &id);

    assert!(state.search_ui.history.iter().all(|e| e.id != id));
    assert_eq!(state.search_ui.history.len(), 1);
    assert_eq!(state.notice, None);
}

// ── Row 2: reset moved to `router/tests.rs` (Task 097) ───────────────────

// ── Rows 3 and 4: settings ───────────────────────────────────────────────

/// Row 3: `persist_locale` writes the catalog, so a write lock fails it.
#[test]
fn a_language_that_is_not_saved_says_so_and_offers_the_same_change() {
    let profile = Profile::new();
    let mut state = AppState::default();
    let _lock = profile.write_lock();

    persist_locale(&profile.catalog, &mut state, Locale::Ja);

    assert_eq!(state.notice, Some(UserNotice::SettingCouldNotBeSaved));
    assert!(matches!(
        retry(&state),
        Some(Message::PersistLocale(Locale::Ja))
    ));
}

/// Row 4: the settings file is written to `<data>/settings.json` (the data
/// override puts settings in the data folder, RFC-055). A directory at that
/// path makes the write fail for real, with no root caveat.
#[test]
fn a_remember_setting_that_is_not_saved_says_so_and_offers_the_same_change() {
    let profile = Profile::new();
    let mut state = AppState::default();
    let settings = profile.data.join("settings.json");
    let _ = std::fs::remove_file(&settings);
    std::fs::create_dir_all(&settings).unwrap();
    assert!(
        bootstrap::save_runtime_settings(&profile.context, &Default::default()).is_err(),
        "control: the settings file cannot be written"
    );

    toggle_remember_recent_searches(&profile.context, &profile.catalog, &mut state, true);

    assert_eq!(state.notice, Some(UserNotice::SettingCouldNotBeSaved));
    assert!(matches!(
        retry(&state),
        Some(Message::ToggleRememberRecentSearches(true))
    ));
}

/// Row 4, the history half: turning it off clears history, and when that
/// clear fails the list stays and the notice says so.
#[test]
fn turning_remember_off_when_the_clear_fails_keeps_the_list_and_says_so() {
    let profile = Profile::new();
    let mut state = AppState::default();
    profile.add_history(&mut state, &["alpha"]);
    let _lock = profile.write_lock();

    toggle_remember_recent_searches(&profile.context, &profile.catalog, &mut state, false);

    assert_eq!(
        state.search_ui.history.len(),
        1,
        "the list still shows the entry"
    );
    assert_eq!(state.notice, Some(UserNotice::RecentSearchesNotCleared));
}

// ── Row 5: Safe cleanups ─────────────────────────────────────────────────

/// Each cleanup writes the catalog before the cache
/// (`CleanupService::run_safe`), so a write lock fails every one of them.
#[test]
fn a_cleanup_that_fails_says_so_and_offers_that_same_cleanup() {
    for cleanup in [
        Message::CleanSnippets,
        Message::CleanSearchCache,
        Message::CleanTemporaryExtraction,
        Message::RemoveReplacedStaleIndexes,
    ] {
        let profile = Profile::new();
        let mut state = AppState::default();
        let _lock = profile.write_lock();

        run_cleanup(&profile.catalog, profile.cache(), &mut state, &cleanup);

        assert_eq!(
            state.notice,
            Some(UserNotice::CleanupDidNotFinish),
            "{cleanup:?}"
        );
        let action = retry(&state);
        assert_eq!(
            std::mem::discriminant(action.as_ref().unwrap()),
            std::mem::discriminant(&cleanup),
            "{cleanup:?}: the retry is the same cleanup, got {action:?}"
        );
    }
}

// ── Row 6: refresh a folder ──────────────────────────────────────────────

#[test]
fn a_refresh_that_fails_says_so_and_offers_the_same_refresh() {
    let profile = Profile::new();
    let mut state = AppState::default();
    let id = profile.add_folder(&mut state);
    let _lock = profile.write_lock();

    refresh_source(&profile.catalog, &mut state, &id);

    assert_eq!(state.notice, Some(UserNotice::FolderNotChecked));
    assert!(matches!(retry(&state), Some(Message::SourceRefreshRequested(r)) if r == id));
}
