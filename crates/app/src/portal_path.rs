//! Task 139: resolve a document-portal path (`/run/user/<uid>/doc/<id>/…`
//! or `/run/flatpak/doc/<id>/…`) the system folder picker can return, back
//! to the real filesystem path -- when orbok can already read it there.
//!
//! Review 317 §4 measured this for real, inside the sandbox
//! (`packaging/flatpak/test/`): even a folder already under the home
//! folder, which `--filesystem=home:ro` already grants read access to,
//! came back from the FileChooser portal as a document-portal path, not
//! the real one. Left unresolved, RFC-064's canonical-path overlap check
//! would see the document-portal mount's own path, not the real one -- a
//! folder added by typed path and the same folder picked would not merge
//! into one, and a picked subfolder of an already-added folder would not
//! be recognised as one either.
//!
//! **The parsing is pure** ([`parse_document_portal_path`]) and tested
//! without D-Bus. **The one D-Bus call** (`GetHostPaths`, Documents
//! portal interface version 5+) sits behind [`DocumentPortal`], a small
//! trait, so a unit test can give a fixed answer -- the real
//! implementation, [`RealDocumentPortal`], is the only one `router.rs`'s
//! real picker ever uses.
//!
//! No "am I in a Flatpak" switch anywhere here: the rule is about the
//! path itself (whether it is under a document-portal mount), so it
//! covers any other portal-based sandbox the same way, and does nothing
//! at all on a system with no document portal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A path under a document-portal mount, split into the document id (the
/// portal's one grant for the user's pick) and whatever path components
/// the pick added below it. The mount always shows the granted item's own
/// basename right after the id (`<mount>/<id>/<basename>`, matching what
/// `GetHostPaths` itself returns, which already ends in that basename --
/// measured for real, Review 317 §4), so that one component is discarded,
/// not kept: `remainder` holds only what comes *after* it, a descendant
/// of the granted folder the pick went on to choose (Task 139 §3's
/// "components below the entry": `…/doc/<id>/docs/sub` → `<host>/sub`,
/// `docs` discarded as the placeholder, `sub` kept).
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentPortalRef {
    doc_id: String,
    remainder: PathBuf,
}

/// Strips a document-portal mount prefix, if `path` has one, leaving
/// whatever comes after it (the document id and any remainder). Two
/// forms are recognised, both measured for real (Review 317 §4): `/run/
/// flatpak/doc/…`, how the mount reads from inside a Flatpak sandbox,
/// and `/run/user/<uid>/doc/…`, how the same grant reads from outside
/// one (any sandbox that does not remap `/run`, or no sandbox at all).
/// Neither is derived from `$XDG_RUNTIME_DIR` or a `GetMountPoint` call:
/// the Flatpak-internal form never varies by uid, and parsing a path
/// should not need a D-Bus round trip of its own.
fn strip_document_portal_mount(path: &Path) -> Option<&Path> {
    if let Ok(rest) = path.strip_prefix("/run/flatpak/doc") {
        return Some(rest);
    }
    let mut components = path.components();
    if components.next() != Some(std::path::Component::RootDir) {
        return None;
    }
    if components.next()?.as_os_str() != "run" {
        return None;
    }
    if components.next()?.as_os_str() != "user" {
        return None;
    }
    let uid = components.next()?.as_os_str().to_str()?;
    if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if components.next()?.as_os_str() != "doc" {
        return None;
    }
    Some(components.as_path())
}

/// `None` when `path` is not under a document-portal mount at all -- the
/// case on any unsandboxed system, and the common case even inside one
/// (most of what orbok touches never goes through the picker). `Some`
/// splits it into the document id and the remainder below it, as
/// [`DocumentPortalRef`] documents.
fn parse_document_portal_path(path: &Path) -> Option<DocumentPortalRef> {
    let rest = strip_document_portal_mount(path)?;
    let mut components = rest.components();
    let doc_id = components.next()?.as_os_str().to_str()?.to_string();
    if doc_id.is_empty() {
        return None;
    }
    // The mount's own basename placeholder for the granted item -- see
    // this struct's own doc comment for why it is skipped, not kept.
    components.next();
    Some(DocumentPortalRef {
        doc_id,
        remainder: components.as_path().to_path_buf(),
    })
}

/// `true` only when `path` both exists as a directory and can actually be
/// listed -- `metadata` alone would pass for a directory orbok has no
/// permission to read into, which is exactly the case Task 139 §1 asks
/// to fall back from, not use.
fn is_readable_dir(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::read_dir(path).is_ok(),
        _ => false,
    }
}

/// What resolving a document-portal path needs from the Documents portal:
/// the real host path for one document id, or `None` for every "the
/// portal did not help" outcome this module never treats as a failure --
/// no session, an interface older than version 5, or the call itself
/// failing. [`RealDocumentPortal`] is the one implementation a real pick
/// ever uses; tests give a fixed [`HashMap`] instead.
pub(crate) trait DocumentPortal {
    async fn host_paths(&self, doc_id: &str) -> Option<HashMap<String, PathBuf>>;
}

