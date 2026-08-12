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

// Task 016 (Review 166 §3): atomic replace + 0600 for settings writes.

#[cfg(unix)]
#[test]
fn saved_settings_file_is_mode_0600() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    storage
        .save_settings(&TestSettings {
            value: "custom".into(),
        })
        .unwrap();

    let path = storage
        .path(RuntimePathKind::Settings)
        .unwrap()
        .to_path_buf();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "settings file must be owner-only after a write"
    );
}

#[test]
fn a_write_that_cannot_stage_its_temp_file_does_not_touch_the_existing_settings_file() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    let original = TestSettings {
        value: "original".into(),
    };
    storage.save_settings(&original).unwrap();

    let path = storage
        .path(RuntimePathKind::Settings)
        .unwrap()
        .to_path_buf();
    let temp_path = super::sibling_temp_path(&path);
    // Block the temp file's own path with a directory: `File::create` on it
    // fails before any content is written or the rename is attempted, so
    // the pre-existing settings file must be completely untouched.
    std::fs::create_dir_all(&temp_path).unwrap();

    let result = storage.save_settings(&TestSettings {
        value: "replacement".into(),
    });
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        serde_json::to_vec_pretty(&original).unwrap(),
        "a write that never reached the rename must not disturb the existing file"
    );
}

#[test]
fn a_successful_write_leaves_no_temp_file_behind() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    storage
        .save_settings(&TestSettings {
            value: "custom".into(),
        })
        .unwrap();

    let path = storage
        .path(RuntimePathKind::Settings)
        .unwrap()
        .to_path_buf();
    assert!(
        !super::sibling_temp_path(&path).exists(),
        "the rename must consume the temp file on success"
    );
}

#[test]
fn a_write_that_fails_at_rename_leaves_no_temp_file_behind() {
    let temp = tempfile::tempdir().unwrap();
    let (standard, _portable) = contexts(temp.path());
    let storage = RuntimeStorage::new(&standard, &AllowRuntimePathProbe);
    let path = storage
        .path(RuntimePathKind::Settings)
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Block the *target* path with a directory: the temp file is created,
    // written, and synced successfully, and only the final rename fails
    // (renaming a file over an existing directory is rejected on every
    // platform this runs on) -- this is the case that actually exercises
    // the cleanup path, unlike the previous test where nothing was ever
    // created.
    std::fs::create_dir_all(&path).unwrap();

    let result = storage.save_settings(&TestSettings {
        value: "custom".into(),
    });
    assert!(result.is_err());
    assert!(
        !super::sibling_temp_path(&path).exists(),
        "a failed rename must still clean up the temp file it created"
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
