//! Task 120: what orbok skips is precise, on every platform. Hidden means what
//! the platform means; a folder a tool generated is recognised by evidence, not
//! by common words; a folder that is skipped has nothing prepared (rows an
//! earlier scan wrote there are erased, never marked missing).

use crate::tests::common::{register_dir_source, register_dir_source_with, scan};
use orbok_core::{ExtractionId, FileId, HiddenFilePolicy, SymlinkPolicy};
use orbok_db::Catalog;
use orbok_db::repo::{ChunkRepository, ChunkSpec};
use rusqlite::params;
use std::path::Path;

const SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

/// `files` of `rels` (relative, `/`-separated), each holding a line of text.
fn write_files(root: &Path, rels: &[&str]) {
    for rel in rels {
        let path = rel
            .split('/')
            .fold(root.to_path_buf(), |path, part| path.join(part));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("# {rel}\n\ntext\n")).unwrap();
    }
}

fn prepared(catalog: &Catalog) -> Vec<String> {
    let conn = catalog.lock();
    let mut stmt = conn
        .prepare("SELECT display_path FROM files ORDER BY display_path")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|p| p.unwrap().replace('\\', "/"))
        .collect()
}

/// Scan a fresh copy of `rels` under `root` and say which files got a row.
fn prepared_from(root: &Path, rels: &[&str]) -> Vec<String> {
    write_files(root, rels);
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root);
    scan(&catalog, &source.source_id);
    prepared(&catalog)
}

/// §2.3: the folder the user added is never skipped as hidden -- they chose it.
#[test]
fn an_added_folder_is_scanned_even_when_it_is_hidden() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join(".notes");
    assert_eq!(
        prepared_from(&root, &["a.md", ".secret/b.md"]),
        ["a.md"],
        "the dot-named root is scanned; a hidden folder inside it is not"
    );
}

/// The dot rule now carries `.git`, `.cache` and `.venv` (they left the name list).
#[test]
fn dot_folders_are_still_skipped() {
    let base = tempfile::tempdir().unwrap();
    assert_eq!(
        prepared_from(
            base.path(),
            &["a.md", ".git/b.md", ".cache/c.md", ".venv/d.md"]
        ),
        ["a.md"]
    );
}

/// §2.4: a valid `CACHEDIR.TAG` marks a folder; a wrong signature does not.
#[test]
fn a_folder_with_a_valid_cachedir_tag_is_skipped_and_a_wrong_one_is_not() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path();
    write_files(root, &["a.md", "tagged/b.md", "wrong/c.md", "empty/d.md"]);
    std::fs::write(
        root.join("tagged").join("CACHEDIR.TAG"),
        format!("{SIGNATURE}\n# This file is a cache directory tag.\n"),
    )
    .unwrap();
    std::fs::write(
        root.join("wrong").join("CACHEDIR.TAG"),
        "Signature: 00000000000000000000000000000000\n",
    )
    .unwrap();
    std::fs::write(root.join("empty").join("CACHEDIR.TAG"), "").unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root);
    scan(&catalog, &source.source_id);
    let markdown: Vec<String> = prepared(&catalog)
        .into_iter()
        .filter(|p| p.ends_with(".md"))
        .collect();
    assert_eq!(
        markdown,
        ["a.md", "empty/d.md", "wrong/c.md"],
        "only the folder with the standard signature is skipped"
    );
}

/// §2.4: names tools own are skipped by name.
#[test]
fn node_modules_and_pycache_are_skipped_by_name() {
    let base = tempfile::tempdir().unwrap();
    assert_eq!(
        prepared_from(
            base.path(),
            &["a.md", "node_modules/b.md", "src/__pycache__/c.md"]
        ),
        ["a.md"]
    );
}

