//! Platform durability primitives for RFC-050 managed model generations.
//!
//! This module is intentionally not wired into delivery/lifecycle call sites
//! until its target-specific behavior has completed independent review.

use std::path::Path;

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ModelDurabilityError {
    #[error("managed model durability path is invalid")]
    InvalidPath,
    #[error("managed model durability path escapes the managed root")]
    OutsideManagedRoot,
    #[error("managed model durability path crosses a reparse boundary")]
    ReparseBoundary,
    #[error("managed model durability storage is unsupported")]
    UnsupportedStorage,
    #[error("managed model durability rename crosses volumes")]
    CrossVolume,
    #[error("managed model durability destination already exists")]
    DestinationExists,
    #[error("managed model durability operation failed: {operation} (OS code {code:?})")]
    Os {
        operation: &'static str,
        code: Option<i32>,
    },
}

impl ModelDurabilityError {
    fn io_error(operation: &'static str, error: &std::io::Error) -> Self {
        Self::Os {
            operation,
            code: error.raw_os_error(),
        }
    }

    #[cfg(windows)]
    fn last_os_error(operation: &'static str) -> Self {
        Self::Os {
            operation,
            code: std::io::Error::last_os_error().raw_os_error(),
        }
    }
}

/// Validate that a managed-store root meets the platform durability contract.
#[allow(dead_code)]
pub(crate) fn preflight_managed_store(root: &Path) -> Result<(), ModelDurabilityError> {
    imp::preflight_managed_store(root)
}

/// Rename without replacement through the platform durability boundary.
#[allow(dead_code)]
pub(crate) fn durable_rename(
    managed_root: &Path,
    source: &Path,
    destination: &Path,
) -> Result<(), ModelDurabilityError> {
    imp::durable_rename(managed_root, source, destination)
}

#[cfg(unix)]
mod imp {
    use super::*;

    pub(super) fn preflight_managed_store(root: &Path) -> Result<(), ModelDurabilityError> {
        if !root.is_absolute() || !root.is_dir() {
            return Err(ModelDurabilityError::InvalidPath);
        }
        Ok(())
    }

    pub(super) fn durable_rename(
        managed_root: &Path,
        source: &Path,
        destination: &Path,
    ) -> Result<(), ModelDurabilityError> {
        preflight_managed_store(managed_root)?;
        if !source.is_absolute()
            || !destination.is_absolute()
            || source
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || destination
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || !source.starts_with(managed_root)
            || !destination.starts_with(managed_root)
        {
            return Err(ModelDurabilityError::OutsideManagedRoot);
        }
        if destination.exists() {
            return Err(ModelDurabilityError::DestinationExists);
        }
        std::fs::rename(source, destination)
            .map_err(|error| ModelDurabilityError::io_error("rename", &error))
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    use std::path::{Component, PathBuf, Prefix};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetDriveTypeW, GetFileAttributesW,
        GetVolumeInformationW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
        INVALID_FILE_ATTRIBUTES, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

    const WINDOWS_PATH_CAPACITY: usize = 32_768;

    #[derive(Debug, PartialEq, Eq)]
    struct VolumeIdentity {
        name: Vec<u16>,
    }

    pub(super) fn preflight_managed_store(root: &Path) -> Result<(), ModelDurabilityError> {
        validate_absolute_path(root)?;
        validate_existing_ancestors(root)?;
        let Some(attributes) = path_attributes(root)? else {
            return Err(ModelDurabilityError::InvalidPath);
        };
        if attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            return Err(ModelDurabilityError::InvalidPath);
        }
        supported_volume(root).map(|_| ())
    }

