use super::*;
use std::sync::Mutex;

fn absolute(parts: &[&str]) -> PathBuf {
    #[cfg(windows)]
    let mut path = PathBuf::from(r"C:\");
    #[cfg(not(windows))]
    let mut path = PathBuf::from("/");

    for part in parts {
        path.push(part);
    }
    path
}

fn platform_paths<'a>(data: &'a Path, settings: &'a Path) -> PlatformRuntimePaths<'a> {
    PlatformRuntimePaths {
        standard_data_dir: Some(data),
        standard_settings_dir: settings,
    }
}

#[test]
fn empty_override_is_unset_and_standard_paths_are_preserved() {
    let startup = absolute(&["launch"]);
    let standard_data = absolute(&["local-data", "orbok"]);
    let standard_settings = absolute(&["config", "orbok"]);

    for override_value in [None, Some(OsString::new())] {
        let selection = RuntimeSelection::resolve(false, override_value).unwrap();
        let context = RuntimeContext::resolve(
            selection,
            &startup,
            platform_paths(&standard_data, &standard_settings),
        )
        .unwrap();

        assert_eq!(context.mode(), RuntimeMode::Standard);
        assert_eq!(context.data_dir(), standard_data);
        assert_eq!(
            context.catalog_file(),
            standard_data.join(orbok_db::CATALOG_FILE_NAME)
        );
        assert_eq!(
            context.cache_file(),
            standard_data.join(orbok_db::CACHE_FILE_NAME)
        );
        assert_eq!(context.models_dir(), standard_data.join("models"));
        assert_eq!(
            context.settings_file(),
            standard_settings.join("settings.json")
        );
    }
}

#[test]
fn standard_override_is_anchored_once_and_normalized() {
    let startup = absolute(&["launch", "directory"]);
    let standard_data = absolute(&["local-data", "orbok"]);
    let standard_settings = absolute(&["config", "orbok"]);
    let selection = RuntimeSelection::resolve(false, Some("../override/./profile".into())).unwrap();

    let context = RuntimeContext::resolve(
        selection,
        &startup,
        platform_paths(&standard_data, &standard_settings),
    )
    .unwrap();

    assert_eq!(
        context.data_dir(),
        absolute(&["launch", "override", "profile"])
    );
    assert_eq!(context.startup_dir(), startup);
}

#[test]
fn missing_platform_data_root_preserves_the_anchored_standard_fallback() {
    let startup = absolute(&["launch", "directory"]);
    let standard_settings = absolute(&["config", "orbok"]);
    let selection = RuntimeSelection::resolve(false, None).unwrap();

    let context = RuntimeContext::resolve(
        selection,
        &startup,
        PlatformRuntimePaths {
            standard_data_dir: None,
            standard_settings_dir: &standard_settings,
        },
    )
    .unwrap();

    assert_eq!(context.mode(), RuntimeMode::Standard);
    assert_eq!(context.data_dir(), startup.join("orbok-data"));
}

#[test]
fn portable_mode_fails_closed_when_it_would_alias_the_standard_fallback() {
    let startup = absolute(&["launch", "directory"]);
    let standard_settings = absolute(&["config", "orbok"]);
    let selection = RuntimeSelection::resolve(true, None).unwrap();

    let error = RuntimeContext::resolve(
        selection,
        &startup,
        PlatformRuntimePaths {
            standard_data_dir: None,
            standard_settings_dir: &standard_settings,
        },
    )
    .unwrap_err();

    assert_eq!(error, RuntimeContextError::ProfilePathAlias);
}

#[test]
fn portable_mode_fails_closed_when_a_platform_root_aliases_its_profile() {
    let startup = absolute(&["launch", "directory"]);
    let standard_data = startup.join("orbok-data");
    let standard_settings = absolute(&["config", "orbok"]);
    let selection = RuntimeSelection::resolve(true, None).unwrap();

    let error = RuntimeContext::resolve(
        selection,
        &startup,
        platform_paths(&standard_data, &standard_settings),
    )
    .unwrap_err();

    assert_eq!(error, RuntimeContextError::ProfilePathAlias);
}

#[test]
fn portable_mode_fails_closed_when_standard_settings_overlap_its_profile() {
    let startup = absolute(&["launch", "directory"]);
    let standard_data = absolute(&["standard", "data"]);
    let standard_settings = startup.join("orbok-data").join("settings");
    let selection = RuntimeSelection::resolve(true, None).unwrap();

    let error = RuntimeContext::resolve(
        selection,
        &startup,
        platform_paths(&standard_data, &standard_settings),
    )
    .unwrap_err();

    assert_eq!(error, RuntimeContextError::ProfilePathAlias);
}

#[test]
fn case_insensitive_comparison_detects_exact_and_containment_aliases() {
    let exact_left = absolute(&["Launch", "orbok-data"]);
    let exact_right = absolute(&["launch", "ORBOK-DATA"]);
    let descendant = exact_right.join("settings");

    assert!(paths_overlap_with_case(&exact_left, &exact_right, true));
    assert!(paths_overlap_with_case(&exact_left, &descendant, true));
    assert!(paths_overlap_with_case(&descendant, &exact_left, true));
}

#[cfg(windows)]
#[test]
fn windows_target_detects_drive_and_component_case_aliases() {
    let portable = Path::new(r"C:\Launch\orbok-data");
    let standard_exact = Path::new(r"c:\launch\ORBOK-DATA");
    let standard_parent = Path::new(r"c:\LAUNCH");

    assert!(paths_overlap(portable, standard_exact));
    assert!(paths_overlap(portable, standard_parent));
}

#[cfg(target_os = "macos")]
#[test]
fn macos_target_conservatively_detects_component_case_aliases() {
    let portable = Path::new("/Users/Example/Launch/orbok-data");
    let standard = Path::new("/users/example/launch/ORBOK-DATA/settings");

    assert!(paths_overlap(portable, standard));
}

#[test]
fn portable_mode_uses_only_the_frozen_startup_anchor() {
    let startup = absolute(&["launch", "directory"]);
    let standard_data = absolute(&["inactive", "data"]);
    let standard_settings = absolute(&["inactive", "settings"]);

    for override_value in [None, Some(OsString::new())] {
        let selection = RuntimeSelection::resolve(true, override_value).unwrap();
        let context = RuntimeContext::resolve(
            selection,
            &startup,
            platform_paths(&standard_data, &standard_settings),
        )
        .unwrap();

        let portable = startup.join("orbok-data");
        assert_eq!(context.mode(), RuntimeMode::Portable);
        assert_eq!(context.data_dir(), portable);
        assert_eq!(context.settings_file(), portable.join("settings.json"));
        assert!(!context.catalog_file().starts_with(&standard_data));
        assert!(!context.settings_file().starts_with(&standard_settings));
    }
}

#[test]
fn portable_and_nonempty_override_conflict_before_context_resolution() {
    let error = RuntimeSelection::resolve(true, Some("standard-profile".into())).unwrap_err();
    assert!(matches!(
        error,
        RuntimeContextError::PortableOverrideConflict
    ));
}

#[test]
fn relative_startup_directory_fails_closed() {
    let standard_data = absolute(&["local-data", "orbok"]);
    let standard_settings = absolute(&["config", "orbok"]);
    let selection = RuntimeSelection::resolve(true, Option::<OsString>::None).unwrap();

    let error = RuntimeContext::resolve(
        selection,
        Path::new("relative-startup"),
        platform_paths(&standard_data, &standard_settings),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        RuntimeContextError::StartupDirectoryNotAbsolute
    ));
}

