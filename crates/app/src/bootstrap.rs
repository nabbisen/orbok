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
use orbok_db::Catalog;
use orbok_db::repo::SettingsRepository;
use orbok_embed::{create_embedding_model, recommended_config_from_model_dir};
use orbok_models::EmbeddingModel;
use orbok_models::{ManagedModelStore, ModelStoreLockError, ModelStoreMutationGuard, SharedAccess};
use orbok_search::HybridSearchService;
use orbok_ui::AppState;
use orbok_ui::i18n::Locale;
use orbok_ui::state::ModelProvenance;
use orbok_ui::theme::{TextScale, Theme};
use orbok_workers::verify_embedding_model;
use std::path::PathBuf;
use std::time::Duration;

use crate::settings::{OrbokSettings, load_settings};
use orbok::runtime_context::{
    AllowRuntimePathProbe, PlatformRuntimePaths, RuntimeAccess, RuntimeContext, RuntimeMode,
    RuntimePathKind, RuntimePathProbe, RuntimeSelection, path_is_within, paths_overlap,
};

/// Capture process inputs once and construct the immutable RFC-049 context.
pub fn resolve_runtime_context(
    portable: bool,
) -> Result<RuntimeContext, Box<dyn std::error::Error>> {
    let startup_dir = std::env::current_dir()?;
    let data_override = std::env::var_os("ORBOK_DATA_DIR");
    let standard_data_dir = dirs::data_local_dir().map(|directory| directory.join("orbok"));
    let standard_settings_file = crate::settings::standard_settings_file();
    let standard_settings_dir = standard_settings_file
        .parent()
        .ok_or("standard settings path has no parent directory")?;
    let selection = RuntimeSelection::resolve(portable, data_override)?;
    let context = RuntimeContext::resolve(
        selection,
        &startup_dir,
        PlatformRuntimePaths {
            standard_data_dir: standard_data_dir.as_deref(),
            standard_settings_dir,
        },
    )?;
    if context.mode() == RuntimeMode::Portable {
        validate_physical_profile_separation(
            &context,
            standard_data_dir.as_deref(),
            standard_settings_dir,
        )?;
    }
    Ok(context)
}

