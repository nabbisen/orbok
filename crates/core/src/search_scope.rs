//! Which files a search may return (RFC-060 §7).
//!
//! Plain data, shared by the query layer (`orbok-db` builds the SQL) and
//! the search engine (`orbok-search` fills it in from the UI's filters and
//! chosen folder). Both halves must agree, and a scope applied anywhere
//! but the query is not a scope: post-filtering shrinks the result set
//! below the requested limit and makes "no results" ambiguous
//! (RFC-041 §25.5).

/// One folder restriction: a registered source, optionally a subfolder of
/// it, and whether nested folders count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScope {
    pub source_id: String,
    /// Task 113: the canonical path of a folder **inside** the source, when
    /// the search is limited to that subfolder. `None` searches the source
    /// itself. A file is inside when this is a path-component prefix of its
    /// path (`orbok_core::folder_cover`); "folder only" is then counted from
    /// this folder, not from the source.
    pub limit_path: Option<String>,
    /// `false` restricts to files sitting directly in the folder (the
    /// source, or `limit_path` when it is set; RFC-045 §6.3's "folder only").
    pub include_subfolders: bool,
}

/// The scope of one search. Empty means unrestricted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchScope {
    /// Lowercased file extensions, without the dot. Empty means every
    /// kind; the UI's kind filters expand to these.
    pub extensions: Vec<String>,
    pub folder: Option<FolderScope>,
}

impl SearchScope {
    /// Whether this scope restricts anything at all.
    pub fn is_unrestricted(&self) -> bool {
        self.extensions.is_empty() && self.folder.is_none()
    }
}
