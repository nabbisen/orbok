//! What "a folder covers" means for paths (RFC-064 §3.3, Task 113).
//!
//! A folder covers a path when the folder's canonical path is a
//! **path-component prefix** of it, never a string prefix: `~/docs` covers
//! `~/docs/work` and does not cover `~/docs2`. The one rule is here so the
//! add routine, the startup combine and the search limit cannot disagree.
//!
//! **Case.** Windows and the default macOS volume compare names without
//! regard to case, and `canonicalize` does not promise the stored case (on
//! Windows it returns the on-disk spelling; on macOS it can return the
//! spelling it was given). So on those two platforms the comparison here
//! ignores case (a Unicode lowercase of each component) and two spellings of
//! one folder are the same folder. The cost is one wrong answer in a rare
//! place: a macOS volume formatted case-sensitive, holding two folders that
//! differ only in case, treats them as one. Everywhere else the comparison
//! is exact.

use std::path::{Component, Path, PathBuf};

/// Whether this platform's default file system ignores case in names.
pub const CASE_INSENSITIVE_FS: bool = cfg!(any(windows, target_os = "macos"));

/// The part of `path` below `root`, if `root` is a component-wise prefix of
/// it (empty when they are the same folder), using this platform's case
/// rule. `None` when `path` is not under `root`.
pub fn relative_under(root: &Path, path: &Path) -> Option<PathBuf> {
    relative_under_with(root, path, CASE_INSENSITIVE_FS)
}

/// [`relative_under`] with the case rule chosen by the caller, so both rules
/// are testable on any platform.
pub fn relative_under_with(root: &Path, path: &Path, case_insensitive: bool) -> Option<PathBuf> {
    if !case_insensitive {
        return path.strip_prefix(root).ok().map(Path::to_path_buf);
    }
    let mut below = path.components();
    for root_part in root.components() {
        let part = below.next()?;
        if fold(root_part) != fold(part) {
            return None;
        }
    }
    Some(below.as_path().to_path_buf())
}

fn fold(component: Component<'_>) -> String {
    component.as_os_str().to_string_lossy().to_lowercase()
}

/// Whether `root` covers `path`: the same folder, or a folder above it.
pub fn covers(root: &Path, path: &Path) -> bool {
    relative_under(root, path).is_some()
}

/// Whether `path` lies strictly inside `root`: under it and not the same
/// folder.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    relative_under(root, path).is_some_and(|rest| rest.components().next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_covers_what_is_under_it_by_components() {
        assert!(covers(Path::new("/h/docs"), Path::new("/h/docs/work")));
        assert!(covers(Path::new("/h/docs"), Path::new("/h/docs")));
        assert!(is_inside(Path::new("/h/docs"), Path::new("/h/docs/a/b")));
        assert!(!is_inside(Path::new("/h/docs"), Path::new("/h/docs")));
        assert!(!covers(Path::new("/h/docs/work"), Path::new("/h/docs")));
    }

    /// Task 113 §2.3: `a2` is not under `a`. A string prefix would say it is.
    #[test]
    fn the_component_rule_is_not_a_string_prefix() {
        assert!(!covers(Path::new("/h/a"), Path::new("/h/a2")));
        assert!(!covers(Path::new("/h/a"), Path::new("/h/a2/x")));
        assert!(!is_inside(Path::new("/h/docs"), Path::new("/h/docs2")));
        for ci in [false, true] {
            assert_eq!(
                relative_under_with(Path::new("/h/a"), Path::new("/h/a2"), ci),
                None
            );
        }
    }

    #[test]
    fn the_rest_is_what_lies_below_the_root() {
        for ci in [false, true] {
            assert_eq!(
                relative_under_with(Path::new("/h/a"), Path::new("/h/a/b/c"), ci),
                Some(PathBuf::from("b/c"))
            );
            assert_eq!(
                relative_under_with(Path::new("/h/a"), Path::new("/h/a"), ci),
                Some(PathBuf::new())
            );
        }
    }

    #[test]
    fn case_differences_matter_only_where_the_file_system_ignores_them() {
        let root = Path::new("/h/Docs");
        let path = Path::new("/h/docs/Work");
        assert_eq!(relative_under_with(root, path, false), None);
        assert_eq!(
            relative_under_with(root, path, true),
            Some(PathBuf::from("Work"))
        );
    }
}
