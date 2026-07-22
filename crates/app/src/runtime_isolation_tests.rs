use super::bootstrap;
use super::settings::{self, OrbokSettings};
use orbok::runtime_context::{
    PlatformRuntimePaths, RuntimeContext, RuntimePathKind, RuntimePathProbe, RuntimeSelection,
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

impl RuntimePathProbe for RecordingProbe {
    fn before_access(&self, kind: RuntimePathKind, path: &Path) -> io::Result<()> {
        self.0.lock().unwrap().push((kind, path.to_path_buf()));
        Ok(())
    }
}

#[test]
fn production_persistent_open_apis_remain_confined_to_the_runtime_boundary() {
    let bootstrap = include_str!("bootstrap.rs");
    let main = include_str!("main.rs");
    let model_flow = include_str!("model_flow.rs");
    let download = include_str!("download.rs");
    let settings = include_str!("settings.rs");
    let outside_boundary = [main, model_flow, download].join("\n");

    assert!(!outside_boundary.contains("Catalog::open"));
    assert!(!outside_boundary.contains("CacheService::new"));
    assert_eq!(bootstrap.matches("Catalog::open").count(), 1);
    assert_eq!(bootstrap.matches("CacheService::new").count(), 1);
    assert_eq!(
        settings
            .matches("ConfigManager::<OrbokSettings>::new()")
            .count(),
        1
    );
    assert!(!settings.contains("at_custom_dir"));
}

fn contexts(root: &Path) -> (RuntimeContext, RuntimeContext) {
    let startup = root.join("startup");
    let standard_data = root.join("standard-data");
    let standard_settings = root.join("standard-settings");
    std::fs::create_dir_all(&startup).unwrap();
    let platform = PlatformRuntimePaths {
        standard_data_dir: Some(&standard_data),
        standard_settings_dir: &standard_settings,
    };
    let standard = RuntimeContext::resolve(
        RuntimeSelection::resolve(false, None).unwrap(),
        &startup,
        platform,
    )
    .unwrap();
    let portable = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, None).unwrap(),
        &startup,
        platform,
    )
    .unwrap();
    (standard, portable)
}

fn seed_profile(context: &RuntimeContext, marker: &str, locale: &str) {
    let catalog = bootstrap::open_catalog(context).unwrap();
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
    settings::save_settings(context.settings_file(), &persisted).unwrap();
    std::fs::write(context.data_dir().join("profile-sentinel"), marker).unwrap();
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

fn logical_snapshot(context: &RuntimeContext) -> LogicalSnapshot {
    let catalog = bootstrap::open_catalog(context).unwrap();
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
    let settings = settings::load_settings(context.settings_file());
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

#[cfg(unix)]
struct DeniedProfile {
    paths: Vec<(PathBuf, std::fs::Permissions)>,
}

#[cfg(unix)]
impl DeniedProfile {
    fn new(context: &RuntimeContext) -> Self {
        use std::os::unix::fs::PermissionsExt as _;
        let mut roots = vec![context.data_dir().to_path_buf()];
        let settings_root = context.settings_file().parent().unwrap().to_path_buf();
        if settings_root != context.data_dir() {
            roots.push(settings_root);
        }
        let paths = roots
            .into_iter()
            .map(|path| {
                let permissions = std::fs::metadata(&path).unwrap().permissions();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
                (path, permissions)
            })
            .collect();
        Self { paths }
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

fn exercise_and_assert_isolation(
    active: &RuntimeContext,
    inactive: &RuntimeContext,
    marker: &str,
    expected_locale: orbok_ui::i18n::Locale,
) {
    let inactive_data = snapshot(inactive.data_dir());
    let inactive_settings = snapshot(inactive.settings_file().parent().unwrap());
    let inactive_logical = logical_snapshot(inactive);
    let probe = RecordingProbe::default();

    #[cfg(unix)]
    let denied = DeniedProfile::new(inactive);

    let state = bootstrap::load_initial_state_with(active, &probe).unwrap();
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
    let active_catalog = bootstrap::open_catalog_with(active, &probe).unwrap();
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
    bootstrap::run_check_with(active, &probe).unwrap();
    let source_path = active.startup_dir().join(format!("later-source-{marker}"));
    std::fs::create_dir_all(&source_path).unwrap();
    bootstrap::exercise_later_profile_operations_with(active, &probe, &source_path).unwrap();

    #[cfg(unix)]
    drop(denied);

    assert_eq!(snapshot(inactive.data_dir()), inactive_data);
    assert_eq!(
        snapshot(inactive.settings_file().parent().unwrap()),
        inactive_settings
    );
    assert_eq!(logical_snapshot(inactive), inactive_logical);
    let calls = probe.0.lock().unwrap();
    assert!(!calls.is_empty());
    for (kind, path) in calls.iter() {
        assert_eq!(path, active.path(*kind));
        assert_ne!(path, inactive.path(*kind));
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

#[test]
fn standard_and_portable_startup_check_recovery_and_later_access_stay_isolated() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(temp.path());
    seed_profile(&standard, "standard", "en");
    seed_profile(&portable, "portable", "ja");

    exercise_and_assert_isolation(&standard, &portable, "standard", orbok_ui::i18n::Locale::En);
    exercise_and_assert_isolation(&portable, &standard, "portable", orbok_ui::i18n::Locale::Ja);
}

#[test]
fn invalid_portable_root_fails_without_standard_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(temp.path());
    seed_profile(&standard, "standard", "en");
    let before_data = snapshot(standard.data_dir());
    let before_settings = snapshot(standard.settings_file().parent().unwrap());
    std::fs::write(portable.data_dir(), "not a directory").unwrap();

    assert!(bootstrap::load_initial_state(&portable).is_err());
    assert_eq!(snapshot(standard.data_dir()), before_data);
    assert_eq!(
        snapshot(standard.settings_file().parent().unwrap()),
        before_settings
    );
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
                standard_settings_dir: &other.path().join("settings"),
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
    let result = bootstrap::validate_physical_profile_separation(
        &RuntimeContext::resolve(
            RuntimeSelection::resolve(true, None).unwrap(),
            &startup,
            PlatformRuntimePaths {
                standard_data_dir: Some(&standard),
                standard_settings_dir: &settings,
            },
        )
        .unwrap(),
        Some(&standard),
        &settings,
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
            standard_settings_dir: &settings,
        },
    )
    .unwrap();

    assert!(
        bootstrap::validate_physical_profile_separation(&context, Some(&standard), &settings)
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
                standard_settings_dir: &settings,
            },
        )
        .unwrap();
        assert!(
            bootstrap::validate_physical_profile_separation(&context, Some(&standard), &settings)
                .is_err()
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
            standard_settings_dir: &settings,
        },
    )
    .unwrap();

    assert!(
        bootstrap::validate_physical_profile_separation(&context, Some(&standard), &settings)
            .is_err()
    );
    assert!(!standard.join(orbok_db::CATALOG_FILE_NAME).exists());
}