    pub(super) fn durable_rename(
        managed_root: &Path,
        source: &Path,
        destination: &Path,
    ) -> Result<(), ModelDurabilityError> {
        preflight_managed_store(managed_root)?;
        validate_managed_path(managed_root, source)?;
        validate_managed_path(managed_root, destination)?;
        validate_existing_ancestors(source)?;
        let destination_parent = destination
            .parent()
            .ok_or(ModelDurabilityError::InvalidPath)?;
        validate_existing_ancestors(destination_parent)?;
        let source_attributes = path_attributes(source)?;
        let destination_parent_attributes = path_attributes(destination_parent)?;
        if source_attributes.is_none()
            || !destination_parent_attributes
                .is_some_and(|attributes| attributes & FILE_ATTRIBUTE_DIRECTORY != 0)
        {
            return Err(ModelDurabilityError::InvalidPath);
        }
        if path_attributes(destination)?.is_some() {
            return Err(ModelDurabilityError::DestinationExists);
        }

        let source_volume = supported_volume(source)?;
        let destination_volume = supported_volume(destination_parent)?;
        ensure_same_volume(&source_volume, &destination_volume)?;

        let source_wide = extended_wide_path(source)?;
        let destination_wide = extended_wide_path(destination)?;
        // SAFETY: both buffers are NUL-terminated, remain alive for the call,
        // and the reviewed flag forbids replacement/copy/delayed semantics.
        let moved = unsafe {
            MoveFileExW(
                source_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        };
        if moved == 0 {
            return Err(ModelDurabilityError::last_os_error("MoveFileExW"));
        }
        Ok(())
    }

    fn validate_managed_path(managed_root: &Path, path: &Path) -> Result<(), ModelDurabilityError> {
        validate_absolute_path(path)?;
        if !path.starts_with(managed_root) {
            return Err(ModelDurabilityError::OutsideManagedRoot);
        }
        Ok(())
    }

    fn validate_absolute_path(path: &Path) -> Result<(), ModelDurabilityError> {
        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(ModelDurabilityError::InvalidPath);
        }
        match path.components().next() {
            Some(Component::Prefix(prefix))
                if matches!(
                    prefix.kind(),
                    Prefix::Disk(_)
                        | Prefix::UNC(_, _)
                        | Prefix::Verbatim(_)
                        | Prefix::VerbatimDisk(_)
                        | Prefix::VerbatimUNC(_, _)
                ) =>
            {
                Ok(())
            }
            _ => Err(ModelDurabilityError::InvalidPath),
        }
    }

