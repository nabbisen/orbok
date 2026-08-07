use super::bootstrap;
use super::settings::{self, OrbokSettings};
use orbok::runtime_context::{
    PlatformRuntimePaths, RuntimeContext, RuntimePathKind, RuntimeSelection,
};
use orbok_core::{
    HiddenFilePolicy, IndexMode, JobStatus, JobType, PersistenceMode, SearchHistorySettings,
    SourceType, SymlinkPolicy,
};
use orbok_db::repo::{IndexJobRepository, NewSource, SearchHistoryRepository, SourceRepository};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
struct RecordingProbe(Mutex<Vec<(RuntimePathKind, PathBuf)>>);

impl orbok::runtime_context::RuntimePathProbe for RecordingProbe {
    fn before_access(&self, kind: RuntimePathKind, path: &Path) -> io::Result<()> {
        self.0.lock().unwrap().push((kind, path.to_path_buf()));
        Ok(())
    }
}

/// Reads every top-level `*.rs` file in `dir` — not recursive, which
/// deliberately excludes `bootstrap/tests/`: the boundary this enforces is a
/// *production* confinement rule, and a test calling `Catalog::open`
/// directly is not a production bypass (Correction Request 111 §4 C5;
/// Response 130 §4). `bootstrap.rs` itself is included by the caller separately via
/// `include_str!`, matching how the other single-file modules are read.
///
/// Panics loudly rather than silently scanning nothing: `include_str!`
/// binds a literal path at compile time and a typo or wrong path would
/// fail to build, but a runtime directory read can silently return an
/// empty or wrong result instead. Response 130 §3 requires this scope be
/// proven non-empty and non-trivial before it is trusted — the same
/// property `assert_denial_armed` (Review 114 §4) established for the
/// RFC-049 denial harness and Task 003 Part C established for the RFC
/// lifecycle gate.
fn read_top_level_rs_files(dir: &str) -> String {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("bootstrap/ must be readable at {dir}: {e}"))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    assert!(
        entries.len() >= 5,
        "bootstrap/ yielded only {} top-level .rs file(s) at {dir} — the scan is reading \
         far less than expected, check the path",
        entries.len()
    );
    let content: String = entries
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        content.len() > 2000,
        "bootstrap/ content is only {} bytes at {dir} — suspiciously small, \
         the scan may be reading the wrong path",
        content.len()
    );
    content
}

