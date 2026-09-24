//! Task 109 §1.5: every catalog key is read by some code that could show it.
//! A key nobody reads is copy nobody sees -- and copy the glossary and the
//! translators still spend their checks on.
//!
//! **Mechanism (as simple as the glossary's source scan):** read every `.rs`
//! file under `crates/` except the two catalogs and test code (the enum's own
//! file is read: its formatters name keys, and its key list does not spell
//! `MessageKey::`), and look for `MessageKey::<Name>` as a whole word. A key reached
//! through a `label_key()` match counts, because the match names it.
//! `use MessageKey::*` appears only in the two catalogs, so no bare name is
//! missed.
//!
//! **Limits, stated:** a key named only inside an inline `#[cfg(test)]`
//! module of a production file counts as referenced (the scan does not parse
//! modules); and "referenced" is not "rendered on a path a user reaches" --
//! that is what Task 107's audit did by hand.

use crate::i18n::{ALL_KEYS, MessageKey};
use std::path::{Path, PathBuf};

/// `(key, why)` -- keys deliberately unreferenced. Checked by
/// `every_unreferenced_key_exemption_is_load_bearing`, like the glossary's.
///
/// **These are not deleted on purpose** (Task 109 §6): an unread key is dead
/// text or a feature that was never connected, and only the owner can say
/// which. Each row says which RFC the copy belongs to. Deleting a row's key
/// (both locales) or building the feature removes the row.
const UNREFERENCED: &[(MessageKey, &str)] = &[
    (
        MessageKey::SearchNarrowResults,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchNarrowedBy,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchMoreWays,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchClearFilters,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchNoResultsFiltered,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchNoResultsFilteredBody,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchInThisFolder,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchShowNearby,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchShowSimilar,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::SearchResultsUpdating,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterKind,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterChanged,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterSearchIn,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterReadyStatus,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterKindPdfs,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterKindNotes,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterKindCode,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterKindDocuments,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterKindSpreadsheets,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterChangedToday,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterChangedThisWeek,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterChangedThisMonth,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterChangedAnyTime,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::FilterAllFolders,
        "RFC-041 (Accepted) narrow / browse-around copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsTitle,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsIntro,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsPreviewTitle,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsIncludedLabel,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsExcludedLabel,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsOptInFolderNames,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsOptInFolderNamesHint,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsOptInSearchWords,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsOptInSearchWordsHint,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
    (
        MessageKey::DiagnosticsShowFile,
        "RFC-040 (Accepted) support-bundle copy that nothing renders: unbuilt feature -- owner decision",
    ),
];

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "tests" && name != "target" {
                source_files(&path, out);
            }
        } else if name.ends_with(".rs")
            && name != "tests.rs"
            && !path.ends_with("i18n/en.rs")
            && !path.ends_with("i18n/ja.rs")
        {
            out.push(path);
        }
    }
}

fn production_source() -> String {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    source_files(&crates, &mut files);
    assert!(files.len() > 50, "found only {} source files", files.len());
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

fn referenced(source: &str, key: MessageKey) -> bool {
    let needle = format!("MessageKey::{key:?}");
    source.match_indices(&needle).any(|(at, _)| {
        !source[at + needle.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

fn unreferenced_keys() -> Vec<MessageKey> {
    let source = production_source();
    ALL_KEYS
        .iter()
        .copied()
        .filter(|&key| !referenced(&source, key))
        .collect()
}

#[test]
fn every_catalog_key_is_referenced_by_production_code() {
    let unlisted: Vec<_> = unreferenced_keys()
        .into_iter()
        .filter(|key| !UNREFERENCED.iter().any(|&(k, _)| k == *key))
        .collect();
    assert!(
        unlisted.is_empty(),
        "\n{} catalog key(s) no production code names: {unlisted:?}\n\
         Delete each (both locales), or show it. UNREFERENCED only shrinks.",
        unlisted.len()
    );
}

#[test]
fn every_unreferenced_key_exemption_is_load_bearing() {
    let unreferenced = unreferenced_keys();
    for &(key, why) in UNREFERENCED {
        assert!(
            unreferenced.contains(&key),
            "{key:?} ({why}) is referenced now -- remove it from UNREFERENCED"
        );
    }
}

/// `UNREFERENCED` only shrinks (the rule of `LEGACY-ALLOWLIST.txt`, made
/// mechanical): removing an entry lowers this number in the same commit, and
/// nothing can raise it without editing it here, where a reviewer sees it.
const UNREFERENCED_CEILING: usize = 34;

#[test]
fn the_unreferenced_list_only_shrinks() {
    assert!(
        UNREFERENCED.len() <= UNREFERENCED_CEILING,
        "UNREFERENCED has {} entries; the ceiling is {UNREFERENCED_CEILING}. Delete or show \
         the key instead of listing it.",
        UNREFERENCED.len()
    );
    assert_eq!(
        UNREFERENCED.len(),
        UNREFERENCED_CEILING,
        "an entry was removed: lower UNREFERENCED_CEILING to {} in the same commit",
        UNREFERENCED.len()
    );
}
