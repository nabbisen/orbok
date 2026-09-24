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
use crate::ResetOutcome;
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

/// Task 096/097 test 1: with a reader forcing the busy path, `route` itself
/// -- the update thread -- must still return promptly. Before Task 096,
/// compaction ran inline inside this same call; before Task 097, the
/// delete itself still did (727.9 ms on a 600 MB catalog, Review 274 §4 --
/// the dominant cost, not compaction's 56.8 ms). 500 ms is the bound: an
/// order of magnitude below the 5 s timeout a synchronous checkpoint would
/// hit, and two orders above what deleting a handful of seeded rows
/// genuinely costs, so it cannot pass by accident on a slow CI runner --
/// only by the whole reset genuinely not running inline.
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
         -- the reset's own work must not run inline; took {elapsed:?}"
    );
    assert_eq!(
        task.units(),
        1,
        "the reset (then compaction, then measurement) must still be \
         dispatched as a real task, just not run synchronously here"
    );
}

/// Task 097 §3 test 2: the §0 second-half hazard, extended from Task 096's
/// own compaction-only proof to the whole reset -- moving work off the
/// update thread is not enough if it still takes the *shared* connection's
/// mutex, since a background thread holding that mutex for the busy
/// timeout blocks the update thread's next catalog access just the same.
/// Runs `reset_catalog_delete_compact_and_measure` (the exact function
/// `reset_task` wraps) on a background thread under forced contention, and
/// proves `deps.catalog` -- the shared one -- answers an ordinary read
/// promptly throughout.
///
/// **No corresponding mutation**, deliberately: unlike Task 096's
/// `compact_reset_files` (which took the shared catalog as a parameter it
/// promised never to touch -- Review 274 §3 flagged that as an invitation
/// for a future edit to "fix" the unused parameter and reintroduce the
/// freeze), `reset_catalog_delete_compact_and_measure` never receives the
/// shared catalog at all. There is no reference to misuse, so this
/// property is enforced by the function's own signature, not by a test
/// that could be deleted alongside a regression.
#[test]
fn the_shared_connection_stays_free_while_the_reset_runs_on_its_own() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 50);

    let reader = hold_a_read_transaction(deps.catalog.path());

    let runtime = deps.runtime.clone();
    let reset_thread =
        std::thread::spawn(move || crate::reset_catalog_delete_compact_and_measure(&runtime));

    // Give the reset thread time to actually reach the checkpoint and
    // start contending, so the read below measures real overlap rather
    // than a race at start-up.
    std::thread::sleep(std::time::Duration::from_millis(300));

    let start = std::time::Instant::now();
    let _: i64 = deps
        .catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    let elapsed = start.elapsed();

    reader.execute_batch("COMMIT;").unwrap();
    reset_thread.join().unwrap();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "the shared connection's own read must not be blocked while the \
         reset contends on its own, separate connection; took {elapsed:?}"
    );
}

/// Task 096/097 test 4: the Storage measurement taken after a reset must
/// reflect the *compacted* file size, not whatever was on disk before
/// compaction ran -- still true now that the delete moved off-thread too,
/// since `reset_catalog_delete_compact_and_measure` runs all three steps
/// in that plain sequential order, on the one connection it opens.
#[test]
fn the_post_reset_measurement_sees_the_compacted_catalog_size() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 4000);

    let before = std::fs::metadata(deps.catalog.path()).unwrap().len();
    assert!(
        before > 512 * 1024,
        "baseline: must be meaningfully large before reset, or this test \
         proves nothing (was {before} bytes)"
    );

    // The exact function `reset_task` wraps -- proves the real wiring's
    // order, not a reimplementation of it.
    let outcome = crate::reset_catalog_delete_compact_and_measure(&deps.runtime);
    let ResetOutcome::Succeeded {
        rows,
        cache_file_bytes: _,
    } = outcome
    else {
        panic!("expected a successful reset, got {outcome:?}");
    };

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

// ── Task 097 §3 test 3: the outcome is unchanged (migrated from
// `backend_actions/tests.rs`'s own Row 2, Review 253 §2.4 -- the write-lock
// scenario there is dropped: the fresh connection this task opens per
// attempt uses the standard 5 s busy timeout, not the 50 ms
// `Profile::new()` set on its own, single, long-lived connection there, so
// reusing that mechanism here would make an equivalent test ~5 s slower
// for coverage `orbok-db`'s own busy-checkpoint test already provides) ──

