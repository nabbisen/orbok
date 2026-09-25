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

    let result = bootstrap::add_source(&catalog, None, &file.to_string_lossy());

    assert!(result.is_err(), "a file is not a folder");
    assert_eq!(SourceRepository::new(&catalog).list().unwrap().len(), 0);
}

/// §2.2: a path under `~` expands to the home directory; other paths are left
/// alone (only a *leading* tilde counts).
#[test]
fn a_leading_tilde_is_the_home_directory() {
    let home = Some(std::path::Path::new("/home/me"));
    assert_eq!(expand_home("~/Documents", home), "/home/me/Documents");
    assert_eq!(expand_home("~", home), "/home/me");
    assert_eq!(expand_home("/data/a~b", home), "/data/a~b");
    assert_eq!(expand_home("docs/~x", home), "docs/~x");
}

/// Task 120 review §3: the home is the one the caller passes in, on every
/// platform -- including a Windows-shaped one, where `HOME` is usually unset. The
/// old code read `HOME` alone, so a typed `~\Documents` became `\Documents`.
#[test]
fn a_tilde_expands_to_the_injected_home_even_where_home_is_unset() {
    let windows = Some(std::path::Path::new(r"C:\Users\me"));
    assert_eq!(
        expand_home(r"~\Documents", windows),
        r"C:\Users\me\Documents"
    );
    assert_eq!(expand_home("~", windows), r"C:\Users\me");
    // No home known: the tilde is left as typed (and then fails like any path
    // that cannot be reached), never turned into a path from the root.
    assert_eq!(expand_home(r"~\Documents", None), r"~\Documents");
    assert_eq!(expand_home("~/Documents", None), "~/Documents");
}
