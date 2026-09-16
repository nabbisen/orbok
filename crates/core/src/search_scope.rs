//! Which files a search may return (RFC-060 §7).
//!
//! Plain data, shared by the query layer (`orbok-db` builds the SQL) and
//! the search engine (`orbok-search` fills it in from the UI's filters and
//! chosen folder). Both halves must agree, and a scope applied anywhere
//! but the query is not a scope: post-filtering shrinks the result set
//! below the requested limit and makes "no results" ambiguous
//! (RFC-041 §25.5).

/// One folder restriction: a registered source, and whether nested
/// folders count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderScope {
    pub source_id: String,
    /// `false` restricts to files sitting directly in the folder
    /// (RFC-045 §6.3's "folder only").
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
