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
mod settings;

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

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("orbok {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let portable = args.iter().any(|a| a == "--portable");
    if portable {
        eprintln!("orbok: portable mode — data directory: ./orbok-data/");
    }
    if args.iter().any(|a| a == "--check") {
        return bootstrap::run_check();
    }

    let state = bootstrap::load_initial_state()?;
    let data_dir = bootstrap::data_dir_for_args(portable);
    bootstrap::ensure_default_model_store(&data_dir)?;
    let catalog_path = data_dir.join(orbok_db::CATALOG_FILE_NAME);

    iced::application(
        move || OrbokApp::with_state(state.clone()),
        move |app: &mut OrbokApp, message: Message| -> iced::Task<Message> {
            // Handle backend effects before passing message to UI state.
            match &message {
                Message::DownloadModel => {
                    let dest = bootstrap::default_model_store_root(&data_dir);
                    let dest_str = dest.to_string_lossy().to_string();
                    app.update(Message::DownloadStarted { dest_dir: dest_str });
                    let (tx, rx) = iced::futures::channel::mpsc::channel::<Message>(64);
                    tokio::spawn(download::run(dest, catalog_path.clone(), tx));
                    return iced::Task::stream(rx);
                }
                Message::WizardValidate => {
                    let path = app.state.wizard_path_input.trim().to_string();
                    let outcome = verify_embedding_model(Some(&path));
                    let (checks, all_ok) = build_wizard_checks(&outcome, &path);
                    app.update(Message::WizardChecked {
                        model_dir: path,
                        checks,
                        all_ok,
                    });
                    return iced::Task::none();
                }
                Message::WizardAccept => {
                    // Persist the accepted model directory to OrbokSettings.
                    if let Some(orbok_ui::state::WizardState::Ready { model_dir }) =
                        &app.state.wizard
                    {
                        if let Err(e) = bootstrap::persist_model_dir(model_dir.as_str()) {
                            tracing::error!("failed to save model dir: {e}");
                        }
                    }
                }
                Message::RequestAddSource => {
                    // Open the OS-native folder picker.
                    // `pick_folder()` is synchronous; it blocks the update loop
                    // while the dialog is open, which is expected for a modal dialog.
                    let picked = rfd::FileDialog::new()
                        .set_title("Select folder to search")
                        .pick_folder();
                    if let Some(folder) = picked {
                        let path = folder.to_string_lossy().to_string();
                        app.update(Message::SourcePathChanged(path.clone()));
                        if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                            let cache = orbok_cache::CacheService::new(&data_dir);
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
                                    match bootstrap::scan_and_index_source(
                                        &catalog, &cache, &source_id,
                                    ) {
                                        Ok(health) => app.update(Message::ScanCompleted(health)),
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
                        }
                    }
                    return iced::Task::none();
                }
                Message::CleanSnippets => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        let cache = orbok_cache::CacheService::new(&data_dir);
                        let cache_db = data_dir.join("orbok-cache.sqlite3");
                        match bootstrap::clean_snippets(&catalog, &cache, &cache_db) {
                            Ok(_) => app.update(Message::CleanupDone),
                            Err(e) => tracing::error!("clean snippets failed: {e}"),
                        }
                    }
                    return iced::Task::none();
                }
                Message::CleanSearchCache => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        let cache = orbok_cache::CacheService::new(&data_dir);
                        let cache_db = data_dir.join("orbok-cache.sqlite3");
                        match bootstrap::clean_search_cache(&catalog, &cache, &cache_db) {
                            Ok(_) => app.update(Message::CleanupDone),
                            Err(e) => tracing::error!("clean search cache failed: {e}"),
                        }
                    }
                    return iced::Task::none();
                }
                Message::ConfirmResetCatalog => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        let cache = orbok_cache::CacheService::new(&data_dir);
                        let cache_db = data_dir.join("orbok-cache.sqlite3");
                        let _ = bootstrap::reset_catalog(&catalog, &cache, &cache_db);
                    }
                    // UI state pre-cleared in AppState::update; fall through for update().
                }
                Message::SourceRemoved(source_id) => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        let _ = bootstrap::remove_source(&catalog, source_id);
                    }
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
                Message::PersistLocale(locale) => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        let _ = bootstrap::persist_locale(&catalog, locale);
                    }
                }
                Message::SetTheme(theme) => {
                    let _ = bootstrap::persist_theme(*theme);
                }
                Message::SetTextScale(scale) => {
                    let _ = bootstrap::persist_text_scale(*scale);
                }
                Message::SetReducedMotion(val) => {
                    let _ = bootstrap::persist_reduced_motion(*val);
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
                            return iced::Task::perform(
                                async {
                                    rfd::AsyncFileDialog::new()
                                        .set_title("Choose folder to search")
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
                        if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                            match bootstrap::run_search(&catalog, &query, 20) {
                                Ok(results) => {
                                    let count = results.len();
                                    app.update(message.clone());
                                    app.update(Message::SearchResultsReady(results));
                                    // RFC-042: record this search if history is on.
                                    let s = settings::load_settings();
                                    history::record_search(
                                        &catalog,
                                        &s.privacy_settings(),
                                        &s.history_settings(),
                                        &query,
                                        &app.state.search_ui.active_filters,
                                        count,
                                        &s.locale,
                                    );
                                    app.update(Message::HistoryLoaded(history::load_history(
                                        &catalog,
                                    )));
                                    return iced::Task::none();
                                }
                                Err(e) => {
                                    app.update(message.clone());
                                    app.update(Message::SearchError(e.to_string()));
                                    return iced::Task::none();
                                }
                            }
                        }
                    }
                }
                // RFC-045: folder picked — create or reuse the remembered folder.
                Message::FolderPicked(path) => {
                    let path_str = path.to_string_lossy().to_string();
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
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
                        let cache = orbok_cache::CacheService::new(&data_dir);
                        match bootstrap::scan_and_index_source(&catalog, &cache, source_id.as_str())
                        {
                            Ok(health) => app.update(Message::ScanCompleted(health)),
                            Err(e) => tracing::warn!("initial scan failed: {e}"),
                        }

                        // Resume the search that triggered the picker.
                        let query = app.state.last_query.clone().unwrap_or_default();
                        if !query.is_empty() {
                            match bootstrap::run_search(&catalog, &query, 20) {
                                Ok(results) => {
                                    app.update(Message::SearchResultsReady(results));
                                }
                                Err(e) => {
                                    app.update(Message::SearchError(e.to_string()));
                                }
                            }
                        }
                    }
                    return iced::Task::none();
                }
                // RFC-042: Search again — restore text + valid filters, rerun.
                Message::SearchAgain(id) => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        if let Some(entry) = history::get_entry(&catalog, id) {
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

                            // Rerun against current files (RFC-042 §9 step 6).
                            let query = entry.search_text.trim().to_string();
                            if !query.is_empty() {
                                match bootstrap::run_search(&catalog, &query, 20) {
                                    Ok(results) => {
                                        app.update(Message::SearchResultsReady(results));
                                    }
                                    Err(e) => {
                                        app.update(Message::SearchError(e.to_string()));
                                    }
                                }
                            }
                            app.update(Message::RecentSearchRestored(id.clone()));
                            app.update(Message::HistoryLoaded(history::load_history(&catalog)));
                        }
                    }
                    return iced::Task::none();
                }
                // RFC-042: remove one entry.
                Message::RemoveRecentSearch(id) => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        let refreshed = history::remove_entry(&catalog, id);
                        app.update(message.clone());
                        app.update(Message::HistoryLoaded(refreshed));
                    }
                    return iced::Task::none();
                }
                // RFC-042: clear all entries.
                Message::ConfirmClearRecentSearches => {
                    if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                        history::clear_history(&catalog);
                    }
                    app.update(Message::RecentSearchesCleared);
                    app.update(Message::ShowNotice(
                        orbok_ui::notice::UserNotice::RecentSearchesCleared,
                    ));
                    return iced::Task::none();
                }
                // RFC-042: toggle the Remember recent searches setting.
                Message::ToggleRememberRecentSearches(on) => {
                    let mut s = settings::load_settings();
                    s.remember_recent_searches = *on;
                    let _ = settings::save_settings(&s);
                    // If turned off, also clear existing entries (RFC-042 §13.4
                    // "Turn off and clear" — default safe behavior here).
                    if !*on {
                        if let Ok(catalog) = orbok_db::Catalog::open(&catalog_path) {
                            history::clear_history(&catalog);
                            app.update(Message::RecentSearchesCleared);
                        }
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
    .subscription(|app: &OrbokApp| {
        let focused = app.search_focused;
        iced::keyboard::listen()
            .with(focused)
            .filter_map(|(focused, event)| {
                use iced::keyboard::Event;
                match event {
                    Event::KeyPressed { key, modifiers, .. } => {
                        key_to_message(&key, modifiers, focused)
                    }
                    _ => None,
                }
            })
    })
    .run()?;
    Ok(())
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
