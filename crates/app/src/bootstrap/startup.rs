//! Initial `AppState` population, headless `--check` validation, and the
//! sidebar/source-list queries both draw on.

use super::model_resolution::{ResolvedModelDir, resolve_model_dir};
use crate::settings::OrbokSettings;
use orbok::runtime_context::{AllowRuntimePathProbe, RuntimeContext, RuntimePathProbe};
use orbok::runtime_storage::RuntimeStorage;
use orbok_db::Catalog;
use orbok_db::repo::SettingsRepository;
use orbok_ui::AppState;
use orbok_ui::i18n::Locale;
use orbok_ui::theme::{TextScale, Theme};
use orbok_workers::verify_embedding_model;

/// Build the initial `AppState` from persisted settings and startup
/// model verification. Activates the wizard when any required model
/// file is missing or not yet configured.
pub fn load_initial_state(
    context: &RuntimeContext,
) -> Result<AppState, Box<dyn std::error::Error>> {
    load_initial_state_with(context, &AllowRuntimePathProbe)
}

pub fn load_initial_state_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let storage = RuntimeStorage::new(context, probe);
    let model_store = storage.model_store()?;
    let catalog = storage.open_catalog()?;

    // RFC-018: reset any jobs left running from a crashed session.
    let recovery = storage.run_startup_recovery(&catalog)?;
    if recovery.jobs_reset > 0 {
        tracing::warn!(
            reset = recovery.jobs_reset,
            "reset interrupted jobs on startup"
        );
    }

    // RFC-050: epoch advancement, staged-generation recovery, and real
    // later-startup load validation precede any managed runtime resolution.
    let model_recovery = storage.run_managed_model_startup(&catalog, &model_store)?;
    tracing::info!(
        startup_epoch = model_recovery.startup_epoch,
        recovered_inactive = model_recovery.recovered_inactive,
        quarantined_staging = model_recovery.quarantined_staging,
        quarantined_generations = model_recovery.quarantined_generations,
        rolled_back = model_recovery.rolled_back,
        "managed model startup recovery completed"
    );

    // Load persisted OrbokSettings (app-json-settings).
    let settings = storage.load_settings::<OrbokSettings>()?;

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
    let sources = get_sources(&catalog);
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
pub fn run_check(context: &RuntimeContext) -> Result<(), Box<dyn std::error::Error>> {
    run_check_with(context, &AllowRuntimePathProbe)
}

pub fn run_check_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = RuntimeStorage::new(context, probe);
    storage.model_store()?;
    tracing::info!(path = %context.descriptor(), "opening catalog");
    let catalog = storage.open_catalog()?;
    let version = catalog.schema_version()?;
    let expected = orbok_db::migrations::latest_version();
    if version != expected {
        return Err(format!("schema version {version} != expected {expected}").into());
    }

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
    use orbok_core::FileStatus;
    use orbok_db::repo::{FileRepository, IndexJobRepository};
    let files = FileRepository::new(catalog);
    let indexed = files.count_with_status(FileStatus::Indexed).unwrap_or(0);
    let stale = files.count_with_status(FileStatus::Stale).unwrap_or(0);
    let failed = files.count_with_status(FileStatus::Failed).unwrap_or(0);
    let queued = IndexJobRepository::new(catalog)
        .list_queued(u32::MAX)
        .unwrap_or_default()
        .len() as u64;
    orbok_ui::state::IndexHealth {
        indexed,
        stale,
        failed,
        queued,
    }
}

/// Load all registered sources for the Sources view.
pub fn get_sources(catalog: &Catalog) -> Vec<orbok_ui::state::SourceCard> {
    use orbok_core::FileStatus;
    use orbok_db::repo::{FileRepository, SourceRepository};
    SourceRepository::new(catalog)
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|src| {
            let files = FileRepository::new(catalog);
            let indexed = files
                .count_for_source_with_status(&src.source_id, FileStatus::Indexed)
                .unwrap_or(0);
            let stale = files
                .count_for_source_with_status(&src.source_id, FileStatus::Stale)
                .unwrap_or(0);
            let failed = files
                .count_for_source_with_status(&src.source_id, FileStatus::Failed)
                .unwrap_or(0);
            orbok_ui::state::SourceCard {
                display_name: src.display_name.unwrap_or_else(|| "source".into()),
                display_path: src.canonical_path,
                indexed,
                stale,
                failed,
                active: matches!(src.status, orbok_core::SourceStatus::Active),
                source_id: src.source_id.as_str().to_string(),
            }
        })
        .collect()
}
