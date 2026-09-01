//! PathGuard tests (RFC-003 §14: path traversal, symlink escape, hidden
//! exclusion, outside-source rejection) and sensitive-path warnings.

use crate::path_guard::{GuardedSource, PathGuard};
use crate::sensitive::sensitive_warning;
use crate::tests::common::{register_dir_source, register_dir_source_with};
use orbok_core::{HiddenFilePolicy, OrbokError, SymlinkPolicy};
use orbok_db::Catalog;
use std::fs;
use std::path::Path;

fn guard_for(catalog: &Catalog, root: &Path) -> PathGuard {
    let record = register_dir_source(catalog, root);
    PathGuard::new(vec![GuardedSource::from_record(&record)])
}

// RFC-003 §14 test 5: reject request for non-source path.
#[test]
fn rejects_path_outside_sources() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret.txt"), "secret").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let guard = guard_for(&catalog, dir.path());

    let err = guard
        .validate(&outside.path().join("secret.txt"))
        .unwrap_err();
    assert!(matches!(err, OrbokError::PathOutsideSources));
}

// RFC-003 §14 test 4: reject path traversal read. `..` segments resolve
// during canonicalization; the canonical target decides membership.
#[test]
fn rejects_dot_dot_traversal() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(parent.path().join("outside.txt"), "x").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let guard = guard_for(&catalog, &root);

    let sneaky = root.join("..").join("outside.txt");
    let err = guard.validate(&sneaky).unwrap_err();
    assert!(matches!(err, OrbokError::PathOutsideSources));
}

// RFC-003 §14 test 7: symlink pointing outside the source is rejected
// (membership is decided on the canonical target).
#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("escape.txt"), "x").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("escape.txt"),
        root.path().join("link.txt"),
    )
    .unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let guard = guard_for(&catalog, root.path());

    let err = guard.validate(&root.path().join("link.txt")).unwrap_err();
    assert!(matches!(err, OrbokError::PathOutsideSources));
}

// Symlink inside the source under Ignore policy: also blocked.
#[cfg(unix)]
#[test]
fn ignore_policy_blocks_internal_symlink() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("real.txt"), "x").unwrap();
    std::os::unix::fs::symlink(root.path().join("real.txt"), root.path().join("alias.txt"))
        .unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let guard = guard_for(&catalog, root.path());

    let err = guard.validate(&root.path().join("alias.txt")).unwrap_err();
    assert!(matches!(
        err,
        OrbokError::PolicyBlocked("symlink_policy_blocked")
    ));
    // The real file is fine.
    assert!(guard.validate(&root.path().join("real.txt")).is_ok());
}

// Task 003 Part B: the Ignore policy must be spelling-independent. A
// request routed through a symlinked ancestor of the source root (the
// macOS default, `/var` -> `/private/var`, and reachable on Linux through
// any symlinked ancestor) must not bypass the check just because the
// request's spelling never matches the canonical root by string prefix.
// This is not macOS-specific, so it is constructed explicitly here rather
// than relying on a platform default, to reproduce on Linux CI too.
#[cfg(unix)]
#[test]
fn ignore_policy_blocks_internal_symlink_via_symlinked_ancestor() {
    let parent = tempfile::tempdir().unwrap();
    let real_root = parent.path().join("real_root");
    fs::create_dir(&real_root).unwrap();
    let link_root = parent.path().join("link_root");
    std::os::unix::fs::symlink(&real_root, &link_root).unwrap();

    fs::write(real_root.join("target.txt"), "x").unwrap();
    std::os::unix::fs::symlink(real_root.join("target.txt"), real_root.join("alias.txt")).unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let guard = guard_for(&catalog, &real_root);

    // Requested via the symlinked ancestor, not the canonical root.
    let err = guard.validate(&link_root.join("alias.txt")).unwrap_err();
    assert!(matches!(
        err,
        OrbokError::PolicyBlocked("symlink_policy_blocked")
    ));
    // The real file, reached through the same symlinked ancestor, is fine.
    assert!(guard.validate(&link_root.join("target.txt")).is_ok());
}

// FollowWithinSource admits internal links but still rejects escapes.
#[cfg(unix)]
#[test]
fn follow_within_source_admits_internal_rejects_external() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(root.path().join("real.txt"), "x").unwrap();
    fs::write(outside.path().join("evil.txt"), "x").unwrap();
    std::os::unix::fs::symlink(root.path().join("real.txt"), root.path().join("ok.txt")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("evil.txt"), root.path().join("bad.txt"))
        .unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let record = register_dir_source_with(
        &catalog,
        root.path(),
        HiddenFilePolicy::Exclude,
        SymlinkPolicy::FollowWithinSource,
    );
    let guard = PathGuard::new(vec![GuardedSource::from_record(&record)]);

    assert!(guard.validate(&root.path().join("ok.txt")).is_ok());
    assert!(matches!(
        guard.validate(&root.path().join("bad.txt")).unwrap_err(),
        OrbokError::PathOutsideSources
    ));
}

