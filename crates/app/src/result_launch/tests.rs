//! HANDOFF-041 §4: the boundary, with a recording launcher -- no test here
//! launches a real application.

use super::{
    LaunchAction, LaunchFailure, Launcher, classify_refusal, explorer_path, explorer_select_arg,
    launch_request, launch_result, reveal_command,
};
use crate::bootstrap;
use orbok_core::OrbokError;
use orbok_db::Catalog;
use orbok_db::repo::SourceRepository;
use orbok_fs::ValidatedPath;
use orbok_ui::state::SearchResultDisplay;
use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct RecordingLauncher(pub(crate) RefCell<Vec<(LaunchAction, PathBuf)>>);

impl Launcher for RecordingLauncher {
    fn open(&self, path: &ValidatedPath) -> io::Result<()> {
        self.0
            .borrow_mut()
            .push((LaunchAction::Open, path.canonical.clone()));
        Ok(())
    }
    fn reveal(&self, path: &ValidatedPath) -> io::Result<()> {
        self.0
            .borrow_mut()
            .push((LaunchAction::Reveal, path.canonical.clone()));
        Ok(())
    }
}

fn result_for(path: &Path) -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: path.display().to_string(),
        canonical_path: path.display().to_string(),
        title: None,
        heading_path: None,
        snippet: None,
        keyword_rank: 1,
        badges: vec![],
        trust: Default::default(),
    }
}

/// A catalog with one registered source holding `note.md`, and a second
/// directory -- never registered -- holding `outside.md`.
fn fixture(temp: &Path) -> (Catalog, PathBuf, PathBuf, orbok_core::SourceId) {
    let source = temp.join("source");
    let elsewhere = temp.join("elsewhere");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(source.join("note.md"), "# Note\n").unwrap();
    std::fs::write(elsewhere.join("outside.md"), "# Outside\n").unwrap();
    let catalog = Catalog::open_in_memory().unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&catalog, &source.to_string_lossy()).unwrap();
    let inside = std::fs::canonicalize(source.join("note.md")).unwrap();
    let outside = std::fs::canonicalize(elsewhere.join("outside.md")).unwrap();
    (
        catalog,
        inside,
        outside,
        orbok_core::SourceId::from_string(card.source_id),
    )
}

#[test]
fn a_result_inside_a_searchable_source_reaches_the_launcher_once_with_its_path() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, _) = fixture(temp.path());
    let launcher = RecordingLauncher::default();
    for action in [LaunchAction::Open, LaunchAction::Reveal] {
        assert_eq!(
            launch_result(&catalog, &[result_for(&inside)], 0, action, &launcher),
            None
        );
    }
    assert_eq!(
        launcher.0.borrow().as_slice(),
        [
            (LaunchAction::Open, inside.clone()),
            (LaunchAction::Reveal, inside)
        ]
    );
}

#[test]
fn a_result_outside_every_source_is_refused_and_never_launched() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, _, outside, _) = fixture(temp.path());
    let launcher = RecordingLauncher::default();
    for action in [LaunchAction::Open, LaunchAction::Reveal] {
        assert_eq!(
            launch_result(&catalog, &[result_for(&outside)], 0, action, &launcher),
            Some(LaunchFailure::NotFound),
            "Task 065: outside every folder orbok searches is not found"
        );
    }
    assert!(
        launcher.0.borrow().is_empty(),
        "a path outside every source must not reach the launcher"
    );
}

#[test]
fn a_result_in_a_paused_source_is_refused_and_never_launched() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, source_id) = fixture(temp.path());
    SourceRepository::new(&catalog)
        .set_status(&source_id, orbok_core::SourceStatus::Paused)
        .unwrap();
    let launcher = RecordingLauncher::default();
    assert_eq!(
        launch_result(
            &catalog,
            &[result_for(&inside)],
            0,
            LaunchAction::Open,
            &launcher
        ),
        Some(LaunchFailure::NotFound),
        "a paused folder is not searched, so its file is not found (Folders resumes it)"
    );
    assert!(
        launcher.0.borrow().is_empty(),
        "a paused source's file must not reach the launcher"
    );
}