/// The real Documents portal, called over the session bus exactly as
/// documented: `org.freedesktop.portal.Documents`'s `GetHostPaths(IN as
/// doc_ids, OUT a{say} paths)`, callable inside the sandbox for documents
/// the app may read. Confirmed against the running portal on this
/// machine before writing this: service `org.freedesktop.portal.
/// Documents`, object path `/org/freedesktop/portal/documents`, `version`
/// property `5`; `GetHostPaths`'s own byte-array values are NUL-
/// terminated (also confirmed directly, against `GetMountPoint`'s own
/// `ay` output) -- the trailing `0` is stripped before the bytes become a
/// `PathBuf`, or the portal's own padding would end up inside it.
pub(crate) struct RealDocumentPortal;

impl DocumentPortal for RealDocumentPortal {
    #[cfg(target_os = "linux")]
    async fn host_paths(&self, doc_id: &str) -> Option<HashMap<String, PathBuf>> {
        use std::os::unix::ffi::OsStringExt;

        let connection = zbus::Connection::session().await.ok()?;
        let proxy = zbus::Proxy::new(
            &connection,
            "org.freedesktop.portal.Documents",
            "/org/freedesktop/portal/documents",
            "org.freedesktop.portal.Documents",
        )
        .await
        .ok()?;
        let version: u32 = proxy.get_property("version").await.ok()?;
        if version < 5 {
            return None;
        }
        let raw: HashMap<String, Vec<u8>> =
            proxy.call("GetHostPaths", &(vec![doc_id],)).await.ok()?;
        Some(
            raw.into_iter()
                .map(|(id, mut bytes)| {
                    if bytes.last() == Some(&0) {
                        bytes.pop();
                    }
                    (id, PathBuf::from(std::ffi::OsString::from_vec(bytes)))
                })
                .collect(),
        )
    }

    #[cfg(not(target_os = "linux"))]
    async fn host_paths(&self, _doc_id: &str) -> Option<HashMap<String, PathBuf>> {
        // The picker never returns a document-portal path on Windows or
        // macOS -- `parse_document_portal_path` already returns `None`
        // for every path there, so this is never actually called, only
        // kept so the type checks on every platform.
        None
    }
}

