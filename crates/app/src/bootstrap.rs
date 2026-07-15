//! Backend bootstrap: data-directory resolution, catalog open, settings
//! load, model verification, and initial view-model population.
//!
//! Startup sequence (RFC-027, design §startup):
//! 1. resolve data directory (env > portable flag > platform dir)
//! 2. open catalog and run migrations
//! 3. run startup recovery (RFC-018)
//! 4. load `OrbokSettings` from platform config dir
//! 5. verify embedding model files (design §startup-verify)
//! 6. build initial `AppState` (wizard active if model missing)

use orbok_core::OrbokResult;
use orbok_db::repo::SettingsRepository;
use orbok_db::{CATALOG_FILE_NAME, Catalog};
use orbok_embed::{create_embedding_model, recommended_config_from_model_dir};
use orbok_models::EmbeddingModel;
use orbok_models::SearchCapability;
use orbok_models::{ManagedModelStore, ModelStoreLockError, ModelStoreMutationGuard, SharedAccess};
use orbok_search::HybridSearchService;
use orbok_ui::AppState;
use orbok_ui::i18n::Locale;
use orbok_ui::state::{WizardFileCheck, WizardState};
use orbok_ui::theme::{TextScale, Theme};
use orbok_workers::{VerifyOutcome, verify_embedding_model};
use std::path::PathBuf;
use std::time::Duration;

use crate::settings::{OrbokSettings, load_settings};

/// Resolve the orbok local-data directory.
pub fn data_dir() -> PathBuf {
    if let Ok(env) = std::env::var("ORBOK_DATA_DIR") {
        return PathBuf::from(env);
    }
    dirs::data_local_dir()
        .map(|d| d.join("orbok"))
        .unwrap_or_else(|| PathBuf::from("orbok-data"))
}

/// Resolve considering `--portable` flag (RFC-030).
pub fn data_dir_for_args(portable: bool) -> PathBuf {
    if portable {
        PathBuf::from("orbok-data")
    } else {
        data_dir()
    }
}

/// Open the catalog, creating the data directory if needed.
pub fn open_catalog(data_dir: &std::path::Path) -> OrbokResult<Catalog> {
    std::fs::create_dir_all(data_dir)?;
    Catalog::open(data_dir.join(CATALOG_FILE_NAME))
}

pub fn default_model_store_root(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("models").join("multilingual-e5-small")
}

pub fn ensure_default_model_store(data_dir: &std::path::Path) -> std::io::Result<PathBuf> {
    let root = default_model_store_root(data_dir);
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

#[derive(Debug)]
enum ManagedModelResolutionError {
    CatalogPath,
    StoreLock(ModelStoreLockError),
    Catalog,
}

impl std::fmt::Display for ManagedModelResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CatalogPath => formatter.write_str("managed model catalog path is unavailable"),
            Self::StoreLock(error) => {
                write!(formatter, "managed model store is unavailable: {error}")
            }
            Self::Catalog => formatter.write_str("managed model catalog state is unavailable"),
        }
    }
}

impl std::error::Error for ManagedModelResolutionError {}

struct ResolvedModelDir {
    _guard: Option<ModelStoreMutationGuard<SharedAccess>>,
    path: Option<String>,
}

fn managed_current_model_dir_timeout(
    catalog: &Catalog,
    timeout: Duration,
) -> Result<Option<(ModelStoreMutationGuard<SharedAccess>, PathBuf)>, ManagedModelResolutionError> {
    let data_dir = catalog
        .path()
        .parent()
        .ok_or(ManagedModelResolutionError::CatalogPath)?;
    let store = ManagedModelStore::default_embedding(default_model_store_root(data_dir));
    let guard = store
        .acquire_shared(timeout)
        .map_err(ManagedModelResolutionError::StoreLock)?;
    let snapshot = orbok_db::repo::ManagedGenerationRepository::new(catalog)
        .load_shared(&guard)
        .map_err(|_| ManagedModelResolutionError::Catalog)?;
    let Some(generation_id) = snapshot.profile.current_generation_id else {
        return Ok(None);
    };
    let generation_dir = store
        .models_dir()
        .join("generations")
        .join(generation_id.as_str());
    Ok(Some((guard, generation_dir)))
}

