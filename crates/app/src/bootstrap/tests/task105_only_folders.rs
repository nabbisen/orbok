//! Task 105: `add_source` registers folders only, and expands `~`.

use crate::bootstrap::{self, sources::expand_home};
use orbok_db::Catalog;
use orbok_db::repo::SourceRepository;

/// A file path is refused for every caller, and no row is created.
#[test]
fn add_source_refuses_a_file_and_creates_no_row() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("one.md");
    std::fs::write(&file, "# one\n").unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();

    let result = bootstrap::add_source(&catalog, &file.to_string_lossy());

    assert!(result.is_err(), "a file is not a folder");
    assert_eq!(SourceRepository::new(&catalog).list().unwrap().len(), 0);
}

/// §2.2: a path under `~` expands to the home directory; other paths are left
/// alone (only a *leading* tilde counts).
#[test]
fn a_leading_tilde_is_the_home_directory() {
    assert_eq!(expand_home("~/Documents", "/home/me"), "/home/me/Documents");
    assert_eq!(expand_home("~", "/home/me"), "/home/me");
    assert_eq!(expand_home("/data/a~b", "/home/me"), "/data/a~b");
    assert_eq!(expand_home("docs/~x", "/home/me"), "docs/~x");
}