/// Defense-in-depth only: this is a source-text check, not the primary
/// guarantee. The primary guarantee is structural — `RuntimeContext`'s path
/// accessors are `pub(crate)` to the `orbok` library crate, so this binary
/// crate cannot construct a `Catalog`, `CacheService`, or managed model store
/// from an arbitrary path at all; a bypass attempt fails to compile. This
/// test only catches an accidental future bypass of `RuntimeStorage` itself
/// and is deliberately not extended with more API names as a substitute for
/// that structural boundary (Correction Request 111 §4 C5).
#[test]
fn production_persistent_open_apis_remain_confined_to_the_runtime_boundary() {
    let runtime_storage = include_str!("runtime_storage.rs");
    let bootstrap_root = include_str!("bootstrap.rs");
    let bootstrap_dir =
        read_top_level_rs_files(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bootstrap"));
    let bootstrap = format!("{bootstrap_root}\n{bootstrap_dir}");
    let main = include_str!("main.rs");
    let model_flow = include_str!("model_flow.rs");
    let download = include_str!("download.rs");
    let settings = include_str!("settings.rs");
    let outside_boundary = [bootstrap.as_str(), main, model_flow, download].join("\n");

    assert!(!outside_boundary.contains("Catalog::open"));
    assert!(!outside_boundary.contains("CacheService::new"));
    assert_eq!(runtime_storage.matches("Catalog::open").count(), 1);
    assert_eq!(runtime_storage.matches("CacheService::new").count(), 1);
    // RFC-055 §3/§4.2: the fail-closed for_app() constructor, not new().
    assert_eq!(
        settings
            .matches(r#"ConfigManager::<OrbokSettings>::for_app("orbok")"#)
            .count(),
        1
    );
    assert!(!settings.contains("ConfigManager::<OrbokSettings>::new()"));
    assert!(!settings.contains("at_custom_dir"));
}

/// A `RuntimeContext` plus the test's own independently-computed expectation
/// of where it resolves each profile-resource path. Tests build these
/// expectations from the same temporary roots they seed, rather than reading
/// them back off the (now-sealed) context — see Correction Request 111 §4 C1
/// ("tests already build expected paths from the temporary roots they seed;
/// keep it that way").
struct Profile {
    context: RuntimeContext,
    data_dir: PathBuf,
    settings_dir: PathBuf,
}

impl Profile {
    fn settings_file(&self) -> PathBuf {
        self.settings_dir.join("settings.json")
    }

    fn path(&self, kind: RuntimePathKind) -> PathBuf {
        match kind {
            RuntimePathKind::Catalog => self.data_dir.join(orbok_db::CATALOG_FILE_NAME),
            RuntimePathKind::Cache => self.data_dir.join(orbok_db::CACHE_FILE_NAME),
            RuntimePathKind::Models => self.data_dir.join("models"),
            RuntimePathKind::Settings => self.settings_file(),
            RuntimePathKind::Recovery => self.data_dir.clone(),
            RuntimePathKind::Diagnostics => self.data_dir.join("diagnostics"),
            RuntimePathKind::Temporary => self.data_dir.join("tmp"),
        }
    }
}

fn contexts(root: &Path) -> (Profile, Profile) {
    let startup = root.join("startup");
    let standard_data = root.join("standard-data");
    let standard_settings = root.join("standard-settings");
    std::fs::create_dir_all(&startup).unwrap();
    let platform = PlatformRuntimePaths {
        standard_data_dir: Some(&standard_data),
        standard_settings_dir: Some(&standard_settings),
    };
    let standard_context = RuntimeContext::resolve(
        RuntimeSelection::resolve(false, None).unwrap(),
        &startup,
        platform,
    )
    .unwrap();
    let portable_context = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, None).unwrap(),
        &startup,
        platform,
    )
    .unwrap();
    let standard = Profile {
        context: standard_context,
        data_dir: standard_data,
        settings_dir: standard_settings,
    };
    let portable_data = startup.join("orbok-data");
    let portable = Profile {
        context: portable_context,
        data_dir: portable_data.clone(),
        settings_dir: portable_data,
    };
    (standard, portable)
}