fn resolve_model_dir(
    catalog: &Catalog,
    settings: &OrbokSettings,
) -> Result<ResolvedModelDir, ManagedModelResolutionError> {
    resolve_model_dir_with_timeout(catalog, settings, Duration::from_secs(5))
}

fn resolve_model_dir_with_timeout(
    catalog: &Catalog,
    settings: &OrbokSettings,
    timeout: Duration,
) -> Result<ResolvedModelDir, ManagedModelResolutionError> {
    let data_dir = catalog
        .path()
        .parent()
        .ok_or(ManagedModelResolutionError::CatalogPath)?;
    if let Some((guard, path)) = managed_current_model_dir_timeout(catalog, timeout)? {
        return Ok(ResolvedModelDir {
            _guard: Some(guard),
            path: Some(path.to_string_lossy().into_owned()),
        });
    }
    let store_root = default_model_store_root(data_dir);
    let manual = settings
        .embedding_model_dir
        .as_ref()
        .filter(|path| !is_within_model_store(std::path::Path::new(path), &store_root))
        .cloned();
    Ok(ResolvedModelDir {
        _guard: None,
        path: manual,
    })
}

fn is_within_model_store(candidate: &std::path::Path, store_root: &std::path::Path) -> bool {
    fn comparable(path: &std::path::Path) -> PathBuf {
        path.canonicalize()
            .or_else(|_| std::path::absolute(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
    comparable(candidate).starts_with(comparable(store_root))
}

/// Build the initial `AppState` from persisted settings and startup
/// model verification. Activates the wizard when any required model
/// file is missing or not yet configured.
pub fn load_initial_state(dir: &std::path::Path) -> Result<AppState, Box<dyn std::error::Error>> {
    let model_store_root = ensure_default_model_store(dir)?;
    let catalog = open_catalog(dir)?;

    // RFC-018: reset any jobs left running from a crashed session.
    let cache_path = dir.join(orbok_db::CACHE_FILE_NAME);
    let recovery = orbok_workers::run_startup_recovery(&catalog, &cache_path)?;
    if recovery.jobs_reset > 0 {
        tracing::warn!(
            reset = recovery.jobs_reset,
            "reset interrupted jobs on startup"
        );
    }

    // RFC-050: epoch advancement, staged-generation recovery, and real
    // later-startup load validation precede any managed runtime resolution.
    let model_store = ManagedModelStore::default_embedding(model_store_root);
    let model_recovery = orbok_workers::run_managed_model_startup(&catalog, &model_store)?;
    tracing::info!(
        startup_epoch = model_recovery.startup_epoch,
        recovered_inactive = model_recovery.recovered_inactive,
        quarantined_staging = model_recovery.quarantined_staging,
        quarantined_generations = model_recovery.quarantined_generations,
        rolled_back = model_recovery.rolled_back,
        "managed model startup recovery completed"
    );

    // Load persisted OrbokSettings (app-json-settings).
    let settings = load_settings();

    // Locale priority: user settings file → catalog → OS LANG env → default (En).
    // The OS detection satisfies RFC-031 §3 "auto locale resolves Japanese
    // OS environments to ja".
    let locale = Locale::parse(&settings.locale)
        .or_else(|| {
            SettingsRepository::new(&catalog)
                .get::<String>("ui.locale")
                .ok()
                .flatten()
                .and_then(|s| Locale::parse(&s))
        })
        .or_else(Locale::from_env)
        .unwrap_or_default();

    // Verify embedding model files (design §startup-verify).
    let resolved_model = match resolve_model_dir(&catalog, &settings) {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(category = %error, "managed model resolution failed closed");
            ResolvedModelDir {
                _guard: None,
                path: None,
            }
        }
    };
    let outcome = verify_embedding_model(resolved_model.path.as_deref());
    tracing::info!("{}", orbok_workers::verify_outcome_summary(&outcome));

    let (capability, wizard) = build_capability_and_wizard(outcome, &settings);

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
        reduced_motion: settings.reduced_motion || resolve_os_reduced_motion(),
        capability,
        wizard,
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

/// Determine search capability and wizard state from the verify outcome.
fn build_capability_and_wizard(
    outcome: VerifyOutcome,
    _settings: &OrbokSettings,
) -> (SearchCapability, Option<WizardState>) {
    match outcome {
        VerifyOutcome::Ready => (SearchCapability::Hybrid, None),
        VerifyOutcome::NotConfigured => (
            SearchCapability::KeywordOnly,
            Some(WizardState::NotConfigured),
        ),
        VerifyOutcome::FilesInvalid { model_dir, issues } => {
            let checks: Vec<WizardFileCheck> = orbok_workers::model_verifier::REQUIRED_MODEL_FILES
                .iter()
                .map(|rel| {
                    let found = !issues.iter().any(|i| i.relative_path == *rel);
                    WizardFileCheck {
                        relative_path: rel.to_string(),
                        found,
                        size_mb: None,
                    }
                })
                .collect();
            let wizard = WizardState::FileMissing {
                previous_dir: model_dir,
                checks,
            };
            (SearchCapability::KeywordOnly, Some(wizard))
        }
    }
}

/// Execute a keyword/hybrid search and convert results to UI structs.
/// Uses hybrid search (keyword + semantic) when an embedding model is
/// configured and the tract feature is compiled in; keyword-only
/// otherwise (RFC-008/009).
pub(crate) fn run_search(
    catalog: &Catalog,
    query: &str,
    limit: u32,
) -> Result<Vec<orbok_ui::state::SearchResultDisplay>, Box<dyn std::error::Error>> {
    let settings = load_settings();
    let resolved_model = match resolve_model_dir(catalog, &settings) {
        Ok(resolved) => resolved,
        Err(error) => {
            tracing::warn!(category = %error, "managed model resolution failed closed");
            ResolvedModelDir {
                _guard: None,
                path: None,
            }
        }
    };
    let results = if let Some(dir) = &resolved_model.path {
        let config = recommended_config_from_model_dir(dir);
        match create_embedding_model(&config) {
            Ok(model) => {
                // Real model available — use hybrid search.
                let model_ref: &dyn EmbeddingModel = model.as_ref();
                let service =
                    HybridSearchService::with_model(catalog, model_ref, &config.model_name);
                service.search(query, orbok_search::SearchMode::Auto, limit)?
            }
            Err(_) => {
                // Model configured but backend not compiled in (e.g. no --features tract).
                // Fall back to keyword-only.
                HybridSearchService::keyword_only(catalog).search(
                    query,
                    orbok_search::SearchMode::Auto,
                    limit,
                )?
            }
        }
    } else {
        // No model configured — keyword-only.
        HybridSearchService::keyword_only(catalog).search(
            query,
            orbok_search::SearchMode::Auto,
            limit,
        )?
    };
    Ok(results
        .into_iter()
        .map(|r| orbok_ui::state::SearchResultDisplay {
            display_path: r.display_path,
            title: r.title,
            heading_path: r.heading_path,
            snippet: r.snippet,
            keyword_rank: r.keyword_rank,
            badges: r.badges.iter().map(|b| format!("{b:?}")).collect(),
            trust: orbok_ui::state::ResultTrustDisplay::default(),
        })
        .collect())
}

/// Headless backend validation (`--check` mode, RFC-017).
pub fn run_check() -> Result<(), Box<dyn std::error::Error>> {
    let dir = data_dir();
    ensure_default_model_store(&dir)?;
    tracing::info!(path = %dir.display(), "opening catalog");
    let catalog = open_catalog(&dir)?;
    let version = catalog.schema_version()?;
    let expected = orbok_db::migrations::latest_version();
    if version != expected {
        return Err(format!("schema version {version} != expected {expected}").into());
    }

    // Report model status in --check output.
    let settings = load_settings();
    let resolved_model = resolve_model_dir(&catalog, &settings)?;
    let outcome = verify_embedding_model(resolved_model.path.as_deref());
    println!(
        "orbok --check OK  data_dir={}  schema_version={}  model={}",
        dir.display(),
        version,
        orbok_workers::verify_outcome_summary(&outcome)
    );
    Ok(())
}

/// Persist locale to the catalog (called when the user changes language).
pub fn persist_locale(catalog: &Catalog, locale: &Locale) -> OrbokResult<()> {
    SettingsRepository::new(catalog).set("ui.locale", &locale.as_str().to_string())
}

/// Persist the selected UI theme to `OrbokSettings` (RFC-032).
pub fn persist_theme(theme: Theme) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = load_settings();
    settings.theme = theme.as_str().to_string();
    crate::settings::save_settings(&settings)
        .map_err(|e| format!("settings save failed: {e:?}"))?;
    Ok(())
}

/// Persist the text scale to `OrbokSettings` (RFC-035).
pub fn persist_text_scale(scale: TextScale) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = load_settings();
    settings.text_scale = scale.as_str().to_string();
    crate::settings::save_settings(&settings)
        .map_err(|e| format!("settings save failed: {e:?}"))?;
    Ok(())
}

/// Persist the reduced-motion preference to `OrbokSettings` (RFC-035).
pub fn persist_reduced_motion(val: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = load_settings();
    settings.reduced_motion = val;
    crate::settings::save_settings(&settings)
        .map_err(|e| format!("settings save failed: {e:?}"))?;
    Ok(())
}

/// Best-effort OS reduced-motion probe (RFC-035).
///
/// Checks `ORBOK_REDUCE_MOTION=1` env var (override / test hook). A
/// richer per-platform probe (portal, SPI_GETCLIENTAREAANIMATION, NSWorkspace)
/// is a tracked follow-up — returns `false` when unknown.
pub fn resolve_os_reduced_motion() -> bool {
    std::env::var("ORBOK_REDUCE_MOTION")
        .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Persist the validated model directory to `OrbokSettings` (called when
/// the user completes the wizard and accepts a model folder).
pub fn persist_model_dir(model_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = load_settings();
    settings.embedding_model_dir = Some(model_dir.to_string());
    crate::settings::save_settings(&settings)
        .map_err(|e| format!("settings save failed: {e:?}"))?;
    Ok(())
}

pub fn remove_managed_model_dir_setting(
    data_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = load_settings();
    if settings.embedding_model_dir.as_ref().is_some_and(|path| {
        is_within_model_store(
            std::path::Path::new(path),
            &default_model_store_root(data_dir),
        )
    }) {
        settings.embedding_model_dir = None;
        crate::settings::save_settings(&settings)
            .map_err(|e| format!("settings save failed: {e:?}"))?;
    }
    Ok(())
}

// ── Source management ─────────────────────────────────────────────────

/// Add a folder or file as a new searchable source.
/// Returns a populated `SourceCard` for immediate display in the UI.
pub fn add_source(
    catalog: &Catalog,
    raw_path: &str,
) -> Result<(orbok_ui::state::SourceCard, Option<&'static str>), Box<dyn std::error::Error>> {
    use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
    use orbok_db::repo::{NewSource, SourceRepository};
    use std::path::Path;

    let raw = raw_path.trim();
    if raw.is_empty() {
        return Err("path is empty".into());
    }
    // Resolve tilde and canonicalize.
    let expanded = if let Some(stripped) = raw.strip_prefix('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}{stripped}")
    } else {
        raw.to_string()
    };
    let canonical = Path::new(&expanded)
        .canonicalize()
        .map_err(|e| format!("cannot access '{expanded}': {e}"))?
        .to_string_lossy()
        .to_string();

    let source_type = if Path::new(&canonical).is_dir() {
        SourceType::Directory
    } else {
        SourceType::File
    };
    let display_name = Path::new(&canonical)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_string());

    let src = SourceRepository::new(catalog).insert(NewSource {
        source_type,
        persistence_mode: PersistenceMode::Persistent,
        display_name: Some(display_name.clone()),
        original_path: expanded,
        canonical_path: canonical.clone(),
        index_mode: IndexMode::Balanced,
        include_patterns: vec![],
        exclude_patterns: vec![],
        hidden_file_policy: HiddenFilePolicy::Exclude,
        symlink_policy: SymlinkPolicy::Ignore,
        max_file_size_bytes: None,
    })?;

    // RFC-003 acceptance: warn before indexing sensitive directories.
    let sensitive = orbok_fs::sensitive_warning(std::path::Path::new(&canonical));
    if let Some(w) = sensitive {
        tracing::warn!(path = %canonical, warning = w, "sensitive source added");
    }

    Ok((
        orbok_ui::state::SourceCard {
            display_name,
            display_path: canonical,
            indexed: 0,
            stale: 0,
            failed: 0,
            active: true,
            source_id: src.source_id.as_str().to_string(),
        },
        sensitive,
    ))
}

/// Scan a source synchronously and return the updated index health.
/// In production this would run in a background thread; for v0.9 it
/// runs synchronously so the UI reflects results immediately.
pub fn scan_and_index_source(
    catalog: &Catalog,
    cache: &orbok_cache::CacheService,
    source_id_str: &str,
) -> Result<orbok_ui::state::IndexHealth, Box<dyn std::error::Error>> {
    use orbok_core::SourceId;
    use orbok_db::repo::SourceRepository;
    use orbok_fs::{ScanRequest, Scanner};
    use orbok_workers::{ChunkAndIndexWorker, ExtractionWorker, run_pending};
    use std::sync::atomic::AtomicBool;

    let source_id = SourceId::from_string(source_id_str.to_string());
    let src = SourceRepository::new(catalog)
        .get(&source_id)?
        .ok_or("source not found")?;

    Scanner::new(catalog).scan(
        &ScanRequest {
            source_id: src.source_id.clone(),
            force_hash: false,
            enqueue_index_jobs: true,
        },
        &AtomicBool::new(false),
    )?;

    let extract = ExtractionWorker::new(catalog, cache);
    let chunk = ChunkAndIndexWorker::new(catalog, cache);
    run_pending(catalog, &extract, &chunk, None, 2000)?;

    Ok(get_health(catalog))
}

/// Remove a source and its associated indexes from the catalog.
pub fn remove_source(
    catalog: &Catalog,
    source_id_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::SourceId;
    use orbok_db::repo::SourceRepository;
    let source_id = SourceId::from_string(source_id_str.to_string());
    SourceRepository::new(catalog).delete_with_all_data(&source_id)?;
    Ok(())
}

/// Find an existing source whose canonical path matches `canonical_path`.
///
/// Used by the RFC-045 search-in-folder flow to reuse a remembered folder
/// rather than creating a duplicate source record (RFC-045 §6.1, §19.3).
/// Returns `None` when no matching source is found.
pub fn find_source_by_canonical_path(
    catalog: &Catalog,
    canonical_path: &str,
) -> Option<orbok_ui::state::SourceCard> {
    use orbok_core::FileStatus;
    use orbok_db::repo::{FileRepository, SourceRepository};
    SourceRepository::new(catalog)
        .list()
        .unwrap_or_default()
        .into_iter()
        .find(|src| src.canonical_path == canonical_path)
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
            let display_name = src.display_name.unwrap_or_else(|| "folder".to_string());
            orbok_ui::state::SourceCard {
                display_name,
                display_path: src.canonical_path,
                indexed,
                stale,
                failed,
                active: true,
                source_id: src.source_id.as_str().to_string(),
            }
        })
}

