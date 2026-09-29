//! Task 126: the folder list keeps the order the folders were added in,
//! oldest first, through every path that shows or changes it -- a reload
//! (`get_sources`), an in-place refresh (Task 108), and a combine (Task 113).
//! Before this, a reload sorted newest first while an append (`SourceAdded`)
//! left the list oldest first, so pressing anything that reloads (Prepare
//! again, Check again) could reorder the cards under the user's pointer.

use crate::bootstrap;
use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
use orbok_db::Catalog;
use orbok_db::repo::{NewSource, SourceRepository};

fn add(catalog: &Catalog, dir: &std::path::Path, name: &str) -> orbok_core::SourceId {
    let path = dir.join(name);
    std::fs::create_dir(&path).unwrap();
    SourceRepository::new(catalog)
        .insert(NewSource {
            source_type: SourceType::Directory,
            persistence_mode: PersistenceMode::Persistent,
            display_name: None,
            original_path: path.to_string_lossy().into_owned(),
            canonical_path: path.to_string_lossy().into_owned(),
            index_mode: IndexMode::Balanced,
            include_patterns: vec![],
            exclude_patterns: vec![],
            hidden_file_policy: HiddenFilePolicy::Exclude,
            symlink_policy: SymlinkPolicy::Ignore,
            max_file_size_bytes: None,
        })
        .unwrap()
        .source_id
}

fn names(catalog: &Catalog) -> Vec<String> {
    bootstrap::get_sources(catalog)
        .unwrap()
        .into_iter()
        .map(|c| c.display_name)
        .collect()
}

/// §2 test 1: add three folders, reload, Prepare again on the middle one, and
/// combine two -- the order is the add order after each step.
#[test]
fn the_order_is_the_add_order_after_a_reload_a_prepare_again_and_a_combine() {
    let dir = tempfile::tempdir().unwrap();
    // Canonicalized once, up front (Task 113's own tests do the same): on
    // macOS `/tmp` is itself a symlink, and on Windows canonicalizing adds
    // the `\\?\` verbatim prefix, so an un-canonicalized root would make the
    // combine below compare the parent's canonicalized path against "one"/
    // "two"'s un-canonicalized ones and never match (observed on both CI
    // legs: the combine step silently absorbed nothing).
    let root = dir.path().canonicalize().unwrap();
    // "group/one" and "group/two" registered on their own first, so a later
    // add of "group" itself combines them (Task 113): the parent must exist
    // on disk, above already-registered folders, at add time.
    let group = root.join("group");
    std::fs::create_dir(&group).unwrap();
    let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();

    let _one = add(&catalog, &group, "one");
    let two = add(&catalog, &group, "two");
    let _three = add(&catalog, &root, "three");
    assert_eq!(names(&catalog), ["one", "two", "three"], "insertion order");

    // A reload -- the exact call the Folders page and startup make.
    assert_eq!(
        names(&catalog),
        ["one", "two", "three"],
        "a reload keeps the add order"
    );

    // Prepare again on the middle folder: a reachable path check plus a scan,
    // through the same function the card's own button calls.
    bootstrap::prepare_source_again(&catalog, two.as_str()).unwrap();
    assert_eq!(
        names(&catalog),
        ["one", "two", "three"],
        "Prepare again does not reorder the list"
    );

    // Combine: add "group" itself, above "one" and "two". What remains
    // ("three", "group") keeps the order they were last distinct in.
    let bootstrap::AddSourceOutcome::Added { .. } =
        bootstrap::add_source(&catalog, None, &group.to_string_lossy()).unwrap()
    else {
        panic!("expected a new folder");
    };
    assert_eq!(
        names(&catalog),
        ["three", "group"],
        "the combining parent, added last, comes after what it absorbed"
    );
}

/// The order comes from one query, `SourceRepository::list`, and it orders by
/// insertion (`rowid`), not by the stored `created_at` text (Task 116's rule).
/// `get_sources` calls it directly and duplicates none of its SQL.
#[test]
fn one_query_builds_the_list_ordered_by_insertion() {
    let source = include_str!("../../../../data/db/src/repo/sources.rs");
    let from = source
        .find("pub fn list(&self)")
        .expect("SourceRepository::list must exist");
    let body_end = from
        + source[from..]
            .find(
                "
    }
",
            )
            .unwrap();
    let body = &source[from..body_end];
    assert!(
        body.contains("ORDER BY rowid"),
        "SourceRepository::list must order by rowid (insertion order), got:
{body}"
    );
    assert!(
        !body.contains("created_at"),
        "SourceRepository::list must not order the window's folder list by created_at, got:
{body}"
    );
}