fn seed_profile(profile: &Profile, marker: &str, locale: &str) {
    let catalog = bootstrap::open_catalog(&profile.context).unwrap();
    let source = SourceRepository::new(&catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: Some(format!("{marker} source")),
            original_path: format!("/{marker}/source"),
            canonical_path: format!("/{marker}/source"),
            index_mode: IndexMode::Balanced,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap();
    SearchHistoryRepository::new(&catalog)
        .upsert(
            &format!("{marker} history"),
            &[],
            Some(1),
            "en",
            &SearchHistorySettings::default(),
        )
        .unwrap();
    let jobs = IndexJobRepository::new(&catalog);
    let job = jobs
        .enqueue(JobType::Extract, Some(&source.source_id), None)
        .unwrap();
    jobs.set_status(&job, JobStatus::Running).unwrap();
    drop(catalog);

    let persisted = OrbokSettings {
        locale: locale.to_string(),
        ..OrbokSettings::default()
    };
    settings::save_settings(&profile.settings_file(), &persisted).unwrap();
    std::fs::create_dir_all(&profile.data_dir).unwrap();
    std::fs::write(profile.data_dir.join("profile-sentinel"), marker).unwrap();
}

#[derive(Debug, Eq, PartialEq)]
struct LogicalSnapshot {
    sources: Vec<String>,
    history: Vec<String>,
    queued_jobs: usize,
    running_jobs: u64,
    locale: String,
    theme: String,
    model_dir: Option<String>,
}

fn logical_snapshot(profile: &Profile) -> LogicalSnapshot {
    let catalog = bootstrap::open_catalog(&profile.context).unwrap();
    let mut sources: Vec<_> = SourceRepository::new(&catalog)
        .list_active()
        .unwrap()
        .into_iter()
        .filter_map(|source| source.display_name)
        .collect();
    sources.sort();
    let mut history: Vec<_> = SearchHistoryRepository::new(&catalog)
        .list()
        .unwrap()
        .into_iter()
        .map(|entry| entry.search_text)
        .collect();
    history.sort();
    let jobs = IndexJobRepository::new(&catalog);
    let queued_jobs = jobs.list_queued(100).unwrap().len();
    let running_jobs = jobs
        .count_by_status()
        .unwrap()
        .into_iter()
        .find_map(|(status, count)| (status == JobStatus::Running).then_some(count))
        .unwrap_or(0);
    let settings = settings::load_settings(&profile.settings_file());
    LogicalSnapshot {
        sources,
        history,
        queued_jobs,
        running_jobs,
        locale: settings.locale,
        theme: settings.theme,
        model_dir: settings.embedding_model_dir,
    }
}

/// Denies filesystem access to an inactive profile's roots and, critically,
/// self-checks that the denial actually took effect before the caller relies
/// on it (Correction Request 111 §4 C3). A denial harness that cannot arm
/// itself must fail loudly, not silently pass — a privileged or unusual test
/// runner (e.g. root in a container, or an elevated Windows account) can
/// otherwise make the denial a no-op.
///
/// Prove the denial is actually in effect by attempting a known read against
/// a sentinel this profile is guaranteed to have (written by `seed_profile`).
/// If the read still succeeds, the runner defeats the denial and the harness
/// must fail loudly rather than let the caller trust an unarmed boundary.
/// Shared by both platform implementations below.
fn assert_denial_armed(profile: &Profile) {
    let sentinel = profile.data_dir.join("profile-sentinel");
    let result = std::fs::read(&sentinel);
    assert!(
        result.is_err(),
        "denial harness did not arm: {} is still readable under this test runner; \
         byte/logical snapshot comparisons alone cannot detect a read-only \
         inactive-profile access, so this test must not proceed on an unarmed \
         denial boundary",
        sentinel.display()
    );
}

#[cfg(unix)]
struct DeniedProfile {
    paths: Vec<(PathBuf, std::fs::Permissions)>,
}

#[cfg(unix)]
impl DeniedProfile {
    fn new(profile: &Profile) -> Self {
        use std::os::unix::fs::PermissionsExt as _;
        let mut roots = vec![profile.data_dir.clone()];
        let settings_root = profile.settings_dir.clone();
        if settings_root != profile.data_dir {
            roots.push(settings_root);
        }
        let paths: Vec<_> = roots
            .into_iter()
            .map(|path| {
                let permissions = std::fs::metadata(&path).unwrap().permissions();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
                (path, permissions)
            })
            .collect();
        let denied = Self { paths };
        assert_denial_armed(profile);
        denied
    }
}

#[cfg(unix)]
impl Drop for DeniedProfile {
    fn drop(&mut self) {
        for (path, permissions) in &self.paths {
            std::fs::set_permissions(path, permissions.clone()).unwrap();
        }
    }
}

/// Windows equivalent using an explicit deny ACE (Correction Request 111 §4
/// C2). This is the C2 feasibility spike itself, per the architect's
/// clarification response §1.1: rather than reasoning in advance about
/// whether a non-elevated `windows-latest` CI account can arm a deny ACE,
/// `assert_denial_armed` measures it directly on the runner, at a known
/// commit, in a reproducible log. If it cannot arm here, this test fails
/// loudly rather than silently passing.
///
/// The denied rights deliberately exclude `WD` (write DAC) so the same
/// (denied) account retains the right to restore its own ACL on teardown —
/// denying `F` (full control, which includes write-DAC) would lock this
/// harness out of its own cleanup.
#[cfg(windows)]
struct DeniedProfile {
    paths: Vec<PathBuf>,
    account: String,
}

#[cfg(windows)]
impl DeniedProfile {
    fn new(profile: &Profile) -> Self {
        let account = std::env::var("USERNAME").expect("USERNAME must be set on Windows");
        let mut roots = vec![profile.data_dir.clone()];
        let settings_root = profile.settings_dir.clone();
        if settings_root != profile.data_dir {
            roots.push(settings_root);
        }
        for path in &roots {
            let status = std::process::Command::new("icacls")
                .arg(path)
                .arg("/deny")
                .arg(format!("{account}:(OI)(CI)(RD,REA,RA,RC,X,W,WA,WEA,AD,DC)"))
                .status()
                .expect("failed to invoke icacls /deny");
            assert!(
                status.success(),
                "icacls /deny failed for {}",
                path.display()
            );
        }
        let denied = Self {
            paths: roots,
            account,
        };
        assert_denial_armed(profile);
        denied
    }
}

#[cfg(windows)]
impl Drop for DeniedProfile {
    fn drop(&mut self) {
        for path in &self.paths {
            let status = std::process::Command::new("icacls")
                .arg(path)
                .arg("/remove:d")
                .arg(&self.account)
                .status()
                .expect("failed to invoke icacls /remove:d");
            assert!(
                status.success(),
                "icacls /remove:d failed for {} during teardown",
                path.display()
            );
        }
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, output: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !path.exists() {
            return;
        }
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, output);
            } else {
                output.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

fn exercise_later_profile_operations_with<P: orbok::runtime_context::RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    source_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    bootstrap::persist_theme_with(context, probe, orbok_ui::theme::Theme::Dark)?;
    bootstrap::persist_model_dir_with(context, probe, &source_path.to_string_lossy())?;

    let storage = orbok::runtime_storage::RuntimeStorage::new(context, probe);
    storage.ensure_support_dir(RuntimePathKind::Diagnostics)?;
    storage.ensure_support_dir(RuntimePathKind::Temporary)?;
    let catalog = storage.open_catalog()?;
    let cache = storage.cache()?;
    let (source, _) = bootstrap::add_source(&catalog, &source_path.to_string_lossy())?;
    let _ = bootstrap::run_search_with(context, probe, &catalog, "isolation", 20)?;
    SearchHistoryRepository::new(&catalog).upsert(
        "later isolation search",
        &[],
        Some(0),
        "en",
        &SearchHistorySettings::default(),
    )?;
    bootstrap::clean_snippets(&catalog, &cache)?;
    bootstrap::clean_search_cache(&catalog, &cache)?;
    bootstrap::remove_source(&catalog, &source.source_id)?;
    bootstrap::reset_catalog(&catalog, &cache)?;
    Ok(())
}

fn exercise_and_assert_isolation(
    active: &Profile,
    inactive: &Profile,
    marker: &str,
    expected_locale: orbok_ui::i18n::Locale,
) {
    let inactive_data = snapshot(&inactive.data_dir);
    let inactive_settings = snapshot(&inactive.settings_dir);
    let inactive_logical = logical_snapshot(inactive);
    let probe = RecordingProbe::default();

    let denied = DeniedProfile::new(inactive);

    let state = bootstrap::load_initial_state_with(&active.context, &probe).unwrap();
    assert_eq!(state.locale, expected_locale);
    assert!(
        state
            .sources
            .iter()
            .any(|source| source.display_name == format!("{marker} source"))
    );
    assert!(
        state
            .search_ui
            .history
            .iter()
            .any(|entry| entry.search_text == format!("{marker} history"))
    );
    let active_catalog =
        orbok::runtime_storage::open_catalog_with(&active.context, &probe).unwrap();
    let active_jobs = IndexJobRepository::new(&active_catalog);
    assert_eq!(active_jobs.list_queued(100).unwrap().len(), 1);
    assert_eq!(
        active_jobs
            .count_by_status()
            .unwrap()
            .into_iter()
            .find_map(|(status, count)| (status == JobStatus::Running).then_some(count))
            .unwrap_or(0),
        0
    );
    drop(active_catalog);
    bootstrap::run_check_with(&active.context, &probe).unwrap();
    let source_path = active
        .context
        .startup_dir()
        .join(format!("later-source-{marker}"));
    std::fs::create_dir_all(&source_path).unwrap();
    exercise_later_profile_operations_with(&active.context, &probe, &source_path).unwrap();

    // §5.1/§5.2: exercise the actual lazy cache open and managed model
    // delivery, not merely construct the sealed handles. Both must stay
    // confined to the active profile under the same armed denial boundary.
    let cache_catalog = orbok::runtime_storage::open_catalog_with(&active.context, &probe).unwrap();
    exercise_lazy_cache_open(&active.context, &probe, &cache_catalog);
    drop(cache_catalog);
    exercise_managed_model_delivery(&active.context, &probe);

    drop(denied);

    assert_eq!(snapshot(&inactive.data_dir), inactive_data);
    assert_eq!(snapshot(&inactive.settings_dir), inactive_settings);
    assert_eq!(logical_snapshot(inactive), inactive_logical);
    let calls = probe.0.lock().unwrap();
    assert!(!calls.is_empty());
    for (kind, path) in calls.iter() {
        assert_eq!(path, &active.path(*kind));
        assert_ne!(path, &inactive.path(*kind));
    }
    for required in [
        RuntimePathKind::Catalog,
        RuntimePathKind::Cache,
        RuntimePathKind::Models,
        RuntimePathKind::Settings,
        RuntimePathKind::Recovery,
        RuntimePathKind::Diagnostics,
        RuntimePathKind::Temporary,
    ] {
        assert!(calls.iter().any(|(kind, _)| *kind == required));
    }
}

/// Actually open the localcache database (`CacheEngine::builder().build()`
/// happens inside `ProfileCache::engine`) rather than merely constructing the
/// sealed `ProfileCache` handle — Review 111 §5 required this, since a
/// constructed-but-unused handle does not exercise the real I/O path.
fn exercise_lazy_cache_open(
    context: &RuntimeContext,
    probe: &RecordingProbe,
    catalog: &orbok_db::Catalog,
) {
    let cache = orbok::runtime_storage::cache_with(context, probe).unwrap();
    let engine = cache
        .engine::<serde_json::Value>(
            catalog,
            &orbok_cache::OrbokCacheNamespace::PreviewCache,
            orbok_cache::EngineOptions::default(),
        )
        .unwrap();
    // Force the lazy engine to actually touch its backing database.
    let _ = engine.cache_stats();
}

/// Exercise `ProfileModelStore::install_default_model` for real: trust
/// validation, the managed-store preflight, and exclusive-lock acquisition
/// all perform genuine filesystem I/O confined to the sealed store's root
/// before any network step. This sandbox has no reachable model-delivery
/// endpoint, so the eventual network/timeout outcome is not asserted — only
/// that the call does not hang indefinitely and, combined with the denial
/// boundary around it, never touches the inactive profile.
/// Exercises the pre-network phase of managed model delivery for real: trust
/// manifest validation and exclusive-lock acquisition against the sealed
/// store, both genuine work confined to the active profile's store root.
/// Deliberately does **not** call `ProfileModelStore::install_default_model`
/// itself, which would proceed to a real network request on every CI run —
/// for a privacy-first, local-first project that is a poor default (Review
/// 113 §5 non-blocking finding 1). The isolation-relevant property (this
/// touches only the active profile) is fully covered without it.
fn exercise_managed_model_delivery(context: &RuntimeContext, probe: &RecordingProbe) {
    let store = orbok::runtime_storage::model_store_with(context, probe).unwrap();
    orbok_models::trust::DEFAULT_TRUSTED_MODEL
        .validate()
        .expect("the pinned trust manifest must validate");
    let guard = store
        .acquire_exclusive(std::time::Duration::from_secs(5))
        .expect("exclusive lock against the sealed store must succeed");
    drop(guard);
}

#[test]
fn standard_and_portable_startup_check_recovery_and_later_access_stay_isolated() {
    let standard_active = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(standard_active.path());
    seed_profile(&standard, "standard", "en");
    seed_profile(&portable, "portable", "ja");
    exercise_and_assert_isolation(&standard, &portable, "standard", orbok_ui::i18n::Locale::En);

    let portable_active = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(portable_active.path());
    seed_profile(&standard, "standard", "en");
    seed_profile(&portable, "portable", "ja");
    exercise_and_assert_isolation(&portable, &standard, "portable", orbok_ui::i18n::Locale::Ja);
}

#[test]
fn invalid_portable_root_fails_without_standard_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(temp.path());
    seed_profile(&standard, "standard", "en");
    let before_data = snapshot(&standard.data_dir);
    let before_settings = snapshot(&standard.settings_dir);
    std::fs::create_dir_all(portable.data_dir.parent().unwrap()).unwrap();
    std::fs::write(&portable.data_dir, "not a directory").unwrap();

    assert!(bootstrap::load_initial_state(&portable.context).is_err());
    assert_eq!(snapshot(&standard.data_dir), before_data);
    assert_eq!(snapshot(&standard.settings_dir), before_settings);
}

