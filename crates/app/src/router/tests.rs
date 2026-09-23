//! Task 084 §2: `route` is now a plain function, so these are the first
//! tests to call it directly -- each proves a claim an earlier task
//! (073, 075, 081) could not test, because `main.rs`'s closure could not
//! be driven.
//!
//! `iced::Task::units()` is the discriminator used throughout: `Task::none()`
//! carries `units() == 0`; `Task::done`/`Task::perform` carry `units() == 1`.
//! An arm whose correct behaviour dispatches real work (a deferred message,
//! a storage measurement) is pinned by asserting `units() == 1`; the exact
//! mutation §2 names -- deleting the early `return` so the arm falls
//! through to the closure's final `app.update(message); Task::none()` --
//! always drops that to `0`, since the generic path never issues async
//! work of its own for these messages.
//!
//! Four of the seven arms §2 test 1 names (clear-history, refresh,
//! settings, launch) have no such discriminator: their correct behaviour
//! *also* returns `Task::none()`, and every reducer arm behind them
//! (`SourceRefreshRequested`, `OpenResult`/`RevealResult`,
//! `ConfirmClearRecentSearches`, `PersistLocale`) is written to be a safe
//! no-op or an idempotent set specifically so a stray extra `app.update`
//! call is harmless -- this project's existing defence against exactly the
//! failure class this task closes (see each reducer arm's own comment in
//! `crates/ui/src/state.rs`). For those four, falling through is
//! genuinely unobservable through `AppState` or the returned `Task` as
//! they stand today: Task 073's review request (250 §2) reached the same
//! conclusion for `SourceRemoved` alone ("Not covered by any test... the
//! reducer arm now empty, that fall-through would change nothing").
//! Rather than write a test that cannot fail, each of those four gets a
//! positive test proving `route` dispatches its real backend effect
//! (persistence, a catalog write, a raised notice) -- and the gap is
//! named here and in the review request instead of hidden.

use super::*;
use crate::bootstrap;
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeSelection};
use orbok_ui::i18n::Locale;
use orbok_ui::state::{AppState, Message, ResultTrustDisplay, SearchResultDisplay, ViewId};

fn test_deps(dir: &std::path::Path) -> AppDeps {
    let runtime = RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(dir.as_os_str().to_os_string())).unwrap(),
        dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(dir),
            standard_settings_dir: Some(dir),
        },
    )
    .unwrap();
    let catalog = Arc::new(bootstrap::open_catalog(&runtime).unwrap());
    let search_cache = Arc::new(bootstrap::cache_service(&runtime).ok());
    let (resource_signal_tx, _resource_signal_rx) =
        futures::channel::mpsc::channel::<scheduler_host::ResourceObservation>(16);
    AppDeps {
        runtime,
        catalog,
        search_model: Arc::new(search_model::SearchModel::new(None)),
        search_cache,
        resource_signal_tx,
        active_download_cancel: Arc::new(Mutex::new(None)),
    }
}

fn result_for(canonical_path: &str) -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: canonical_path.to_string(),
        canonical_path: canonical_path.to_string(),
        title: None,
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: ResultTrustDisplay::default(),
    }
}

// ── §2 test 1: the routing returns (three arms with a real discriminator) ─

#[test]
fn removal_dispatches_the_deferred_message_not_a_fallthrough_update() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState {
        confirm_remove_source: Some("src-1".into()),
        ..AppState::default()
    });
    let task = route(&mut app, Message::ConfirmRemoveSource, &deps);
    assert_eq!(
        task.units(),
        1,
        "a pending removal must dispatch SourceRemoved as a real task, \
         not fall through to Task::none()"
    );
    assert!(
        app.state.confirm_remove_source.is_none(),
        "take_confirmed_removal always clears the pending id"
    );
}

#[test]
fn reset_dispatches_a_measurement_task_not_a_fallthrough_update() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState::default());
    let task = route(&mut app, Message::ConfirmResetCatalog, &deps);
    assert_eq!(
        task.units(),
        1,
        "a confirmed reset must dispatch a fresh storage measurement, \
         not fall through to Task::none()"
    );
}