/// §2.4: `build`, `dist` and `target` are a tool's output only beside their
/// project file; without one they are ordinary words and are prepared.
#[test]
fn build_dist_and_target_are_skipped_only_beside_their_project_file() {
    let base = tempfile::tempdir().unwrap();
    let files = [
        // beside their project file: skipped
        "node/package.json",
        "node/build/skipped-1.md",
        "node/dist/skipped-2.md",
        "cargo/Cargo.toml",
        "cargo/target/skipped-3.md",
        "maven/pom.xml",
        "maven/target/skipped-4.md",
        // no project file beside them: a person's own folders
        "clients/target/plan.md",
        "reports/build/q3.md",
        "reports/dist/handout.md",
        // a project file of another tool does not make it that tool's output
        "mixed/Cargo.toml",
        "mixed/build/kept-5.md",
    ];
    write_files(base.path(), &files);
    // `Cargo.toml` and the like are files the scanner also catalogs (as
    // supported source types); only the folders' contents are in question.
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, base.path());
    scan(&catalog, &source.source_id);
    let rows: Vec<String> = prepared(&catalog)
        .into_iter()
        .filter(|p| p.ends_with(".md"))
        .collect();
    assert_eq!(
        rows,
        [
            "clients/target/plan.md",
            "mixed/build/kept-5.md",
            "reports/build/q3.md",
            "reports/dist/handout.md",
        ]
    );
}

fn seed_chunk(catalog: &Catalog, file_id: &FileId, text: &str) {
    let extraction_id = format!("e-{}", file_id.as_str());
    catalog
        .lock()
        .execute(
            "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
             extractor_version, normalization_version, status, created_at, updated_at) \
             VALUES (?1,?2,'text','v1','norm-v1','succeeded','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            params![extraction_id, file_id.as_str()],
        )
        .unwrap();
    ChunkRepository::new(catalog)
        .insert_bundle(
            file_id,
            &ExtractionId::from_string(extraction_id),
            &[ChunkSpec {
                chunk_kind: "paragraph",
                chunk_ordinal: 0,
                heading_path: None,
                title: None,
                normalized_text: text.to_string(),
                line_start: 1,
                line_end: 1,
                byte_start: None,
                byte_end: None,
                location_quality: "exact",
                location_kind: "lines",
                parent_idx: None,
            }],
        )
        .unwrap();
}

fn count(catalog: &Catalog, sql: &str) -> i64 {
    catalog.lock().query_row(sql, [], |r| r.get(0)).unwrap()
}

/// §2.5: a profile from before this task holds files under `target/` (prepared,
/// with chunks and keyword rows). Once the folder is skipped, a rescan erases
/// them -- the row, the chunks, both keyword-index tables -- and marks nothing
/// missing.
#[test]
fn a_folder_that_becomes_skipped_is_erased_not_marked_missing() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path();
    write_files(root, &["keep.md", "target/built.md", "target/deep/more.md"]);
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root);

    // Today's behaviour: no project file, so `target/` is prepared.
    scan(&catalog, &source.source_id);
    assert_eq!(
        prepared(&catalog),
        ["keep.md", "target/built.md", "target/deep/more.md"]
    );
    let ids: Vec<(String, FileId)> = {
        let conn = catalog.lock();
        let mut stmt = conn
            .prepare("SELECT display_path, file_id FROM files")
            .unwrap();
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?.replace('\\', "/"),
                FileId::from_string(r.get::<_, String>(1)?),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    for (path, id) in &ids {
        seed_chunk(&catalog, id, &format!("words of {path}"));
    }
    assert_eq!(count(&catalog, "SELECT COUNT(*) FROM chunks"), 3);
    assert_eq!(count(&catalog, "SELECT COUNT(*) FROM chunk_fts"), 3);

    // The folder becomes a tool's output (a valid tag appears), and is rescanned.
    std::fs::write(
        root.join("target").join("CACHEDIR.TAG"),
        format!("{SIGNATURE}\n"),
    )
    .unwrap();
    let summary = scan(&catalog, &source.source_id);

    assert_eq!(prepared(&catalog), ["keep.md"], "only keep.md has a row");
    assert_eq!(summary.erased_files, 2);
    assert_eq!(summary.missing_files, 0, "erased, not missing");
    assert_eq!(
        count(
            &catalog,
            "SELECT COUNT(*) FROM files WHERE file_status = 'missing'"
        ),
        0
    );
    assert_eq!(count(&catalog, "SELECT COUNT(*) FROM chunks"), 1);
    assert_eq!(count(&catalog, "SELECT COUNT(*) FROM chunk_fts"), 1);
    assert_eq!(count(&catalog, "SELECT COUNT(*) FROM chunk_fts_trigram"), 1);
    let counts = ChunkRepository::new(&catalog)
        .keyword_index_counts()
        .unwrap();
    assert!(counts.violation().is_none(), "RFC-059: {counts:?}");
    let mut erased = summary.erased_paths.clone();
    erased.sort();
    assert!(erased[0].ends_with("built.md") && erased[1].ends_with("more.md"));
}