#[test]
fn frozen_startup_anchor_survives_a_later_current_directory_change() {
    const CHILD: &str = "ORBOK_RFC049_FROZEN_ANCHOR_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let original = std::env::current_dir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let portable = RuntimeContext::resolve(
            RuntimeSelection::resolve(true, None).unwrap(),
            &original,
            PlatformRuntimePaths {
                standard_data_dir: Some(&other.path().join("standard")),
                standard_settings_dir: Some(&other.path().join("settings")),
            },
        )
        .unwrap();
        std::env::set_current_dir(other.path()).unwrap();
        bootstrap::run_check(&portable).unwrap();
        assert!(
            original
                .join("orbok-data")
                .join(orbok_db::CATALOG_FILE_NAME)
                .exists()
        );
        assert!(!other.path().join("orbok-data").exists());
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("runtime_isolation_tests::frozen_startup_anchor_survives_a_later_current_directory_change")
        .arg("--nocapture")
        .current_dir(temp.path())
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// RFC-054 §9.1, exercised through the real production call chain rather
// than RuntimeContext::resolve() alone. runtime_context::tests already
// proves the access seam never authorizes a path under the platform
// config directory; this proves the same property survives all the way
// through `bootstrap::run_check` -- storage.load_settings(), which
// creates a default settings.json on first run (RFC-049 C4) -- against a
// real filesystem, not recorded probe calls. The two are complementary:
// a passing access-seam test does not by itself prove nothing else in
// the call chain independently re-derives the platform settings path.
#[test]
fn standard_override_relocates_settings_through_a_real_run_check() {
    let root = tempfile::tempdir().unwrap();
    let startup = root.path().join("startup");
    let override_dir = root.path().join("override-profile");
    let platform_data = root.path().join("platform-data");
    let platform_settings = root.path().join("platform-settings");
    std::fs::create_dir_all(&startup).unwrap();

    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(override_dir.clone().into_os_string())).unwrap(),
        &startup,
        PlatformRuntimePaths {
            standard_data_dir: Some(&platform_data),
            standard_settings_dir: Some(&platform_settings),
        },
    )
    .unwrap();

    bootstrap::run_check(&context).unwrap();

    assert!(
        override_dir.join("settings.json").exists(),
        "settings.json must be created under the override directory"
    );
    assert!(
        !platform_settings.exists(),
        "the platform config directory must not be created at all -- RFC-054 §4.5, \"not merely unused for the resolved path, untouched\""
    );
}

