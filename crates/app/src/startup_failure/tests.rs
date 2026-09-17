//! Task 071 §4 test 1: each class from a real failure, through the real
//! startup function.

use super::StartupFailure;
use crate::bootstrap::load_initial_state;
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use std::path::Path;

fn context_for(data_dir: &Path, startup_dir: &Path) -> RuntimeContext {
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data_dir.as_os_str().to_os_string())).unwrap(),
        startup_dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(startup_dir),
            standard_settings_dir: Some(startup_dir),
        },
    )
    .unwrap()
}

/// `ORBOK_DATA_DIR` pointing at a regular file.
#[test]
fn a_data_folder_that_is_a_regular_file_is_a_data_folder_failure() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    let context = context_for(&file, temp.path());
    match load_initial_state(&context) {
        Err(StartupFailure::DataFolder { path, .. }) => {
            assert_eq!(
                path,
                context.descriptor().to_string(),
                "the window names the data folder"
            );
        }
        other => panic!(
            "expected DataFolder, got {:?}",
            other.map(|_| "a started app")
        ),
    }
}

/// A catalog stamped one schema version beyond this build, as RFC-062's
/// `schema_version_from_the_future_is_refused_naming_both_versions` does.
#[test]
fn a_catalog_from_a_newer_orbok_is_newer_data() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("profile");
    let context = context_for(&data_dir, temp.path());
    {
        let catalog = orbok::runtime_storage::open_catalog(&context).unwrap();
        catalog
            .lock()
            .execute(
                "INSERT INTO schema_migrations (version, name, applied_at) \
                 VALUES (?1, 'from_the_future', '2099-01-01T00:00:00Z')",
                rusqlite::params![orbok_db::migrations::latest_version() + 1],
            )
            .unwrap();
    }
    match load_initial_state(&context) {
        Err(StartupFailure::NewerData { .. }) => {}
        other => panic!(
            "expected NewerData, got {:?}",
            other.map(|_| "a started app")
        ),
    }
}

/// A catalog file that is not SQLite.
#[test]
fn a_catalog_file_that_is_not_sqlite_is_other() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("profile");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(
        data_dir.join(orbok_db::CATALOG_FILE_NAME),
        "this is not a database",
    )
    .unwrap();
    let context = context_for(&data_dir, temp.path());
    match load_initial_state(&context) {
        Err(StartupFailure::Other { .. }) => {}
        other => panic!("expected Other, got {:?}", other.map(|_| "a started app")),
    }
}
