//! Initial `AppState` population, headless `--check` validation, and the
//! sidebar/source-list queries both draw on.

use super::model_resolution::{ResolvedModelDir, resolve_model_dir};
use crate::settings::OrbokSettings;
use crate::startup_failure::StartupFailure;
use orbok::runtime_context::{AllowRuntimePathProbe, RuntimeContext, RuntimePathProbe};
use orbok::runtime_storage::RuntimeStorage;
use orbok_core::OrbokResult;
use orbok_db::Catalog;
use orbok_db::repo::{SettingsRepository, SourceRepository};
use orbok_ui::AppState;
use orbok_ui::i18n::Locale;
use orbok_ui::theme::{TextScale, Theme};
use orbok_workers::verify_embedding_model;

/// Build the initial `AppState` from persisted settings and startup
/// model verification. Activates the wizard when any required model
/// file is missing or not yet configured.
///
/// Task 071: a failure is a typed [`StartupFailure`], built at the step that
/// failed, so the window can say what to check.
pub fn load_initial_state(context: &RuntimeContext) -> Result<AppState, StartupFailure> {
    load_initial_state_with(context, &AllowRuntimePathProbe)
}

pub fn load_initial_state_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> Result<AppState, StartupFailure> {
    let storage = RuntimeStorage::new(context, probe);
    // Authorises and creates `<data>/models`: the first step that touches
    // the data folder.
    let model_store = storage
        .model_store()
        .map_err(|error| StartupFailure::data_folder(context, error))?;
    let catalog = storage
        .open_catalog_staged()
        .map_err(|error| StartupFailure::from_catalog_open(context, error))?;

    // RFC-018: reset any jobs left running from a crashed session.
    let recovery = storage
        .run_startup_recovery(&catalog)
        .map_err(StartupFailure::other)?;
    if recovery.jobs_reset > 0 {
        tracing::warn!(
            reset = recovery.jobs_reset,
            "reset interrupted jobs on startup"
        );
    }

    // Task 079 (Review Request 255 §6): a namespace this project has
    // retired (a prior payload-shape change left it behind) is purged
    // once, here, rather than left for the user to press "Clear temporary
    // extraction" to reclaim -- measured cheap at scale (182 ms at 20,000
    // rows, `task079_purge_cost_at_20000_retired_rows`). Best-effort: a
    // failure here does not stop orbok from starting, unlike the recovery
    // step above -- this is space reclamation, not repair the rest of
    // startup depends on.
    match storage.cache() {
        Ok(cache) => match cache.service().purge_retired_namespaces() {
            Ok(removed) if removed > 0 => {
                tracing::info!(removed, "purged retired cache namespaces on startup");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "could not purge retired cache namespaces on startup");
            }
        },
        Err(error) => {
            tracing::warn!(%error, "could not open the cache to purge retired namespaces on startup");
        }
    }

    // RFC-037 §10.1 startup check (Task 035): "check registered folder
    // exists / check permission lightly / detect obvious changed/missing
    // files / queue safe refresh work" -- runs after crash recovery
    // (interrupted jobs must be reset before anything new is enqueued) and
    // before model resolution, so an early failure there does not skip it.
    // `check_and_refresh_source` is the exact function manual refresh also
    // calls (§4.2): a startup rescan is that same operation, run once per
    // eligible source instead of on explicit user action -- no second path.
    //
    // Paused sources are skipped entirely, not just left unscanned: §7.4
    // defines Paused as "user or resource policy paused *preparation*",
    // and enqueueing a Scan job is preparation. This is a deliberate
    // reading of an RFC-037 question the text does not resolve on its own
    // (§10.1's blanket "check registered folder exists" against §7.4's
    // narrower "preparation" scope) -- recorded here rather than only in
    // the task's review request, since nothing currently sets a source to
    // Paused (no UI action does), so this is a forward-looking choice, not
    // an observed behaviour.
    // Task 113 (RFC-064 §3.3): folders an older version let overlap are made
    // part of the top folder once, before anything is scanned, so no file is
    // prepared under two folders again. Idempotent: the next start finds
    // nothing and says nothing. A failure is not a failure to start -- it is
    // tried again next time -- and says nothing either, since nothing changed.
    let combined = match super::combine_overlapping_folders(&catalog) {
        Ok(combined) => combined,
        Err(error) => {
            tracing::warn!(%error, "could not combine overlapping folders on startup");
            Vec::new()
        }
    };
    for group in &combined {
        tracing::info!(
            parent = %group.parent_name,
            combined = group.folders.len(),
            "combined overlapping folders on startup"
        );
    }
    // One notice slot: the first group is the one named. A profile with more
    // than one group of overlaps is not expected.
    let combined_notice =
        combined
            .first()
            .map(|group| orbok_ui::notice::UserNotice::FoldersCombined {
                folders: group
                    .folders
                    .iter()
                    .map(|f| f.display_name.clone())
                    .collect(),
                parent: group.parent_name.clone(),
            });
    for source in SourceRepository::new(&catalog).list().unwrap_or_default() {
        if source.status == orbok_core::SourceStatus::Paused {
            continue;
        }
        if let Err(e) = super::check_and_refresh_source(&catalog, source.source_id.as_str()) {
            tracing::warn!(source = source.source_id.as_str(), error = %e, "startup source check failed");
        }
    }

    // RFC-050: epoch advancement, staged-generation recovery, and real
    // later-startup load validation precede any managed runtime resolution.
    // A missing or invalid model is not an error here: an invalid generation
    // is quarantined or rolled back and the wizard opens. What fails is the
    // store itself (unavailable, busy, filesystem, catalog).
    let model_recovery = storage
        .run_managed_model_startup(&catalog, &model_store)
        .map_err(StartupFailure::other)?;
    tracing::info!(
        startup_epoch = model_recovery.startup_epoch,
        recovered_inactive = model_recovery.recovered_inactive,
        quarantined_staging = model_recovery.quarantined_staging,
        quarantined_generations = model_recovery.quarantined_generations,
        rolled_back = model_recovery.rolled_back,
        "managed model startup recovery completed"
    );

    // Load persisted OrbokSettings (app-json-settings). Only authorising the
    // path or writing a first default can fail (an unreadable or malformed
    // file falls back to defaults), and the settings file can live outside
    // the data folder, whose path the window names -- so this is Other.
    let settings = storage
        .load_settings::<OrbokSettings>()
        .map_err(StartupFailure::other)?;

    let catalog_locale = SettingsRepository::new(&catalog)
        .get::<String>("ui.locale")
        .ok()
        .flatten();
    let locale = resolve_locale(
        &settings.locale,
        catalog_locale.as_deref(),
        Locale::from_env(),
    );

    // Verify embedding model files (design §startup-verify).
    let resolved_model = match resolve_model_dir(context, probe, &catalog, &settings) {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(category = %error, "managed model resolution failed closed");
            ResolvedModelDir {
                _guard: None,
                path: None,
                provenance: None,
            }
        }
    };
    let outcome = verify_embedding_model(resolved_model.path.as_deref());
    tracing::info!("{}", orbok_workers::verify_outcome_summary(&outcome));

    let projection = crate::model_flow::project_startup(outcome, resolved_model.provenance);

    // Theme priority (RFC-032): stored intent is kept as-is; `System` is
    // resolved once here to a concrete preset for token construction. The OS
    // probe is best-effort (Theme::from_env), falling back to Light.
    let stored_theme = Theme::parse(&settings.theme).unwrap_or_default();
    let resolved_theme = match stored_theme {
        Theme::System => Theme::from_env().unwrap_or(Theme::Light),
        concrete => concrete,
    };

    let health = get_health(&catalog);
    // Task 075 (Review 253 §2.3): an unreadable folder list is a startup
    // failure, not an empty list.
    let sources = get_sources(&catalog).map_err(StartupFailure::other)?;
    // RFC-042: reflect the persisted history setting and load entries.
    let privacy = settings.privacy_settings();
    let history = if privacy.effective_recent_searches() {
        orbok_db::repo::SearchHistoryRepository::new(&catalog)
            .list()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let state = AppState {
        locale,
        theme: stored_theme,
        tokens: resolved_theme.tokens(),
        text_scale: TextScale::parse(&settings.text_scale).unwrap_or_default(),
        reduced_motion: settings.reduced_motion || super::resolve_os_reduced_motion(),
        capability: projection.capability,
        active_model_provenance: projection.active_provenance,
        wizard: projection.wizard,
        model_download_consent: Some(orbok_ui::ModelDownloadConsent::trusted_default(
            model_store.models_dir_display(),
        )),
        health,
        sources,
        notice: combined_notice,
        remember_recent_searches: settings.remember_recent_searches,
        search_ui: orbok_ui::state::search::SearchUiState {
            history,
            ..Default::default()
        },
        ..Default::default()
    };
    Ok(state)
}

