//! Pure RFC-049 runtime-profile selection and path resolution.
//!
//! Slice 1 deliberately performs no filesystem or process-environment access.
//! The binary will supply the startup directory, platform locations, and the
//! optional `ORBOK_DATA_DIR` value when bootstrap propagation is implemented.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

const PORTABLE_DATA_DIR: &str = "orbok-data";
const SETTINGS_FILE: &str = "settings.json";
const DIAGNOSTICS_DIR: &str = "diagnostics";
const TEMPORARY_DIR: &str = "tmp";

/// The single profile selected for one process lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    Standard,
    Portable,
}

/// Pure command/environment inputs used to select a runtime profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSelection {
    mode: RuntimeMode,
    standard_data_override: Option<OsString>,
}

impl RuntimeSelection {
    /// Apply the RFC-049 precedence rule without reading process-global state.
    ///
    /// An absent or empty override is unset. A non-empty override conflicts
    /// with portable mode and fails before any profile path is authorized.
    pub fn resolve(
        portable: bool,
        data_dir_override: Option<OsString>,
    ) -> Result<Self, RuntimeContextError> {
        let standard_data_override = data_dir_override.filter(|value| !value.is_empty());

        if portable && standard_data_override.is_some() {
            return Err(RuntimeContextError::PortableOverrideConflict);
        }

        Ok(Self {
            mode: if portable {
                RuntimeMode::Portable
            } else {
                RuntimeMode::Standard
            },
            standard_data_override,
        })
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }
}

/// Platform paths captured before runtime context construction.
#[derive(Clone, Copy, Debug)]
pub struct PlatformRuntimePaths<'a> {
    /// Existing standard-mode data location, including the `orbok` component.
    pub standard_data_dir: Option<&'a Path>,
    /// Existing standard-mode settings directory, including the app component.
    pub standard_settings_dir: &'a Path,
}

/// Immutable paths for the one active runtime profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeContext {
    mode: RuntimeMode,
    startup_dir: PathBuf,
    data_dir: PathBuf,
    catalog_file: PathBuf,
    cache_file: PathBuf,
    models_dir: PathBuf,
    settings_file: PathBuf,
    diagnostics_dir: PathBuf,
    temporary_dir: PathBuf,
}

impl RuntimeContext {
    /// Resolve and freeze every runtime path without opening or probing it.
    pub fn resolve(
        selection: RuntimeSelection,
        startup_dir: &Path,
        platform: PlatformRuntimePaths<'_>,
    ) -> Result<Self, RuntimeContextError> {
        let startup_dir = normalize_absolute(startup_dir)
            .ok_or(RuntimeContextError::StartupDirectoryNotAbsolute)?;
        let portable_data_dir = startup_dir.join(PORTABLE_DATA_DIR);
        let standard_default_data_dir = platform
            .standard_data_dir
            .map(|path| anchor_and_normalize(&startup_dir, path))
            .transpose()?
            .unwrap_or_else(|| portable_data_dir.clone());
        let standard_settings_dir =
            anchor_and_normalize(&startup_dir, platform.standard_settings_dir)?;

        let data_dir = match selection.mode {
            RuntimeMode::Portable => {
                if paths_overlap(&portable_data_dir, &standard_default_data_dir)
                    || paths_overlap(&portable_data_dir, &standard_settings_dir)
                {
                    return Err(RuntimeContextError::ProfilePathAlias);
                }
                portable_data_dir
            }
            RuntimeMode::Standard => {
                if let Some(override_path) = selection.standard_data_override.as_deref() {
                    anchor_and_normalize(&startup_dir, Path::new(override_path))?
                } else {
                    standard_default_data_dir
                }
            }
        };

        let settings_dir = match selection.mode {
            RuntimeMode::Portable => data_dir.clone(),
            RuntimeMode::Standard => standard_settings_dir,
        };

        Ok(Self {
            mode: selection.mode,
            startup_dir,
            catalog_file: data_dir.join(orbok_db::CATALOG_FILE_NAME),
            cache_file: data_dir.join(orbok_db::CACHE_FILE_NAME),
            models_dir: data_dir.join("models"),
            settings_file: settings_dir.join(SETTINGS_FILE),
            diagnostics_dir: data_dir.join(DIAGNOSTICS_DIR),
            temporary_dir: data_dir.join(TEMPORARY_DIR),
            data_dir,
        })
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    pub fn startup_dir(&self) -> &Path {
        &self.startup_dir
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn catalog_file(&self) -> &Path {
        &self.catalog_file
    }

    pub fn cache_file(&self) -> &Path {
        &self.cache_file
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    pub fn settings_file(&self) -> &Path {
        &self.settings_file
    }

    pub fn diagnostics_dir(&self) -> &Path {
        &self.diagnostics_dir
    }

    pub fn temporary_dir(&self) -> &Path {
        &self.temporary_dir
    }

    pub fn path(&self, kind: RuntimePathKind) -> &Path {
        match kind {
            RuntimePathKind::Catalog => self.catalog_file(),
            RuntimePathKind::Cache => self.cache_file(),
            RuntimePathKind::Models => self.models_dir(),
            RuntimePathKind::Settings => self.settings_file(),
            RuntimePathKind::Recovery => self.data_dir(),
            RuntimePathKind::Diagnostics => self.diagnostics_dir(),
            RuntimePathKind::Temporary => self.temporary_dir(),
        }
    }
}

/// Persistent path classes that Slice 2 must route through the access seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimePathKind {
    Catalog,
    Cache,
    Models,
    Settings,
    Recovery,
    Diagnostics,
    Temporary,
}

/// Injectable observation/denial hook invoked immediately before an open or probe.
pub trait RuntimePathProbe {
    fn before_access(&self, kind: RuntimePathKind, path: &Path) -> io::Result<()>;
}

/// Production probe which permits the already-resolved active path.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllowRuntimePathProbe;

impl RuntimePathProbe for AllowRuntimePathProbe {
    fn before_access(&self, _kind: RuntimePathKind, _path: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// Binds access authorization to exactly one immutable runtime context.
pub struct RuntimeAccess<'a, P: ?Sized> {
    context: &'a RuntimeContext,
    probe: &'a P,
}

impl<'a, P: RuntimePathProbe + ?Sized> RuntimeAccess<'a, P> {
    pub fn new(context: &'a RuntimeContext, probe: &'a P) -> Self {
        Self { context, probe }
    }

    /// Authorize the active path for one persistent service.
    pub fn active_path(&self, kind: RuntimePathKind) -> io::Result<&'a Path> {
        let path = self.context.path(kind);
        self.probe.before_access(kind, path)?;
        Ok(path)
    }

    /// Reject a caller-supplied path unless it is the frozen active path.
    ///
    /// Slice 2 can use this when an existing API still accepts an explicit
    /// path, allowing tests to detect attempted inactive-profile access.
    pub fn authorize_path(
        &self,
        kind: RuntimePathKind,
        requested: &Path,
    ) -> Result<&'a Path, RuntimeAccessError> {
        let active = self.context.path(kind);
        if requested != active {
            return Err(RuntimeAccessError::InactiveProfilePath(kind));
        }
        self.probe
            .before_access(kind, active)
            .map_err(RuntimeAccessError::Probe)?;
        Ok(active)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeContextError {
    PortableOverrideConflict,
    ProfilePathAlias,
    StartupDirectoryNotAbsolute,
    ResolvedPathNotAbsolute,
}

impl fmt::Display for RuntimeContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortableOverrideConflict => formatter
                .write_str("portable mode conflicts with the non-empty ORBOK_DATA_DIR override"),
            Self::ProfilePathAlias => {
                formatter.write_str("portable and standard runtime profile paths overlap")
            }
            Self::StartupDirectoryNotAbsolute => {
                formatter.write_str("startup directory must be absolute")
            }
            Self::ResolvedPathNotAbsolute => {
                formatter.write_str("resolved runtime path must be absolute")
            }
        }
    }
}

