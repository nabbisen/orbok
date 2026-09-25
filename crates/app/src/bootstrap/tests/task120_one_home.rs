//! Task 120 review §3: one home directory, resolved once and passed in. What
//! `~` means and where `AppData` / `Library` is are both judged against the home
//! the caller gives, never one read from the environment.

use crate::bootstrap;
use orbok_db::Catalog;

/// The folder the platform keeps application data in, directly under the home.
#[cfg(windows)]
const APPLICATION_DATA: Option<&str> = Some("AppData");
#[cfg(target_os = "macos")]
const APPLICATION_DATA: Option<&str> = Some("Library");
#[cfg(not(any(windows, target_os = "macos")))]
const APPLICATION_DATA: Option<&str> = None;

/// Adding the application-data folder directly asks first; the same name
/// anywhere else does not. Runs where the platform has such a folder (the
/// Windows and macOS legs); the rule itself is tested everywhere in `orbok-fs`.
#[test]
fn application_data_under_the_given_home_is_asked_about() {
    let Some(name) = APPLICATION_DATA else {
        return;
    };
    let base = tempfile::tempdir().unwrap();
    let home = base.path().canonicalize().unwrap().join("home");
    let inside = home.join(name);
    let elsewhere = home.join("Documents").join(name);
    for dir in [&inside, &elsewhere] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let catalog = Catalog::open_in_memory().unwrap();

    assert!(
        bootstrap::needs_private_folder_question(&catalog, Some(&home), &inside.to_string_lossy()),
        "{name} directly under the home is asked about"
    );
    assert!(
        !bootstrap::needs_private_folder_question(
            &catalog,
            Some(&home),
            &elsewhere.to_string_lossy()
        ),
        "a folder of that name somewhere else is not"
    );
    assert!(
        !bootstrap::needs_private_folder_question(&catalog, None, &inside.to_string_lossy()),
        "with no home known nothing is judged against one"
    );
}