#[test]
fn cleanup_dispatches_a_measurement_task_not_a_fallthrough_update() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    for cleanup in [
        Message::CleanSnippets,
        Message::CleanSearchCache,
        Message::CleanTemporaryExtraction,
        Message::RemoveReplacedStaleIndexes,
    ] {
        let mut app = OrbokApp::with_state(AppState::default());
        let task = route(&mut app, cleanup.clone(), &deps);
        assert_eq!(
            task.units(),
            1,
            "{cleanup:?} must dispatch a fresh storage measurement, \
             not fall through to Task::none()"
        );
    }
}

/// §2 test 3: `NoticeActionPressed` is handled before the generic path --
/// the same discriminator as the three tests above. The mutation this
/// pins is different from theirs (§2's own list): moving the *generic
/// fall-through* above this check, not deleting this arm's own `return`.
#[test]
fn notice_action_pressed_dispatches_the_stored_retry_not_a_fallthrough_update() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState::default());
    app.state.update(&Message::ShowNoticeWithAction {
        notice: orbok_ui::notice::UserNotice::StorageUnavailable,
        action: Box::new(Message::CleanSnippets),
    });
    let task = route(&mut app, Message::NoticeActionPressed, &deps);
    assert_eq!(
        task.units(),
        1,
        "a pending notice action must be dispatched as a real task, \
         not fall through to Task::none()"
    );
    assert!(
        app.state.notice_action.is_none(),
        "take_notice_action always clears the stored retry"
    );
}

// ── §2 test 1: the remaining four arms (no Task/state discriminator) ──────
//
// Each proves `route` reaches the real backend effect. None can also prove
// "and app.update(message) was not additionally called" -- see this file's
// module doc comment for why that is not a gap in these tests.

#[test]
fn clear_history_actually_clears_the_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let settings = bootstrap::load_runtime_settings(&deps.runtime).unwrap_or_default();
    history::record_search(
        &deps.catalog,
        &settings.privacy_settings(),
        &settings.history_settings(),
        "a past query",
        &[],
        1,
        &settings.locale,
    );
    assert_eq!(
        history::load_history(&deps.catalog).len(),
        1,
        "baseline: the entry must exist before it is cleared"
    );
    let mut app = OrbokApp::with_state(AppState::default());
    let task = route(&mut app, Message::ConfirmClearRecentSearches, &deps);
    assert_eq!(task.units(), 0, "clearing history is synchronous");
    assert!(
        history::load_history(&deps.catalog).is_empty(),
        "the catalog's history must actually be cleared, not just the UI list"
    );
}

#[test]
fn refresh_actually_reloads_sources_from_the_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&deps.catalog, &source_dir.to_string_lossy()).unwrap();
    let mut app = OrbokApp::with_state(AppState::default());
    assert!(
        app.state.sources.is_empty(),
        "baseline: nothing loaded into UI state yet"
    );
    let task = route(
        &mut app,
        Message::SourceRefreshRequested(card.source_id.clone()),
        &deps,
    );
    assert_eq!(task.units(), 0, "the refresh runs synchronously");
    assert_eq!(
        app.state.sources.len(),
        1,
        "the refresh must load the real source list from the catalog, \
         not leave the UI's list empty"
    );
}

#[test]
fn settings_actually_persists_the_new_locale() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState::default());
    let task = route(&mut app, Message::PersistLocale(Locale::Ja), &deps);
    assert_eq!(task.units(), 0, "persisting a setting runs synchronously");
    assert_eq!(app.state.locale, Locale::Ja);
    let stored = orbok_db::repo::SettingsRepository::new(&deps.catalog)
        .get::<String>("ui.locale")
        .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some(Locale::Ja.as_str()),
        "the new locale must actually be written to the catalog, not only held in memory"
    );
}

#[test]
fn launch_raises_the_real_refusal_notice_for_a_path_outside_every_source() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let outside = temp.path().join("outside.md");
    std::fs::write(&outside, "not in any source").unwrap();
    let mut app = OrbokApp::with_state(AppState {
        search_results: vec![result_for(&outside.to_string_lossy())],
        ..AppState::default()
    });
    let task = route(&mut app, Message::OpenResult(0), &deps);
    assert_eq!(task.units(), 0, "a refused launch runs synchronously");
    assert_eq!(
        app.state.notice,
        Some(orbok_ui::notice::UserNotice::FileCouldNotBeFound),
        "a path outside every registered source must raise the real \
         refusal notice, proving route reached result_launch"
    );
}