/// (a) The reload read fails too: the `sources` table is moved aside by a
/// second connection, so the reset fails on it and so does the reload. No
/// list to show.
#[test]
fn a_reset_whose_reload_also_fails_reports_no_sources_to_show() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    bootstrap::add_source_expect_added(&deps.catalog, &source_dir.to_string_lossy()).unwrap();

    let mover = rusqlite::Connection::open(deps.catalog.path()).unwrap();
    mover
        .execute_batch("ALTER TABLE sources RENAME TO sources_moved_aside;")
        .unwrap();

    let outcome = crate::reset_catalog_delete_compact_and_measure(&deps.runtime);

    let read_failed = bootstrap::get_sources(&deps.catalog).is_err();
    mover
        .execute_batch("ALTER TABLE sources_moved_aside RENAME TO sources;")
        .unwrap();
    assert!(
        read_failed,
        "control: the reload's read fails while the table is aside"
    );

    match outcome {
        ResetOutcome::Failed { sources: None } => {}
        other => panic!("expected Failed {{ sources: None }}, got {other:?}"),
    }
}

/// (b) The catalog step commits and the cache purge fails: a directory
/// sits where the cache database file belongs, so the purge cannot open
/// it. The reset reports failure, and the reload shows the truth -- the
/// catalog no longer holds the folder, so the list is empty.
#[test]
fn a_reset_that_fails_after_the_catalog_step_reports_what_the_catalog_holds() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    bootstrap::add_source_expect_added(&deps.catalog, &source_dir.to_string_lossy()).unwrap();
    let cache_db = temp.path().join(orbok_db::CACHE_FILE_NAME);
    std::fs::create_dir_all(&cache_db).unwrap();

    let outcome = crate::reset_catalog_delete_compact_and_measure(&deps.runtime);

    assert!(
        bootstrap::get_sources(&deps.catalog).unwrap().is_empty(),
        "control: the catalog step committed"
    );
    match outcome {
        ResetOutcome::Failed {
            sources: Some(cards),
        } => {
            assert!(
                cards.is_empty(),
                "the reload shows what the catalog holds: no folders"
            );
        }
        other => panic!("expected Failed {{ sources: Some([]) }}, got {other:?}"),
    }
}

/// (c) A reset that succeeds clears everything.
#[test]
fn a_reset_that_succeeds_reports_success_and_an_empty_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    bootstrap::add_source_expect_added(&deps.catalog, &source_dir.to_string_lossy()).unwrap();

    let outcome = crate::reset_catalog_delete_compact_and_measure(&deps.runtime);

    assert!(matches!(outcome, ResetOutcome::Succeeded { .. }));
    assert!(bootstrap::get_sources(&deps.catalog).unwrap().is_empty());
}

/// Task 097 §3 mutation target ("success dispatched before the delete
/// completes"): a genuine failure (the table-moved-aside trick above) must
/// never be reported as `Succeeded`. Mutating
/// `reset_catalog_delete_compact_and_measure` to ignore
/// `bootstrap::reset_catalog`'s `Result` and always proceed as if it were
/// `Ok` makes this fail -- see the review request for the mutation run.
#[test]
fn a_failed_delete_is_never_reported_as_succeeded() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mover = rusqlite::Connection::open(deps.catalog.path()).unwrap();
    mover
        .execute_batch("ALTER TABLE sources RENAME TO sources_moved_aside;")
        .unwrap();

    let outcome = crate::reset_catalog_delete_compact_and_measure(&deps.runtime);

    mover
        .execute_batch("ALTER TABLE sources_moved_aside RENAME TO sources;")
        .unwrap();

    assert!(
        !matches!(outcome, ResetOutcome::Succeeded { .. }),
        "a delete that never committed must not be reported as succeeded; got {outcome:?}"
    );
}

// ── Task 097 §2 measurement: how long the window is in the in-flight
// state, on the same 100,000-file profile Task 095/096 measured -- not a
// gate, `#[ignore]`d. Run:
// `cargo test -p orbok --release --bin orbok router::tests::task097_measure_the_in_flight_window -- --ignored --nocapture`

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn task097_measure_the_in_flight_window() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 100_000);

    let start = std::time::Instant::now();
    let outcome = crate::reset_catalog_delete_compact_and_measure(&deps.runtime);
    let elapsed = start.elapsed();

    assert!(matches!(outcome, ResetOutcome::Succeeded { .. }));
    println!(
        "reset_catalog_delete_compact_and_measure (delete + compact + measure, off the \
         update thread) took {elapsed:?} on a 100,000-file profile"
    );
}

