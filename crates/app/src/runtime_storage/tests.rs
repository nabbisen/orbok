//! C4: settings first-load creation semantics (Correction Request 111 §4 C4).

use super::*;
use crate::runtime_context::{PlatformRuntimePaths, RuntimeSelection};

#[derive(serde::Serialize, serde::Deserialize, Default, PartialEq, Debug)]
struct TestSettings {
    value: String,
}

fn contexts(root: &Path) -> (RuntimeContext, RuntimeContext) {
    let startup = root.join("startup");
    let standard_data = root.join("standard-data");
    let standard_settings = root.join("standard-settings");
    std::fs::create_dir_all(&startup).unwrap();
    let platform = PlatformRuntimePaths {
        standard_data_dir: Some(&standard_data),
        standard_settings_dir: Some(&standard_settings),
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

#[test]
fn missing_settings_file_is_created_with_the_default_value_at_the_active_path() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);

    assert!(!standard.path(RuntimePathKind::Settings).exists());
    let loaded: TestSettings = storage.load_settings().unwrap();
    assert_eq!(loaded, TestSettings::default());
    assert!(standard.path(RuntimePathKind::Settings).exists());

    // The persisted bytes round-trip to the same default value on a second
    // load, proving a real file was written (not merely returned in-memory).
    let reloaded: TestSettings = storage.load_settings().unwrap();
    assert_eq!(reloaded, TestSettings::default());
}

#[test]
fn first_load_creates_settings_only_at_the_active_path_in_both_directions() {
    let standard_active = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(standard_active.path());
    let _: TestSettings = RuntimeStorage::new(&standard, &AllowRuntimePathProbe)
        .load_settings()
        .unwrap();
    assert!(standard.path(RuntimePathKind::Settings).exists());
    assert!(!portable.path(RuntimePathKind::Settings).exists());

    let portable_active = tempfile::tempdir().unwrap();
    let (standard, portable) = contexts(portable_active.path());
    let _: TestSettings = RuntimeStorage::new(&portable, &AllowRuntimePathProbe)
        .load_settings()
        .unwrap();
    assert!(portable.path(RuntimePathKind::Settings).exists());
    assert!(!standard.path(RuntimePathKind::Settings).exists());
}

#[test]
fn corrupt_settings_file_falls_back_to_default_without_overwriting_it() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    let path = storage
        .path(RuntimePathKind::Settings)
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json").unwrap();

    let loaded: TestSettings = storage.load_settings().unwrap();
    assert_eq!(loaded, TestSettings::default());
    // Best-effort: the corrupt bytes are left as-is, matching the prior
    // behavior of falling back to a default without persisting it.
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
}

#[test]
fn non_not_found_read_failure_falls_back_to_default_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    let path = storage
        .path(RuntimePathKind::Settings)
        .unwrap()
        .to_path_buf();
    // A directory at the settings path makes `std::fs::read` fail with
    // `IsADirectory`, not `NotFound` -- the guard this exercises
    // (`load_settings`'s `error.kind() == io::ErrorKind::NotFound` match
    // arm) must not treat it as a first-load and must not attempt to write
    // over it (Correction Request 111 C4 / Review 165 §4: only a genuinely
    // missing file gets a default written; every other read failure
    // returns the default in memory only, so a transient or permission
    // error on an existing settings file can never overwrite it).
    std::fs::create_dir_all(&path).unwrap();

    let loaded: TestSettings = storage.load_settings().unwrap();
    assert_eq!(loaded, TestSettings::default());
    assert!(
        path.is_dir(),
        "a non-NotFound read failure must not attempt to write the settings path"
    );
}

#[test]
fn save_then_load_round_trips_a_non_default_value() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    let value = TestSettings {
        value: "custom".into(),
    };
    storage.save_settings(&value).unwrap();
    let loaded: TestSettings = storage.load_settings().unwrap();
    assert_eq!(loaded, value);
}
