//! Sensitive-directory warnings (RFC-003 §7, external design §18.2).
//!
//! Warnings, not blocks: the user may proceed with "Add Anyway", but the
//! default recommendation is not to index credential-bearing folders.

use std::path::Path;

/// Directory names (final or intermediate components) that very likely
/// contain credentials or secrets.
const SENSITIVE_COMPONENTS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".kube",
    ".docker",
    ".password-store",
    ".mozilla",
    ".thunderbird",
];

/// Task 120: the folder directly under the user's home that the system keeps
/// application data in -- `AppData` on Windows, `Library` on macOS. **Exactly
/// that folder**, under the home directory: not any folder called "Library"
/// (a user's own `Documents/Library` is theirs). Elsewhere there is none.
#[cfg(windows)]
const HOME_APPLICATION_DATA: &[&str] = &["AppData"];
#[cfg(target_os = "macos")]
const HOME_APPLICATION_DATA: &[&str] = &["Library"];
#[cfg(not(any(windows, target_os = "macos")))]
const HOME_APPLICATION_DATA: &[&str] = &[];

/// Absolute prefixes that are system directories.
#[cfg(unix)]
const SYSTEM_PREFIXES: &[&str] = &["/etc", "/usr", "/bin", "/sbin", "/boot", "/proc", "/sys"];

#[cfg(not(unix))]
const SYSTEM_PREFIXES: &[&str] = &[
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
];

/// Returns a warning reason when `path` looks like a sensitive location.
/// `None` means no warning is needed.
///
/// `home` is the user's home directory, from the runtime context (Task 120
/// review: resolved once, never read from the environment here). `None` means
/// the platform has none, and nothing is judged relative to a home.
pub fn sensitive_warning(path: &Path, home: Option<&Path>) -> Option<&'static str> {
    if in_home_application_data(
        &path.to_string_lossy(),
        home.map(|h| h.to_string_lossy()).as_deref(),
        HOME_APPLICATION_DATA,
    ) {
        return Some("application_data_directory");
    }
    let path_str = path.to_string_lossy();
    for prefix in SYSTEM_PREFIXES {
        if path_str.starts_with(prefix) {
            return Some("system_directory");
        }
    }
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if SENSITIVE_COMPONENTS.contains(&name.as_ref()) {
            return Some("credential_directory");
        }
        // `.config` only as the home config root, not arbitrary names.
        if name == ".config" {
            return Some("hidden_configuration_directory");
        }
    }
    None
}

/// Whether `path` is one of `names` directly under `home`, or inside it
/// (Task 120). Pure, so it is tested on every platform with either kind of
/// separator; the comparison ignores case (both platforms that have such a
/// folder are case-insensitive by default) and a Windows `\\?\` prefix, which
/// a canonical path carries and the home directory does not.
pub(crate) fn in_home_application_data(path: &str, home: Option<&str>, names: &[&str]) -> bool {
    fn parts(p: &str) -> Vec<&str> {
        p.strip_prefix("\\\\?\\")
            .unwrap_or(p)
            .split(['/', '\\'])
            .filter(|c| !c.is_empty())
            .collect()
    }
    let Some(home) = home else { return false };
    let (path, home) = (parts(path), parts(home));
    if home.is_empty() || path.len() <= home.len() {
        return false;
    }
    home.iter()
        .zip(&path)
        .all(|(h, p)| h.eq_ignore_ascii_case(p))
        && names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(path[home.len()]))
}