// ── Startup population ─────────────────────────────────────────────────

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

// ── Storage cleanup ────────────────────────────────────────────────────

/// Clear the snippet cache (safe, rebuilds on demand).
pub fn clean_snippets(
    catalog: &Catalog,
    cache: &orbok_cache::CacheService,
    cache_db_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::{CleanupAction, CleanupPlan};
    use orbok_workers::CleanupService;
    let plan = CleanupPlan::for_action(CleanupAction::ClearSnippetCache, 0);
    CleanupService::new(catalog, cache, cache_db_path).run_safe(&plan)?;
    Ok(())
}

/// Clear expired search cache (safe, rebuilds on demand).
pub fn clean_search_cache(
    catalog: &Catalog,
    cache: &orbok_cache::CacheService,
    cache_db_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::{CleanupAction, CleanupPlan};
    use orbok_workers::CleanupService;
    let plan = CleanupPlan::for_action(CleanupAction::ClearExpiredSearchCache, 0);
    CleanupService::new(catalog, cache, cache_db_path).run_safe(&plan)?;
    Ok(())
}

/// Full catalog reset (destructive — caller must have confirmed).
pub fn reset_catalog(
    catalog: &Catalog,
    cache: &orbok_cache::CacheService,
    cache_db_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use orbok_core::{CleanupAction, CleanupPlan};
    use orbok_workers::CleanupService;
    let plan = CleanupPlan::for_action(CleanupAction::ResetCatalog, 0);
    CleanupService::new(catalog, cache, cache_db_path).run_reset(&plan, true)?;
    Ok(())
}

