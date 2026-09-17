//! Task 071: why a GUI startup failed, classified by the step that failed --
//! never by an error's message text.
//!
//! `main` shows the result in `orbok_ui::views::startup_failure`'s window.
//! `--check` keeps printing its own error and never reaches this type.

use orbok::runtime_context::RuntimeContext;
use orbok::runtime_storage::CatalogOpenError;
use orbok_core::OrbokError;
use orbok_ui::views::startup_failure::StartupFailureCause;
use std::error::Error;

#[derive(Debug)]
pub(crate) enum StartupFailure {
    /// Creating, authorising or opening the data folder failed.
    DataFolder {
        path: String,
        source: Box<dyn Error>,
    },
    /// The catalog was written by a newer orbok (RFC-062 §8 criterion 4).
    NewerData { source: OrbokError },
    /// Any other step: resolving the profile, opening or migrating the
    /// catalog, crash recovery, managed-model recovery, settings.
    Other { source: Box<dyn Error> },
}

impl StartupFailure {
    /// A step that creates, authorises or opens the data folder failed. The
    /// window names the resolved data folder.
    pub(crate) fn data_folder(context: &RuntimeContext, source: impl Into<Box<dyn Error>>) -> Self {
        Self::DataFolder {
            path: context.descriptor().to_string(),
            source: source.into(),
        }
    }

    pub(crate) fn other(source: impl Into<Box<dyn Error>>) -> Self {
        Self::Other {
            source: source.into(),
        }
    }

    /// Opening the catalog: the folder stage is a data-folder failure; in
    /// the file stage, only the typed newer-schema refusal is newer data.
    pub(crate) fn from_catalog_open(context: &RuntimeContext, error: CatalogOpenError) -> Self {
        match error {
            CatalogOpenError::DataFolder(error) => Self::data_folder(context, error),
            CatalogOpenError::Catalog(source @ OrbokError::SchemaVersionUnsupported { .. }) => {
                Self::NewerData { source }
            }
            CatalogOpenError::Catalog(source) => Self::other(source),
        }
    }

    /// The class alone, without the path, for the log line (RFC-061 §10
    /// criterion 4's evidence reads it): `DataFolder`, `NewerData` or
    /// `Other`.
    pub(crate) fn class(&self) -> &'static str {
        match self {
            Self::DataFolder { .. } => "DataFolder",
            Self::NewerData { .. } => "NewerData",
            Self::Other { .. } => "Other",
        }
    }

    /// What the window says.
    pub(crate) fn cause(&self) -> StartupFailureCause {
        match self {
            Self::DataFolder { path, .. } => StartupFailureCause::DataFolder { path: path.clone() },
            Self::NewerData { .. } => StartupFailureCause::NewerData,
            Self::Other { .. } => StartupFailureCause::Other,
        }
    }

    /// The underlying error, printed to stderr and logged exactly as a
    /// failed startup printed it before this window existed.
    pub(crate) fn source(&self) -> &dyn std::fmt::Debug {
        match self {
            Self::DataFolder { source, .. } | Self::Other { source } => source,
            Self::NewerData { source } => source,
        }
    }
}

#[cfg(test)]
mod tests;
