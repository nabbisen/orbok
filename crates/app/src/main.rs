//! orbok application binary.
//!
//! Startup sequence (RFC-027, design §startup):
//! 1. parse flags (--version, --portable, --check)
//! 2. resolve data directory
//! 3. open catalog, run migrations, run startup recovery (RFC-018)
//! 4. load OrbokSettings, verify model files → build AppState
//! 5. if wizard active: show wizard until resolved or skipped
//! 6. launch main GUI

// Task 061 §1: a GUI program on Windows, so launching it from the Start
// menu or the Store opens no console window behind the app. Command-line
// output reaches a terminal through `platform_host::attach_parent_console`.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod backend_actions;
mod bootstrap;
mod cli;
mod diagnostics;
mod download;
mod history;
mod model_flow;
mod notice_retry;
mod platform_host;
mod result_launch;
#[cfg(test)]
mod rfc059_cache_measurement;
#[cfg(test)]
mod rfc060_duplication_measurement;
#[cfg(test)]
mod rfc061_acceptance_tests;
#[cfg(test)]
mod rfc062_acceptance_tests;
#[cfg(test)]
mod runtime_isolation_tests;
mod scheduler_host;
mod search_flow;
mod search_model;
mod settings;
mod source_removal;
mod startup_failure;
mod trust_actions;
#[cfg(test)]
mod wired_application_tests;

