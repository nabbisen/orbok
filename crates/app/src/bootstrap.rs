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
use orbok_embed::{create_embedding_model, recommended_config};
use orbok_models::EmbeddingModel;
use orbok_models::SearchCapability;
use orbok_search::HybridSearchService;
use orbok_ui::AppState;
use orbok_ui::i18n::Locale;
use orbok_ui::state::{WizardFileCheck, WizardState};
use orbok_ui::theme::{TextScale, Theme};
use orbok_workers::{VerifyOutcome, verify_embedding_model};
use std::path::PathBuf;

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

/// Build the initial `AppState` from persisted settings and startup
/// model verification. Activates the wizard when any required model
/// file is missing or not yet configured.
pub fn load_initial_state() -> Result<AppState, Box<dyn std::error::Error>> {
    let dir = data_dir();
    let catalog = open_catalog(&dir)?;

    // RFC-018: reset any jobs left running from a crashed session.
    let cache_path = dir.join(orbok_db::CACHE_FILE_NAME);
    let recovery = orbok_workers::run_startup_recovery(&catalog, &cache_path)?;
    if recovery.jobs_reset > 0 {
        tracing::warn!(
            reset = recovery.jobs_reset,
            "reset interrupted jobs on startup"
        );
    }

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
    let outcome = verify_embedding_model(settings.embedding_model_dir.as_deref());
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
    let mut state = AppState::default();
    state.locale = locale;
    state.theme = stored_theme;
    state.tokens = resolved_theme.tokens();
    state.text_scale = TextScale::parse(&settings.text_scale).unwrap_or_default();
    state.reduced_motion = settings.reduced_motion || resolve_os_reduced_motion();
    state.capability = capability;
    state.wizard = wizard;
    state.health = health;
    state.sources = sources;
    // RFC-042: reflect the persisted history setting and load entries.
    state.remember_recent_searches = settings.remember_recent_searches;
    let privacy = settings.privacy_settings();
    if privacy.effective_recent_searches() {
        state.search_ui.history = orbok_db::repo::SearchHistoryRepository::new(&catalog)
            .list()
            .unwrap_or_default();
    }
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
    let results = if let Some(dir) = &settings.embedding_model_dir {
        let weights = format!("{dir}/onnx/model.onnx");
        let config = recommended_config(weights);
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
    tracing::info!(path = %dir.display(), "opening catalog");
    let catalog = open_catalog(&dir)?;
    let version = catalog.schema_version()?;
    let expected = orbok_db::migrations::latest_version();
    if version != expected {
        return Err(format!("schema version {version} != expected {expected}").into());
    }

    // Report model status in --check output.
    let settings = load_settings();
    let outcome = verify_embedding_model(settings.embedding_model_dir.as_deref());
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
    let expanded = if raw.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}{}", &raw[1..])
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