/// The other direction: a folder that is **no longer** skipped (a user's own
/// `build/`, which the old name list dropped) is prepared by the same scan.
#[test]
fn a_folder_that_is_no_longer_skipped_is_prepared() {
    let base = tempfile::tempdir().unwrap();
    assert_eq!(
        prepared_from(base.path(), &["keep.md", "build/plan.md"]),
        ["build/plan.md", "keep.md"]
    );
}

/// A hidden entry is judged only when the folder's policy excludes hidden ones.
#[test]
fn including_hidden_files_still_prepares_a_dot_folder() {
    let base = tempfile::tempdir().unwrap();
    write_files(base.path(), &[".notes/a.md"]);
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source_with(
        &catalog,
        base.path(),
        HiddenFilePolicy::Include,
        SymlinkPolicy::Ignore,
    );
    scan(&catalog, &source.source_id);
    assert_eq!(prepared(&catalog), [".notes/a.md"]);
}

/// §2.1, Windows: a folder with the Hidden attribute, and one with System, inside
/// an added folder are skipped (this is what `AppData` is).
#[cfg(windows)]
#[test]
fn windows_hidden_and_system_folders_are_skipped() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path();
    write_files(
        root,
        &["a.md", "hidden-dir/b.md", "system-dir/c.md", "plain/d.md"],
    );
    for (attribute, dir) in [("+h", "hidden-dir"), ("+s", "system-dir")] {
        let status = std::process::Command::new("attrib")
            .args([attribute, &root.join(dir).to_string_lossy()])
            .status()
            .unwrap();
        assert!(status.success(), "attrib {attribute} {dir}");
    }
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root);
    scan(&catalog, &source.source_id);
    assert_eq!(prepared(&catalog), ["a.md", "plain/d.md"]);
}

/// §2.2, macOS: a folder with `UF_HIDDEN` inside an added folder is skipped (this
/// is what `~/Library` is).
#[cfg(target_os = "macos")]
#[test]
fn a_macos_hidden_flag_folder_is_skipped() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path();
    write_files(root, &["a.md", "flagged/b.md", "plain/c.md"]);
    let status = std::process::Command::new("chflags")
        .args(["hidden", &root.join("flagged").to_string_lossy()])
        .status()
        .unwrap();
    assert!(status.success(), "chflags hidden");
    let catalog = Catalog::open_in_memory().unwrap();
    let source = register_dir_source(&catalog, root);
    scan(&catalog, &source.source_id);
    assert_eq!(prepared(&catalog), ["a.md", "plain/c.md"]);
}

/// §2.6 (§1.4): the folder the system keeps application data in, directly under
/// the home directory, is asked about when added -- and only exactly that folder.
/// The rule is pure, so it is checked here on every platform with both kinds of
/// separator; the platform's own names are wired by `sensitive_warning`.
#[test]
fn application_data_under_home_is_asked_about_and_no_other_folder_of_that_name() {
    use crate::sensitive::in_home_application_data as asked;
    let windows = ["AppData"];
    let mac = ["Library"];

    assert!(asked("/Users/me/Library", Some("/Users/me"), &mac));
    assert!(asked("/Users/me/Library/Caches/x", Some("/Users/me"), &mac));
    assert!(asked(
        r"\\?\C:\Users\me\AppData",
        Some(r"C:\Users\me"),
        &windows
    ));
    assert!(asked(
        r"C:\Users\me\AppData\Local\Temp",
        Some(r"C:\Users\me"),
        &windows
    ));
    assert!(
        asked(r"c:\users\ME\appdata", Some(r"C:\Users\me"), &windows),
        "case does not matter on these platforms"
    );

    // Not "any folder called Library": a user's own, or the wrong place.
    assert!(!asked(
        "/Users/me/Documents/Library",
        Some("/Users/me"),
        &mac
    ));
    assert!(!asked("/Library", Some("/Users/me"), &mac));
    assert!(!asked("/Users/other/Library", Some("/Users/me"), &mac));
    assert!(!asked("/Users/me/LibraryOfMine", Some("/Users/me"), &mac));
    // The home directory itself is not asked about; only what is under it.
    assert!(!asked("/Users/me", Some("/Users/me"), &mac));
    // No home directory known, or no such folder on this platform.
    assert!(!asked("/Users/me/Library", None, &mac));
    assert!(!asked("/Users/me/Library", Some("/Users/me"), &[]));
}