#[test]
fn constructed_context_owns_the_frozen_startup_anchor() {
    let original_startup = absolute(&["first"]);
    let mut caller_startup = original_startup.clone();
    let standard_data = absolute(&["local-data", "orbok"]);
    let standard_settings = absolute(&["config", "orbok"]);

    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, Option::<OsString>::None).unwrap(),
        &caller_startup,
        platform_paths(&standard_data, &standard_settings),
    )
    .unwrap();
    caller_startup = absolute(&["second"]);

    assert_eq!(context.data_dir(), original_startup.join("orbok-data"));
    assert_eq!(context.startup_dir(), original_startup);
    assert_ne!(context.startup_dir(), caller_startup);
}

#[derive(Default)]
struct RecordingProbe {
    accesses: Mutex<Vec<(RuntimePathKind, PathBuf)>>,
}

impl RuntimePathProbe for RecordingProbe {
    fn before_access(&self, kind: RuntimePathKind, path: &Path) -> io::Result<()> {
        self.accesses
            .lock()
            .unwrap()
            .push((kind, path.to_path_buf()));
        Ok(())
    }
}

#[test]
fn access_seam_reports_each_active_profile_path() {
    let startup = absolute(&["launch"]);
    let inactive_data = absolute(&["inactive", "data"]);
    let inactive_settings = absolute(&["inactive", "settings"]);
    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, Option::<OsString>::None).unwrap(),
        &startup,
        platform_paths(&inactive_data, &inactive_settings),
    )
    .unwrap();
    let probe = RecordingProbe::default();
    let access = RuntimeAccess::new(&context, &probe);
    let kinds = [
        RuntimePathKind::Catalog,
        RuntimePathKind::Cache,
        RuntimePathKind::Models,
        RuntimePathKind::Settings,
        RuntimePathKind::Recovery,
        RuntimePathKind::Diagnostics,
        RuntimePathKind::Temporary,
    ];

    for kind in kinds {
        assert_eq!(access.active_path(kind).unwrap(), context.path(kind));
    }

    let recorded = probe.accesses.lock().unwrap();
    assert_eq!(recorded.len(), kinds.len());
    assert!(
        recorded
            .iter()
            .all(|(_, path)| !path.starts_with(&inactive_data))
    );
    assert!(
        recorded
            .iter()
            .all(|(_, path)| !path.starts_with(&inactive_settings))
    );
}

