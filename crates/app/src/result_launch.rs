//! HANDOFF-041: opening a search result, and showing it in its folder.
//!
//! The first place orbok launches anything outside itself, so the boundary
//! is the point of this module, not the button:
//!
//! 1. **Validated immediately before launching**, through the same
//!    searchable-source [`PathGuard`](orbok_fs::PathGuard) the snippet path
//!    uses (RFC-060 Slice 2). A path outside every registered, searchable
//!    source is refused.
//! 2. **Never through a shell.** Each platform's launcher is started with
//!    [`Command::new`] and the path as one argument -- no `sh -c`, no
//!    `cmd /c`, no string built from the path.
//! 3. **Only paths a search returned.** The messages carry a result index;
//!    the path is looked up here from the displayed results.
//! 4. **Check-then-use.** Validation and launch are two steps, so the file
//!    can change between them. That is the same window RFC-060 §9 already
//!    accepts for snippets, and it is recorded rather than solved: the
//!    launched application opens whatever is at the path by then.
//!
//! **Why not the `opener` crate** (HANDOFF-041 §2 asked for this report):
//! `opener` 0.8.5's Linux `open` falls back to spawning `sh -s <path>` with
//! a bundled `xdg-open` script piped to stdin when the system `xdg-open`
//! cannot be spawned (`src/linux_and_more.rs`, `open_with_internal_xdg_open`),
//! and its `reveal` falls back to that same `open` when D-Bus fails. That is
//! the shell §1.2 forbids, so the three launchers are written directly:
//!
//! | Platform | Open | Show in folder |
//! |---|---|---|
//! | Linux and other freedesktop Unix | `xdg-open <path>` | `xdg-open <parent directory>` |
//! | macOS | `open -- <path>` | `open -R -- <path>` |
//! | Windows | `explorer.exe <path>` | `explorer.exe /select,<path>` |
//!
//! On Linux, "show in folder" opens the containing folder without selecting
//! the file; selecting it needs the `org.freedesktop.FileManager1` D-Bus
//! call, which is reported rather than built here.
//!
//! `xdg-open` is itself usually a `/bin/sh` script, run through its shebang.
//! orbok does not start a shell or build a command line: the path reaches
//! `xdg-open` as its first argument, exactly as it would from a file manager.
//!
//! On Windows, `std::fs::canonicalize` (inside the guard) returns verbatim
//! `\\?\C:\...` paths, which Explorer is not documented to accept, so the
//! prefix is removed with [`explorer_path`] after validation.

use orbok_core::OrbokResult;
use orbok_db::Catalog;
use orbok_fs::ValidatedPath;
use orbok_search::ResultRecoveryAction;
use orbok_ui::notice::UserNotice;
use orbok_ui::state::{Message, SearchResultDisplay};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// What to do with a validated result path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchAction {
    Open,
    Reveal,
}

/// Which result a message asks to launch, and how: `OpenResult` and
/// `RevealResult` (the selected row's buttons and Enter), and RFC-038's
/// `OpenAnyway` / `ShowInFolder` recovery actions, which are the same two
/// operations on the same result. `None` for every other message.
pub(crate) fn launch_request(message: &Message) -> Option<(usize, LaunchAction)> {
    match message {
        Message::OpenResult(index) => Some((*index, LaunchAction::Open)),
        Message::RevealResult(index) => Some((*index, LaunchAction::Reveal)),
        Message::TrustRecoveryAction { result_idx, action } => match action {
            ResultRecoveryAction::OpenAnyway => Some((*result_idx, LaunchAction::Open)),
            ResultRecoveryAction::ShowInFolder => Some((*result_idx, LaunchAction::Reveal)),
            _ => None,
        },
        _ => None,
    }
}

