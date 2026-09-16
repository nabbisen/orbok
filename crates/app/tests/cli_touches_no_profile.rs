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