pub(crate) fn validate_physical_profile_separation(
    context: &RuntimeContext,
    standard_data_dir: Option<&std::path::Path>,
    standard_settings_dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let portable_paths = [
        context.data_dir().to_path_buf(),
        context.catalog_file().to_path_buf(),
        context.settings_file().to_path_buf(),
    ];
    let mut standard_paths = vec![
        standard_settings_dir.to_path_buf(),
        standard_settings_dir.join("settings.json"),
    ];
    if let Some(data_dir) = standard_data_dir {
        standard_paths.push(data_dir.to_path_buf());
        standard_paths.push(data_dir.join(orbok_db::CATALOG_FILE_NAME));
    }
    for portable_path in &portable_paths {
        let portable = physical_location(portable_path)?;
        for standard_path in &standard_paths {
            let standard = physical_location(standard_path)?;
            let canonical_overlap = paths_overlap(&portable.resolved_path, &standard.resolved_path);
            let identity_overlap = portable.identity == standard.identity
                && paths_overlap(&portable.missing_suffix, &standard.missing_suffix);
            if canonical_overlap || identity_overlap {
                return Err(
                    "portable and standard runtime profiles resolve to the same physical path"
                        .into(),
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct PhysicalLocation {
    identity: FileIdentity,
    missing_suffix: PathBuf,
    resolved_path: PathBuf,
}

#[cfg(unix)]
#[derive(Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Debug, Eq, PartialEq)]
struct FileIdentity {
    volume: u32,
    index: u64,
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Eq, PartialEq)]
struct FileIdentity(PathBuf);

#[cfg(unix)]
fn file_identity(path: &std::path::Path) -> std::io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = std::fs::metadata(path)?;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn file_identity(path: &std::path::Path) -> std::io::Result<FileIdentity> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
        OPEN_EXISTING,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated and remains alive for the call. The
    // returned handle is checked and closed on every subsequent path.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `handle` is valid and `information` points to writable storage
    // of the structure required by `GetFileInformationByHandle`.
    let result = unsafe { GetFileInformationByHandle(handle, &mut information) };
    let error = (result == 0).then(std::io::Error::last_os_error);
    // SAFETY: `handle` is an owned valid handle and is closed exactly once.
    unsafe { CloseHandle(handle) };
    if let Some(error) = error {
        return Err(error);
    }
    Ok(FileIdentity {
        volume: information.dwVolumeSerialNumber,
        index: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    })
}

#[cfg(not(any(unix, windows)))]
fn file_identity(path: &std::path::Path) -> std::io::Result<FileIdentity> {
    Ok(FileIdentity(std::fs::canonicalize(path)?))
}

/// Resolve the nearest existing ancestor and retain both its filesystem object
/// identity and a policy-checked absent suffix. Identity catches bind mounts
/// and other aliases whose canonical names remain distinct.
fn physical_location(path: &std::path::Path) -> std::io::Result<PhysicalLocation> {
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match std::fs::canonicalize(existing) {
            Ok(mut resolved) => {
                let identity = file_identity(existing)?;
                let mut missing_suffix = PathBuf::new();
                for component in suffix.iter().rev() {
                    resolved.push(component);
                    missing_suffix.push(component);
                }
                validate_missing_suffix(&missing_suffix)?;
                return Ok(PhysicalLocation {
                    identity,
                    missing_suffix,
                    resolved_path: resolved,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    std::io::Error::new(error.kind(), "runtime path has no existing ancestor")
                })?;
                suffix.push(name.to_os_string());
                existing = existing.parent().ok_or_else(|| {
                    std::io::Error::new(error.kind(), "runtime path has no existing ancestor")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn validate_missing_suffix(suffix: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if !suffix.as_os_str().is_ascii() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "non-ASCII absent profile suffix cannot be identity-validated on macOS",
        ));
    }
    let _ = suffix;
    Ok(())
}

/// The only production boundary permitted to perform profile filesystem I/O.
/// Tests replace its probe with a recorder/denier while retaining the exact
/// same operations, so observation surrounds the operation rather than merely
/// returning a selected path to an unrelated caller.
struct RuntimeStorage<'a, P: ?Sized> {
    context: &'a RuntimeContext,
    probe: &'a P,
}

impl<'a, P: RuntimePathProbe + ?Sized> RuntimeStorage<'a, P> {
    fn new(context: &'a RuntimeContext, probe: &'a P) -> Self {
        Self { context, probe }
    }

    fn path(&self, kind: RuntimePathKind) -> std::io::Result<&'a std::path::Path> {
        RuntimeAccess::new(self.context, self.probe).active_path(kind)
    }

    fn open_catalog(&self) -> OrbokResult<Catalog> {
        let path = self.path(RuntimePathKind::Catalog)?;
        std::fs::create_dir_all(self.context.data_dir())?;
        Catalog::open(path)
    }

    fn cache_service(&self) -> std::io::Result<orbok_cache::CacheService> {
        self.path(RuntimePathKind::Cache)?;
        Ok(orbok_cache::CacheService::new(self.context.data_dir()))
    }

    fn ensure_default_model_store(&self) -> std::io::Result<PathBuf> {
        self.path(RuntimePathKind::Models)?;
        let root = default_model_store_root(self.context.data_dir());
        std::fs::create_dir_all(&root)?;
        Ok(root)
    }

    #[cfg(test)]
    fn ensure_support_dir(&self, kind: RuntimePathKind) -> std::io::Result<PathBuf> {
        debug_assert!(matches!(
            kind,
            RuntimePathKind::Diagnostics | RuntimePathKind::Temporary
        ));
        let path = self.path(kind)?;
        std::fs::create_dir_all(path)?;
        Ok(path.to_path_buf())
    }

    fn load_settings(&self) -> std::io::Result<OrbokSettings> {
        let path = self.path(RuntimePathKind::Settings)?;
        Ok(load_settings(path))
    }

    fn save_settings(&self, settings: &OrbokSettings) -> std::io::Result<()> {
        let path = self.path(RuntimePathKind::Settings)?;
        crate::settings::save_settings(path, settings)
    }

    fn run_startup_recovery(
        &self,
        catalog: &Catalog,
    ) -> OrbokResult<orbok_workers::RecoveryReport> {
        let data_dir = self.path(RuntimePathKind::Recovery)?;
        orbok_workers::run_startup_recovery(catalog, &data_dir.join(orbok_db::CACHE_FILE_NAME))
    }

    fn run_managed_model_startup(
        &self,
        catalog: &Catalog,
        model_store: &ManagedModelStore,
    ) -> Result<orbok_workers::ManagedModelStartupOutcome, orbok_workers::ModelLifecycleError> {
        // The model root was authorized immediately before its creation.
        orbok_workers::run_managed_model_startup(catalog, model_store)
    }
}

pub fn open_catalog(context: &RuntimeContext) -> OrbokResult<Catalog> {
    open_catalog_with(context, &AllowRuntimePathProbe)
}

pub fn open_catalog_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> OrbokResult<Catalog> {
    RuntimeStorage::new(context, probe).open_catalog()
}

pub fn cache_service(context: &RuntimeContext) -> std::io::Result<orbok_cache::CacheService> {
    cache_service_with(context, &AllowRuntimePathProbe)
}

pub fn cache_service_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> std::io::Result<orbok_cache::CacheService> {
    RuntimeStorage::new(context, probe).cache_service()
}

pub fn active_model_store_root(context: &RuntimeContext) -> std::io::Result<PathBuf> {
    RuntimeAccess::new(context, &AllowRuntimePathProbe).active_path(RuntimePathKind::Models)?;
    Ok(default_model_store_root(context.data_dir()))
}

pub fn load_runtime_settings(context: &RuntimeContext) -> std::io::Result<OrbokSettings> {
    runtime_settings_with(context, &AllowRuntimePathProbe)
}

pub(crate) fn runtime_settings_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> std::io::Result<OrbokSettings> {
    RuntimeStorage::new(context, probe).load_settings()
}

pub fn save_runtime_settings(
    context: &RuntimeContext,
    settings: &OrbokSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    save_runtime_settings_with(context, &AllowRuntimePathProbe, settings)
}

pub(crate) fn save_runtime_settings_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    settings: &OrbokSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    RuntimeStorage::new(context, probe)
        .save_settings(settings)
        .map_err(|error| format!("settings save failed: {error:?}").into())
}

pub fn default_model_store_root(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("models").join("multilingual-e5-small")
}

#[cfg(test)]
pub fn ensure_default_model_store<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> std::io::Result<PathBuf> {
    RuntimeStorage::new(context, probe).ensure_default_model_store()
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
    provenance: Option<ModelProvenance>,
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
            provenance: Some(ModelProvenance::AppManaged),
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
        provenance: manual.as_ref().map(|_| ModelProvenance::UserSupplied),
        path: manual,
    })
}