// ── Task 099 §5 test 5: rebuild actions run off the update thread ──────
// (RFC-011 §14 criteria 5/6, Task 097's shape) ─────────────────────────

/// `route()` itself must return promptly for `ConfirmDeleteKeywordIndex`
/// even under checkpoint/write contention -- the delete-then-backfill work
/// must not run inline on the update thread. Same bound and reasoning as
/// `confirming_a_reset_returns_promptly_even_when_a_reader_would_block_compaction`.
#[test]
fn confirming_a_keyword_rebuild_returns_promptly_even_when_a_reader_would_block() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 50);

    let reader = hold_a_read_transaction(deps.catalog.path());

    let mut app = OrbokApp::with_state(AppState::default());
    let start = std::time::Instant::now();
    let task = route(&mut app, Message::ConfirmDeleteKeywordIndex, &deps);
    let elapsed = start.elapsed();

    reader.execute_batch("COMMIT;").unwrap();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "route() must return promptly regardless of checkpoint contention -- the keyword \
         rebuild's own work must not run inline; took {elapsed:?}"
    );
    assert_eq!(
        task.units(),
        1,
        "the delete-then-backfill (then measurement) must still be dispatched as a real task"
    );
}

/// Same proof, `ConfirmDeleteVectorIndex`.
#[test]
fn confirming_a_vector_rebuild_returns_promptly_even_when_a_reader_would_block() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    seed_bulk_files(&deps.catalog, 50);

    let reader = hold_a_read_transaction(deps.catalog.path());

    let mut app = OrbokApp::with_state(AppState::default());
    let start = std::time::Instant::now();
    let task = route(&mut app, Message::ConfirmDeleteVectorIndex, &deps);
    let elapsed = start.elapsed();

    reader.execute_batch("COMMIT;").unwrap();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "route() must return promptly regardless of checkpoint contention -- the vector \
         rebuild's own work must not run inline; took {elapsed:?}"
    );
    assert_eq!(
        task.units(),
        1,
        "the delete-then-backfill (then measurement) must still be dispatched as a real task"
    );
}

/// §5 test 6 / Task 075: a failed rebuild (a moved-aside table forces the
/// delete itself to fail) raises `CleanupDidNotFinish` with `Try again`
/// re-opening the confirmation -- never silently swallowed, never
/// reported as if it had succeeded.
#[test]
fn a_failed_keyword_rebuild_raises_cleanup_did_not_finish_with_retry() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mover = rusqlite::Connection::open(deps.catalog.path()).unwrap();
    mover
        .execute_batch("ALTER TABLE keyword_index_records RENAME TO keyword_index_records_moved;")
        .unwrap();

    let outcome = crate::delete_keyword_index_and_measure(&deps.runtime);

    mover
        .execute_batch("ALTER TABLE keyword_index_records_moved RENAME TO keyword_index_records;")
        .unwrap();

    assert!(
        matches!(outcome, crate::RebuildOutcome::Failed),
        "a delete that could not run must be reported as Failed, got {outcome:?}"
    );
    let messages = crate::rebuild_outcome_to_messages(outcome, Message::AskDeleteKeywordIndex);
    assert!(
        messages.iter().any(|m| matches!(
            m,
            Message::ShowNoticeWithAction { notice, action }
                if *notice == orbok_ui::notice::UserNotice::CleanupDidNotFinish
                    && matches!(action.as_ref(), Message::AskDeleteKeywordIndex)
        )),
        "expected a CleanupDidNotFinish notice retrying AskDeleteKeywordIndex, got {messages:?}"
    );
}

// ── Task 105: choosing a folder always does something ───────────────────

fn source_rows(deps: &AppDeps) -> i64 {
    deps.catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))
        .unwrap()
}

fn typed(app: &mut OrbokApp, deps: &AppDeps, text: &str) -> iced::Task<Message> {
    app.update(Message::SourcePathChanged(text.to_string()));
    route(app, Message::SubmitSourcePath, deps)
}