#[test]
fn a_deleted_file_is_refused_and_never_launched() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, _) = fixture(temp.path());
    std::fs::remove_file(&inside).unwrap();
    let launcher = RecordingLauncher::default();
    assert_eq!(
        launch_result(
            &catalog,
            &[result_for(&inside)],
            0,
            LaunchAction::Open,
            &launcher
        ),
        Some(LaunchFailure::NotFound),
        "Task 065: a file deleted before validation is not found"
    );
    assert!(launcher.0.borrow().is_empty());
}

#[test]
fn explorer_path_removes_only_the_verbatim_prefix() {
    assert_eq!(
        explorer_path(Path::new(r"\\?\C:\Users\a b\note.md")),
        PathBuf::from(r"C:\Users\a b\note.md")
    );
    assert_eq!(
        explorer_path(Path::new(r"\\?\UNC\server\share\note.md")),
        PathBuf::from(r"\\server\share\note.md")
    );
    assert_eq!(
        explorer_path(Path::new(r"C:\plain\note.md")),
        PathBuf::from(r"C:\plain\note.md")
    );
    assert_eq!(
        explorer_path(Path::new("/home/user/note.md")),
        PathBuf::from("/home/user/note.md")
    );
}

/// HANDOFF-041 §4 test 4: how each launcher is built, inspected without
/// running it. The program is the platform launcher itself -- never `sh`,
/// `bash` or `cmd` -- and the path is exactly one argument, not text spliced
/// into another. On Windows, Open is `ShellExecuteW` (no process and no
/// command line), so only Show in folder is a `Command` there.
#[test]
fn the_launcher_is_the_platform_opener_with_the_path_as_one_argument() {
    use std::ffi::OsStr;
    use std::process::Command;
    let path = Path::new(if cfg!(windows) {
        r"C:\docs\a b; rm -rf ~\note.md"
    } else {
        "/docs/a b; rm -rf ~/note.md"
    });
    #[cfg(not(windows))]
    let commands: Vec<(Command, &str)> = vec![
        (super::open_command(path), "open"),
        (reveal_command(path), "reveal"),
    ];
    #[cfg(windows)]
    let commands: Vec<(Command, &str)> = vec![(reveal_command(path), "reveal")];
    for (command, action) in commands {
        let program = command.get_program();
        for shell in ["sh", "bash", "cmd", "cmd.exe", "powershell"] {
            assert_ne!(
                program,
                OsStr::new(shell),
                "{action}: launched through a shell"
            );
        }
        #[cfg(not(windows))]
        {
            let args: Vec<&OsStr> = command.get_args().collect();
            let path_args = args
                .iter()
                .filter(|arg| arg.to_string_lossy().contains("rm -rf"))
                .count();
            assert_eq!(
                path_args, 1,
                "{action}: the path must be exactly one argument, got {args:?}"
            );
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let args: Vec<&OsStr> = command.get_args().collect();
            assert_eq!(program, OsStr::new("xdg-open"));
            let expected = if action == "open" {
                path.as_os_str()
            } else {
                path.parent().unwrap().as_os_str()
            };
            assert_eq!(args, [expected]);
        }
        #[cfg(target_os = "macos")]
        assert_eq!(program, OsStr::new("open"));
        #[cfg(windows)]
        assert_eq!(program, OsStr::new("explorer.exe"));
    }
}

