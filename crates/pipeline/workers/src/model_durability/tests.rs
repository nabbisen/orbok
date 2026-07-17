use super::*;

#[test]
fn io_error_mapping_preserves_the_authoritative_code_without_a_path() {
    let source = std::io::Error::from_raw_os_error(12_345);
    let error = ModelDurabilityError::io_error("test-io-error", &source);

    assert_eq!(
        error,
        ModelDurabilityError::Os {
            operation: "test-io-error",
            code: Some(12_345),
        }
    );
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("test-io-error"));
    assert!(diagnostic.contains("12345"));
    assert!(!diagnostic.contains(r"C:\"));
}

#[cfg(unix)]
#[test]
fn unix_rename_refuses_replacement_and_stays_under_managed_root() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&destination, b"destination").unwrap();

    assert_eq!(
        durable_rename(temp.path(), &source, &destination),
        Err(ModelDurabilityError::DestinationExists)
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"source");
    assert_eq!(std::fs::read(&destination).unwrap(), b"destination");

    let outside = temp.path().parent().unwrap().join("outside");
    assert_eq!(
        durable_rename(temp.path(), &source, &outside),
        Err(ModelDurabilityError::OutsideManagedRoot)
    );
    let traversal = temp.path().join("nested").join("..").join("escaped");
    assert_eq!(
        durable_rename(temp.path(), &source, &traversal),
        Err(ModelDurabilityError::OutsideManagedRoot)
    );
}