/// §2.1: Enter in the path field with a real directory's path adds it, clears
/// the field, and opens no picker.
#[test]
fn a_typed_path_is_added_and_no_picker_opens() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let folder = temp.path().join("notes");
    std::fs::create_dir(&folder).unwrap();
    let mut app = OrbokApp::with_state(AppState::default());

    let task = typed(&mut app, &deps, &folder.to_string_lossy());

    assert_eq!(task.units(), 0, "no picker task is returned");
    assert_eq!(app.state.sources.len(), 1, "the folder was added");
    assert_eq!(source_rows(&deps), 1);
    assert!(app.state.source_path_input.is_empty(), "the field clears");
}

/// §2.3: a bad path raises the failure notice, keeps the typed text, adds
/// nothing.
#[test]
fn a_bad_typed_path_is_kept_for_correction_and_adds_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState::default());
    let bad = temp
        .path()
        .join("no-such-folder")
        .to_string_lossy()
        .to_string();

    let task = typed(&mut app, &deps, &bad);

    assert_eq!(task.units(), 0);
    assert_eq!(
        app.state.notice,
        Some(orbok_ui::notice::UserNotice::FolderCouldNotBeAdded)
    );
    assert_eq!(app.state.source_path_input, bad, "the text is unchanged");
    assert_eq!(source_rows(&deps), 0);
}

/// Task 105 §1.1: only a folder can be added. A typed file path is refused
/// with the same notice, and no row is created (single files were dropped,
/// RFC-003 Amendment 1).
#[test]
fn a_typed_file_path_is_refused_and_creates_no_row() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let file = temp.path().join("one.md");
    std::fs::write(&file, "# one\n").unwrap();
    let mut app = OrbokApp::with_state(AppState::default());

    let _ = typed(&mut app, &deps, &file.to_string_lossy());

    assert_eq!(
        app.state.notice,
        Some(orbok_ui::notice::UserNotice::FolderCouldNotBeAdded)
    );
    assert_eq!(source_rows(&deps), 0, "no row for a file");
    assert!(app.state.sources.is_empty());
}

/// §2.4: Enter with an empty (or blank) field does nothing.
#[test]
fn enter_in_an_empty_path_field_does_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState::default());

    for text in ["", "   "] {
        let task = typed(&mut app, &deps, text);
        assert_eq!(task.units(), 0, "no task for {text:?}");
        assert!(app.state.notice.is_none(), "no notice for {text:?}");
    }
    assert_eq!(source_rows(&deps), 0);
}

/// §2.5: the Add folder button still opens the picker, and only it does.
#[test]
fn the_button_opens_the_picker_and_the_field_does_not() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState::default());

    let task = route(&mut app, Message::RequestAddSource, &deps);
    assert_eq!(task.units(), 1, "the button opens the picker");
    assert!(app.state.add_source_picker_in_progress);
}

/// §2.6: "Choose a folder" opens the search picker and respects the
/// one-picker flag: a second press while it is open does nothing.
#[test]
fn choose_a_folder_opens_the_picker_once() {
    let temp = tempfile::tempdir().unwrap();
    let deps = test_deps(temp.path());
    let mut app = OrbokApp::with_state(AppState {
        query: "meeting notes".into(),
        ..AppState::default()
    });

    let first = route(&mut app, Message::ChooseSearchFolder, &deps);
    assert_eq!(first.units(), 1, "the picker task");
    assert!(app.state.search_location.picker_in_progress);
    assert_eq!(
        app.state.search_location.pending_query.as_deref(),
        Some("meeting notes"),
        "the query is kept for after the pick"
    );

    let second = route(&mut app, Message::ChooseSearchFolder, &deps);
    assert_eq!(second.units(), 0, "a second press does nothing");
}

/// §2.7: the picker's arm and the typed path's arm both call the one
/// routine, so a notice, the already-added check and the scan cannot differ.
#[test]
fn both_add_arms_call_the_one_routine() {
    let source = include_str!("../router.rs");
    let arm = |start: &str, end: &str| {
        let from = source.find(start).unwrap();
        let to = from + source[from..].find(end).unwrap();
        &source[from..to]
    };
    for (name, body) in [
        (
            "AddSourceFolderPicked",
            arm(
                "Message::AddSourceFolderPicked(folder) =>",
                "Message::SubmitSourcePath =>",
            ),
        ),
        (
            "SubmitSourcePath",
            arm(
                "Message::SubmitSourcePath =>",
                "Message::AddSourceFolderPickerCancelled =>",
            ),
        ),
    ] {
        assert!(
            body.contains("add_folder_from_path("),
            "the {name} arm must call add_folder_from_path"
        );
    }
}
