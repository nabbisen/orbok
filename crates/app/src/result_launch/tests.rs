//! HANDOFF-041 §4: the boundary, with a recording launcher -- no test here
//! launches a real application.

use super::{
    LaunchAction, Launcher, explorer_path, explorer_select_arg, launch_request, launch_result,
    reveal_command,
};
use crate::bootstrap;
use orbok_db::Catalog;
use orbok_db::repo::SourceRepository;
use orbok_fs::ValidatedPath;
use orbok_ui::notice::UserNotice;
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
            Some(UserNotice::FilesMovedOrMissing),
            "a refused launch shows the existing file-missing wording"
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
        Some(UserNotice::FilesMovedOrMissing)
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
        Some(UserNotice::FilesMovedOrMissing)
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