#[cfg(test)]
mod managed_resolution_tests {
    use super::*;

    #[test]
    fn initial_state_advances_managed_model_startup_epoch() {
        let temp = tempfile::tempdir().unwrap();

        let _state = load_initial_state(temp.path()).unwrap();

        let catalog = open_catalog(temp.path()).unwrap();
        let store = ManagedModelStore::default_embedding(default_model_store_root(temp.path()));
        let guard = store.acquire_shared(Duration::from_secs(1)).unwrap();
        let snapshot = orbok_db::repo::ManagedGenerationRepository::new(&catalog)
            .load_shared(&guard)
            .unwrap();
        assert_eq!(snapshot.profile.startup_epoch.get(), 1);
    }

    #[test]
    fn exclusive_owner_prevents_managed_path_from_falling_back_as_manual() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path();
        let root = ensure_default_model_store(data_dir).unwrap();
        let catalog = open_catalog(data_dir).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let _exclusive = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
        let settings = OrbokSettings {
            embedding_model_dir: Some(
                root.join("generations")
                    .join("persisted-managed-path")
                    .to_string_lossy()
                    .into_owned(),
            ),
            ..OrbokSettings::default()
        };

        let result = resolve_model_dir_with_timeout(&catalog, &settings, Duration::from_millis(20));

