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
mod router;
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

use orbok_ui::state::WizardFileCheck;
use orbok_ui::{Message, OrbokApp, key_to_message};
use orbok_workers::VerifyOutcome;
use orbok_workers::model_verifier::REQUIRED_MODEL_FILES;

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
    // Task 087: decided at the same pre-resolution point as
    // `portable_refusal` above, so a debug build cannot create or migrate
    // the default profile even once before refusing. Read here, not left
    // to `resolve_runtime_context`'s own later read of `ORBOK_DATA_DIR`:
    // both checks must agree on what counts as "set" (empty is unset), and
    // this one has to run before that call happens at all.
    let data_dir_override_set =
        std::env::var_os("ORBOK_DATA_DIR").is_some_and(|value| !value.is_empty());
    let allow_default_profile =
        std::env::var_os("ORBOK_ALLOW_DEFAULT_PROFILE").is_some_and(|value| value == "1");
    if let Some(message) = cli::default_profile_refusal(
        &command,
        cfg!(debug_assertions),
        data_dir_override_set,
        allow_default_profile,
    ) {
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

    // Task 084: what the `update` closure below used to capture by `move`
    // directly, now a plain struct `router::route` takes by reference --
    // so `route` is reachable from a test, which the raw closure never
    // was.
    let deps = router::AppDeps {
        runtime,
        catalog,
        search_model,
        search_cache,
        resource_signal_tx,
        active_download_cancel,
    };

    iced::application(
        move || OrbokApp::with_state(state.clone()),
        move |app: &mut OrbokApp, message: Message| -> iced::Task<Message> {
            router::route(app, message, &deps)
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

/// Task 097: what a confirmed reset returns, once it either committed and
/// was compacted and measured, or did not commit at all. Kept separate
/// from `Message` so `reset_catalog_delete_compact_and_measure` stays a
/// plain, directly-testable function -- `reset_outcome_to_messages` is the
/// only place this gets turned into messages, itself also plain and
/// directly-testable for the same reason.
#[derive(Debug)]
enum ResetOutcome {
    Succeeded {
        rows: Vec<(orbok_core::StorageCategory, orbok_core::StorageMeasurement)>,
        cache_file_bytes: Option<u64>,
    },
    /// The catalog or cache could not even be opened. Nothing was
    /// attempted, so nothing changed -- the case
    /// `notice_retry::reset_storage_unavailable` names.
    NotAttempted,
    /// The delete itself failed after a real attempt. `sources` is the
    /// reload `bootstrap::get_sources` produced against the same
    /// connection, `None` only if that read failed too, in which case the
    /// list stays exactly as it was before the request.
    Failed {
        sources: Option<Vec<orbok_ui::state::SourceCard>>,
    },
}

/// Task 097: a confirmed reset's entire own work -- delete, then (only if
/// the delete committed) compact and measure -- as a plain function, on
/// its own connection, off the update thread. A test can call this
/// directly under forced contention, the same reason `compact_reset_files`
/// was one (Task 096) before this task folded it in here.
///
/// Opens its own catalog and cache -- the same deliberate exception to
/// RFC-061 §5's one-catalog rule `scheduler_host.rs` already is, and Task
/// 096 already extended to compaction: the update thread's shared
/// connection must never be held across work this slow. Measured
/// (Review Request 274 §4): the delete alone took 727.9 ms on a 600 MB
/// catalog, against compaction's 56.8 ms -- the delete was always the
/// dominant cost, and it now runs here, off that thread, instead of
/// inline in `router::route`.
fn reset_catalog_delete_compact_and_measure(
    runtime: &orbok::runtime_context::RuntimeContext,
) -> ResetOutcome {
    let Ok(fresh_catalog) = bootstrap::open_catalog(runtime) else {
        tracing::error!("reset catalog failed: catalog unavailable");
        return ResetOutcome::NotAttempted;
    };
    let Ok(cache) = bootstrap::cache_service(runtime) else {
        tracing::error!("reset catalog failed: cache unavailable");
        return ResetOutcome::NotAttempted;
    };
    match bootstrap::reset_catalog(&fresh_catalog, &cache) {
        Ok(()) => {
            bootstrap::compact_after_reset(&fresh_catalog, &cache);
            let (rows, cache_file_bytes) = bootstrap::measure_storage(runtime, &fresh_catalog);
            ResetOutcome::Succeeded {
                rows,
                cache_file_bytes,
            }
        }
        Err(e) => {
            tracing::error!("reset catalog failed: {e}");
            ResetOutcome::Failed {
                sources: bootstrap::get_sources(&fresh_catalog).ok(),
            }
        }
    }
}

/// Task 097: the messages a `ResetOutcome` becomes -- a plain function, not
/// inline in `reset_task`'s `.then()`, so a test can call it directly
/// (`Task::batch`'s own inputs aren't independently drivable from outside
/// `iced`'s runtime, the same reason `compact_then_measure` was split out
/// in Task 096). One outcome can mean two messages: the reset result
/// itself, then either the measurement a success is followed by or the
/// reload a failure is followed by.
fn reset_outcome_to_messages(outcome: ResetOutcome) -> Vec<Message> {
    match outcome {
        ResetOutcome::Succeeded {
            rows,
            cache_file_bytes,
        } => vec![
            Message::CatalogResetSucceeded,
            if bootstrap::storage_measurement_is_failure(&rows, cache_file_bytes) {
                Message::StorageMeasurementFailed
            } else {
                Message::StorageDataReady {
                    rows,
                    cache_file_bytes,
                }
            },
        ],
        ResetOutcome::NotAttempted => vec![notice_retry::reset_storage_unavailable()],
        ResetOutcome::Failed { sources } => {
            let mut messages = vec![notice_retry::reset_failed()];
            if let Some(cards) = sources {
                messages.push(Message::SourcesLoaded(cards));
            }
            messages
        }
    }
}

/// Task 097: `reset_catalog_delete_compact_and_measure` off the update
/// thread, mapped through `reset_outcome_to_messages`.
fn reset_task(runtime: orbok::runtime_context::RuntimeContext) -> iced::Task<Message> {
    iced::Task::perform(
        async move { reset_catalog_delete_compact_and_measure(&runtime) },
        |outcome| outcome,
    )
    .then(|outcome| {
        iced::Task::batch(
            reset_outcome_to_messages(outcome)
                .into_iter()
                .map(iced::Task::done),
        )
    })
}

/// Task 092: what the reset confirmation will remove, counted fresh off
/// the update thread the same way `measure_storage_task` measures storage
/// -- the dialog itself renders in the same `update` pass that dispatches
/// this; the line appears when the counts arrive.
fn reset_counts_task(catalog: std::sync::Arc<orbok_db::Catalog>) -> iced::Task<Message> {
    iced::Task::perform(
        async move { bootstrap::get_reset_counts(&catalog) },
        |result| match result {
            Ok(counts) => Message::ResetCountsReady(counts),
            Err(_) => Message::ResetCountsFailed,
        },
    )
}

/// Task 099: what a confirmed rebuild action (delete the keyword or vector
/// index, then mark its files for re-preparation) returns -- the same
/// three-way shape as `ResetOutcome`, without the compaction step (this
/// action does not run `VACUUM`, so there is nothing to guard on free
/// space here).
#[derive(Debug)]
enum RebuildOutcome {
    Succeeded {
        rows: Vec<(orbok_core::StorageCategory, orbok_core::StorageMeasurement)>,
        cache_file_bytes: Option<u64>,
    },
    /// The catalog or cache could not even be opened. Nothing was
    /// attempted.
    NotAttempted,
    /// The delete itself failed after a real attempt.
    Failed,
}

/// Task 099 (RFC-011 §14 criterion 6): delete the keyword index and mark
/// its files for re-preparation, off the update thread, on its own
/// connection -- Task 097's shape, for the same reason: this deletes from
/// the same tables a reset does, at a comparable scale.
fn delete_keyword_index_and_measure(
    runtime: &orbok::runtime_context::RuntimeContext,
) -> RebuildOutcome {
    let Ok(fresh_catalog) = bootstrap::open_catalog(runtime) else {
        tracing::error!("rebuild keyword index failed: catalog unavailable");
        return RebuildOutcome::NotAttempted;
    };
    let Ok(cache) = bootstrap::cache_service(runtime) else {
        tracing::error!("rebuild keyword index failed: cache unavailable");
        return RebuildOutcome::NotAttempted;
    };
    match bootstrap::delete_keyword_index(&fresh_catalog, &cache) {
        Ok(()) => {
            let (rows, cache_file_bytes) = bootstrap::measure_storage(runtime, &fresh_catalog);
            RebuildOutcome::Succeeded {
                rows,
                cache_file_bytes,
            }
        }
        Err(e) => {
            tracing::error!("rebuild keyword index failed: {e}");
            RebuildOutcome::Failed
        }
    }
}

/// Task 099 (RFC-011 §14 criterion 5): delete the vector index and, if a
/// model is configured, mark its files for re-embedding. Same shape as
/// [`delete_keyword_index_and_measure`].
fn delete_vector_index_and_measure(
    runtime: &orbok::runtime_context::RuntimeContext,
    model_id: Option<orbok_core::ModelId>,
) -> RebuildOutcome {
    let Ok(fresh_catalog) = bootstrap::open_catalog(runtime) else {
        tracing::error!("rebuild vector index failed: catalog unavailable");
        return RebuildOutcome::NotAttempted;
    };
    let Ok(cache) = bootstrap::cache_service(runtime) else {
        tracing::error!("rebuild vector index failed: cache unavailable");
        return RebuildOutcome::NotAttempted;
    };
    match bootstrap::delete_vector_index(&fresh_catalog, &cache, model_id) {
        Ok(()) => {
            let (rows, cache_file_bytes) = bootstrap::measure_storage(runtime, &fresh_catalog);
            RebuildOutcome::Succeeded {
                rows,
                cache_file_bytes,
            }
        }
        Err(e) => {
            tracing::error!("rebuild vector index failed: {e}");
            RebuildOutcome::Failed
        }
    }
}

/// Task 099: the messages a `RebuildOutcome` becomes. `retry` is the
/// `AskDelete*` message a failure notice's "Try again" re-opens -- never
/// the `ConfirmDelete*` that would silently re-run the destructive half
/// without asking again (matches `notice_retry::reset_failed`'s own rule).
fn rebuild_outcome_to_messages(outcome: RebuildOutcome, retry: Message) -> Vec<Message> {
    match outcome {
        RebuildOutcome::Succeeded {
            rows,
            cache_file_bytes,
        } => vec![
            if bootstrap::storage_measurement_is_failure(&rows, cache_file_bytes) {
                Message::StorageMeasurementFailed
            } else {
                Message::StorageDataReady {
                    rows,
                    cache_file_bytes,
                }
            },
        ],
        RebuildOutcome::NotAttempted | RebuildOutcome::Failed => {
            vec![notice_retry::cleanup_did_not_finish(&retry)]
        }
    }
}

/// Task 099: [`delete_keyword_index_and_measure`] off the update thread.
fn delete_keyword_index_task(
    runtime: orbok::runtime_context::RuntimeContext,
) -> iced::Task<Message> {
    iced::Task::perform(
        async move { delete_keyword_index_and_measure(&runtime) },
        |outcome| outcome,
    )
    .then(|outcome| {
        iced::Task::batch(
            rebuild_outcome_to_messages(outcome, Message::AskDeleteKeywordIndex)
                .into_iter()
                .map(iced::Task::done),
        )
    })
}

/// Task 099: [`delete_vector_index_and_measure`] off the update thread.
fn delete_vector_index_task(
    runtime: orbok::runtime_context::RuntimeContext,
    model_id: Option<orbok_core::ModelId>,
) -> iced::Task<Message> {
    iced::Task::perform(
        async move { delete_vector_index_and_measure(&runtime, model_id) },
        |outcome| outcome,
    )
    .then(|outcome| {
        iced::Task::batch(
            rebuild_outcome_to_messages(outcome, Message::AskDeleteVectorIndex)
                .into_iter()
                .map(iced::Task::done),
        )
    })
}

/// Task 099: either rebuild confirmation's own counted line, fetched fresh
/// off the update thread when it opens -- the same `Task::perform` shape
/// as `reset_counts_task`.
fn keyword_rebuild_counts_task(catalog: std::sync::Arc<orbok_db::Catalog>) -> iced::Task<Message> {
    iced::Task::perform(
        async move { bootstrap::get_keyword_rebuild_count(&catalog) },
        |result| match result {
            Ok(count) => Message::RebuildCountsReady(count),
            Err(_) => Message::RebuildCountsFailed,
        },
    )
}

/// Task 099: the vector rebuild's own counted line. `None` model means no
/// model is configured, so there is nothing to count against -- `Ok(0)`,
/// same rendered result ("no line") as a genuine zero.
fn vector_rebuild_counts_task(
    catalog: std::sync::Arc<orbok_db::Catalog>,
    model_id: Option<orbok_core::ModelId>,
) -> iced::Task<Message> {
    iced::Task::perform(
        async move {
            match model_id {
                Some(id) => bootstrap::get_vector_rebuild_count(&catalog, &id),
                None => Ok(0),
            }
        },
        |result| match result {
            Ok(count) => Message::RebuildCountsReady(count),
            Err(_) => Message::RebuildCountsFailed,
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