/// RFC-031 §48/§130/§166 locale priority chain: settings file → catalog →
/// OS environment → default (`En`). Pure and injectable: `env_locale` is
/// resolved once by the caller (the real OS-environment detector, in
/// production) and passed in as already-decided data, rather than this
/// function reading `std::env` itself -- the same capture-once-then-decide
/// shape RFC-049/054/055 use for process inputs, and the only way to exercise
/// this chain in a test without mutating process environment variables
/// (`unsafe` in this edition, races the parallel harness; see
/// HANDOFF-055 §5).
///
/// `Locale::parse` returning `None` for `"auto"` (RFC-031's third settings
/// value, alongside `"en"`/`"ja"`) is load-bearing, not incidental: it is
/// the sentinel that lets a fresh profile's settings value fall through to
/// `catalog_locale` and then `env_locale` instead of stopping at the first
/// step. Adding an `"auto" => Some(...)` arm to `Locale::parse` would
/// silently disable OS detection again (Task 009) -- if `parse` ever needs
/// to change to accommodate `"auto"`, this fall-through is being bypassed
/// somewhere else, and that is a decision to stop and report, not make
/// here.
pub(crate) fn resolve_locale(
    settings_locale: &str,
    catalog_locale: Option<&str>,
    env_locale: Option<Locale>,
) -> Locale {
    Locale::parse(settings_locale)
        .or_else(|| catalog_locale.and_then(Locale::parse))
        .or(env_locale)
        .unwrap_or_default()
}

