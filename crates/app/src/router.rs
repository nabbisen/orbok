//! Task 084: the message router as a plain function `main`'s `update`
//! closure can no longer hide behind. Three tasks in a row (073, 075, 081)
//! had to say the closure could not be driven from a test; this is that
//! gap closed.
//!
//! `route` is a mechanical move of the closure body that used to live
//! directly inside `iced::application(..)` in `main.rs` -- same arms, same
//! order, same early returns, same `Task`s. What the closure captured by
//! `move` is now [`AppDeps`], built once in `main` and passed in by
//! reference; nothing else changed. The extracted decision modules
//! (`backend_actions`, `source_removal`, `notice_retry`, `trust_actions`,
//! `search_flow`) are called exactly as `main.rs` called them.

use crate::{
    backend_actions, bootstrap, download, history, model_flow, notice_retry, result_launch,
    scheduler_host, search_flow, search_model, source_removal, trust_actions,
};
use orbok::runtime_context::RuntimeContext;
use orbok::runtime_storage::ProfileCache;
use orbok_db::Catalog;
use orbok_ui::i18n::{dialog_title_add_source, dialog_title_choose_search_folder};
use orbok_ui::{Message, OrbokApp};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// What `main.rs`'s `update` closure used to capture by `move`. Built once
/// in `main`, held for the whole event loop's lifetime, and constructible
/// in a test the way `backend_actions`' own tests already build a real
/// catalog and runtime (`tests::test_deps`, `#[cfg(test)]` below).
pub(crate) struct AppDeps {
    pub(crate) runtime: RuntimeContext,
    pub(crate) catalog: Arc<Catalog>,
    pub(crate) search_model: Arc<search_model::SearchModel>,
    pub(crate) search_cache: Arc<Option<ProfileCache>>,
    pub(crate) resource_signal_tx:
        futures::channel::mpsc::Sender<scheduler_host::ResourceObservation>,
    pub(crate) active_download_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}

