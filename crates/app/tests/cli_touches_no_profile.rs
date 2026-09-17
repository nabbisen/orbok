//! Task 051: `orbok --help` and an unrecognised argument must touch no
//! profile.
//!
//! The property is "creates and opens nothing", so that is what is
//! asserted: each run gets a fresh, empty `ORBOK_DATA_DIR` (which relocates
//! the whole profile, settings included -- RFC-054), and the directory must
//! still be empty afterwards. Checking only the message would test the text
//! and not the harm: before this fix, `--help` resolved that directory,
//! created a catalog in it and ran every migration.
//!
//! Runs the real binary via `CARGO_BIN_EXE_orbok`. The display variables are
//! removed so that, if a regression ever lets an argument fall through to
//! startup, the GUI cannot open a window and the run ends on its own; a
//! timeout backs that up.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run_with_fresh_profile(arg: &str) -> (Output, tempfile::TempDir) {
    let data_dir = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_orbok"))
        .arg(arg)
        .env("ORBOK_DATA_DIR", data_dir.path())
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    (child.wait_with_output().unwrap(), data_dir)
}

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn help_prints_usage_to_stdout_exits_zero_and_touches_no_profile() {
    let (output, data_dir) = run_with_fresh_profile("--help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        entries(data_dir.path()),
        Vec::<String>::new(),
        "--help must not create or open anything in the profile directory"
    );
    assert_eq!(output.status.code(), Some(0), "--help exits 0");
    assert!(
        stdout.contains("Usage: orbok"),
        "usage goes to stdout, got {stdout:?}"
    );
}

#[test]
fn an_unknown_argument_names_itself_exits_two_and_touches_no_profile() {
    let (output, data_dir) = run_with_fresh_profile("--chek");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        entries(data_dir.path()),
        Vec::<String>::new(),
        "an unrecognised argument must not create or open anything in the profile directory"
    );
    assert_eq!(output.status.code(), Some(2), "a usage error exits 2");
    assert!(
        stderr.contains("'--chek'") && stderr.contains("Usage: orbok"),
        "the argument is named and usage follows on stderr, got {stderr:?}"
    );
    assert!(
        output.stdout.is_empty(),
        "nothing on stdout for a usage error"
    );
}

/// Task 071 §4 test 4: `--check` still reports a startup failure as text and
/// exits non-zero -- it never reaches the startup-failure window. The data
/// folder is a regular file. With the display variables removed, a window
/// attempt would fail differently and log the window's own line; the run
/// must also end on its own, not at the timeout.
#[test]
fn check_reports_an_unusable_data_folder_as_text_and_opens_no_window() {
    let temp = tempfile::tempdir().unwrap();
    let data_file = temp.path().join("not-a-folder");
    std::fs::write(&data_file, "x").unwrap();
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_orbok"))
        .arg("--check")
        .env("ORBOK_DATA_DIR", &data_file)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = started + Duration::from_secs(60);
    let mut killed = false;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            killed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!killed, "--check must end on its own");
    assert_eq!(output.status.code(), Some(1), "a failed --check exits 1");
    assert!(
        stderr.contains("Error: "),
        "the error is printed to stderr, got {stderr:?}"
    );
    for (name, text) in [("stdout", &stdout), ("stderr", &stderr)] {
        assert!(
            !text.contains("could not start"),
            "--check must not reach the startup-failure window; {name}: {text:?}"
        );
    }
}

/// RFC-061 §10 criterion 4, startup half (Review 247 §3): an **existing**
/// profile whose data folder cannot be read makes a GUI launch fail at
/// startup as a data-folder failure -- logged at `error!` -- rather than
/// start a UI that never indexes.
///
/// Linux only: the display variables are removed so no window can open and
/// the run ends by itself; on macOS and Windows a window would open and
/// wait. Skipped when a mode-000 directory is still readable (root).
#[cfg(target_os = "linux")]
#[test]
fn an_unreadable_existing_data_folder_fails_startup_as_a_data_folder_failure() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let created = Command::new(env!("CARGO_BIN_EXE_orbok"))
        .arg("--check")
        .env("ORBOK_DATA_DIR", &profile)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "--check creates the profile first"
    );
    assert!(profile.join("orbok-catalog.sqlite3").exists());

    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&profile).is_ok() {
        std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: a mode-000 directory is readable (running as root)");
        return;
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_orbok"))
        .env("ORBOK_DATA_DIR", &profile)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut killed = false;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            killed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().unwrap();
    std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o755)).unwrap();
    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!killed, "startup must end by itself");
    assert!(
        !output.status.success(),
        "an unreadable data folder must not start successfully"
    );
    let failure_line = stdout
        .lines()
        .find(|line| line.contains("ERROR") && line.contains("orbok could not start"))
        .unwrap_or_else(|| panic!("startup failure logged at error!; stdout: {stdout:?}"));
    assert!(
        failure_line.contains("cause=DataFolder"),
        "classified as a data-folder failure, got {failure_line:?}"
    );
    assert!(
        stderr.contains("PermissionDenied"),
        "the underlying error is printed, got {stderr:?}"
    );
}

/// The log's colour codes split `field=value` pairs; remove them.
#[cfg(target_os = "linux")]
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
