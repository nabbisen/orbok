//! Application error types.
//!
//! Backend crates return typed errors; the UI maps them to i18n message
//! keys (RFC-031 §6 rule 2). Error categories follow the failure
//! taxonomies of RFC-004 §16 and RFC-005 §13.

use thiserror::Error;

/// Stable error categories recorded in the catalog
/// (`extraction_records.error_category`, `index_jobs.error_category`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    SourceMissing,
    PermissionDenied,
    PathCanonicalizationFailed,
    SymlinkPolicyBlocked,
    FileTooLarge,
    UnsupportedType,
    UnsupportedFormat,
    EncodingError,
    ParserError,
    EncryptedDocument,
    FileChangedDuringRead,
    ReadError,
    HashError,
    Timeout,
    OutOfMemory,
    Canceled,
    ModelUnavailable,
    ParserPanic,
    InternalError,
}

impl ErrorCategory {
    /// Stable catalog string (snake_case, as in RFC-004/RFC-005).
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::SourceMissing => "source_missing",
            ErrorCategory::PermissionDenied => "permission_denied",
            ErrorCategory::PathCanonicalizationFailed => "path_canonicalization_failed",
            ErrorCategory::SymlinkPolicyBlocked => "symlink_policy_blocked",
            ErrorCategory::FileTooLarge => "file_too_large",
            ErrorCategory::UnsupportedType => "unsupported_type",
            ErrorCategory::UnsupportedFormat => "unsupported_format",
            ErrorCategory::EncodingError => "encoding_error",
            ErrorCategory::ParserError => "parser_error",
            ErrorCategory::EncryptedDocument => "encrypted_document",
            ErrorCategory::FileChangedDuringRead => "file_changed_during_read",
            ErrorCategory::ReadError => "read_error",
            ErrorCategory::HashError => "hash_error",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::OutOfMemory => "out_of_memory",
            ErrorCategory::Canceled => "canceled",
            ErrorCategory::ModelUnavailable => "model_unavailable",
            ErrorCategory::ParserPanic => "parser_panic",
            ErrorCategory::InternalError => "internal_error",
        }
    }
}

/// Top-level orbok error.
///
/// Messages intentionally avoid document contents; paths appear only
/// where required for actionability (NFR-014 log hygiene).
#[derive(Debug, Error)]
pub enum OrbokError {
    #[error("database error: {0}")]
    Database(String),

    #[error("migration failed at version {version}: {message}")]
    MigrationFailed { version: i64, message: String },

    #[error("path is outside all active sources")]
    PathOutsideSources,

    #[error("path canonicalization failed: {0}")]
    PathCanonicalization(String),

    #[error("blocked by source policy: {0}")]
    PolicyBlocked(&'static str),

    #[error("source not found")]
    SourceNotFound,

    #[error("file not found in catalog")]
    FileNotFound,

    #[error("cleanup plan would touch persistent catalog data")]
    CleanupWouldTouchPersistentData,

    #[error("cache engine error: {0}")]
    Cache(String),

    #[error("extraction failed: {category:?}")]
    Extraction {
        category: ErrorCategory,
        message: String,
    },

    /// RFC-008 §15's embedding job failure categories, written directly as
    /// the literal spec'd strings (`"model_missing"`, `"inference_error"`,
    /// etc.) rather than through an intermediate enum like
    /// [`ErrorCategory`] — deliberately, following Task 013/Review 161:
    /// `ErrorCategory::ModelUnavailable` stringifies to
    /// `"model_unavailable"`, which does not match RFC-008 §15's own
    /// vocabulary, and reusing the extraction-shaped [`Extraction`] variant
    /// for an embedding-job failure would be semantically wrong. `category`
    /// is `&'static str` rather than `String`: every caller passes a
    /// literal from RFC-008 §15's named set, and `Scheduler::fail`
    /// (RFC-036 §20.1) matches directly on it to decide whether the
    /// category is retryable.
    #[error("embedding job failed: {category} — {message}")]
    Embedding {
        category: &'static str,
        message: String,
    },

    #[error("invalid value in catalog column {column}: {value}")]
    InvalidCatalogValue { column: &'static str, value: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation canceled")]
    Canceled,

    #[error("queue {queue} is full — backpressure active")]
    BackpressureActive { queue: String },
}

/// Convenience result alias.
pub type OrbokResult<T> = Result<T, OrbokError>;
