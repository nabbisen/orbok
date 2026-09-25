//! Compiled source policy (RFC-003 §6, RFC-004 scanner inputs).
//!
//! Pattern semantics are deliberately simple and documented:
//! - an exclude pattern (the folder's own, never a default) matches when it
//!   equals any path component or, in `*.ext` form, the file name extension;
//! - what is always skipped is decided by rule, not by a list of common
//!   words: hidden entries (Task 120, [`platform_hidden`]) and folders a tool
//!   generated ([`is_generated_folder`]);
//! - include patterns apply to file names only, in `*.ext` form or as an
//!   exact name; an empty include list means "all supported types".

use orbok_core::{HiddenFilePolicy, SymlinkPolicy};
use orbok_db::repo::SourceRecord;
use std::path::Path;

/// Task 120: the folders programming tools create for themselves. **The one
/// place** that names them and the project file each depends on.
///
/// - an empty project-file list: the name alone marks it (`node_modules` and
///   `__pycache__` are names tools own, never a person's documents);
/// - a project-file list: the folder is a tool's output only when one of these
///   files sits **beside** it (`target` next to `Cargo.toml`, `dist` and `build`
///   next to `package.json`). Without one, `build/` and `target/` are ordinary
///   words in a person's own folders and are prepared.
///
/// Any folder holding a valid `CACHEDIR.TAG` is also one (see
/// [`is_generated_folder`]). Nothing else is skipped by name, and `.git`,
/// `.cache` and `.venv` are not listed: the hidden rule already covers them.
const TOOL_OUTPUT_FOLDERS: &[(&str, &[&str])] = &[
    ("node_modules", &[]),
    ("__pycache__", &[]),
    ("target", &["Cargo.toml", "pom.xml"]),
    ("dist", &["package.json"]),
    ("build", &["package.json"]),
];

/// The Cache Directory Tagging Specification's file name and signature line.
const CACHEDIR_TAG: &str = "CACHEDIR.TAG";
const CACHEDIR_TAG_SIGNATURE: &str = "Signature: 8a477f597d28d172789f06886806bc55";

/// Task 120: whether `dir` is a folder a tool generated, by evidence rather
/// than by common words: it holds a `CACHEDIR.TAG` whose first line is the
/// standard signature, or it is one of [`TOOL_OUTPUT_FOLDERS`] (by name, or by
/// name with its project file beside it). The folder the user added is never
/// asked; the scanner asks it of what is inside.
pub fn is_generated_folder(dir: &Path) -> bool {
    if has_cachedir_tag(dir) {
        return true;
    }
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    TOOL_OUTPUT_FOLDERS.iter().any(|(folder, project_files)| {
        *folder == name
            && (project_files.is_empty()
                || dir.parent().is_some_and(|parent| {
                    project_files.iter().any(|file| parent.join(file).is_file())
                }))
    })
}

/// A `CACHEDIR.TAG` in `dir` whose first line is the standard signature.
fn has_cachedir_tag(dir: &Path) -> bool {
    use std::io::Read;
    let Ok(file) = std::fs::File::open(dir.join(CACHEDIR_TAG)) else {
        return false;
    };
    // The signature line is 43 bytes; a little more shows whether it ends there.
    let mut head = Vec::with_capacity(64);
    if file.take(64).read_to_end(&mut head).is_err() {
        return false;
    }
    let first_line = head.split(|b| *b == b'\n').next().unwrap_or(&[]);
    let first_line = first_line.strip_suffix(b"\r").unwrap_or(first_line);
    first_line == CACHEDIR_TAG_SIGNATURE.as_bytes()
}

/// Task 120: whether the platform itself hides this entry. Windows: the Hidden
/// or System attribute. macOS: the `UF_HIDDEN` flag (what hides `~/Library`).
/// Elsewhere the platform has no such mark and only the dot rule applies. Reads
/// the entry's own metadata (a link is judged as the link).
pub fn platform_hidden(entry: &std::fs::DirEntry) -> bool {
    platform_hidden_impl(entry)
}

#[cfg(windows)]
fn platform_hidden_impl(entry: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    entry
        .metadata()
        .is_ok_and(|m| m.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0)
}

#[cfg(target_os = "macos")]
fn platform_hidden_impl(entry: &std::fs::DirEntry) -> bool {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    entry
        .metadata()
        .is_ok_and(|m| m.st_flags() & UF_HIDDEN != 0)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_hidden_impl(_entry: &std::fs::DirEntry) -> bool {
    false
}

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
    /// Compile from a catalog source record. The hidden and generated-folder
    /// rules are always active in addition to the folder's own excludes.
    pub fn from_source(source: &SourceRecord) -> Self {
        let mut include_extensions = Vec::new();
        let mut include_names = Vec::new();
        for pattern in &source.include_patterns {
            match pattern.strip_prefix("*.") {
                Some(ext) => include_extensions.push(ext.to_ascii_lowercase()),
                None => include_names.push(pattern.clone()),
            }
        }
        let mut exclude_components: Vec<String> = Vec::new();
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