/// The message router: every message `iced` delivers to the running app
/// passes through here exactly once. A mechanical extraction of what used
/// to be `main.rs`'s `update` closure body -- see that commit's review
/// request for the function-by-function walk that convinced the reviewer
/// nothing else changed.
pub(crate) fn route(app: &mut OrbokApp, message: Message, deps: &AppDeps) -> iced::Task<Message> {
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
        && let Err(error) = deps
            .resource_signal_tx
            .clone()
            .try_send(scheduler_host::ResourceObservation::EmbeddingModelChanged)
    {
        tracing::warn!(%error, "could not ask background preparation to load the model again");
    }
    if matches!(message, Message::QueryChanged(_) | Message::SubmitSearch) {
        let _ = deps
            .resource_signal_tx
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
                let model_store = match bootstrap::model_store(&deps.runtime) {
                    Ok(store) => store,
                    Err(e) => {
                        tracing::error!("model store unavailable: {e}");
                        // Task 063: a failed download, not a notice
                        // over an endless "Downloading".
                        return iced::Task::done(model_flow::download_could_not_start());
                    }
                };
                let (tx, rx) = iced::futures::channel::mpsc::channel::<Message>(64);
                let cancel = Arc::new(AtomicBool::new(false));
                *deps.active_download_cancel.lock().unwrap() = Some(cancel.clone());
                tokio::spawn(download::run(model_store, deps.catalog.clone(), tx, cancel));
                iced::Task::stream(rx)
            }
            model_flow::ModelFlowEffect::CancelManagedDownload => {
                if let Some(flag) = deps.active_download_cancel.lock().unwrap().as_ref() {
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
                if let Err(error) = deps
                    .resource_signal_tx
                    .clone()
                    .try_send(scheduler_host::ResourceObservation::EmbeddingModelChanged)
                {
                    tracing::warn!(%error, "could not tell background preparation about the new model");
                }
                // Task 055 §1(c): resolve for search off the update
                // thread, swap it in, and only then report -- the
                // report is what sets `capability = Hybrid`.
                let activation_model = deps.search_model.clone();
                let activation_runtime = deps.runtime.clone();
                iced::Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            activation_model.activate(|| {
                                let catalog = bootstrap::open_catalog(&activation_runtime).ok()?;
                                let settings =
                                    bootstrap::load_runtime_settings(&activation_runtime).ok()?;
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
                let persistence_runtime = deps.runtime.clone();
                iced::Task::perform(
                    async move {
                        model_flow::execute_production_persistence(effect, &persistence_runtime)
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
            &deps.catalog,
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
        trust_actions::recover(
            &deps.catalog,
            &mut app.state,
            *result_idx,
            *action,
            &message,
        );
        app.update(message.clone());
        return iced::Task::none();
    }
    match &message {
        Message::WizardValidate => {
            let path = app.state.wizard_path_input.trim().to_string();
            let outcome = orbok_workers::verify_embedding_model(Some(&path));
            let (checks, all_ok) = crate::build_wizard_checks(&outcome, &path);
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
            match bootstrap::add_source(&deps.catalog, &path) {
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
                    match bootstrap::scan_and_index_source(&deps.catalog, &source_id) {
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
                &deps.catalog,
                bootstrap::cache_service(&deps.runtime),
                &mut app.state,
                &message,
            );
            return crate::measure_storage_task(deps.runtime.clone(), deps.catalog.clone());
        }
        // Task 092: the confirmation itself opens synchronously below (the
        // generic `app.update` fallthrough); its own line is fetched off
        // the update thread, the same `Task::perform` shape as Storage's
        // own measurement.
        Message::AskResetCatalog => {
            app.update(message.clone());
            return crate::reset_counts_task(deps.catalog.clone());
        }
        Message::ConfirmResetCatalog => {
            let succeeded = backend_actions::reset_catalog(
                &deps.catalog,
                bootstrap::cache_service(&deps.runtime),
                &mut app.state,
            );
            // Task 081: reset clears `storage_rows` (the reducer's
            // own `CatalogResetSucceeded` arm); this replaces the
            // RFC-011 §13.1 empty state that would otherwise leave
            // with the real measured post-reset numbers.
            //
            // Task 096: compaction (Task 095) only when the delete work
            // actually committed -- the same gate the old combined
            // `CleanupService::run_reset` had structurally, via its own
            // `?` short-circuiting before reaching compaction on a
            // failed delete. It runs off this thread, on its own
            // connection, then chains the same measurement either way.
            if succeeded {
                return crate::compact_reset_and_measure_task(
                    deps.runtime.clone(),
                    deps.catalog.clone(),
                );
            }
            return crate::measure_storage_task(deps.runtime.clone(), deps.catalog.clone());
        }
        // Task 081: "Calculate now", and every switch to the
        // Storage view (so the numbers shown are current, not
        // whatever was last measured).
        Message::StorageMeasurementRequested => {
            app.update(message.clone());
            return crate::measure_storage_task(deps.runtime.clone(), deps.catalog.clone());
        }
        Message::Switch(orbok_ui::state::ViewId::Storage) => {
            app.update(message.clone());
            return crate::measure_storage_task(deps.runtime.clone(), deps.catalog.clone());
        }
        Message::SourceRemoved(source_id) => {
            source_removal::remove(&deps.catalog, &mut app.state, source_id);
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
            backend_actions::refresh_source(&deps.catalog, &mut app.state, source_id);
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
            backend_actions::persist_locale(&deps.catalog, &mut app.state, *locale);
            return iced::Task::none();
        }
        Message::SetTheme(theme) => {
            if let Err(e) = bootstrap::persist_theme(&deps.runtime, *theme) {
                tracing::error!("persist theme failed: {e}");
                app.update(notice_retry::setting_not_saved(&message));
            }
        }
        Message::SetTextScale(scale) => {
            if let Err(e) = bootstrap::persist_text_scale(&deps.runtime, *scale) {
                tracing::error!("persist text scale failed: {e}");
                app.update(notice_retry::setting_not_saved(&message));
            }
        }
        Message::SetReducedMotion(val) => {
            if let Err(e) = bootstrap::persist_reduced_motion(&deps.runtime, *val) {
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
                let catalog_task = deps.catalog.clone();
                let search_model_task = deps.search_model.current();
                let search_cache_task = deps.search_cache.clone();
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
                    let s = bootstrap::load_runtime_settings(&deps.runtime).unwrap_or_default();
                    history::record_search(
                        &deps.catalog,
                        &s.privacy_settings(),
                        &s.history_settings(),
                        query,
                        &app.state.search_ui.active_filters,
                        count,
                        &s.locale,
                    );
                    app.update(Message::HistoryLoaded(history::load_history(&deps.catalog)));
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
                bootstrap::find_source_by_canonical_path(&deps.catalog, &path_str)
            {
                existing
            } else {
                match bootstrap::add_source(&deps.catalog, &path_str) {
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
            match bootstrap::scan_and_index_source(&deps.catalog, source_id.as_str()) {
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
            let Some(entry) = history::get_entry(&deps.catalog, id) else {
                return iced::Task::none();
            };
            // Restore search text immediately (RFC-042 §9 step 1).
            app.state.query = entry.search_text.clone();
            app.state.search_ui.text = entry.search_text.clone();

            // Restore valid filters; drop missing folders.
            let (kept, dropped) = history::restore_valid_filters(&deps.catalog, &entry);
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
            app.update(Message::HistoryLoaded(history::load_history(&deps.catalog)));

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
            let catalog_task = deps.catalog.clone();
            let search_model_task = deps.search_model.current();
            let search_cache_task = deps.search_cache.clone();
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
            backend_actions::remove_recent_search(&deps.catalog, &mut app.state, id);
            return iced::Task::none();
        }
        // RFC-042: clear all entries.
        Message::ConfirmClearRecentSearches => {
            backend_actions::clear_recent_searches(&deps.catalog, &mut app.state);
            return iced::Task::none();
        }
        // RFC-042: toggle the Remember recent searches setting.
        Message::ToggleRememberRecentSearches(on) => {
            backend_actions::toggle_remember_recent_searches(
                &deps.runtime,
                &deps.catalog,
                &mut app.state,
                *on,
            );
            return iced::Task::none();
        }
        _ => {}
    }
    app.update(message);
    iced::Task::none()
}

#[cfg(test)]
mod tests;