// ── §2 test 2: the Storage measurement is asked for ───────────────────────

/// Task 081's untestable wiring, closed: switching to Storage, finishing a
/// cleanup, and finishing a reset each dispatch a real measurement task.
#[test]
fn storage_measurement_is_requested_by_switching_cleaning_and_resetting() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());

    let mut switched = OrbokApp::with_state(AppState::default());
    let task = route(&mut switched, Message::Switch(ViewId::Storage), &deps);
    assert_eq!(
        task.units(),
        1,
        "switching to Storage must dispatch a measurement"
    );
    assert_eq!(switched.state.active_view, ViewId::Storage);

    let mut cleaned = OrbokApp::with_state(AppState::default());
    let task = route(&mut cleaned, Message::CleanSnippets, &deps);
    assert_eq!(
        task.units(),
        1,
        "finishing a cleanup must dispatch a measurement"
    );

    let mut reset = OrbokApp::with_state(AppState::default());
    let task = route(&mut reset, Message::ConfirmResetCatalog, &deps);
    assert_eq!(
        task.units(),
        1,
        "finishing a reset must dispatch a measurement"
    );
}

// ── Task 096: compaction never freezes the window ──────────────────────

/// Inflate the catalog with schema-valid file rows, fast -- one source,
/// `count` files, one transaction, padded columns. The same shape Task
/// 095's own `seed_bulk_files` uses (`orbok-workers`'s tests), duplicated
/// here rather than shared across crates for one small helper.
fn seed_bulk_files(catalog: &orbok_db::Catalog, count: usize) {
    let source_id = orbok_db::repo::SourceRepository::new(catalog)
        .insert(orbok_db::repo::NewSource {
            source_type: orbok_core::SourceType::File,
            persistence_mode: orbok_core::PersistenceMode::Persistent,
            display_name: Some("bulk".into()),
            original_path: "/bulk".into(),
            canonical_path: "/bulk".into(),
            index_mode: orbok_core::IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: orbok_core::HiddenFilePolicy::Exclude,
            symlink_policy: orbok_core::SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap()
        .source_id;

    let padding = "x".repeat(600);
    let mut conn = catalog.lock();
    let tx = conn.transaction().unwrap();
    for i in 0..count {
        tx.execute(
            "INSERT INTO files (file_id, source_id, original_path, canonical_path, \
             display_path, extension, file_size_bytes, modified_at, platform_file_key, \
             content_hash, hash_algorithm, file_status, last_seen_at, last_scanned_at, \
             created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,'md',1024,'2026-01-01T00:00:00Z',NULL,?6,'sha256', \
             'indexed','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            rusqlite::params![
                format!("file_{i}_{padding}"),
                source_id.as_str(),
                format!("/bulk/file_{i}_{padding}.md"),
                format!("/bulk/file_{i}_{padding}.md"),
                format!("file_{i}.md"),
                format!("{padding}{i}"),
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

/// Force whichever connection next attempts a `wal_checkpoint(TRUNCATE)`
/// against `path` into the busy path -- the same mechanism
/// `orbok-db`'s own `vacuum_logs_a_warning_when_another_connection_blocks_the_checkpoint`
/// uses: a genuine second connection to the same on-disk file, holding an
/// open read transaction. Not a mock -- this is the real condition a
/// lingering scheduler-host query produces.
fn hold_a_read_transaction(path: &std::path::Path) -> rusqlite::Connection {
    let reader = rusqlite::Connection::open(path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT * FROM schema_migrations LIMIT 1;")
        .unwrap();
    reader
}

/// Task 096 §2 test 1: with a reader forcing the busy path, `route` itself
/// -- the update thread -- must still return promptly. Before Task 096,
/// compaction ran inline inside this same call, so this contention alone
/// made it take the full 5 s busy timeout (confirmed directly: reverting
/// `ConfirmResetCatalog`'s arm to call `compact_reset_files` synchronously
/// before returning reproduces exactly that). 500 ms is the bound: an
/// order of magnitude below the 5 s timeout a synchronous checkpoint would
/// hit, and two orders above what deleting a handful of seeded rows
/// genuinely costs, so it cannot pass by accident on a slow CI runner --
/// only by compaction genuinely not running inline.
#[test]
fn confirming_a_reset_returns_promptly_even_when_a_reader_would_block_compaction() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 50);

    let reader = hold_a_read_transaction(deps.catalog.path());

    let mut app = OrbokApp::with_state(AppState::default());
    let start = std::time::Instant::now();
    let task = route(&mut app, Message::ConfirmResetCatalog, &deps);
    let elapsed = start.elapsed();

    reader.execute_batch("COMMIT;").unwrap();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "route() must return promptly regardless of checkpoint contention \
         -- compaction must not run inline; took {elapsed:?}"
    );
    assert_eq!(
        task.units(),
        1,
        "compaction (then measurement) must still be dispatched as a \
         real task, just not run synchronously here"
    );
}

/// Task 096 §2 test 2: the §0 second-half hazard -- moving compaction off
/// the update thread is not enough if it still takes the *shared*
/// connection's mutex, since a background thread holding that mutex for
/// the busy timeout blocks the update thread's next catalog access just
/// the same. Forces `compact_reset_files`'s own contention (the reader),
/// runs it on a background thread against a *fresh* handle, and proves
/// `deps.catalog` -- the shared one -- answers an ordinary read promptly
/// throughout.
#[test]
fn the_shared_connection_stays_free_while_compaction_runs_on_its_own() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());

    let reader = hold_a_read_transaction(deps.catalog.path());

    let runtime = deps.runtime.clone();
    let shared = Arc::clone(&deps.catalog);
    let compaction_thread =
        std::thread::spawn(move || crate::compact_reset_files(&runtime, &shared));

    // Give the compaction thread time to actually reach the checkpoint and
    // start contending, so the read below measures real overlap rather
    // than a race at start-up.
    std::thread::sleep(std::time::Duration::from_millis(300));

    let start = std::time::Instant::now();
    let _: i64 = deps
        .catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
        .unwrap();
    let elapsed = start.elapsed();

    reader.execute_batch("COMMIT;").unwrap();
    compaction_thread.join().unwrap();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "the shared connection's own read must not be blocked while \
         compaction contends on its own, separate connection; took {elapsed:?}"
    );
}