#[test]
fn access_seam_rejects_inactive_profile_path_without_probing_it() {
    let startup = absolute(&["launch"]);
    let inactive_data = absolute(&["inactive", "data"]);
    let inactive_settings = absolute(&["inactive", "settings"]);
    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(true, Option::<OsString>::None).unwrap(),
        &startup,
        platform_paths(&inactive_data, &inactive_settings),
    )
    .unwrap();
    let probe = RecordingProbe::default();
    let access = RuntimeAccess::new(&context, &probe);

    let error = access
        .authorize_path(
            RuntimePathKind::Catalog,
            &inactive_data.join("orbok.sqlite3"),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        RuntimeAccessError::InactiveProfilePath(RuntimePathKind::Catalog)
    ));
    assert!(probe.accesses.lock().unwrap().is_empty());
}

#[test]
fn access_probe_can_fail_closed_without_exposing_a_path_in_the_error() {
    struct Deny;

    impl RuntimePathProbe for Deny {
        fn before_access(&self, _kind: RuntimePathKind, _path: &Path) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        }
    }

    let startup = absolute(&["launch"]);
    let standard_data = absolute(&["standard", "data"]);
    let standard_settings = absolute(&["standard", "settings"]);
    let context = RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Option::<OsString>::None).unwrap(),
        &startup,
        platform_paths(&standard_data, &standard_settings),
    )
    .unwrap();

    let error = RuntimeAccess::new(&context, &Deny)
        .authorize_path(RuntimePathKind::Settings, context.settings_file())
        .unwrap_err();

    assert!(matches!(error, RuntimeAccessError::Probe(_)));
    assert!(
        !error
            .to_string()
            .contains(standard_settings.to_string_lossy().as_ref())
    );
}
