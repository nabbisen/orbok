//! RFC-037 acceptance tests: Source Lifecycle, Refresh Policy, Change Detection.
//!
//! Covers §20.1 unit tests and §21 acceptance criteria.

use crate::source_lifecycle::{FileFingerprint, FileState, SourceState, check_source_path};

// ── §21.1: Source states are explicit ────────────────────────────────

#[test]
fn source_state_user_labels_are_plain() {
    let forbidden = [
        "source", "index", "watcher", "inode", "mtime", "hash", "reindex",
    ];
    for state in [
        SourceState::Active,
        SourceState::Preparing,
        SourceState::NeedsUpdate,
        SourceState::Paused,
        SourceState::FolderNotFound,
        SourceState::PermissionProblem,
        SourceState::Removed,
    ] {
        let label = state.user_label();
        for term in forbidden {
            assert!(
                !label.to_lowercase().contains(term),
                "SourceState::{state:?} label '{label}' contains forbidden term '{term}'"
            );
        }
    }
}

// ── §21.2: File states are explicit ──────────────────────────────────

#[test]
fn file_state_user_labels_are_plain() {
    let forbidden = ["index", "cache", "watcher", "inode", "mtime", "hash"];
    for state in [
        FileState::Discovered,
        FileState::Preparing,
        FileState::Ready,
        FileState::NeedsUpdate,
        FileState::PartlyPrepared,
        FileState::CouldNotPrepare,
        FileState::FileNotFound,
        FileState::Ignored,
        FileState::NoTextFound,
    ] {
        let label = state.user_label();
        for term in forbidden {
            assert!(
                !label.to_lowercase().contains(term),
                "FileState::{state:?} label '{label}' contains forbidden term '{term}'"
            );
        }
    }
}

// ── §21.7: Change detection maps catalog status to file state ─────────

#[test]
fn indexed_maps_to_ready() {
    assert_eq!(FileState::from_catalog_status("indexed"), FileState::Ready);
}

#[test]
fn stale_maps_to_needs_update() {
    assert_eq!(
        FileState::from_catalog_status("stale"),
        FileState::NeedsUpdate
    );
}

#[test]
fn missing_maps_to_file_not_found() {
    assert_eq!(
        FileState::from_catalog_status("missing"),
        FileState::FileNotFound
    );
}

#[test]
fn deleted_maps_to_file_not_found() {
    assert_eq!(
        FileState::from_catalog_status("deleted"),
        FileState::FileNotFound
    );
}

#[test]
fn permission_denied_maps_to_could_not_prepare() {
    assert_eq!(
        FileState::from_catalog_status("permission_denied"),
        FileState::CouldNotPrepare
    );
}

#[test]
fn unsupported_maps_to_ignored() {
    assert_eq!(
        FileState::from_catalog_status("unsupported"),
        FileState::Ignored
    );
}

/// Task 080: a file orbok read but found no text in maps to its own
/// state, distinct from `Ready` (0 chunks would otherwise be
/// indistinguishable from "prepared and searchable") and from
/// `Discovered` (nothing is pending -- every job succeeded).
#[test]
fn no_text_found_maps_to_its_own_state() {
    assert_eq!(
        FileState::from_catalog_status("no_text_found"),
        FileState::NoTextFound
    );
}

#[test]
fn discovered_maps_to_discovered() {
    assert_eq!(
        FileState::from_catalog_status("discovered"),
        FileState::Discovered
    );
}

// ── §21.5: Missing folders are recoverable ───────────────────────────

#[test]
fn nonexistent_path_gives_folder_not_found() {
    let state = check_source_path(std::path::Path::new("/nonexistent/path/does/not/exist"));
    assert_eq!(state, SourceState::FolderNotFound);
}

#[test]
fn existing_dir_gives_active() {
    let dir = tempfile::tempdir().unwrap();
    let state = check_source_path(dir.path());
    assert_eq!(state, SourceState::Active);
}

// ── §21.9: Live watcher not required ─────────────────────────────────

#[test]
fn source_state_is_searchable_for_active_and_degraded() {
    assert!(SourceState::Active.is_searchable());
    assert!(SourceState::NeedsUpdate.is_searchable());
    assert!(SourceState::Preparing.is_searchable());
    assert!(!SourceState::FolderNotFound.is_searchable());
    assert!(!SourceState::Removed.is_searchable());
}

// ── §11.1: File fingerprint metadata comparison ───────────────────────

#[test]
fn same_fingerprint_not_changed() {
    let a = FileFingerprint {
        size_bytes: 1024,
        modified_at: Some("1700000000".into()),
        content_hash: None,
    };
    let b = a.clone();
    assert!(!a.metadata_changed(&b));
}

#[test]
fn different_size_is_changed() {
    let a = FileFingerprint {
        size_bytes: 1024,
        modified_at: Some("1700000000".into()),
        content_hash: None,
    };
    let b = FileFingerprint {
        size_bytes: 2048,
        modified_at: Some("1700000000".into()),
        content_hash: None,
    };
    assert!(a.metadata_changed(&b));
}

#[test]
fn different_mtime_is_changed() {
    let a = FileFingerprint {
        size_bytes: 1024,
        modified_at: Some("1700000000".into()),
        content_hash: None,
    };
    let b = FileFingerprint {
        size_bytes: 1024,
        modified_at: Some("1700001234".into()),
        content_hash: None,
    };
    assert!(a.metadata_changed(&b));
}

// ── §21.10: User labels avoid internal terms ─────────────────────────

#[test]
fn source_can_refresh_active_states() {
    assert!(SourceState::Active.can_refresh());
    assert!(SourceState::NeedsUpdate.can_refresh());
    assert!(SourceState::FolderNotFound.can_refresh());
    assert!(SourceState::PermissionProblem.can_refresh());
    assert!(!SourceState::Removed.can_refresh());
    assert!(!SourceState::Paused.can_refresh());
}