// RFC-003 §14 test 6: hidden file excluded by default.
#[test]
fn hidden_file_excluded_by_default() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".env"), "SECRET=1").unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let guard = guard_for(&catalog, root.path());

    let err = guard.validate(&root.path().join(".env")).unwrap_err();
    assert!(matches!(
        err,
        OrbokError::PolicyBlocked("hidden_file_excluded")
    ));
}

// RFC-003 §8 item 6: file size limit enforced at the boundary.
#[test]
fn oversized_file_blocked() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("big.txt"), vec![b'a'; 64]).unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let mut record = register_dir_source(&catalog, root.path());
    record.max_file_size_bytes = Some(16);
    let guard = PathGuard::new(vec![GuardedSource::from_record(&record)]);

    let err = guard.validate(&root.path().join("big.txt")).unwrap_err();
    assert!(matches!(err, OrbokError::PolicyBlocked("file_too_large")));
}

// Task 034 §4 (audit S-09): the size gate previously fell back to
// silently admitting the path whenever `metadata()` errored -- a race,
// a permissions change, a special file -- unlike every other check in
// `validate()`, which fails closed. The real defect is a TOCTOU race
// between `canonicalize()` succeeding and `metadata()` running inside
// `validate()` itself; that specific window cannot be asserted on
// soundly from outside the function -- an external post-hoc
// `symlink_metadata` recheck cannot distinguish "the fix works, and
// the file was deleted a moment after `validate` legitimately returned
// Ok" from "the bug fired." (Confirmed by trying: a background-thread
// create/delete race with an external existence recheck reliably
// caught the defect against the unfixed code, but also fired against
// the fixed code, because that race window exists in `validate()`
// regardless of correctness -- proof the black-box check was unsound,
// not evidence of a remaining bug.) `check_size_limit` is `validate`'s
// size-check logic pulled out to take an already-obtained
// `std::io::Result<Metadata>` directly, so the exact failure this task
// is about -- a synthetic `Err` -- is testable deterministically.
#[test]
fn size_gate_fails_closed_when_metadata_errors() {
    use crate::path_guard::check_size_limit;
    use crate::policy::CompiledPolicy;

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("racy.txt");
    let catalog = Catalog::open_in_memory().unwrap();
    let record = register_dir_source(&catalog, root.path());
    let policy = CompiledPolicy::from_source(&record);

    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "vanished mid-race");
    let result = check_size_limit(&path, Err(err), &policy);

    assert!(
        matches!(result, Err(OrbokError::PathCanonicalization(_))),
        "a metadata() error must fail closed as PathCanonicalization, not silently admit the path -- got {result:?}"
    );
}

// Sanity check that the harness above is testing the real defect, not a
// trivially-always-failing function: a genuinely oversized file, given
// through the same seam as a successful `metadata()` result, is still
// rejected on size, exactly as before this task.
#[test]
fn size_gate_still_rejects_oversized_files_through_the_same_seam() {
    use crate::path_guard::check_size_limit;
    use crate::policy::CompiledPolicy;

    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("big.txt");
    fs::write(&file, vec![b'a'; 64]).unwrap();

    let catalog = Catalog::open_in_memory().unwrap();
    let mut record = register_dir_source(&catalog, root.path());
    record.max_file_size_bytes = Some(16);
    let policy = CompiledPolicy::from_source(&record);

    let metadata = fs::metadata(&file).unwrap();
    let result = check_size_limit(&file, Ok(metadata), &policy);

    assert!(matches!(
        result,
        Err(OrbokError::PolicyBlocked("file_too_large"))
    ));
}

// RFC-003 §14 test 10: sensitive path warning triggered.
#[test]
fn sensitive_paths_warn() {
    assert_eq!(
        sensitive_warning(Path::new("/home/user/.ssh")),
        Some("credential_directory")
    );
    assert_eq!(
        sensitive_warning(Path::new("/home/user/.config")),
        Some("hidden_configuration_directory")
    );
    assert!(sensitive_warning(Path::new("/home/user/Documents")).is_none());
    #[cfg(unix)]
    assert_eq!(
        sensitive_warning(Path::new("/etc/passwd")),
        Some("system_directory")
    );
}