    fn validate_existing_ancestors(path: &Path) -> Result<(), ModelDurabilityError> {
        walk_rooted_ancestors(path, |current| {
            let Some(attributes) = path_attributes(current)? else {
                return Ok(false);
            };
            if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(ModelDurabilityError::ReparseBoundary);
            }
            Ok(true)
        })
    }

    fn walk_rooted_ancestors(
        path: &Path,
        mut inspect: impl FnMut(&Path) -> Result<bool, ModelDurabilityError>,
    ) -> Result<(), ModelDurabilityError> {
        validate_absolute_path(path)?;
        let mut current = PathBuf::new();
        let mut root_complete = false;
        for component in path.components() {
            current.push(component.as_os_str());
            match component {
                Component::Prefix(_) => continue,
                Component::RootDir => root_complete = true,
                Component::Normal(_) if root_complete => {}
                _ => return Err(ModelDurabilityError::InvalidPath),
            }
            if !inspect(&current)? {
                return Ok(());
            }
        }
        if root_complete {
            Ok(())
        } else {
            Err(ModelDurabilityError::InvalidPath)
        }
    }

    fn path_attributes(path: &Path) -> Result<Option<u32>, ModelDurabilityError> {
        let path_wide = extended_wide_path(path)?;
        // SAFETY: path_wide is a live, NUL-terminated UTF-16 buffer.
        let attributes = unsafe { GetFileAttributesW(path_wide.as_ptr()) };
        if attributes != INVALID_FILE_ATTRIBUTES {
            return Ok(Some(attributes));
        }

        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(ModelDurabilityError::io_error("GetFileAttributesW", &error))
        }
    }

    fn supported_volume(path: &Path) -> Result<VolumeIdentity, ModelDurabilityError> {
        let path_wide = extended_wide_path(path)?;
        let mut volume_path = vec![0_u16; WINDOWS_PATH_CAPACITY];
        // SAFETY: input is NUL-terminated and output is a valid writable buffer.
        let found = unsafe {
            GetVolumePathNameW(
                path_wide.as_ptr(),
                volume_path.as_mut_ptr(),
                volume_path.len() as u32,
            )
        };
        if found == 0 {
            return Err(ModelDurabilityError::last_os_error("GetVolumePathNameW"));
        }
        truncate_at_nul(&mut volume_path)?;
        let mut volume_path_nul = volume_path.clone();
        volume_path_nul.push(0);

        // SAFETY: volume_path_nul is NUL-terminated.
        let drive_type = unsafe { GetDriveTypeW(volume_path_nul.as_ptr()) };

        let mut filesystem = vec![0_u16; 32];
        // SAFETY: all optional outputs are null and both path/buffer pointers are valid.
        let information = unsafe {
            GetVolumeInformationW(
                volume_path_nul.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            )
        };
        if information == 0 {
            return Err(ModelDurabilityError::last_os_error("GetVolumeInformationW"));
        }
        truncate_at_nul(&mut filesystem)?;
        let filesystem = OsString::from_wide(&filesystem)
            .to_string_lossy()
            .to_ascii_uppercase();
        validate_storage_kind(drive_type, &filesystem)?;

        let mut volume_name = vec![0_u16; WINDOWS_PATH_CAPACITY];
        // SAFETY: input is NUL-terminated and output is a valid writable buffer.
        let named = unsafe {
            GetVolumeNameForVolumeMountPointW(
                volume_path_nul.as_ptr(),
                volume_name.as_mut_ptr(),
                volume_name.len() as u32,
            )
        };
        if named == 0 {
            return Err(ModelDurabilityError::last_os_error(
                "GetVolumeNameForVolumeMountPointW",
            ));
        }
        truncate_at_nul(&mut volume_name)?;
        Ok(VolumeIdentity { name: volume_name })
    }

    fn ensure_same_volume(
        source: &VolumeIdentity,
        destination: &VolumeIdentity,
    ) -> Result<(), ModelDurabilityError> {
        if source == destination {
            Ok(())
        } else {
            Err(ModelDurabilityError::CrossVolume)
        }
    }

    fn validate_storage_kind(
        drive_type: u32,
        filesystem: &str,
    ) -> Result<(), ModelDurabilityError> {
        if drive_type == DRIVE_FIXED && matches!(filesystem, "NTFS" | "REFS") {
            Ok(())
        } else {
            Err(ModelDurabilityError::UnsupportedStorage)
        }
    }

    fn extended_wide_path(path: &Path) -> Result<Vec<u16>, ModelDurabilityError> {
        validate_absolute_path(path)?;
        let raw = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if raw.contains(&0) {
            return Err(ModelDurabilityError::InvalidPath);
        }
        let prefix = match path.components().next() {
            Some(Component::Prefix(prefix)) => prefix.kind(),
            _ => return Err(ModelDurabilityError::InvalidPath),
        };
        let mut extended = match prefix {
            Prefix::Disk(_) => OsStr::new(r"\\?\")
                .encode_wide()
                .chain(raw)
                .collect::<Vec<_>>(),
            Prefix::UNC(_, _) => OsStr::new(r"\\?\UNC\")
                .encode_wide()
                .chain(raw.into_iter().skip(2))
                .collect::<Vec<_>>(),
            Prefix::Verbatim(_) | Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(_, _) => raw,
            _ => return Err(ModelDurabilityError::InvalidPath),
        };
        extended.push(0);
        Ok(extended)
    }

    fn truncate_at_nul(buffer: &mut Vec<u16>) -> Result<(), ModelDurabilityError> {
        let length = buffer
            .iter()
            .position(|unit| *unit == 0)
            .ok_or(ModelDurabilityError::InvalidPath)?;
        buffer.truncate(length);
        Ok(())
    }

    #[cfg(test)]
    pub(super) mod test_support {
        use super::*;

        pub(crate) fn extended_path(path: &Path) -> Result<Vec<u16>, ModelDurabilityError> {
            extended_wide_path(path)
        }

        pub(crate) fn validate_absolute(path: &Path) -> Result<(), ModelDurabilityError> {
            validate_absolute_path(path)
        }

        pub(crate) fn reject_different_volumes() -> Result<(), ModelDurabilityError> {
            ensure_same_volume(
                &VolumeIdentity { name: vec![1] },
                &VolumeIdentity { name: vec![2] },
            )
        }

        pub(crate) fn validate_storage(
            drive_type: u32,
            filesystem: &str,
        ) -> Result<(), ModelDurabilityError> {
            validate_storage_kind(drive_type, filesystem)
        }

        pub(crate) fn ancestor_probe_paths(
            path: &Path,
        ) -> Result<Vec<PathBuf>, ModelDurabilityError> {
            let mut probes = Vec::new();
            walk_rooted_ancestors(path, |probe| {
                probes.push(probe.to_path_buf());
                Ok(true)
            })?;
            Ok(probes)
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use super::*;

    pub(super) fn preflight_managed_store(_root: &Path) -> Result<(), ModelDurabilityError> {
        Err(ModelDurabilityError::UnsupportedStorage)
    }

    pub(super) fn durable_rename(
        _managed_root: &Path,
        _source: &Path,
        _destination: &Path,
    ) -> Result<(), ModelDurabilityError> {
        Err(ModelDurabilityError::UnsupportedStorage)
    }
}

#[cfg(test)]
#[path = "model_durability/tests.rs"]
mod tests;