/// Task 096 §2 test 4: the Storage measurement taken after a reset must
/// reflect the *compacted* file size, not whatever was on disk the
/// instant compaction started.
#[test]
fn the_post_reset_measurement_sees_the_compacted_catalog_size() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 4000);

    // The real sequence: delete (what `bootstrap::reset_catalog` does,
    // synchronously, on the update thread), *then* compact -- a plain
    // `DELETE` never shrinks the file on its own (RFC-059 §10 criterion
    // 6), so `before` here is deliberately measured after the delete, not
    // before it, to isolate what compaction alone changes.
    let cache = bootstrap::cache_service(&deps.runtime).unwrap();
    bootstrap::reset_catalog(&deps.catalog, &cache).unwrap();

    let before = std::fs::metadata(deps.catalog.path()).unwrap().len();
    assert!(
        before > 512 * 1024,
        "baseline: must be meaningfully large before compaction, or this \
         test proves nothing (was {before} bytes)"
    );

    // The exact function `compact_reset_and_measure_task` wraps -- proves
    // the real wiring's order, not a reimplementation of it.
    let (rows, _cache_file_bytes) = crate::compact_then_measure(&deps.runtime, &deps.catalog);

    let after = std::fs::metadata(deps.catalog.path()).unwrap().len();
    assert!(
        after < before / 4,
        "baseline: compaction must have actually shrunk the file, or this \
         test proves nothing (before={before} after={after})"
    );

    let persistent = rows
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::PersistentCatalog)
        .map(|(_, m)| *m)
        .unwrap();
    assert_eq!(
        persistent,
        orbok_core::StorageMeasurement::Measured {
            bytes: after,
            items: 0,
        },
        "the measurement must reflect the compacted file size ({after} bytes), \
         not the pre-compaction one ({before} bytes)"
    );
}