fn is_within_model_store(candidate: &std::path::Path, store_root: &std::path::Path) -> bool {
    fn comparable(path: &std::path::Path) -> PathBuf {
        physical_location(path)
            .map(|location| location.resolved_path)
            .or_else(|_| std::path::absolute(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
    path_is_within(&comparable(candidate), &comparable(store_root))
}

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
    let model_store_root = storage.ensure_default_model_store()?;
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
    let model_store = ManagedModelStore::default_embedding(model_store_root);
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
    let settings = storage.load_settings()?;

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
        reduced_motion: settings.reduced_motion || resolve_os_reduced_motion(),
        capability: projection.capability,
        active_model_provenance: projection.active_provenance,
        wizard: projection.wizard,
        model_download_consent: Some(orbok_ui::ModelDownloadConsent::trusted_default(
            default_model_store_root(context.data_dir())
                .to_string_lossy()
                .into_owned(),
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

/// Execute a keyword/hybrid search and convert results to UI structs.
/// Uses hybrid search (keyword + semantic) when an embedding model is
/// configured and the tract feature is compiled in; keyword-only
/// otherwise (RFC-008/009).
pub(crate) fn run_search(
    context: &RuntimeContext,
    catalog: &Catalog,
    query: &str,
    limit: u32,
) -> Result<Vec<orbok_ui::state::SearchResultDisplay>, Box<dyn std::error::Error>> {
    run_search_with(context, &AllowRuntimePathProbe, catalog, query, limit)
}

pub(crate) fn run_search_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    catalog: &Catalog,
    query: &str,
    limit: u32,
) -> Result<Vec<orbok_ui::state::SearchResultDisplay>, Box<dyn std::error::Error>> {
    let settings = runtime_settings_with(context, probe)?;
    let resolved_model = match resolve_model_dir(catalog, &settings) {
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
pub fn run_check(context: &RuntimeContext) -> Result<(), Box<dyn std::error::Error>> {
    run_check_with(context, &AllowRuntimePathProbe)
}

pub fn run_check_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = RuntimeStorage::new(context, probe);
    storage.ensure_default_model_store()?;
    tracing::info!(path = %context.data_dir().display(), "opening catalog");
    let catalog = storage.open_catalog()?;
    let version = catalog.schema_version()?;
    let expected = orbok_db::migrations::latest_version();
    if version != expected {
        return Err(format!("schema version {version} != expected {expected}").into());
    }

    // Report model status in --check output.
    let settings = storage.load_settings()?;
    let resolved_model = resolve_model_dir(&catalog, &settings)?;
    let outcome = verify_embedding_model(resolved_model.path.as_deref());
    println!(
        "orbok --check OK  data_dir={}  schema_version={}  model={}",
        context.data_dir().display(),
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
pub fn persist_theme(
    context: &RuntimeContext,
    theme: Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    persist_theme_with(context, &AllowRuntimePathProbe, theme)
}

pub(crate) fn persist_theme_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    theme: Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = runtime_settings_with(context, probe)?;
    settings.theme = theme.as_str().to_string();
    save_runtime_settings_with(context, probe, &settings)
}

/// Persist the text scale to `OrbokSettings` (RFC-035).
pub fn persist_text_scale(
    context: &RuntimeContext,
    scale: TextScale,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = runtime_settings_with(context, &AllowRuntimePathProbe)?;
    settings.text_scale = scale.as_str().to_string();
    save_runtime_settings_with(context, &AllowRuntimePathProbe, &settings)
}

/// Persist the reduced-motion preference to `OrbokSettings` (RFC-035).
pub fn persist_reduced_motion(
    context: &RuntimeContext,
    val: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = runtime_settings_with(context, &AllowRuntimePathProbe)?;
    settings.reduced_motion = val;
    save_runtime_settings_with(context, &AllowRuntimePathProbe, &settings)
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
pub fn persist_model_dir(
    context: &RuntimeContext,
    model_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    persist_model_dir_with(context, &AllowRuntimePathProbe, model_dir)
}

pub(crate) fn persist_model_dir_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    model_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = runtime_settings_with(context, probe)?;
    settings.embedding_model_dir = Some(model_dir.to_string());
    save_runtime_settings_with(context, probe, &settings)
}

pub fn remove_managed_model_dir_setting(
    context: &RuntimeContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = runtime_settings_with(context, &AllowRuntimePathProbe)?;
    if settings.embedding_model_dir.as_ref().is_some_and(|path| {
        is_within_model_store(
            std::path::Path::new(path),
            &default_model_store_root(context.data_dir()),
        )
    }) {
        settings.embedding_model_dir = None;
        save_runtime_settings_with(context, &AllowRuntimePathProbe, &settings)?;
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
pub(crate) fn exercise_later_profile_operations_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    source_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    persist_theme_with(context, probe, Theme::Dark)?;
    persist_model_dir_with(context, probe, &source_path.to_string_lossy())?;

    let storage = RuntimeStorage::new(context, probe);
    storage.ensure_support_dir(RuntimePathKind::Diagnostics)?;
    storage.ensure_support_dir(RuntimePathKind::Temporary)?;
    let catalog = storage.open_catalog()?;
    let cache = storage.cache_service()?;
    let (source, _) = add_source(&catalog, &source_path.to_string_lossy())?;
    let _ = run_search_with(context, probe, &catalog, "isolation", 20)?;
    orbok_db::repo::SearchHistoryRepository::new(&catalog).upsert(
        "later isolation search",
        &[],
        Some(0),
        "en",
        &orbok_core::SearchHistorySettings::default(),
    )?;
    clean_snippets(&catalog, &cache, context.cache_file())?;
    clean_search_cache(&catalog, &cache, context.cache_file())?;
    remove_source(&catalog, &source.source_id)?;
    reset_catalog(&catalog, &cache, context.cache_file())?;
    Ok(())
}

#[cfg(test)]
mod managed_resolution_tests {
    use super::*;
    use orbok_workers::VerifyOutcome;

    fn test_context(data_dir: &std::path::Path) -> RuntimeContext {
        RuntimeContext::resolve(
            RuntimeSelection::resolve(false, Some(data_dir.as_os_str().to_os_string())).unwrap(),
            data_dir,
            PlatformRuntimePaths {
                standard_data_dir: Some(data_dir),
                standard_settings_dir: data_dir,
            },
        )
        .unwrap()
    }

    #[test]
    fn initial_state_advances_managed_model_startup_epoch() {
        let temp = tempfile::tempdir().unwrap();

        let context = test_context(temp.path());
        let _state = load_initial_state(&context).unwrap();

        let catalog = open_catalog(&context).unwrap();
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
        let context = test_context(data_dir);
        let root = ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
        let catalog = open_catalog(&context).unwrap();
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
        let context = test_context(temp.path());
        let root = ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
        let catalog = open_catalog(&context).unwrap();
        let managed_path = root.join("generations").join("old-managed-path");
        let settings = OrbokSettings {
            embedding_model_dir: Some(managed_path.to_string_lossy().into_owned()),
            ..OrbokSettings::default()
        };

        let resolved = resolve_model_dir(&catalog, &settings).unwrap();

        assert!(resolved.path.is_none());
        assert_eq!(resolved.provenance, None);
        assert!(resolved._guard.is_none());
    }

    #[test]
    fn genuine_manual_setting_remains_available_when_no_managed_current_exists() {
        let temp = tempfile::tempdir().unwrap();
        let context = test_context(temp.path());
        ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
        let catalog = open_catalog(&context).unwrap();
        let manual = temp.path().join("user-model");
        let settings = OrbokSettings {
            embedding_model_dir: Some(manual.to_string_lossy().into_owned()),
            ..OrbokSettings::default()
        };

        let resolved = resolve_model_dir(&catalog, &settings).unwrap();

        assert_eq!(resolved.path.as_deref(), manual.to_str());
        assert_eq!(resolved.provenance, Some(ModelProvenance::UserSupplied));
        assert!(resolved._guard.is_none());
    }

    #[test]
    fn ready_startup_distinguishes_managed_and_manual_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let context = test_context(temp.path());
        let root = ensure_default_model_store(&context, &AllowRuntimePathProbe).unwrap();
        let catalog = open_catalog(&context).unwrap();
        let store = ManagedModelStore::default_embedding(&root);
        let generation_id = orbok_models::ManagedGenerationId::generate();
        {
            let guard = store.acquire_exclusive(Duration::from_secs(1)).unwrap();
            let repository = orbok_db::repo::ManagedGenerationRepository::new(&catalog);
            repository
                .register_inactive(
                    &guard,
                    generation_id.clone(),
                    orbok_models::trust::DEFAULT_TRUSTED_MODEL.manifest_id,
                )
                .unwrap();
            repository.activate(&guard, &generation_id).unwrap();
        }

        let managed = resolve_model_dir(&catalog, &OrbokSettings::default()).unwrap();
        assert_eq!(
            crate::model_flow::project_startup(VerifyOutcome::Ready, managed.provenance)
                .active_provenance,
            Some(ModelProvenance::AppManaged)
        );

        let manual_temp = tempfile::tempdir().unwrap();
        let manual_context = test_context(manual_temp.path());
        ensure_default_model_store(&manual_context, &AllowRuntimePathProbe).unwrap();
        let manual_catalog = open_catalog(&manual_context).unwrap();
        let manual_path = manual_temp.path().join("user-model");
        let manual_settings = OrbokSettings {
            embedding_model_dir: Some(manual_path.to_string_lossy().into_owned()),
            ..OrbokSettings::default()
        };
        let manual = resolve_model_dir(&manual_catalog, &manual_settings).unwrap();
        assert_eq!(
            crate::model_flow::project_startup(VerifyOutcome::Ready, manual.provenance)
                .active_provenance,
            Some(ModelProvenance::UserSupplied)
        );
        assert_eq!(
            crate::model_flow::project_startup(
                VerifyOutcome::FilesInvalid {
                    model_dir: manual_path.to_string_lossy().into_owned(),
                    issues: Vec::new(),
                },
                manual.provenance,
            )
            .active_provenance,
            None
        );
    }
}