/// Review 230 §3: Explorer splits an unquoted argument at a comma, and
/// `Command` quotes only arguments with spaces or tabs. The select argument
/// is always one quoted token, whatever the path holds.
#[test]
fn the_explorer_select_argument_always_quotes_the_path() {
    assert_eq!(
        explorer_select_arg(Path::new(r"C:\docs\report,final.pdf")),
        std::ffi::OsString::from(r#"/select,"C:\docs\report,final.pdf""#)
    );
    assert_eq!(
        explorer_select_arg(Path::new(r"\\?\C:\docs\plain.pdf")),
        std::ffi::OsString::from(r#"/select,"C:\docs\plain.pdf""#),
        "the verbatim prefix is removed before quoting"
    );
}

/// RFC-038's `OpenAnyway` and `ShowInFolder` recovery actions reach the same
/// two operations as the row's buttons; no other recovery action launches.
#[test]
fn recovery_actions_share_the_open_and_reveal_path() {
    use orbok_search::ResultRecoveryAction;
    use orbok_ui::state::Message;
    assert_eq!(
        launch_request(&Message::OpenResult(2)),
        Some((2, LaunchAction::Open))
    );
    assert_eq!(
        launch_request(&Message::RevealResult(2)),
        Some((2, LaunchAction::Reveal))
    );
    assert_eq!(
        launch_request(&Message::TrustRecoveryAction {
            result_idx: 3,
            action: ResultRecoveryAction::OpenAnyway,
        }),
        Some((3, LaunchAction::Open))
    );
    assert_eq!(
        launch_request(&Message::TrustRecoveryAction {
            result_idx: 3,
            action: ResultRecoveryAction::ShowInFolder,
        }),
        Some((3, LaunchAction::Reveal))
    );
    assert_eq!(
        launch_request(&Message::TrustRecoveryAction {
            result_idx: 3,
            action: ResultRecoveryAction::PrepareAgain,
        }),
        None
    );
    assert_eq!(launch_request(&Message::SelectResult(1)), None);
}

/// A launcher whose open and reveal both fail, as when no app handles the
/// file.
struct FailingLauncher;

impl Launcher for FailingLauncher {
    fn open(&self, _path: &ValidatedPath) -> io::Result<()> {
        Err(io::Error::other("no application handled the file"))
    }
    fn reveal(&self, _path: &ValidatedPath) -> io::Result<()> {
        Err(io::Error::other("no file manager"))
    }
}

/// Task 065 §5 test 1 (red on the old API): an existing file that no app
/// opened is not reported as "files may have moved".
#[test]
fn a_file_that_exists_but_would_not_open_is_not_reported_as_moved() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, _) = fixture(temp.path());
    let got = launch_result(
        &catalog,
        &[result_for(&inside)],
        0,
        LaunchAction::Open,
        &FailingLauncher,
    );
    assert_eq!(
        got,
        Some(LaunchFailure::CouldNotOpen {
            index: 0,
            action: LaunchAction::Open
        }),
        "the file is right there; it just would not open"
    );
}

/// Task 065 §5 test 1: a failed reveal is "could not open" too, and names
/// the reveal as what failed.
#[test]
fn a_reveal_that_failed_is_could_not_open_for_the_reveal() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, _) = fixture(temp.path());
    assert_eq!(
        launch_result(
            &catalog,
            &[result_for(&inside)],
            0,
            LaunchAction::Reveal,
            &FailingLauncher
        ),
        Some(LaunchFailure::CouldNotOpen {
            index: 0,
            action: LaunchAction::Reveal
        })
    );
}

/// Task 070 §B.2 row 2: a folder rule refusing the file -- a real hidden
/// file under an exclude-hidden folder, and `file_too_large` through the
/// classifier -- is Not allowed.
#[test]
fn a_policy_refusal_is_not_allowed() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, _) = fixture(temp.path());
    let hidden_dir = inside.parent().unwrap().join(".hidden");
    std::fs::create_dir_all(&hidden_dir).unwrap();
    let hidden = hidden_dir.join("secret.md");
    std::fs::write(&hidden, "# Secret\n").unwrap();
    let launcher = RecordingLauncher::default();
    assert_eq!(
        launch_result(
            &catalog,
            &[result_for(&std::fs::canonicalize(&hidden).unwrap())],
            0,
            LaunchAction::Open,
            &launcher
        ),
        Some(LaunchFailure::NotAllowed)
    );
    assert!(launcher.0.borrow().is_empty());
    assert_eq!(
        classify_refusal(&OrbokError::PolicyBlocked("file_too_large"), &inside),
        LaunchFailure::NotAllowed
    );
}