        assert!(matches!(
            result,
            Err(ManagedModelResolutionError::StoreLock(
                ModelStoreLockError::Timeout
            ))
        ));
    }

    #[test]
    fn managed_setting_is_not_treated_as_manual_without_a_catalog_current() {
        let temp = tempfile::tempdir().unwrap();
        let root = ensure_default_model_store(temp.path()).unwrap();
        let catalog = open_catalog(temp.path()).unwrap();
        let managed_path = root.join("generations").join("old-managed-path");
        let settings = OrbokSettings {
            embedding_model_dir: Some(managed_path.to_string_lossy().into_owned()),
            ..OrbokSettings::default()
        };

        let resolved = resolve_model_dir(&catalog, &settings).unwrap();

        assert!(resolved.path.is_none());
        assert!(resolved._guard.is_none());
    }

    #[test]
    fn genuine_manual_setting_remains_available_when_no_managed_current_exists() {
        let temp = tempfile::tempdir().unwrap();
        ensure_default_model_store(temp.path()).unwrap();
        let catalog = open_catalog(temp.path()).unwrap();
        let manual = temp.path().join("user-model");
        let settings = OrbokSettings {
            embedding_model_dir: Some(manual.to_string_lossy().into_owned()),
            ..OrbokSettings::default()
        };

        let resolved = resolve_model_dir(&catalog, &settings).unwrap();

        assert_eq!(resolved.path.as_deref(), manual.to_str());
        assert!(resolved._guard.is_none());
    }
}
