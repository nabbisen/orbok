//! Compiled source policy (RFC-003 §6, RFC-004 scanner inputs).
//!
//! Pattern semantics are deliberately simple and documented:
//! - an exclude pattern matches when it equals any path component
//!   (".git", "node_modules", "target") or, in `*.ext` form, the file
//!   name extension;
//! - include patterns apply to file names only, in `*.ext` form or as an
//!   exact name; an empty include list means "all supported types".

use orbok_core::{HiddenFilePolicy, SymlinkPolicy};
use orbok_db::repo::SourceRecord;
use std::path::Path;

/// Default exclude set (RFC-003 §6.3).
pub const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".cache",
    ".venv",
    "__pycache__",
];

/// File-type classification for scanner cataloging (RFC-005 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTypeClass {
    Supported,
    Unsupported,
}

/// Extensions of the initial supported formats (RFC-005 §5). Source-code
/// extensions cover the common cases; unknown ones catalog as
/// unsupported rather than failing.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    // text-oriented documents
    "txt", "log", "md", "markdown", "html", "htm", "pdf", "docx", "csv",
    // source code (line-aware text)
    "rs", "py", "js", "ts", "jsx", "tsx", "java", "c", "h", "cpp", "hpp", "go", "rb", "php", "sh",
    "bash", "sql", "toml", "yaml", "yml", "json", "xml", "css",
];

/// A source policy compiled for fast per-entry checks.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    pub hidden_file_policy: HiddenFilePolicy,
    pub symlink_policy: SymlinkPolicy,
    pub max_file_size_bytes: Option<u64>,
    include_extensions: Vec<String>,
    include_names: Vec<String>,
    exclude_components: Vec<String>,
    exclude_extensions: Vec<String>,
}

impl CompiledPolicy {
    /// Compile from a catalog source record. The default excludes are
    /// always active in addition to user excludes.
    pub fn from_source(source: &SourceRecord) -> Self {
        let mut include_extensions = Vec::new();
        let mut include_names = Vec::new();
        for pattern in &source.include_patterns {
            match pattern.strip_prefix("*.") {
                Some(ext) => include_extensions.push(ext.to_ascii_lowercase()),
                None => include_names.push(pattern.clone()),
            }
        }
        let mut exclude_components: Vec<String> =
            DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect();
        let mut exclude_extensions = Vec::new();
        for pattern in &source.exclude_patterns {
            match pattern.strip_prefix("*.") {
                Some(ext) => exclude_extensions.push(ext.to_ascii_lowercase()),
                None => exclude_components.push(pattern.clone()),
            }
        }
        Self {
            hidden_file_policy: source.hidden_file_policy,
            symlink_policy: source.symlink_policy,
            max_file_size_bytes: source.max_file_size_bytes,
            include_extensions,
            include_names,
            exclude_components,
            exclude_extensions,
        }
    }

    /// Whether a directory or file component is excluded by name.
    pub fn component_excluded(&self, name: &str) -> bool {
        self.exclude_components.iter().any(|p| p == name)
    }

    /// Whether a component is hidden (dotfile convention).
    pub fn component_hidden(name: &str) -> bool {
        name.starts_with('.')
    }

    /// Whether a file name passes the include/exclude pattern rules.
    pub fn file_included(&self, file_name: &str) -> bool {
        let ext = extension_of(file_name);
        if let Some(ext) = &ext
            && self.exclude_extensions.iter().any(|e| e == ext)
        {
            return false;
        }
        if self.component_excluded(file_name) {
            return false;
        }
        if self.include_extensions.is_empty() && self.include_names.is_empty() {
            return true;
        }
        if self.include_names.iter().any(|n| n == file_name) {
            return true;
        }
        match ext {
            Some(ext) => self.include_extensions.iter().any(|e| e == &ext),
            None => false,
        }
    }

    /// Whether a file size is within the policy limit.
    pub fn size_allowed(&self, size: u64) -> bool {
        match self.max_file_size_bytes {
            Some(max) => size <= max,
            None => true,
        }
    }
}

/// Supported/unsupported classification by extension (RFC-004 §10,
/// RFC-005 §5).
pub fn classify_file_type(path: &Path) -> FileTypeClass {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
    {
        Some(ext) if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) => FileTypeClass::Supported,
        _ => FileTypeClass::Unsupported,
    }
}

fn extension_of(file_name: &str) -> Option<String> {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
}