/// Task 070 §B.2 rows 4 and 5: a canonicalization or io failure that is not
/// permission denied is Not found -- the file is gone, or the path is a
/// dangling link. The classifier reads the path, never the message: a
/// message saying "permission denied" does not make a missing file Not
/// allowed.
#[test]
fn a_canonicalization_failure_that_is_not_permission_is_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let gone = temp.path().join("gone.md");
    for error in [
        OrbokError::PathCanonicalization("permission denied".into()),
        OrbokError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
    ] {
        assert_eq!(
            classify_refusal(&error, &gone),
            LaunchFailure::NotFound,
            "{error:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_dangling_link_is_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, _, _) = fixture(temp.path());
    let link = inside.parent().unwrap().join("dangling.md");
    std::os::unix::fs::symlink(temp.path().join("nowhere.md"), &link).unwrap();
    assert_eq!(
        launch_result(
            &catalog,
            &[result_for(&link)],
            0,
            LaunchAction::Open,
            &RecordingLauncher::default()
        ),
        Some(LaunchFailure::NotFound)
    );
}

/// Task 070 §B.2's named arm: a variant `PathGuard::validate` never returns
/// is Not found when it comes from the path stage -- never Busy, whose Try
/// again would repeat an error that comes back the same. (From the catalog
/// stage, the same `Database` error is Busy: see
/// `a_catalog_that_cannot_be_read_is_busy`.)
#[test]
fn an_error_validation_never_returns_is_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let (_, inside, _, _) = fixture(temp.path());
    assert_eq!(
        classify_refusal(
            &OrbokError::Database("no such table: sources".into()),
            &inside
        ),
        LaunchFailure::NotFound
    );
}

// ── Task 070 Part B: every refused open gets a truthful notice ─────────

/// A catalog on disk with one registered source holding `note.md`, and a
/// short busy timeout so a locked catalog fails fast.
fn file_fixture(temp: &Path) -> (Catalog, PathBuf, PathBuf) {
    let source = temp.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("note.md"), "# Note\n").unwrap();
    let db = temp.join("orbok-catalog.sqlite3");
    let catalog = Catalog::open(&db).unwrap();
    bootstrap::add_source_expect_added(&catalog, &source.to_string_lossy()).unwrap();
    catalog
        .lock()
        .busy_timeout(std::time::Duration::from_millis(50))
        .unwrap();
    let inside = std::fs::canonicalize(source.join("note.md")).unwrap();
    (catalog, inside, db)
}

/// Task 070 §B.4 test 1 (catalog stage): a catalog that cannot be read for
/// the folder list is Busy. A second connection cannot lock a WAL catalog
/// against readers while orbok's own connection is open (taking exclusive
/// locking mode is itself refused as busy), so the read is made to fail by
/// moving the folder table aside for the duration of the open.
#[test]
fn a_catalog_that_cannot_be_read_is_busy() {
    let temp = tempfile::tempdir().unwrap();
    let (catalog, inside, db) = file_fixture(temp.path());
    let locker = rusqlite::Connection::open(&db).unwrap();
    locker
        .execute_batch("ALTER TABLE sources RENAME TO sources_moved_aside;")
        .unwrap();
    let got = launch_result(
        &catalog,
        &[result_for(&inside)],
        0,
        LaunchAction::Open,
        &RecordingLauncher::default(),
    );
    locker
        .execute_batch("ALTER TABLE sources_moved_aside RENAME TO sources;")
        .unwrap();
    assert_eq!(
        got,
        Some(LaunchFailure::Busy {
            index: 0,
            action: LaunchAction::Open
        }),
        "a catalog that cannot be read is busy"
    );
}

/// Task 070 §B.4 test 1 (path stage, Not allowed): a file orbok may not read.
/// Skipped when the process can still read a mode-000 directory (running as
/// root), so a root CI container cannot produce a false pass.
#[cfg(unix)]
#[test]
fn a_file_orbok_may_not_read_is_not_allowed() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let (catalog, _, _, _) = fixture(temp.path());
    let locked_dir = temp.path().join("source").join("locked");
    std::fs::create_dir_all(&locked_dir).unwrap();
    let file = locked_dir.join("secret.md");
    std::fs::write(&file, "# Secret\n").unwrap();
    let file = std::fs::canonicalize(&file).unwrap();
    std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = std::fs::read_dir(&locked_dir).is_ok();
    let got = (!readable_anyway).then(|| {
        launch_result(
            &catalog,
            &[result_for(&file)],
            0,
            LaunchAction::Open,
            &RecordingLauncher::default(),
        )
    });
    std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let Some(got) = got else {
        eprintln!("skipped: a mode-000 directory is readable (running as root)");
        return;
    };
    assert_eq!(
        got,
        Some(LaunchFailure::NotAllowed),
        "permission denied is not allowed"
    );
}