impl std::error::Error for RuntimeContextError {}

#[derive(Debug)]
pub enum RuntimeAccessError {
    InactiveProfilePath(RuntimePathKind),
    Probe(io::Error),
}

impl fmt::Display for RuntimeAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InactiveProfilePath(kind) => {
                write!(
                    formatter,
                    "inactive runtime profile access denied for {kind:?}"
                )
            }
            Self::Probe(error) => write!(formatter, "runtime path access denied: {error}"),
        }
    }
}

impl std::error::Error for RuntimeAccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InactiveProfilePath(_) => None,
            Self::Probe(error) => Some(error),
        }
    }
}

fn anchor_and_normalize(anchor: &Path, path: &Path) -> Result<PathBuf, RuntimeContextError> {
    let anchored = if path.is_absolute() {
        path.to_path_buf()
    } else {
        anchor.join(path)
    };
    normalize_absolute(&anchored).ok_or(RuntimeContextError::ResolvedPathNotAbsolute)
}

pub fn paths_overlap(left: &Path, right: &Path) -> bool {
    paths_overlap_with_case(left, right, cfg!(any(windows, target_os = "macos")))
}

/// Target-aware whole-component containment used after physical resolution.
pub fn path_is_within(path: &Path, prefix: &Path) -> bool {
    components_start_with(path, prefix, cfg!(any(windows, target_os = "macos")))
}

fn paths_overlap_with_case(left: &Path, right: &Path, case_insensitive: bool) -> bool {
    components_start_with(left, right, case_insensitive)
        || components_start_with(right, left, case_insensitive)
}

fn components_start_with(path: &Path, prefix: &Path, case_insensitive: bool) -> bool {
    let mut path_components = path.components();
    prefix.components().all(|expected| {
        path_components
            .next()
            .is_some_and(|actual| components_equal(actual, expected, case_insensitive))
    })
}

fn components_equal(left: Component<'_>, right: Component<'_>, case_insensitive: bool) -> bool {
    if !case_insensitive {
        return left == right;
    }

    left.as_os_str().to_string_lossy().to_lowercase()
        == right.as_os_str().to_string_lossy().to_lowercase()
}

fn normalize_absolute(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
        }
    }
    Some(normalized)
}

#[cfg(test)]
mod tests;