/// Resolves what the folder picker returned, Task 139's whole point:
/// **never fails the pick.** Every path that is not under a document-
/// portal mount passes through unchanged (`portal` is not even asked) --
/// the only case outside a sandbox, and the common case inside one too.
/// Under one, the portal is asked for the real host path, and it is used
/// only once two things both hold: the portal actually answered for this
/// document id, and the resulting path is one orbok can read into here
/// and now. Any other outcome keeps the document-portal path the picker
/// already returned, logged at `debug` so each case is visible without a
/// sandbox session to hand -- never surfaced to the user, who asked to
/// choose a folder and got one.
pub(crate) async fn resolve_portal_path(path: PathBuf, portal: &impl DocumentPortal) -> PathBuf {
    let Some(parsed) = parse_document_portal_path(&path) else {
        return path;
    };
    let Some(mut answers) = portal.host_paths(&parsed.doc_id).await else {
        tracing::debug!(
            path = %path.display(),
            "document portal did not answer (no session, too old, or the call failed) -- keeping the document-portal path"
        );
        return path;
    };
    let Some(host_path) = answers.remove(&parsed.doc_id) else {
        tracing::debug!(
            path = %path.display(),
            doc_id = %parsed.doc_id,
            "document portal gave no host path for this document -- keeping the document-portal path"
        );
        return path;
    };
    let resolved = if parsed.remainder.as_os_str().is_empty() {
        host_path
    } else {
        host_path.join(&parsed.remainder)
    };
    if is_readable_dir(&resolved) {
        resolved
    } else {
        tracing::debug!(
            path = %path.display(),
            host_path = %resolved.display(),
            "document portal's host path is not readable here -- keeping the document-portal path"
        );
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A portal that must never be asked -- used for every case where
    /// `path` is not under a document-portal mount at all, so `resolve_
    /// portal_path` must return before touching `portal` in any way.
    struct PanicsIfAsked;
    impl DocumentPortal for PanicsIfAsked {
        async fn host_paths(&self, _doc_id: &str) -> Option<HashMap<String, PathBuf>> {
            panic!("the portal must never be asked for a path that is not under its own mount")
        }
    }

    struct FixedAnswer(Option<HashMap<String, PathBuf>>);
    impl DocumentPortal for FixedAnswer {
        async fn host_paths(&self, _doc_id: &str) -> Option<HashMap<String, PathBuf>> {
            self.0.clone()
        }
    }

    fn fixed(pairs: &[(&str, &str)]) -> FixedAnswer {
        FixedAnswer(Some(
            pairs
                .iter()
                .map(|(id, p)| ((*id).to_string(), PathBuf::from(p)))
                .collect(),
        ))
    }

    // ── Parsing: pure, no D-Bus, no filesystem ───────────────────────

    #[test]
    fn an_ordinary_path_is_not_a_document_portal_path() {
        assert_eq!(
            parse_document_portal_path(Path::new("/home/alice/Documents")),
            None
        );
    }

    #[test]
    fn a_relative_path_is_not_a_document_portal_path() {
        assert_eq!(
            parse_document_portal_path(Path::new("run/user/1000/doc/abc")),
            None
        );
    }

    #[test]
    fn the_flatpak_internal_mount_parses_the_id() {
        assert_eq!(
            parse_document_portal_path(Path::new("/run/flatpak/doc/abc123/Documents")),
            Some(DocumentPortalRef {
                doc_id: "abc123".to_string(),
                remainder: PathBuf::new(),
            })
        );
    }

    #[test]
    fn the_xdg_runtime_mount_parses_the_id_for_any_uid() {
        assert_eq!(
            parse_document_portal_path(Path::new("/run/user/1000/doc/abc123/Documents")),
            Some(DocumentPortalRef {
                doc_id: "abc123".to_string(),
                remainder: PathBuf::new(),
            })
        );
        assert_eq!(
            parse_document_portal_path(Path::new("/run/user/0/doc/xyz")),
            Some(DocumentPortalRef {
                doc_id: "xyz".to_string(),
                remainder: PathBuf::new(),
            })
        );
    }

    #[test]
    fn a_non_numeric_uid_segment_does_not_match() {
        assert_eq!(
            parse_document_portal_path(Path::new("/run/user/not-a-uid/doc/abc")),
            None
        );
    }

    #[test]
    fn components_below_the_entry_are_kept_as_the_remainder() {
        assert_eq!(
            parse_document_portal_path(Path::new("/run/flatpak/doc/abc123/Documents/sub/deeper")),
            Some(DocumentPortalRef {
                doc_id: "abc123".to_string(),
                remainder: PathBuf::from("sub/deeper"),
            })
        );
    }

    #[test]
    fn the_bare_mount_with_no_id_is_not_a_document_portal_path() {
        assert_eq!(
            parse_document_portal_path(Path::new("/run/flatpak/doc")),
            None
        );
        assert_eq!(
            parse_document_portal_path(Path::new("/run/flatpak/doc/")),
            None
        );
    }

    // ── Resolution: red first, then every "did not help" case ───────

    /// Red first: fails until `resolve_portal_path` and `DocumentPortal`
    /// exist. A fixed answer for a readable host path is used.
    #[tokio::test]
    async fn a_document_portal_path_with_a_readable_host_path_resolves_to_it() {
        let real_dir = tempfile::tempdir().unwrap();
        let path = PathBuf::from("/run/flatpak/doc/abc123/docs-home");
        let portal = fixed(&[("abc123", real_dir.path().to_str().unwrap())]);
        let resolved = resolve_portal_path(path, &portal).await;
        assert_eq!(resolved, real_dir.path());
    }

    #[tokio::test]
    async fn components_below_the_entry_are_rejoined_onto_the_host_path() {
        let real_dir = tempfile::tempdir().unwrap();
        let sub = real_dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let path = PathBuf::from("/run/flatpak/doc/abc123/Documents/sub");
        let portal = fixed(&[("abc123", real_dir.path().to_str().unwrap())]);
        let resolved = resolve_portal_path(path, &portal).await;
        assert_eq!(resolved, sub);
    }

    #[tokio::test]
    async fn a_path_outside_any_document_portal_mount_is_unchanged() {
        let path = PathBuf::from("/home/alice/Documents");
        let resolved = resolve_portal_path(path.clone(), &PanicsIfAsked).await;
        assert_eq!(resolved, path);
    }

    #[tokio::test]
    async fn the_portal_not_answering_keeps_the_document_path() {
        let path = PathBuf::from("/run/flatpak/doc/abc123/Documents");
        let portal = FixedAnswer(None);
        let resolved = resolve_portal_path(path.clone(), &portal).await;
        assert_eq!(resolved, path);
    }

    #[tokio::test]
    async fn no_host_path_for_this_document_id_keeps_the_document_path() {
        let path = PathBuf::from("/run/flatpak/doc/abc123/Documents");
        // Answers, but not for the id that was asked about.
        let portal = fixed(&[("someone-elses-id", "/tmp")]);
        let resolved = resolve_portal_path(path.clone(), &portal).await;
        assert_eq!(resolved, path);
    }

    #[tokio::test]
    async fn an_unreadable_host_path_keeps_the_document_path() {
        let path = PathBuf::from("/run/flatpak/doc/abc123/Documents");
        let portal = fixed(&[("abc123", "/this/does/not/exist/anywhere")]);
        let resolved = resolve_portal_path(path.clone(), &portal).await;
        assert_eq!(resolved, path);
    }

    #[tokio::test]
    async fn a_host_path_that_is_a_file_not_a_directory_keeps_the_document_path() {
        let real_dir = tempfile::tempdir().unwrap();
        let file = real_dir.path().join("not-a-folder.txt");
        std::fs::write(&file, b"hello").unwrap();
        let path = PathBuf::from("/run/flatpak/doc/abc123/not-a-folder.txt");
        let portal = fixed(&[("abc123", file.to_str().unwrap())]);
        let resolved = resolve_portal_path(path.clone(), &portal).await;
        assert_eq!(resolved, path);
    }
}
