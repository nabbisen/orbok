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
//! | Windows | `ShellExecuteW` with the `open` verb | `explorer.exe /select,"<path>"` |
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

use orbok_core::OrbokError;
use orbok_db::Catalog;
use orbok_fs::ValidatedPath;
use orbok_search::ResultRecoveryAction;
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

/// Why a result was not launched (Tasks 065, 070). `main.rs` turns it into
/// the user's notice through `notice_retry::result_not_launched`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchFailure {
    /// Validation refused it because the file is no longer where orbok found
    /// it: gone from disk, outside every folder orbok searches, or on a
    /// drive that cannot be reached.
    NotFound,
    /// Validation refused it because orbok may not open it: a folder rule
    /// blocks it, or the system denied permission.
    NotAllowed,
    /// The folder list could not be read to validate it. Retrying repeats
    /// `action` on the same result.
    Busy { index: usize, action: LaunchAction },
    /// The file validated, but the launcher failed on `action`.
    CouldNotOpen { index: usize, action: LaunchAction },
}

/// Classify a refusal from the path stage of validation (Task 070 §B.2).
/// The guard flattens io errors into strings, so a canonicalization failure
/// is classified by looking at the path itself, never by the message.
///
/// No wildcard arm: a new `OrbokError` variant fails to compile here until
/// it is classified.
pub(crate) fn classify_refusal(error: &OrbokError, path: &Path) -> LaunchFailure {
    match error {
        OrbokError::PathOutsideSources => LaunchFailure::NotFound,
        OrbokError::PolicyBlocked(_) => LaunchFailure::NotAllowed,
        // `canonicalize`, the size check's `metadata`, and the symlink walk's
        // `symlink_metadata` all fail with io errors, reported through these
        // two variants. Permission denied is Not allowed; anything else --
        // gone, a dangling link, an unreachable drive -- is Not found.
        OrbokError::PathCanonicalization(_) | OrbokError::Io(_) => {
            if std::fs::symlink_metadata(path)
                .is_err_and(|e| e.kind() == io::ErrorKind::PermissionDenied)
            {
                LaunchFailure::NotAllowed
            } else {
                LaunchFailure::NotFound
            }
        }
        // `PathGuard::validate` returns none of these: it reads no catalog,
        // cache, model or queue. Were one to arrive from the path stage, it
        // would come back the same on every retry, so it is not Busy (whose
        // Try again would repeat it forever). Not found's Go to Folders is
        // the one way out that stays useful. A catalog error never reaches
        // this arm: the catalog stage is classified before it.
        OrbokError::Database(_)
        | OrbokError::MigrationFailed { .. }
        | OrbokError::SourceNotFound
        | OrbokError::FileNotFound
        | OrbokError::CleanupWouldTouchPersistentData
        | OrbokError::Cache(_)
        | OrbokError::Extraction { .. }
        | OrbokError::Embedding { .. }
        | OrbokError::ExtractionCacheMissing
        | OrbokError::InvalidCatalogValue { .. }
        | OrbokError::Canceled
        | OrbokError::BackpressureActive { .. }
        | OrbokError::SchemaVersionUnsupported { .. } => LaunchFailure::NotFound,
    }
}

/// Validate the result at `index` and launch it. `None` means it was handed
/// to the launcher; otherwise the reason it was not, never a raw error
/// string.
pub(crate) fn launch_result(
    catalog: &Catalog,
    results: &[SearchResultDisplay],
    index: usize,
    action: LaunchAction,
    launcher: &dyn Launcher,
) -> Option<LaunchFailure> {
    // An index with no result (the list changed under a stale selection)
    // launches nothing and needs no notice.
    let result = results.get(index)?;
    let path = Path::new(&result.canonical_path);
    // Task 070 §B.2: classified by the stage that failed. The catalog stage
    // reads the folder list; the startup check has already refused an
    // unusable catalog, so a failure here is a lock or busy condition.
    let guard = match orbok_search::snippet::searchable_path_guard(catalog) {
        Ok(guard) => guard,
        Err(error) => {
            tracing::warn!(%error, "a search result was not opened: the folder list could not be read");
            return Some(LaunchFailure::Busy { index, action });
        }
    };
    let validated = match guard.validate(path) {
        Ok(validated) => validated,
        Err(error) => {
            tracing::warn!(%error, "a search result was not opened: it failed validation");
            return Some(classify_refusal(&error, path));
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
            Some(LaunchFailure::CouldNotOpen { index, action })
        }
    }
}

/// The real launcher: one process or shell-API call per action, the path
/// passed whole.
pub(crate) struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn open(&self, path: &ValidatedPath) -> io::Result<()> {
        open_path(&path.canonical)
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

#[cfg(not(windows))]
fn open_path(path: &Path) -> io::Result<()> {
    spawn(open_command(path))
}

/// Review 230 §3: `explorer.exe <path>` is not a documented way to open a
/// file, and Explorer's command-line parser splits on commas that `Command`
/// does not quote. `ShellExecuteW` with the `open` verb is the documented
/// API and parses no command line at all.
#[cfg(windows)]
fn open_path(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let mut file: Vec<u16> = explorer_path(path).as_os_str().encode_wide().collect();
    if file.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path contains a NUL character",
        ));
    }
    file.push(0);
    let verb: Vec<u16> = "open".encode_utf16().chain([0]).collect();
    // SAFETY: `verb` and `file` are NUL-terminated and outlive the call; the
    // window handle, parameters and directory are documented as optional.
    let instance = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Documented: a value greater than 32 means success.
    if instance as usize > 32 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn reveal_command(path: &Path) -> Command {
    // Review 230 §3: passed raw and always quoted. `Command` quotes only
    // arguments containing spaces or tabs, so a path with a comma reached
    // Explorer unquoted and was split there.
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("explorer.exe");
    command.raw_arg(explorer_select_arg(path));
    command
}

/// `/select,"<path>"`, the path always double-quoted. Safe because a Windows
/// path cannot contain `"`. Pure string handling, tested on every platform.
#[cfg(any(windows, test))]
fn explorer_select_arg(path: &Path) -> std::ffi::OsString {
    let mut argument = std::ffi::OsString::from("/select,\"");
    argument.push(explorer_path(path));
    argument.push("\"");
    argument
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
