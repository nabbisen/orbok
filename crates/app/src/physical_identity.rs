//! Physical-identity comparison (RFC-049 §6 construction-time alias check).
//!
//! Moved into the library crate per Review 113 F2: `validate_physical_profile_separation`
//! previously lived in the binary crate and reached back into
//! `RuntimeContext` through a narrow `physical_alias_candidates()` exception.
//! Living here instead, alongside `RuntimeContext`, it can read the context's
//! ordinary `pub(crate)` path accessors directly, so that exception no
//! longer needs to exist. [`ProfileModelStore::contains`] reuses
//! [`physical_location`] for the same canonicalization-with-fallback
//! behavior, rather than duplicating it.
//!
//! This is a targeted move of one function group, not the deferred general
//! `bootstrap.rs` split (Correction Request 111 §8, D3), which stays
//! deferred.

use crate::runtime_context::{RuntimeContext, paths_overlap};
use std::path::{Path, PathBuf};

/// Reject a portable profile whose resolved or physical location overlaps
/// the standard profile's data or settings location.
pub fn validate_physical_profile_separation(
    context: &RuntimeContext,
    standard_data_dir: Option<&Path>,
    standard_settings_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let portable_paths = [
        context.data_dir().to_path_buf(),
        context.catalog_file().to_path_buf(),
        context.settings_file().to_path_buf(),
    ];
    let mut standard_paths = vec![
        standard_settings_dir.to_path_buf(),
        standard_settings_dir.join("settings.json"),
    ];
    if let Some(data_dir) = standard_data_dir {
        standard_paths.push(data_dir.to_path_buf());
        standard_paths.push(data_dir.join(orbok_db::CATALOG_FILE_NAME));
    }
    for portable_path in &portable_paths {
        let portable = physical_location(portable_path)?;
        for standard_path in &standard_paths {
            let standard = physical_location(standard_path)?;
            let canonical_overlap = paths_overlap(&portable.resolved_path, &standard.resolved_path);
            let identity_overlap = portable.identity == standard.identity
                && paths_overlap(&portable.missing_suffix, &standard.missing_suffix);
            if canonical_overlap || identity_overlap {
                return Err(
                    "portable and standard runtime profiles resolve to the same physical path"
                        .into(),
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PhysicalLocation {
    pub(crate) identity: FileIdentity,
    pub(crate) missing_suffix: PathBuf,
    pub(crate) resolved_path: PathBuf,
}

#[cfg(unix)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct FileIdentity {
    volume: u32,
    index: u64,
}

#[cfg(not(any(unix, windows)))]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct FileIdentity(PathBuf);

#[cfg(unix)]
fn file_identity(path: &Path) -> std::io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = std::fs::metadata(path)?;
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn file_identity(path: &Path) -> std::io::Result<FileIdentity> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
        OPEN_EXISTING,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated and remains alive for the call. The
    // returned handle is checked and closed on every subsequent path.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `handle` is valid and `information` points to writable storage
    // of the structure required by `GetFileInformationByHandle`.
    let result = unsafe { GetFileInformationByHandle(handle, &mut information) };
    let error = (result == 0).then(std::io::Error::last_os_error);
    // SAFETY: `handle` is an owned valid handle and is closed exactly once.
    unsafe { CloseHandle(handle) };
    if let Some(error) = error {
        return Err(error);
    }
    Ok(FileIdentity {
        volume: information.dwVolumeSerialNumber,
        index: (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    })
}

#[cfg(not(any(unix, windows)))]
fn file_identity(path: &Path) -> std::io::Result<FileIdentity> {
    Ok(FileIdentity(std::fs::canonicalize(path)?))
}

/// Resolve the nearest existing ancestor and retain both its filesystem object
/// identity and a policy-checked absent suffix. Identity catches bind mounts
/// and other aliases whose canonical names remain distinct.
pub(crate) fn physical_location(path: &Path) -> std::io::Result<PhysicalLocation> {
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match std::fs::canonicalize(existing) {
            Ok(mut resolved) => {
                let identity = file_identity(existing)?;
                let mut missing_suffix = PathBuf::new();
                for component in suffix.iter().rev() {
                    resolved.push(component);
                    missing_suffix.push(component);
                }
                validate_missing_suffix(&missing_suffix)?;
                return Ok(PhysicalLocation {
                    identity,
                    missing_suffix,
                    resolved_path: resolved,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    std::io::Error::new(error.kind(), "runtime path has no existing ancestor")
                })?;
                suffix.push(name.to_os_string());
                existing = existing.parent().ok_or_else(|| {
                    std::io::Error::new(error.kind(), "runtime path has no existing ancestor")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn validate_missing_suffix(suffix: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if !suffix.as_os_str().is_ascii() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "non-ASCII absent profile suffix cannot be identity-validated on macOS",
        ));
    }
    let _ = suffix;
    Ok(())
}