#[cfg(unix)]
#[test]
fn physical_symlink_alias_is_rejected_before_persistent_access() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let startup = temp.path().join("startup");
    let standard = temp.path().join("standard");
    std::fs::create_dir_all(&standard).unwrap();
    std::fs::create_dir_all(&startup).unwrap();
    symlink(&standard, startup.join("orbok-data")).unwrap();
    let settings = temp.path().join("settings");
    let result = orbok::physical_identity::validate_physical_profile_separation(
        &RuntimeContext::resolve(
            RuntimeSelection::resolve(true, None).unwrap(),
            &startup,
            PlatformRuntimePaths {
                standard_data_dir: Some(&standard),
                standard_settings_dir: Some(&settings),
            },
        )
        .unwrap(),
        Some(&standard),
        Some(&settings),
    );
    assert!(result.is_err());
    assert!(!standard.join(orbok_db::CATALOG_FILE_NAME).exists());
}

#[cfg(unix)]
#[test]
fn physical_catalog_object_identity_alias_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let startup = temp.path().join("startup");
    let portable = startup.join("orbok-data");
    let standard = temp.path().join("standard");
    let settings = temp.path().join("settings");
    std::fs::create_dir_all(&portable).unwrap();
    std::fs::create_dir_all(&standard).unwrap();
    std::fs::create_dir_all(&settings).unwrap();
    let portable_catalog = portable.join(orbok_db::CATALOG_FILE_NAME);
    std::fs::write(&portable_catalog, "identity sentinel").unwrap();
    std::fs::hard_link(
        &portable_catalog,
        standard.join(orbok_db::CATALOG_FILE_NAME),
    )
    .unwrap();
    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, None).unwrap(),
        &startup,
        PlatformRuntimePaths {
            standard_data_dir: Some(&standard),
            standard_settings_dir: Some(&settings),
        },
    )
    .unwrap();

    assert!(
        orbok::physical_identity::validate_physical_profile_separation(
            &context,
            Some(&standard),
            Some(&settings)
        )
        .is_err()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn physical_bind_mount_identity_alias_is_rejected() {
    const CHILD: &str = "ORBOK_RFC049_BIND_ALIAS_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let startup = PathBuf::from(std::env::var_os("ORBOK_RFC049_STARTUP").unwrap());
        let standard = PathBuf::from(std::env::var_os("ORBOK_RFC049_STANDARD").unwrap());
        let settings = PathBuf::from(std::env::var_os("ORBOK_RFC049_SETTINGS").unwrap());
        let context = RuntimeContext::resolve(
            RuntimeSelection::resolve(true, None).unwrap(),
            &startup,
            PlatformRuntimePaths {
                standard_data_dir: Some(&standard),
                standard_settings_dir: Some(&settings),
            },
        )
        .unwrap();
        assert!(
            orbok::physical_identity::validate_physical_profile_separation(
                &context,
                Some(&standard),
                Some(&settings)
            )
            .is_err()
        );
        return;
    }

    // Correction Request 111 §4 F4 (Review 113); corrected per Review 114
    // §4 — `eprintln!`/`println!` are BOTH captured by libtest for a passing
    // test (verified empirically in Review 114; my original claim that
    // `eprintln!` bypasses capture was wrong and made this skip silent). A
    // direct, unbuffered write to the real stderr file descriptor
    // (`emit_capability_skip_notice`) bypasses libtest's capture instead,
    // so the notice is visible in plain `cargo test` output — exercised by
    // `bind_mount_probe_skip_is_visible_without_nocapture` below.
    if !bwrap_can_create_unprivileged_user_namespace() {
        emit_capability_skip_notice(
            "SKIPPED (not silently — see Review 114 §4): \
             physical_bind_mount_identity_alias_is_rejected requires unprivileged Linux \
             user-namespace creation (`bwrap --unshare-user`), which this runner does not \
             permit. RFC-049 §6 bind-mount alias-detection evidence is NOT collected by this \
             run. This is a runner capability gap, not a test failure: rerun on a runner or \
             local sandbox where unprivileged user namespaces are permitted to collect that \
             evidence.",
        );
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let startup = temp.path().join("startup");
    let portable = startup.join("orbok-data");
    let standard = temp.path().join("standard");
    let settings = temp.path().join("settings");
    for path in [&portable, &standard, &settings] {
        std::fs::create_dir_all(path).unwrap();
    }
    let output = std::process::Command::new("bwrap")
        .args(["--ro-bind", "/", "/", "--bind"])
        .arg(&standard)
        .arg(&portable)
        .arg("--setenv")
        .arg(CHILD)
        .arg("1")
        .arg("--setenv")
        .arg("ORBOK_RFC049_STARTUP")
        .arg(&startup)
        .arg("--setenv")
        .arg("ORBOK_RFC049_STANDARD")
        .arg(&standard)
        .arg("--setenv")
        .arg("ORBOK_RFC049_SETTINGS")
        .arg(&settings)
        .arg(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("runtime_isolation_tests::physical_bind_mount_identity_alias_is_rejected")
        .arg("--nocapture")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bind-mount child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Whether this process can create an unprivileged Linux user namespace via
/// `bwrap`. A minimal probe distinct from the real test fixture: it must not
/// depend on anything the real bind-mount setup builds, so a probe failure
/// cleanly means "capability unavailable," not "fixture setup went wrong."
#[cfg(target_os = "linux")]
fn bwrap_can_create_unprivileged_user_namespace() -> bool {
    std::process::Command::new("bwrap")
        .args(["--unshare-user", "--ro-bind", "/", "/", "true"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Write a capability-skip notice directly to the real stderr file
/// descriptor. `println!`/`eprintln!` are both captured by libtest for a
/// passing test and would make this silent (Review 114 §4) — a direct
/// `Write` on `std::io::stderr()` bypasses that capture.
#[cfg(target_os = "linux")]
fn emit_capability_skip_notice(message: &str) {
    use std::io::Write as _;
    let mut stderr = std::io::stderr();
    let _ = stderr.write_all(message.as_bytes());
    let _ = stderr.write_all(b"\n");
    let _ = stderr.flush();
}

/// Deliberately exercises the loud-skip branch above (Review 114 §4
/// required test): runs the real bind-mount test as a child process with a
/// `bwrap` on `PATH` that always fails, *without* `--nocapture`, and asserts
/// the skip notice is present in the child's captured output. This proves
/// visibility under the exact condition `cargo test` (and CI) runs under —
/// not just that the code path is reachable.
#[cfg(target_os = "linux")]
#[test]
fn bind_mount_probe_skip_is_visible_without_nocapture() {
    use std::os::unix::fs::PermissionsExt as _;

    let fake_bin = tempfile::tempdir().unwrap();
    let fake_bwrap = fake_bin.path().join("bwrap");
    std::fs::write(&fake_bwrap, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&fake_bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();

    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let mut forced_path = std::ffi::OsString::from(fake_bin.path());
    forced_path.push(":");
    forced_path.push(&existing_path);

    // No `--nocapture`: this must reproduce what a plain `cargo test` run
    // (and CI) actually sees.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("runtime_isolation_tests::physical_bind_mount_identity_alias_is_rejected")
        .env("PATH", forced_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "the skip path must still report the test as passed, not failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("SKIPPED (not silently"),
        "the loud-skip notice must be visible in captured child output without \
         --nocapture; got:\n{combined}"
    );
}

#[cfg(windows)]
#[test]
fn physical_junction_alias_is_rejected_before_persistent_access() {
    let temp = tempfile::tempdir().unwrap();
    let startup = temp.path().join("startup");
    let standard = temp.path().join("standard");
    std::fs::create_dir_all(&standard).unwrap();
    std::fs::create_dir_all(&startup).unwrap();
    let junction = startup.join("orbok-data");
    let output = std::process::Command::new("cmd")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&standard)
        .output()
        .unwrap();
    assert!(output.status.success(), "mklink /J failed");
    let settings = temp.path().join("settings");
    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, None).unwrap(),
        &startup,
        PlatformRuntimePaths {
            standard_data_dir: Some(&standard),
            standard_settings_dir: Some(&settings),
        },
    )
    .unwrap();

    assert!(
        orbok::physical_identity::validate_physical_profile_separation(
            &context,
            Some(&standard),
            Some(&settings)
        )
        .is_err()
    );
    assert!(!standard.join(orbok_db::CATALOG_FILE_NAME).exists());
}
