//! orbok application binary.
//!
//! Startup sequence (RFC-027, design §startup):
//! 1. parse flags (--version, --portable, --check)
//! 2. resolve data directory
//! 3. open catalog, run migrations, run startup recovery (RFC-018)
//! 4. load OrbokSettings, verify model files → build AppState
//! 5. if wizard active: show wizard until resolved or skipped
//! 6. launch main GUI

mod bootstrap;
mod diagnostics;
mod download;
mod history;
mod model_flow;
#[cfg(test)]
mod runtime_isolation_tests;
mod scheduler_host;
mod settings;
#[cfg(test)]
mod wired_application_tests;

use orbok_ui::i18n::{dialog_title_add_source, dialog_title_choose_search_folder};
use orbok_ui::state::{WizardFileCheck, WizardState};
use orbok_ui::{KeyboardContext, Message, OrbokApp, key_to_message};
use orbok_workers::model_verifier::REQUIRED_MODEL_FILES;
use orbok_workers::{VerifyOutcome, verify_embedding_model};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    install_panic_hook();

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("orbok {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let portable = args.iter().any(|a| a == "--portable");
    let runtime = bootstrap::resolve_runtime_context(portable)?;
    if portable {
        eprintln!("orbok: portable mode — data directory: ./orbok-data/");
    }
    if args.iter().any(|a| a == "--check") {
        bootstrap::run_check(&runtime)?;
        return Ok(());
    }

    let state = bootstrap::load_initial_state(&runtime)?;

    // RFC-061 §5 Slice 1: one `Catalog` for the whole `update` closure's
    // lifetime, opened once here rather than once per message. This is a
    // deliberately separate connection from the one `load_initial_state`
    // just opened and dropped internally above -- that one runs entirely
    // before the event loop starts (crash recovery, the startup rescan,
    // model resolution), so it is not the "per-message" cost this slice
    // targets, and threading it through would touch `load_initial_state`'s
    // signature and every test that calls it for no benefit this RFC asks
    // for. `Arc` rather than a bare reference: several branches below hand
    // it to a spawned task (`tokio::spawn`/`iced::Task::perform`) that must
    // outlive this synchronous closure invocation.
    let catalog = std::sync::Arc::new(bootstrap::open_catalog(&runtime)?);

    // RFC-061 §6 Slice 4: resolve the embedding model once for the whole
    // process, the same way `scheduler_host::run` already resolves its own
    // (separate -- indexing and search are different processes' worth of
    // work sharing one binary, not one model instance) copy once at the top
    // of its loop, instead of `bootstrap::search::run_search` loading a
    // fresh model (a full deserialize off disk) on every single search.
    // `None` when no model is configured or the backend fails to load --
    // `run_search` treats that as keyword-only, mirroring
    // `resolve_embedding_worker_parts`'s own `model_missing` fallback on
    // the indexing side.
    let search_settings = bootstrap::load_runtime_settings(&runtime).unwrap_or_default();
    // `Arc`, not a bare `Option<EmbeddingWorkerParts>`: RFC-061 §7 Slice 5
    // moves `run_search`'s call sites onto `iced::Task::perform`, so each
    // one needs its own cheap, owned handle to move into a `'static` async
    // block -- the same reason `catalog` above is an `Arc`.
    let search_model = std::sync::Arc::new(
        bootstrap::embedding_resolution::resolve_embedding_worker_parts(
            &runtime,
            &orbok::runtime_context::AllowRuntimePathProbe,
            &catalog,
            &search_settings,
        ),
    );

    // RFC-057 §4.1: the resource-observation channel. Constructed once
    // here, not inside the `.subscription(..)` closure below (which iced
    // re-evaluates every frame), so `update` can hold a stable `Sender`
    // for the app's whole lifetime; the receiver reaches the spawned task
    // through `SchedulerSubscriptionData` (a plain `fn(&D) -> S` cannot
    // capture it directly -- see that type's own doc comment).
    let (resource_signal_tx, resource_signal_rx) =
        futures::channel::mpsc::channel::<scheduler_host::ResourceObservation>(16);
    let resource_signals = std::sync::Arc::new(std::sync::Mutex::new(Some(resource_signal_rx)));
    // A second clone for the `.subscription(..)` closure below (RFC-057
    // §4.3d): `update`'s own `move` closure takes ownership of the
    // original, so the battery poller -- handed to `run` via
    // `SchedulerSubscriptionData` -- needs its own.
    let battery_resource_signal_tx = resource_signal_tx.clone();

    // Task 025 §4.1: the cooperative cancellation flag for whichever
    // download is currently in flight, if any. Held here (mirroring
    // `resource_signals` above) rather than in `AppState`, since it is a
    // cross-task handle, not UI state; `AppState::update`/`model_flow.rs`
    // only ever see the `cancelling` bool they already own.
    // `StartManagedDownload` replaces the contents; `CancelManagedDownload`
    // reads them. A new download can only start once the wizard has left
    // `Downloading` for the prior attempt (`cancelling` gates that -- see
    // `WizardState::Downloading::cancelling`'s doc comment), so this is
    // never overwritten while still in use.
    let active_download_cancel: std::sync::Arc<
        std::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    > = std::sync::Arc::new(std::sync::Mutex::new(None));

    iced::application(
        move || OrbokApp::with_state(state.clone()),
        move |app: &mut OrbokApp, message: Message| -> iced::Task<Message> {
            // RFC-057 §4.2: search input/submission are RFC-036 §13.1's
            // user-activity signals. Best effort -- `try_send` never
            // blocks `update`. Each `clone()` gives this call its own
            // guaranteed slot beyond the channel's buffer (futures' bounded
            // mpsc), so this essentially never fails; the buffer can
            // transiently hold more than 16 as a result. Harmless either
            // way -- the loop drains it fully every iteration -- and
            // typing produces many observations in quick succession, so
            // even a dropped one would change nothing observable.
            if matches!(message, Message::QueryChanged(_) | Message::SubmitSearch) {
                let _ = resource_signal_tx
                    .clone()
                    .try_send(scheduler_host::ResourceObservation::UserActive);
            }
            if let Some(effect) = model_flow::reduce(&mut app.state, &message) {
                return match effect {
                    model_flow::ModelFlowEffect::None => iced::Task::none(),
                    model_flow::ModelFlowEffect::StartManagedDownload => {
                        // RFC-061 §8(d): was `.expect("active model store must
                        // be authorized")` -- a bad model-store path
                        // terminated the whole process on starting a
                        // download.
                        let model_store = match bootstrap::model_store(&runtime) {
                            Ok(store) => store,
                            Err(e) => {
                                tracing::error!("model store unavailable: {e}");
                                app.update(Message::ShowNotice(
                                    orbok_ui::notice::UserNotice::StorageUnavailable,
                                ));
                                return iced::Task::none();
                            }
                        };
                        let (tx, rx) = iced::futures::channel::mpsc::channel::<Message>(64);
                        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                        *active_download_cancel.lock().unwrap() = Some(cancel.clone());
                        tokio::spawn(download::run(model_store, catalog.clone(), tx, cancel));
                        iced::Task::stream(rx)
                    }
                    model_flow::ModelFlowEffect::CancelManagedDownload => {
                        if let Some(flag) = active_download_cancel.lock().unwrap().as_ref() {
                            flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        iced::Task::none()
                    }
                    effect @ model_flow::ModelFlowEffect::PersistReady { .. } => {
                        let persistence_runtime = runtime.clone();
                        iced::Task::perform(
                            async move {
                                model_flow::execute_production_persistence(
                                    effect,
                                    &persistence_runtime,
                                )
                                .expect("PersistReady must produce a completion")
                            },
                            |completion| completion,
                        )
                    }
                };
            }
            // Handle backend effects before passing message to UI state.
            match &message {
                Message::WizardValidate => {
                    let path = app.state.wizard_path_input.trim().to_string();
                    let outcome = verify_embedding_model(Some(&path));
                    let (checks, all_ok) = build_wizard_checks(&outcome, &path);
                    let checked = Message::WizardChecked {
                        model_dir: path,
                        checks,
                        all_ok,
                    };
                    let _ = model_flow::reduce(&mut app.state, &checked);
                    return iced::Task::none();
                }
                Message::RequestAddSource => {
                    // RFC-061 §7 Slice 5: `pick_folder()` used to be
                    // synchronous, blocking the whole update loop for as
                    // long as the OS dialog stayed open -- the same
                    // `AsyncFileDialog`/`Task::perform` pattern `SubmitSearch`
                    // (above) already uses for RFC-045's picker.
                    let locale = app.state.locale;
                    return iced::Task::perform(
                        async move {
                            rfd::AsyncFileDialog::new()
                                .set_title(dialog_title_add_source(locale))
                                .pick_folder()
                                .await
                                .map(|h| h.path().to_path_buf())
                        },
                        |result| match result {
                            Some(path) => Message::AddSourceFolderPicked(path),
                            None => Message::AddSourceFolderPickerCancelled,
                        },
                    );
                }
                Message::AddSourceFolderPicked(folder) => {
                    let path = folder.to_string_lossy().to_string();
                    app.update(Message::SourcePathChanged(path.clone()));
                    match bootstrap::add_source(&catalog, &path) {
                        Ok((card, sensitive)) => {
                            if let Some(warning) = sensitive {
                                tracing::warn!("sensitive source: {warning}");
                                app.update(Message::ShowNotice(
                                    orbok_ui::notice::UserNotice::SensitiveSourceAdded,
                                ));
                            }
                            let source_id = card.source_id.clone();
                            app.update(Message::SourceAdded(card));
                            match bootstrap::scan_and_index_source(&catalog, &source_id) {
                                Ok(health) => app.update(Message::HealthUpdated(health)),
                                Err(e) => {
                                    tracing::error!("scan failed: {e}");
                                    app.update(Message::ShowNotice(
                                        orbok_ui::notice::UserNotice::FolderCouldNotBeAdded,
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("add source failed: {e}");
                            app.update(Message::ShowNotice(
                                orbok_ui::notice::UserNotice::FolderCouldNotBeAdded,
                            ));
                        }
                    }
                    return iced::Task::none();
                }
                Message::AddSourceFolderPickerCancelled => {
                    return iced::Task::none();
                }
                Message::CleanSnippets => {
                    // RFC-061 §8(d): was `.expect("active cache path must be
                    // authorized")` -- a bad cache path terminated the whole
                    // process on a click of "Clear temporary previews".
                    match bootstrap::cache_service(&runtime) {
                        Ok(cache) => match bootstrap::clean_snippets(&catalog, &cache) {
                            Ok(_) => app.update(Message::CleanupDone),
                            Err(e) => tracing::error!("clean snippets failed: {e}"),
                        },
                        Err(e) => {
                            tracing::error!("cache handle unavailable for clean snippets: {e}");
                            app.update(Message::ShowNotice(
                                orbok_ui::notice::UserNotice::StorageUnavailable,
                            ));
                        }
                    }
                    return iced::Task::none();
                }
                Message::CleanSearchCache => {
                    // RFC-061 §8(d): same panic-on-bad-cache-path fix as
                    // `CleanSnippets` above.
                    match bootstrap::cache_service(&runtime) {
                        Ok(cache) => match bootstrap::clean_search_cache(&catalog, &cache) {
                            Ok(_) => app.update(Message::CleanupDone),
                            Err(e) => tracing::error!("clean search cache failed: {e}"),
                        },
                        Err(e) => {
                            tracing::error!("cache handle unavailable for clean search cache: {e}");
                            app.update(Message::ShowNotice(
                                orbok_ui::notice::UserNotice::StorageUnavailable,
                            ));
                        }
                    }
                    return iced::Task::none();
                }
                Message::ConfirmResetCatalog => {
                    // RFC-061 §8(d): `.expect(...)` here used to panic the
                    // whole process on a bad cache path. `StorageUnavailable`
                    // covers this open failure; `bootstrap::cache_service`
                    // itself never fails on a legitimately authorized
                    // profile, only on a broken one (RFC-049 §8's sealed
                    // handle contract).
                    match bootstrap::cache_service(&runtime) {
                        Ok(cache) => {
                            // RFC-061 §8(b): a failed reset used to be
                            // silently invisible -- the UI cleared its own
                            // state (below, via the fallthrough `update`)
                            // regardless of whether anything was actually
                            // cleared on disk.
                            if let Err(e) = bootstrap::reset_catalog(&catalog, &cache) {
                                tracing::error!("reset catalog failed: {e}");
                                app.update(Message::ShowNotice(
                                    orbok_ui::notice::UserNotice::CatalogResetFailed,
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::error!("cache handle unavailable for reset: {e}");
                            app.update(Message::ShowNotice(
                                orbok_ui::notice::UserNotice::StorageUnavailable,
                            ));
                        }
                    }
                    // UI state pre-cleared in AppState::update; fall through for update().
                }
                Message::SourceRemoved(source_id) => {
                    if let Err(e) = bootstrap::remove_source(&catalog, source_id) {
                        tracing::error!("remove source failed: {e}");
                        app.update(Message::ShowNotice(
                            orbok_ui::notice::UserNotice::SourceCouldNotBeRemoved,
                        ));
                    }
                }
                // RFC-037 §10.2 manual refresh (Task 035): same function
                // the startup check calls (bootstrap/startup.rs), invoked
                // here by explicit user action instead. Re-fetches sources
                // (the status this call may have just changed) and health
                // (a freshly enqueued Scan job) immediately, the same
                // `HealthUpdated` pattern `RequestAddSource` already uses --
                // the background scheduler emits further updates as the
                // enqueued job actually runs.
                Message::SourceRefreshRequested(source_id) => {
                    match bootstrap::check_and_refresh_source(&catalog, source_id) {
                        Ok(health) => {
                            app.update(Message::SourcesLoaded(bootstrap::get_sources(&catalog)));
                            app.update(Message::HealthUpdated(health));
                        }
                        Err(e) => {
                            tracing::error!("source refresh failed: {e}");
                        }
                    }
                    return iced::Task::none();
                }
                Message::FocusSearch => {
                    app.update(message);
                    // iced 0.14 has no standalone text_input::focus() Task.
                    // Best approximation: switch to the Search view so the
                    // user's next keypress reaches the search input. A proper
                    // programmatic focus Task is tracked as a follow-up once
                    // iced exposes it (see docs/src/maintainers/accessibility.md).
                    app.update(Message::Switch(orbok_ui::state::ViewId::Search));
                    return iced::Task::none();
                }
                // RFC-034 §2.1.1 / Task 024 §3.1: `key_to_message` returns
                // the *intent*; the actual focus movement is an iced Task,
                // issued here -- the same split `FocusSearch` above uses,
                // except `focus_next`/`focus_previous` genuinely exist in
                // iced 0.14 (`iced_runtime::widget::operation`), unlike the
                // standalone `text_input::focus()` `FocusSearch` wanted.
                Message::FocusNext => {
                    app.update(message);
                    return iced::widget::operation::focus_next();
                }
                Message::FocusPrevious => {
                    app.update(message);
                    return iced::widget::operation::focus_previous();
                }
                Message::PersistLocale(locale) => {
                    let _ = bootstrap::persist_locale(&catalog, locale);
                }
                Message::SetTheme(theme) => {
                    if let Err(e) = bootstrap::persist_theme(&runtime, *theme) {
                        tracing::error!("persist theme failed: {e}");
                        app.update(Message::ShowNotice(
                            orbok_ui::notice::UserNotice::SettingCouldNotBeSaved,
                        ));
                    }
                }
                Message::SetTextScale(scale) => {
                    if let Err(e) = bootstrap::persist_text_scale(&runtime, *scale) {
                        tracing::error!("persist text scale failed: {e}");
                        app.update(Message::ShowNotice(
                            orbok_ui::notice::UserNotice::SettingCouldNotBeSaved,
                        ));
                    }
                }
                Message::SetReducedMotion(val) => {
                    if let Err(e) = bootstrap::persist_reduced_motion(&runtime, *val) {
                        tracing::error!("persist reduced motion failed: {e}");
                        app.update(Message::ShowNotice(
                            orbok_ui::notice::UserNotice::SettingCouldNotBeSaved,
                        ));
                    }
                }
                Message::SubmitSearch => {
                    let query = app.state.query.trim().to_string();
                    if !query.is_empty() {
                        // RFC-045: if no search location is selected, open the
                        // folder picker first and store the pending query.
                        if !app.state.search_location.has_selected() {
                            app.update(Message::ChooseFolderRequested);
                            // The actual rfd call is an async Task so it does
                            // not block the iced event loop (RFC-045 §19.0).
                            let locale = app.state.locale;
                            return iced::Task::perform(
                                async move {
                                    rfd::AsyncFileDialog::new()
                                        .set_title(dialog_title_choose_search_folder(locale))
                                        .pick_folder()
                                        .await
                                        .map(|h| h.path().to_path_buf())
                                },
                                |result| match result {
                                    Some(path) => Message::FolderPicked(path),
                                    None => Message::FolderPickerCancelled,
                                },
                            );
                        }
                        // RFC-061 §7 Slice 5: show "Searching…" immediately
                        // (was previously only shown *after* the search
                        // returned, since the whole match below ran
                        // synchronously) -- then run the actual search off
                        // the update thread, the same `Task::perform`
                        // pattern as the picker above.
                        app.update(message.clone());
                        let catalog_task = catalog.clone();
                        let search_model_task = search_model.clone();
                        let query_task = query.clone();
                        return iced::Task::perform(
                            async move {
                                bootstrap::run_search(
                                    &catalog_task,
                                    search_model_task.as_ref().as_ref(),
                                    &query_task,
                                    20,
                                )
                                .map_err(|e| e.to_string())
                            },
                            move |outcome| Message::SubmitSearchCompleted {
                                query: query.clone(),
                                outcome,
                            },
                        );
                    }
                }
                Message::SubmitSearchCompleted { query, outcome } => {
                    match outcome {
                        Ok(results) => {
                            let count = results.len();
                            app.update(Message::SearchResultsReady(results.clone()));
                            // RFC-042: record this search if history is on.
                            let s = bootstrap::load_runtime_settings(&runtime).unwrap_or_default();
                            history::record_search(
                                &catalog,
                                &s.privacy_settings(),
                                &s.history_settings(),
                                query,
                                &app.state.search_ui.active_filters,
                                count,
                                &s.locale,
                            );
                            app.update(Message::HistoryLoaded(history::load_history(&catalog)));
                        }
                        Err(e) => {
                            app.update(Message::SearchError(e.clone()));
                        }
                    }
                    return iced::Task::none();
                }
                // RFC-045: folder picked — create or reuse the remembered folder.
                Message::FolderPicked(path) => {
                    let path_str = path.to_string_lossy().to_string();
                    // Reuse an existing source if the canonical path already
                    // exists — never create duplicates (RFC-045 §19.3).
                    let card = if let Some(existing) =
                        bootstrap::find_source_by_canonical_path(&catalog, &path_str)
                    {
                        existing
                    } else {
                        match bootstrap::add_source(&catalog, &path_str) {
                            Ok((card, sensitive)) => {
                                if let Some(warning) = sensitive {
                                    tracing::warn!("sensitive source: {warning}");
                                    app.update(Message::ShowNotice(
                                        orbok_ui::notice::UserNotice::SensitiveSourceAdded,
                                    ));
                                }
                                app.update(Message::SourceAdded(card.clone()));
                                card
                            }
                            Err(e) => {
                                tracing::error!("add source from search failed: {e}");
                                app.update(Message::FolderPickerCancelled);
                                app.update(Message::ShowNotice(
                                    orbok_ui::notice::UserNotice::FolderCouldNotBeAdded,
                                ));
                                return iced::Task::none();
                            }
                        }
                    };

                    let source_id = orbok_core::SourceId::from_string(card.source_id.clone());
                    let display_name = card.display_name.clone();

                    // Promote to selected search location and run the
                    // pending search — RFC-045 §8.1 "run search as soon
                    // as possible".
                    app.update(Message::SearchLocationSelected(
                        orbok_ui::SearchLocation::remembered(source_id.clone(), display_name),
                    ));

                    // Begin background preparation and immediately search
                    // whatever is already indexed (RFC-045 §14, §8.1).
                    match bootstrap::scan_and_index_source(&catalog, source_id.as_str()) {
                        Ok(health) => app.update(Message::HealthUpdated(health)),
                        Err(e) => tracing::warn!("initial scan failed: {e}"),
                    }

                    // Resume the search that triggered the picker.
                    // RFC-061 §7 Slice 5: off the update thread, same as
                    // `SubmitSearch` above -- this resume doesn't record
                    // history (matches the pre-Slice-5 behavior), so its
                    // outcome maps straight onto the plain
                    // `SearchResultsReady`/`SearchError` messages.
                    let query = app.state.last_query.clone().unwrap_or_default();
                    if query.is_empty() {
                        return iced::Task::none();
                    }
                    let catalog_task = catalog.clone();
                    let search_model_task = search_model.clone();
                    return iced::Task::perform(
                        async move {
                            bootstrap::run_search(
                                &catalog_task,
                                search_model_task.as_ref().as_ref(),
                                &query,
                                20,
                            )
                            .map_err(|e| e.to_string())
                        },
                        |outcome| match outcome {
                            Ok(results) => Message::SearchResultsReady(results),
                            Err(e) => Message::SearchError(e),
                        },
                    );
                }
                // RFC-042: Search again — restore text + valid filters, rerun.
                Message::SearchAgain(id) => {
                    let Some(entry) = history::get_entry(&catalog, id) else {
                        return iced::Task::none();
                    };
                    // Restore search text immediately (RFC-042 §9 step 1).
                    app.state.query = entry.search_text.clone();
                    app.state.search_ui.text = entry.search_text.clone();

                    // Restore valid filters; drop missing folders.
                    let (kept, dropped) = history::restore_valid_filters(&catalog, &entry);
                    if dropped {
                        app.update(Message::ShowNotice(
                            orbok_ui::notice::UserNotice::RecentSearchFilterDropped,
                        ));
                    }
                    // Note: filters are stored for display; re-applying
                    // them to the live ActiveFilter set is a P1 refinement.
                    let _ = kept;

                    // UI status → "Searching again…".
                    app.update(Message::SearchAgain(id.clone()));
                    // RFC-061 §7 Slice 5: these two don't depend on the
                    // search outcome below (restoring the entry and
                    // refreshing the history list are independent of
                    // whether the rerun finds anything), so they no longer
                    // need to wait behind it.
                    app.update(Message::RecentSearchRestored(id.clone()));
                    app.update(Message::HistoryLoaded(history::load_history(&catalog)));

                    // Rerun against current files (RFC-042 §9 step 6), off
                    // the update thread -- same as `SubmitSearch`/
                    // `FolderPicked` above. Doesn't record history (matches
                    // pre-Slice-5 behavior), so its outcome maps straight
                    // onto the plain `SearchResultsReady`/`SearchError`
                    // messages.
                    let query = entry.search_text.trim().to_string();
                    if query.is_empty() {
                        return iced::Task::none();
                    }
                    let catalog_task = catalog.clone();
                    let search_model_task = search_model.clone();
                    return iced::Task::perform(
                        async move {
                            bootstrap::run_search(
                                &catalog_task,
                                search_model_task.as_ref().as_ref(),
                                &query,
                                20,
                            )
                            .map_err(|e| e.to_string())
                        },
                        |outcome| match outcome {
                            Ok(results) => Message::SearchResultsReady(results),
                            Err(e) => Message::SearchError(e),
                        },
                    );
                }
                // RFC-042: remove one entry.
                Message::RemoveRecentSearch(id) => {
                    let refreshed = history::remove_entry(&catalog, id);
                    app.update(message.clone());
                    app.update(Message::HistoryLoaded(refreshed));
                    return iced::Task::none();
                }
                // RFC-042: clear all entries.
                Message::ConfirmClearRecentSearches => {
                    history::clear_history(&catalog);
                    app.update(Message::RecentSearchesCleared);
                    app.update(Message::ShowNotice(
                        orbok_ui::notice::UserNotice::RecentSearchesCleared,
                    ));
                    return iced::Task::none();
                }
                // RFC-042: toggle the Remember recent searches setting.
                Message::ToggleRememberRecentSearches(on) => {
                    let mut s = bootstrap::load_runtime_settings(&runtime).unwrap_or_default();
                    s.remember_recent_searches = *on;
                    let _ = bootstrap::save_runtime_settings(&runtime, &s);
                    // If turned off, also clear existing entries (RFC-042 §13.4
                    // "Turn off and clear" — default safe behavior here).
                    if !*on {
                        history::clear_history(&catalog);
                        app.update(Message::RecentSearchesCleared);
                    }
                    app.update(message.clone());
                    return iced::Task::none();
                }
                _ => {}
            }
            app.update(message);
            iced::Task::none()
        },
        OrbokApp::view,
    )
    .title(|app: &OrbokApp| app.title())
    .theme(|app: &OrbokApp| app.iced_theme())
    .font(orbok_ui::LUCIDE_FONT_BYTES)
    .subscription(move |app: &OrbokApp| {
        // RFC-034 §2.1.1 / Task 024: rebuilt fresh every time iced asks
        // for the current subscription set (the same cadence `focused`
        // was already recomputed at), so this always reflects the latest
        // state despite `key_to_message`'s only path in is a `Hash`ed
        // `Subscription::with` payload -- see `KeyboardContext`'s own doc
        // comment for why it carries only these small pieces rather than
        // `&AppState` itself.
        let ctx = KeyboardContext {
            text_input_focused: app.search_focused,
            active_view: app.state.active_view,
            confirm_reset: app.state.confirm_reset,
            confirm_clear_history: app.state.confirm_clear_history,
            wizard_kind: app.state.wizard.as_ref().map(WizardState::kind),
            selected_source_id: app
                .state
                .selected_source
                .and_then(|i| app.state.sources.get(i))
                .map(|card| card.source_id.clone()),
        };
        iced::Subscription::batch([
            scheduler_host::subscription(scheduler_host::SchedulerSubscriptionData {
                portable,
                resource_signals: resource_signals.clone(),
                resource_signal_tx: battery_resource_signal_tx.clone(),
            }),
            iced::keyboard::listen()
                .with(ctx)
                .filter_map(|(ctx, event)| {
                    use iced::keyboard::Event;
                    match event {
                        Event::KeyPressed { key, modifiers, .. } => {
                            key_to_message(&key, modifiers, &ctx)
                        }
                        _ => None,
                    }
                }),
        ])
    })
    .run()?;
    Ok(())
}

/// RFC-061 §8(d): before this, there was no `std::panic::set_hook` anywhere
/// in the tree. `iced` runs `update`/`view` on the GUI thread; a panic
/// there was previously whatever the Rust default panic hook prints
/// (`panicked at ...` to stderr, no `tracing` structure, easy to miss when
/// stderr isn't captured) and then process termination either way -- this
/// does not change *that* (still not unwind-safe to keep running an `iced`
/// app after `update`/`view` panicked), it changes what gets recorded
/// before the process goes down, so RFC-018's diagnostics have a
/// structured, `tracing`-routed record of the panic rather than raw stderr
/// text that may not have been captured anywhere.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        tracing::error!(
            location = location.as_deref().unwrap_or("unknown"),
            "panic: {info}"
        );
        default_hook(info);
    }));
}

/// Convert a `VerifyOutcome` into the file check list shown in the wizard.
fn build_wizard_checks(outcome: &VerifyOutcome, _path: &str) -> (Vec<WizardFileCheck>, bool) {
    match outcome {
        VerifyOutcome::Ready => {
            let checks = REQUIRED_MODEL_FILES
                .iter()
                .map(|rel| WizardFileCheck {
                    relative_path: rel.to_string(),
                    found: true,
                    size_mb: None,
                })
                .collect();
            (checks, true)
        }
        VerifyOutcome::FilesInvalid { issues, .. } => {
            let checks = REQUIRED_MODEL_FILES
                .iter()
                .map(|rel| WizardFileCheck {
                    relative_path: rel.to_string(),
                    found: !issues.iter().any(|i| i.relative_path == *rel),
                    size_mb: None,
                })
                .collect();
            (checks, false)
        }
        VerifyOutcome::NotConfigured => {
            let checks = REQUIRED_MODEL_FILES
                .iter()
                .map(|rel| WizardFileCheck {
                    relative_path: rel.to_string(),
                    found: false,
                    size_mb: None,
                })
                .collect();
            (checks, false)
        }
    }
}