#[cfg(unix)]
#[test]
fn unix_rename_moves_to_a_new_managed_destination() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"durable").unwrap();

    durable_rename(temp.path(), &source, &destination).unwrap();

    assert!(!source.exists());
    assert_eq!(std::fs::read(destination).unwrap(), b"durable");
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::model_durability::imp::test_support::{
        ancestor_probe_paths, extended_path, reject_different_volumes, validate_absolute,
        validate_storage,
    };
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    use std::os::windows::fs::MetadataExt as _;
    use std::path::PathBuf;
    use std::process::Command;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    use windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOTE;

    fn verbatim_path(path: &Path) -> PathBuf {
        let mut wide = extended_path(path).unwrap();
        assert_eq!(wide.pop(), Some(0));
        PathBuf::from(OsString::from_wide(&wide))
    }

    #[test]
    fn extended_paths_preserve_unicode_drive_and_unc_forms() {
        let drive = PathBuf::from(r"C:\models\日本語\generation");
        let drive_wide = extended_path(&drive).unwrap();
        assert_eq!(
            OsString::from_wide(&drive_wide[..drive_wide.len() - 1]),
            OsString::from(r"\\?\C:\models\日本語\generation")
        );

        let unc = PathBuf::from(r"\\server\share\モデル");
        let unc_wide = extended_path(&unc).unwrap();
        assert_eq!(
            OsString::from_wide(&unc_wide[..unc_wide.len() - 1]),
            OsString::from(r"\\?\UNC\server\share\モデル")
        );
        assert_eq!(
            validate_storage(DRIVE_REMOTE, "NTFS"),
            Err(ModelDurabilityError::UnsupportedStorage),
            "a converted UNC path remains outside the supported local-volume policy"
        );
    }

    #[test]
    fn ancestor_probes_begin_only_after_the_verbatim_root_is_complete() {
        let probes = ancestor_probe_paths(Path::new(r"\\?\C:\models\generation")).unwrap();
        assert_eq!(probes.first(), Some(&PathBuf::from(r"\\?\C:\")));
        assert!(!probes.iter().any(|probe| probe == Path::new(r"\\?\C:")));
        assert_eq!(
            ancestor_probe_paths(Path::new(r"\\?\C:")),
            Err(ModelDurabilityError::InvalidPath)
        );
    }

    #[test]
    fn malformed_relative_and_interior_nul_paths_are_rejected() {
        assert_eq!(
            validate_absolute(Path::new(r"C:relative")),
            Err(ModelDurabilityError::InvalidPath)
        );
        assert_eq!(
            validate_absolute(Path::new(r"\root-relative")),
            Err(ModelDurabilityError::InvalidPath)
        );
        let with_nul = PathBuf::from(OsString::from_wide(&[
            b'C' as u16,
            b':' as u16,
            b'\\' as u16,
            b'a' as u16,
            0,
            b'b' as u16,
        ]));
        assert_eq!(
            extended_path(&with_nul),
            Err(ModelDurabilityError::InvalidPath)
        );
    }

    #[test]
    fn volume_identity_mismatch_fails_closed() {
        assert_eq!(
            reject_different_volumes(),
            Err(ModelDurabilityError::CrossVolume)
        );
    }

    #[test]
    fn storage_policy_accepts_only_fixed_ntfs_or_refs() {
        const DRIVE_FIXED: u32 = 3;
        assert_eq!(validate_storage(DRIVE_FIXED, "NTFS"), Ok(()));
        assert_eq!(validate_storage(DRIVE_FIXED, "REFS"), Ok(()));
        assert_eq!(
            validate_storage(DRIVE_FIXED, "FAT32"),
            Err(ModelDurabilityError::UnsupportedStorage)
        );
        assert_eq!(
            validate_storage(DRIVE_REMOTE, "NTFS"),
            Err(ModelDurabilityError::UnsupportedStorage)
        );
    }

    #[test]
    fn write_through_rename_handles_files_directories_and_existing_targets() {
        let temp = tempfile::tempdir().unwrap();
        preflight_managed_store(temp.path()).unwrap();

        let source_file = temp.path().join("source-file");
        let destination_file = temp.path().join("destination-file");
        std::fs::write(&source_file, b"file").unwrap();
        durable_rename(temp.path(), &source_file, &destination_file).unwrap();
        assert_eq!(std::fs::read(&destination_file).unwrap(), b"file");

        let source_dir = temp.path().join("source-dir");
        let destination_dir = temp.path().join("destination-dir");
        std::fs::create_dir(&source_dir).unwrap();
        std::fs::write(source_dir.join("payload"), b"directory").unwrap();
        durable_rename(temp.path(), &source_dir, &destination_dir).unwrap();
        assert_eq!(
            std::fs::read(destination_dir.join("payload")).unwrap(),
            b"directory"
        );

        let replacement_source = temp.path().join("replacement-source");
        std::fs::write(&replacement_source, b"source").unwrap();
        assert_eq!(
            durable_rename(temp.path(), &replacement_source, &destination_file),
            Err(ModelDurabilityError::DestinationExists)
        );
    }

    #[test]
    fn preflight_and_rename_accept_verbatim_paths_longer_than_max_path() {
        const MAX_PATH: usize = 260;
        let temp = tempfile::tempdir().unwrap();
        let verbatim_root = verbatim_path(temp.path());
        preflight_managed_store(&verbatim_root).unwrap();

        let mut managed = verbatim_root.join("long-root");
        while managed.as_os_str().encode_wide().count() <= MAX_PATH + 40 {
            managed.push("valid-component-0123456789abcdef");
        }
        assert!(managed.as_os_str().encode_wide().count() > MAX_PATH);
        std::fs::create_dir_all(&managed).unwrap();
        preflight_managed_store(&managed).unwrap();

        let source = managed.join("source");
        let destination = managed.join("destination");
        std::fs::write(&source, b"long-path").unwrap();
        durable_rename(&managed, &source, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"long-path");

        std::fs::remove_dir_all(&verbatim_root).unwrap();
    }

    #[test]
    fn non_elevated_junction_ancestor_is_rejected_as_a_reparse_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let junction = temp.path().join("junction");
        let managed = target.join("managed");
        std::fs::create_dir_all(&managed).unwrap();
        let output = Command::new("cmd")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(output.status.success(), "mklink /J failed");
        assert_ne!(
            std::fs::symlink_metadata(&junction)
                .unwrap()
                .file_attributes()
                & FILE_ATTRIBUTE_REPARSE_POINT,
            0
        );
        let verbatim_root = verbatim_path(temp.path());
        assert_eq!(
            preflight_managed_store(&verbatim_root.join("junction").join("managed")),
            Err(ModelDurabilityError::ReparseBoundary)
        );
    }
}