use orbok_ui::i18n::{dialog_title_add_source, dialog_title_choose_search_folder};
use orbok_ui::state::WizardFileCheck;
use orbok_ui::{Message, OrbokApp, key_to_message};
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

    // Task 051: parsed before any runtime context is resolved, so neither
    // `--help` nor an unrecognised argument can resolve, create or migrate
    // a profile.
    let args: Vec<String> = std::env::args().collect();
    let command = cli::parse_args(&args);
    if cli::needs_console(&command) {
        platform_host::attach_parent_console();
    }
    // Task 061 §4: refused before anything is resolved, like an
    // unrecognised argument, so no profile is touched.
    if let Some(message) = cli::portable_refusal(&command, platform_host::is_packaged()) {
        eprint!("{message}");
        std::process::exit(2);
    }
    let (portable, check) = match command {
        cli::CliCommand::Help => {
            print!("{}", cli::USAGE);
            return Ok(());
        }
        cli::CliCommand::Unknown(arg) => {
            eprint!("{}", cli::unknown_argument_message(&arg));
            std::process::exit(2);
        }
        cli::CliCommand::Version => {
            println!("orbok {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        cli::CliCommand::Check { portable } => (portable, true),
        cli::CliCommand::Gui { portable } => (portable, false),
    };
    // Task 071: `--check` keeps reporting a failure as text. A GUI launch
    // shows it in a window instead, since a desktop launch has no terminal.
    let runtime = match bootstrap::resolve_runtime_context(portable) {
        Ok(runtime) => runtime,
        Err(error) if check => return Err(error),
        // Resolving touches no folder: these are configuration conflicts
        // (portable with ORBOK_DATA_DIR, overlapping profiles, no platform
        // settings directory), with no data folder yet to name.
        Err(error) => show_startup_failure(FailedStartup {
            failure: startup_failure::StartupFailure::other(error),
            stored_locale: None,
        }),
    };
    if portable {
        eprintln!("orbok: portable mode — data directory: ./orbok-data/");
    }
    if check {
        bootstrap::run_check(&runtime)?;
        return Ok(());
    }

    let (state, catalog) = match start_gui(&runtime) {
        Ok(started) => started,
        Err(failed) => show_startup_failure(failed),
    };

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
    let catalog = std::sync::Arc::new(catalog);

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
    //
    // Task 055 §1(c): wrapped in `SearchModel` so a model installed during
    // the session can be swapped in; each search still takes a snapshot and
    // never resolves one itself.
    let search_model = std::sync::Arc::new(search_model::SearchModel::new(
        bootstrap::embedding_resolution::resolve_embedding_worker_parts(
            &runtime,
            &orbok::runtime_context::AllowRuntimePathProbe,
            &catalog,
            &search_settings,
        ),
    ));

    // RFC-060 §6: the extraction cache is the only source a page, paragraph
    // or block snippet may be rendered from -- the snippet path never
    // extracts (Amendment 3). `Arc` for the same reason `catalog` is: each
    // `Task::perform` closure needs its own owned handle. `None` if the
    // cache cannot be opened; those formats then show no snippet, and the
    // result is still shown.
    let search_cache = std::sync::Arc::new(match bootstrap::cache_service(&runtime) {
        Ok(cache) => Some(cache),
        Err(error) => {
            tracing::warn!(%error, "snippets for PDF/DOCX/HTML results are unavailable this session");
            None
        }
    });

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
            // Task 057: the notice's "Try again" when background preparation
            // could not load the model -- ask it to load the model again.
            // The state update below dismisses the notice.
            if matches!(message, Message::RetryModelLoad)
                && let Err(error) = resource_signal_tx
                    .clone()
                    .try_send(scheduler_host::ResourceObservation::EmbeddingModelChanged)
            {
                tracing::warn!(%error, "could not ask background preparation to load the model again");
            }
            if matches!(message, Message::QueryChanged(_) | Message::SubmitSearch) {
                let _ = resource_signal_tx
                    .clone()
                    .try_send(scheduler_host::ResourceObservation::UserActive);
            }
            // Task 060: a notice's action button dispatches the concrete retry
            // its raise site stored, after clearing the notice.
            // Task 062: the removal confirmation was confirmed -- dispatch the
            // one existing removal path for the folder the dialog was opened
            // for.
            if matches!(message, Message::ConfirmRemoveSource) {
                return app
                    .state
                    .take_confirmed_removal()
                    .map_or_else(iced::Task::none, iced::Task::done);
            }
            if matches!(message, Message::NoticeActionPressed) {
                return app
                    .state
                    .take_notice_action()
                    .map_or_else(iced::Task::none, iced::Task::done);
            }
            // Task 060: a failed search's Try again -- restore that query,
            // then submit it through the ordinary path.
            if let Message::RetrySearch(_) = &message {
                app.update(message);
                return iced::Task::done(Message::SubmitSearch);
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
                                // Task 063: a failed download, not a notice
                                // over an endless "Downloading".
                                return iced::Task::done(model_flow::download_could_not_start());
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
                    model_flow::ModelFlowEffect::ActivateModel {
                        ready_id,
                        persistence_attempt_id,
                    } => {
                        // Task 055 §1(b): the indexing host re-resolves its
                        // own model and backfills. `clone()` gives this send
                        // its own slot beyond the channel's buffer, so it is
                        // not dropped behind a burst of `UserActive`.
                        if let Err(error) = resource_signal_tx
                            .clone()
                            .try_send(scheduler_host::ResourceObservation::EmbeddingModelChanged)
                        {
                            tracing::warn!(%error, "could not tell background preparation about the new model");
                        }
                        // Task 055 §1(c): resolve for search off the update
                        // thread, swap it in, and only then report -- the
                        // report is what sets `capability = Hybrid`.
                        let activation_model = search_model.clone();
                        let activation_runtime = runtime.clone();
                        iced::Task::perform(
                            async move {
                                tokio::task::spawn_blocking(move || {
                                    activation_model.activate(|| {
                                        let catalog =
                                            bootstrap::open_catalog(&activation_runtime).ok()?;
                                        let settings =
                                            bootstrap::load_runtime_settings(&activation_runtime)
                                                .ok()?;
                                        bootstrap::embedding_resolution::resolve_embedding_worker_parts(
                                            &activation_runtime,
                                            &orbok::runtime_context::AllowRuntimePathProbe,
                                            &catalog,
                                            &settings,
                                        )
                                    })
                                })
                                .await
                                .unwrap_or(false)
                            },
                            move |activated| Message::ModelActivationCompleted {
                                ready_id,
                                persistence_attempt_id,
                                activated,
                            },
                        )
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
            // HANDOFF-041: open a result, or show it in its folder --
            // validated through the searchable-source guard, then launched
            // with the path as one argument. RFC-038's `OpenAnyway` and
            // `ShowInFolder` recovery actions are the same two operations
            // on the same result.
            if let Some((index, action)) = result_launch::launch_request(&message) {
                if let Some(failure) = result_launch::launch_result(
                    &catalog,
                    &app.state.search_results,
                    index,
                    action,
                    &result_launch::SystemLauncher,
                ) {
                    app.update(notice_retry::result_not_launched(failure));
                }
                return iced::Task::none();
            }
            // HANDOFF-038: the recovery actions that touch the catalog.
            if let Message::TrustRecoveryAction { result_idx, action } = &message {
                trust_actions::recover(&catalog, &mut app.state, *result_idx, *action, &message);
                app.update(message.clone());
                return iced::Task::none();
            }
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
                    //
                    // Task 047: this arm returns before the reducer runs, so the
                    // one-dialog-at-a-time flag is checked and set here.
                    if app.state.add_source_picker_in_progress {
                        return iced::Task::none();
                    }
                    app.update(message.clone());
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
                        Ok(bootstrap::AddSourceOutcome::AlreadyRegistered { .. }) => {
                            app.update(Message::ShowNotice(
                                orbok_ui::notice::UserNotice::FolderAlreadyAdded,
                            ));
                        }
                        Ok(bootstrap::AddSourceOutcome::Added { card, sensitive }) => {
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
                                    app.update(notice_retry::add_folder_failed());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("add source failed: {e}");
                            app.update(notice_retry::add_folder_failed());
                        }
                    }
                    // Clears the picker flag whatever the outcome.
                    app.update(message.clone());
                    return iced::Task::none();
                }
                Message::AddSourceFolderPickerCancelled => {
                    app.update(message.clone());
                    return iced::Task::none();
                }
                // Task 075: each backend action's result, reflected truthfully
                // (`backend_actions`). Task 081: the numbers this action
                // just changed are refreshed right after -- a stale number
                // is the same class of defect as the false zero Task 081
                // fixed (Review Request 257 §3), so this does not wait for
                // the user to reopen Storage.
                Message::CleanSnippets
                | Message::CleanSearchCache
                | Message::CleanTemporaryExtraction
                | Message::RemoveReplacedStaleIndexes => {
                    backend_actions::run_cleanup(
                        &catalog,
                        bootstrap::cache_service(&runtime),
                        &mut app.state,
                        &message,
                    );
                    return measure_storage_task(runtime.clone(), catalog.clone());
                }
                Message::ConfirmResetCatalog => {
                    backend_actions::reset_catalog(
                        &catalog,
                        bootstrap::cache_service(&runtime),
                        &mut app.state,
                    );
                    // Task 081: reset clears `storage_rows` (the reducer's
                    // own `CatalogResetSucceeded` arm); this replaces the
                    // RFC-011 §13.1 empty state that would otherwise leave
                    // with the real measured post-reset numbers.
                    return measure_storage_task(runtime.clone(), catalog.clone());
                }
                // Task 081: "Calculate now", and every switch to the
                // Storage view (so the numbers shown are current, not
                // whatever was last measured).
                Message::StorageMeasurementRequested => {
                    app.update(message.clone());
                    return measure_storage_task(runtime.clone(), catalog.clone());
                }
                Message::Switch(orbok_ui::state::ViewId::Storage) => {
                    app.update(message.clone());
                    return measure_storage_task(runtime.clone(), catalog.clone());
                }
                Message::SourceRemoved(source_id) => {
                    source_removal::remove(&catalog, &mut app.state, source_id);
                    return iced::Task::none();
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
                    backend_actions::refresh_source(&catalog, &mut app.state, source_id);
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
                    backend_actions::persist_locale(&catalog, &mut app.state, *locale);
                    return iced::Task::none();
                }
                Message::SetTheme(theme) => {
                    if let Err(e) = bootstrap::persist_theme(&runtime, *theme) {
                        tracing::error!("persist theme failed: {e}");
                        app.update(notice_retry::setting_not_saved(&message));
                    }
                }
                Message::SetTextScale(scale) => {
                    if let Err(e) = bootstrap::persist_text_scale(&runtime, *scale) {
                        tracing::error!("persist text scale failed: {e}");
                        app.update(notice_retry::setting_not_saved(&message));
                    }
                }
                Message::SetReducedMotion(val) => {
                    if let Err(e) = bootstrap::persist_reduced_motion(&runtime, *val) {
                        tracing::error!("persist reduced motion failed: {e}");
                        app.update(notice_retry::setting_not_saved(&message));
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
                        let search_model_task = search_model.current();
                        let search_cache_task = search_cache.clone();
                        // RFC-060 §7: the kind filters and chosen folder the user
                        // actually set, resolved before the task takes ownership.
                        let mode_task = app.state.search_mode;
                        let scope_task = bootstrap::scope_from_ui(
                            &app.state.search_ui.active_filters,
                            app.state.search_location.selected.as_ref(),
                        );
                        let query_task = query.clone();
                        return iced::Task::perform(
                            async move {
                                bootstrap::run_search(
                                    &catalog_task,
                                    search_model_task.as_ref().as_ref(),
                                    search_cache_task
                                        .as_ref()
                                        .as_ref()
                                        .map(|cache| cache.service()),
                                    &query_task,
                                    mode_task,
                                    20,
                                    scope_task,
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
                            app.update(Message::SearchError {
                                query: query.clone(),
                                error: e.clone(),
                            });
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
                            // The lookup above normally catches this; it
                            // differs only if `path_str` was not canonical.
                            Ok(bootstrap::AddSourceOutcome::AlreadyRegistered { card }) => card,
                            Ok(bootstrap::AddSourceOutcome::Added { card, sensitive }) => {
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
                                app.update(notice_retry::search_folder_failed());
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

                    // Resume the search that triggered the picker (RFC-045 §8.1),
                    // through the ordinary path (Task 068): `RetrySearch` restores
                    // the query pending when the picker opened, then dispatches
                    // `SubmitSearch`, which now sees the selected location, sets
                    // `last_query`, runs the search and records history. There is
                    // no second search path here any more.
                    return search_flow::after_folder_picked(&app.state)
                        .map_or_else(iced::Task::none, iced::Task::done);
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
                    let search_model_task = search_model.current();
                    let search_cache_task = search_cache.clone();
                    // RFC-060 §7: the kind filters and chosen folder the user
                    // actually set, resolved before the task takes ownership.
                    let mode_task = app.state.search_mode;
                    let scope_task = bootstrap::scope_from_ui(
                        &app.state.search_ui.active_filters,
                        app.state.search_location.selected.as_ref(),
                    );
                    let failed_query = query.clone();
                    return iced::Task::perform(
                        async move {
                            bootstrap::run_search(
                                &catalog_task,
                                search_model_task.as_ref().as_ref(),
                                search_cache_task
                                    .as_ref()
                                    .as_ref()
                                    .map(|cache| cache.service()),
                                &query,
                                mode_task,
                                20,
                                scope_task,
                            )
                            .map_err(|e| e.to_string())
                        },
                        move |outcome| match outcome {
                            Ok(results) => Message::SearchResultsReady(results),
                            Err(error) => Message::SearchError {
                                query: failed_query,
                                error,
                            },
                        },
                    );
                }
                // RFC-042: remove one entry.
                Message::RemoveRecentSearch(id) => {
                    backend_actions::remove_recent_search(&catalog, &mut app.state, id);
                    return iced::Task::none();
                }
                // RFC-042: clear all entries.
                Message::ConfirmClearRecentSearches => {
                    backend_actions::clear_recent_searches(&catalog, &mut app.state);
                    return iced::Task::none();
                }
                // RFC-042: toggle the Remember recent searches setting.
                Message::ToggleRememberRecentSearches(on) => {
                    backend_actions::toggle_remember_recent_searches(
                        &runtime,
                        &catalog,
                        &mut app.state,
                        *on,
                    );
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
        let ctx = app.keyboard_context();
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

/// Task 071: a GUI startup that failed, and the locale to explain it in.
struct FailedStartup {
    failure: startup_failure::StartupFailure,
    /// The stored UI language, when startup got far enough to load it.
    stored_locale: Option<orbok_ui::i18n::Locale>,
}

/// Task 071: the one GUI startup path -- the initial state, then the
/// process-lifetime catalog (RFC-061 §5 Slice 1, see its comment in `main`).
fn start_gui(
    runtime: &orbok::runtime_context::RuntimeContext,
) -> Result<(orbok_ui::AppState, orbok_db::Catalog), FailedStartup> {
    let state = bootstrap::load_initial_state(runtime).map_err(|failure| FailedStartup {
        failure,
        stored_locale: None,
    })?;
    let catalog = bootstrap::open_catalog_staged(runtime).map_err(|error| FailedStartup {
        failure: startup_failure::StartupFailure::from_catalog_open(runtime, error),
        stored_locale: Some(state.locale),
    })?;
    Ok((state, catalog))
}

/// Task 071: log and print the failure as before, then show it in a small
/// window until the user closes it, and exit 1. The window has no catalog,
/// no scheduler and no retry: starting again means relaunching.
///
/// The locale is the stored setting when it loaded; otherwise the system
/// locale, as a first run chooses it (`Locale::from_env`, then the
/// default) -- the settings file may be what failed.
fn show_startup_failure(failed: FailedStartup) -> ! {
    use orbok_ui::views::startup_failure::{
        StartupFailureMessage, StartupFailureScreen, startup_failure_key,
    };

    let source = failed.failure.source();
    tracing::error!(cause = %failed.failure.class(), error = ?source, "orbok could not start");
    eprintln!("Error: {source:?}");

    let locale = failed
        .stored_locale
        .or_else(orbok_ui::i18n::Locale::from_env)
        .unwrap_or_default();
    let theme = orbok_ui::Theme::from_env().unwrap_or(orbok_ui::Theme::Light);
    let screen = StartupFailureScreen {
        cause: failed.failure.cause(),
        locale,
        theme,
        tokens: theme.tokens(),
    };
    let shown = iced::application(
        move || screen.clone(),
        |_: &mut StartupFailureScreen, message: StartupFailureMessage| match message {
            StartupFailureMessage::Close => iced::exit(),
        },
        StartupFailureScreen::view,
    )
    .title(StartupFailureScreen::title)
    .theme(StartupFailureScreen::iced_theme)
    .font(orbok_ui::LUCIDE_FONT_BYTES)
    .window_size((560.0, 260.0))
    .subscription(|_: &StartupFailureScreen| {
        iced::keyboard::listen().filter_map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, .. } => startup_failure_key(&key),
            _ => None,
        })
    })
    .run();
    if let Err(error) = shown {
        tracing::error!(%error, "the startup-failure window could not be shown");
    }
    std::process::exit(1);
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

/// Task 081: dispatch a fresh storage measurement off the update thread
/// (the `Task::perform` pattern `SubmitSearch` already uses). All eight
/// categories `Unknown` and no cache-file size read is treated as a
/// systemic failure (the catalog or the cache could not be reached at
/// all) rather than eight individually-unmeasurable categories, which is
/// what `Message::StorageMeasurementFailed` raises the notice for
/// (`AppState`'s own reducer, mirroring `Message::SearchError`'s shape --
/// not a separate `notice_retry` call here, since this path is async and
/// has no `&mut AppState` to call one synchronously against).
fn measure_storage_task(
    runtime: orbok::runtime_context::RuntimeContext,
    catalog: std::sync::Arc<orbok_db::Catalog>,
) -> iced::Task<Message> {
    iced::Task::perform(
        async move { bootstrap::measure_storage(&runtime, &catalog) },
        |(rows, cache_file_bytes)| {
            if bootstrap::storage_measurement_is_failure(&rows, cache_file_bytes) {
                Message::StorageMeasurementFailed
            } else {
                Message::StorageDataReady {
                    rows,
                    cache_file_bytes,
                }
            }
        },
    )
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