/// Headless backend validation (`--check` mode, RFC-017).
pub fn run_check(context: &RuntimeContext) -> OrbokResult<()> {
    run_check_with(context, &AllowRuntimePathProbe)
}

pub fn run_check_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> OrbokResult<()> {
    let storage = RuntimeStorage::new(context, probe);
    storage.model_store()?;
    tracing::info!(path = %context.descriptor(), "opening catalog");
    // RFC-062 §6: the schema-version guard now lives in
    // `Catalog::from_connection`, naming both versions in a typed
    // `OrbokError::SchemaVersionUnsupported` -- this used to duplicate that
    // check here with a provisional `Database(...)` string. The
    // `open_catalog()?` call below already surfaces it: `--check` still
    // reports the condition, just via the shared path instead of its own
    // copy.
    let catalog = storage.open_catalog()?;
    let version = catalog.schema_version()?;

    // Report model status in --check output.
    let settings = storage.load_settings::<OrbokSettings>()?;
    let resolved_model = resolve_model_dir(context, probe, &catalog, &settings)?;
    let outcome = verify_embedding_model(resolved_model.path.as_deref());
    println!(
        "orbok --check OK  data_dir={}  schema_version={}  model={}",
        context.descriptor(),
        version,
        orbok_workers::verify_outcome_summary(&outcome)
    );
    Ok(())
}

/// Query index health from the catalog for the sidebar summary.
pub fn get_health(catalog: &Catalog) -> orbok_ui::state::IndexHealth {
    use orbok_core::{FileStatus, JobStatus};
    use orbok_db::repo::{FileRepository, IndexJobRepository};
    let files = FileRepository::new(catalog);
    let indexed = files.count_with_status(FileStatus::Indexed).unwrap_or(0);
    let stale = files.count_with_status(FileStatus::Stale).unwrap_or(0);
    let failed = files.count_with_status(FileStatus::Failed).unwrap_or(0);
    // Task 034 §6 (audit P-02): a COUNT(*), not list_queued(u32::MAX).len()
    // -- this runs after every completed job, and the old form
    // materialized and sorted the entire queued-job table just to call
    // .len() on the result.
    let queued = IndexJobRepository::new(catalog)
        .count_with_status(JobStatus::Queued)
        .unwrap_or(0);
    orbok_ui::state::IndexHealth {
        indexed,
        stale,
        failed,
        queued,
    }
}

/// Task 092/094: what a reset would remove, counted fresh when the
/// confirmation opens. `?` propagates a read failure rather than
/// substituting 0 for any count -- the dialog's own rule is that an
/// unreadable count means no line at all, never a guess, so a failed
/// history read must fail the whole result, not just omit its own
/// clause (Task 094 test 5).
pub fn get_reset_counts(catalog: &Catalog) -> OrbokResult<orbok_ui::state::ResetCounts> {
    use orbok_core::FileStatus;
    use orbok_db::repo::{FileRepository, SearchHistoryRepository, SourceRepository};
    let folders = SourceRepository::new(catalog).count()?;
    let files = FileRepository::new(catalog).count_with_status(FileStatus::Indexed)?;
    let history = SearchHistoryRepository::new(catalog).count()? as u64;
    Ok(orbok_ui::state::ResetCounts {
        folders,
        files,
        history,
    })
}

/// Task 099: how many files the "prepare keyword search again"
/// confirmation's own line names, counted fresh when it opens.
pub fn get_keyword_rebuild_count(catalog: &Catalog) -> OrbokResult<u64> {
    use orbok_db::repo::IndexJobRepository;
    IndexJobRepository::new(catalog).count_extraction_backfill_candidates()
}

/// Task 099: the "prepare search by meaning again" confirmation's own
/// counted line, for the currently configured model.
pub fn get_vector_rebuild_count(
    catalog: &Catalog,
    model_id: &orbok_core::ModelId,
) -> OrbokResult<u64> {
    use orbok_db::repo::IndexJobRepository;
    IndexJobRepository::new(catalog).count_embedding_backfill_candidates(model_id)
}

/// Load all registered sources for the Folders view. Each card is built by
/// `sources::source_card`, the one builder (Task 108).
pub fn get_sources(catalog: &Catalog) -> OrbokResult<Vec<orbok_ui::state::SourceCard>> {
    use orbok_db::repo::SourceRepository;
    Ok(SourceRepository::new(catalog)
        .list()?
        .into_iter()
        .map(|src| super::sources::source_card(catalog, src))
        .collect())
}