/// Launches a validated path outside orbok. The production implementation
/// is [`SystemLauncher`]; tests use a recording fake, so no test launches a
/// real application.
pub(crate) trait Launcher {
    fn open(&self, path: &ValidatedPath) -> io::Result<()>;
    fn reveal(&self, path: &ValidatedPath) -> io::Result<()>;
}

/// Validate the result at `index` and launch it. Returns the notice to show
/// when it is not launched: the existing "files may have moved" wording,
/// never a raw error string. `None` means it was handed to the launcher.
pub(crate) fn launch_result(
    catalog: &Catalog,
    results: &[SearchResultDisplay],
    index: usize,
    action: LaunchAction,
    launcher: &dyn Launcher,
) -> Option<UserNotice> {
    // An index with no result (the list changed under a stale selection)
    // launches nothing and needs no notice.
    let result = results.get(index)?;
    let validated = match validate(catalog, &result.canonical_path) {
        Ok(validated) => validated,
        Err(error) => {
            tracing::warn!(%error, "a search result was not opened: it failed validation");
            return Some(UserNotice::FilesMovedOrMissing);
        }
    };
    // HANDOFF-041 §1.4: the file can change between this validation and the
    // launch below; RFC-060 §9 accepts the same window for snippets.
    let launched = match action {
        LaunchAction::Open => launcher.open(&validated),
        LaunchAction::Reveal => launcher.reveal(&validated),
    };
    match launched {
        Ok(()) => None,
        Err(error) => {
            tracing::warn!(%error, "a search result could not be launched");
            Some(UserNotice::FilesMovedOrMissing)
        }
    }
}

fn validate(catalog: &Catalog, canonical_path: &str) -> OrbokResult<ValidatedPath> {
    orbok_search::snippet::searchable_path_guard(catalog)?.validate(Path::new(canonical_path))
}

/// The real launcher: one process per action, path as a single argument.
pub(crate) struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn open(&self, path: &ValidatedPath) -> io::Result<()> {
        spawn(open_command(&path.canonical))
    }

    fn reveal(&self, path: &ValidatedPath) -> io::Result<()> {
        spawn(reveal_command(&path.canonical))
    }
}

#[cfg(target_os = "macos")]
fn open_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("--").arg(path);
    command
}

#[cfg(target_os = "macos")]
fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("open");
    command.arg("-R").arg("--").arg(path);
    command
}

#[cfg(windows)]
fn open_command(path: &Path) -> Command {
    let mut command = Command::new("explorer.exe");
    command.arg(explorer_path(path));
    command
}

#[cfg(windows)]
fn reveal_command(path: &Path) -> Command {
    // `/select,` must be its own argument: explorer reads the next argument
    // as the item to select. `Command` quotes the path itself.
    let mut command = Command::new("explorer.exe");
    command.arg("/select,").arg(explorer_path(path));
    command
}

/// The same path without Windows' verbatim prefix: `\\?\C:\a` becomes
/// `C:\a`, and `\\?\UNC\server\share` becomes `\\server\share`. Any other
/// path is returned unchanged. Pure string handling, so it is tested on every
/// platform even though only Windows uses it.
#[cfg(any(windows, test))]
fn explorer_path(path: &Path) -> std::path::PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\")
        && rest.as_bytes().get(1) == Some(&b':')
    {
        return std::path::PathBuf::from(rest);
    }
    path.to_path_buf()
}

#[cfg(not(any(target_os = "macos", windows)))]
fn open_command(path: &Path) -> Command {
    // Canonical paths are absolute, so the argument cannot be read as an
    // option.
    let mut command = Command::new("xdg-open");
    command.arg(path);
    command
}

#[cfg(not(any(target_os = "macos", windows)))]
fn reveal_command(path: &Path) -> Command {
    let mut command = Command::new("xdg-open");
    command.arg(path.parent().unwrap_or(path));
    command
}

/// Start `command` detached from orbok's standard streams, and reap it on a
/// background thread so a finished launcher does not linger as a zombie.
fn spawn(mut command: Command) -> io::Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
