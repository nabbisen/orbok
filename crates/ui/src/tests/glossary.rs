//! One glossary, read by one guard, instead of a bespoke test per drifted
//! term. Task 100 replaced three hand-written guards
//! (`default_ui_copy_avoids_forbidden_terms` and `every_exemption_is_load_bearing`
//! in `rfc041_search.rs`, `japanese_copy_spells_folder_one_way` in `i18n.rs`)
//! with this file, after finding that the search-method vocabulary and the
//! "your files are safe" promise had each drifted into four or five
//! spellings the same way RFC-041 §8.2's internal-vocabulary list once had.
//!
//! **The rule for what goes in:** a term earns a row when it has already
//! drifted once, or when it states a promise about the user's files,
//! privacy or data. Nothing else. This is not a glossary of the product's
//! vocabulary; it is a list of the words that have proved they need one.

use crate::i18n::{Locale, MessageKey, tr};

/// One concept, the word(s) to use for it, and the words that must not be
/// used instead.
struct GlossaryTerm {
    /// What this names, for the failure message: "the meaning method".
    concept: &'static str,
    /// The term to use, per locale, long form first. Empty for a row that
    /// only forbids -- RFC-041 §8.2's internal-vocabulary list has no
    /// user-facing replacement, just "don't say this".
    canonical: &'static [(Locale, &'static str)],
    /// Spellings that must not appear, per locale.
    forbidden: &'static [(Locale, &'static str)],
    /// For 「フォルダ」: forbidden unless immediately followed by this.
    allowed_if_followed_by: Option<&'static str>,
    /// `(key, forbidden term, why)` -- explicitly permitted exceptions.
    /// Checked for completeness by `every_glossary_exemption_is_load_bearing`
    /// below: an entry that stops matching (because the copy changed) must
    /// be removed in the same change, not left as a stale permission nobody
    /// re-examines.
    exemptions: &'static [(MessageKey, &'static str, &'static str)],
}

impl GlossaryTerm {
    fn canonical_forms(&self, locale: Locale) -> Vec<&'static str> {
        self.canonical
            .iter()
            .filter(|(l, _)| *l == locale)
            .map(|(_, s)| *s)
            .collect()
    }

    fn is_exempt(&self, key: MessageKey, term: &str) -> bool {
        self.exemptions
            .iter()
            .any(|&(k, t, _)| k == key && t == term)
    }

    fn matches(&self, locale: Locale, copy: &str, term: &str) -> bool {
        match self.allowed_if_followed_by {
            Some(suffix) => contains_not_followed_by(copy, term, suffix),
            None => contains_term(locale, copy, term),
        }
    }

    fn violations(&self, locale: Locale, key: MessageKey, copy: &str) -> Vec<String> {
        let mut out = Vec::new();
        for &(term_locale, term) in self.forbidden {
            if term_locale != locale || !self.matches(locale, copy, term) {
                continue;
            }
            if self.is_exempt(key, term) {
                continue;
            }
            let canon = self.canonical_forms(locale);
            let replacement = match canon.as_slice() {
                [] => format!("forbidden for \"{concept}\"", concept = self.concept),
                [long] => format!(
                    "forbidden for \"{concept}\"; use {long:?} instead",
                    concept = self.concept
                ),
                [long, short, ..] => format!(
                    "forbidden for \"{concept}\"; the term is {long:?} (short: {short:?})",
                    concept = self.concept
                ),
            };
            out.push(format!(
                "{locale:?} {key:?} says {copy:?} -- contains {term:?}, {replacement}"
            ));
        }
        out
    }
}

fn contains_term(locale: Locale, copy: &str, term: &str) -> bool {
    if locale == Locale::Ja {
        copy.contains(term)
    } else {
        copy.to_lowercase().contains(&term.to_lowercase())
    }
}

/// Task 066's フォルダー-vs-フォルダ scan: every occurrence of `term` not
/// immediately followed by `suffix`, including one at the end of `copy`.
fn contains_not_followed_by(copy: &str, term: &str, suffix: &str) -> bool {
    let mut rest = copy;
    while let Some(at) = rest.find(term) {
        let after = &rest[at + term.len()..];
        if !after.starts_with(suffix) {
            return true;
        }
        rest = after;
    }
    false
}

// ── Row 1: RFC-041 §8.2's internal-vocabulary list, carried over verbatim
// ──────────────────────────────────────────────────────────────────────
//
// Per-locale term lists, not one list run against both: RFC-041 §8.2's list
// is written in English, and the English substring "source" does not
// appear in a Japanese string that uses the equivalent concept -- the
// katakana loanword `ソース` or the kanji compound `情報源` does. Each
// Japanese term is one already used elsewhere in this catalog for that
// exact concept (verified by grep before use, not invented), so a real
// drift is what gets caught, not a translation choice this test happens to
// disagree with.

const RFC041_FORBIDDEN: &[(Locale, &str)] = &[
    (Locale::En, "source"),
    (Locale::En, "index"),
    (Locale::En, "catalog"),
    (Locale::En, "cache"),
    (Locale::En, "embedding"),
    (Locale::En, "vector"),
    (Locale::En, "bm25"),
    (Locale::En, "rrf"),
    (Locale::En, "chunk"),
    (Locale::En, "query"),
    (Locale::En, "schema"),
    (Locale::En, "engine"),
    (Locale::En, "backend"),
    // Task 050: the product had two names for one feature. "semantic" is a
    // sibling of every term above -- the gate was right about the class and
    // missed one member of it.
    (Locale::En, "semantic"),
    // RFC-041 §25 criterion 11 / §8.3: former project name.
    (Locale::En, "orbit"),
    (Locale::Ja, "ソース"),
    (Locale::Ja, "情報源"),
    (Locale::Ja, "インデックス"),
    (Locale::Ja, "索引"),
    (Locale::Ja, "カタログ"),
    (Locale::Ja, "キャッシュ"),
    (Locale::Ja, "埋め込み"),
    (Locale::Ja, "エンベディング"),
    (Locale::Ja, "ベクトル"),
    (Locale::Ja, "チャンク"),
    (Locale::Ja, "クエリ"),
    (Locale::Ja, "スキーマ"),
    (Locale::Ja, "エンジン"),
    (Locale::Ja, "バックエンド"),
    (Locale::Ja, "セマンティック"),
    (Locale::Ja, "BM25"),
    (Locale::Ja, "RRF"),
    (Locale::Ja, "オービット"),
];

const RFC041_EXEMPTIONS: &[(MessageKey, &str, &str)] = &[
    // Model *provenance* metadata ("Source: <model id>", alongside
    // "Provider:"/"Revision:" rows in the download-consent screen) -- a
    // different sense of "source" than the document/folder concept RFC-041
    // §8.2 targets. Both locales: en.rs's "Source", ja.rs's "ソース" name
    // the same field.
    (
        MessageKey::ModelConsentSource,
        "source",
        "model provenance metadata (\"Source: <model id>\"), a different sense of \
         \"source\" than the document/folder concept this row targets",
    ),
    (
        MessageKey::ModelConsentSource,
        "ソース",
        "model provenance metadata (\"Source: <model id>\"), a different sense of \
         \"source\" than the document/folder concept this row targets",
    ),
    // Task 081: RFC-011 §11's storage-category labels and the cache-file
    // line, shown only in Advanced view -- whose own copy already says
    // "Show technical detail in search results, preparation, and
    // storage." These *are* the technical detail: RFC-011 names these
    // categories with these exact terms, and a user who has turned
    // Advanced on has asked to see them. Ordinary Storage copy stays plain.
    (
        MessageKey::StorageCategoryPersistentCatalog,
        "catalog",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryPersistentCatalog,
        "カタログ",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryKeywordIndex,
        "index",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryKeywordIndex,
        "索引",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryVectorIndex,
        "index",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryVectorIndex,
        "vector",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryVectorIndex,
        "索引",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCategoryVectorIndex,
        "ベクトル",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCacheFileSize,
        "cache",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
    (
        MessageKey::StorageCacheFileSize,
        "キャッシュ",
        "RFC-011 §11's storage-category label, shown only in Advanced view, \
         whose own copy says it shows technical detail",
    ),
];

// ── Row 2/3: the two search methods (Task 100 §2) ──────────────────────

const KEYWORD_METHOD_FORBIDDEN: &[(Locale, &str)] = &[
    (Locale::En, "Basic search"),
    (Locale::En, "Exact"),
    (Locale::Ja, "基本検索"),
    (Locale::Ja, "完全一致"),
];

const KEYWORD_METHOD_EXEMPTIONS: &[(MessageKey, &str, &str)] = &[(
    MessageKey::ModelConsentExactSize,
    "Exact",
    "a different sense of \"exact\" -- download size, not the search method",
)];

const MEANING_METHOD_FORBIDDEN: &[(Locale, &str)] = &[
    (Locale::En, "Conceptual"),
    // Deliberately not "semantic search": "semantic" is already forbidden
    // by row 1, and no `tr()` value currently contains it.
    (Locale::Ja, "意味検索"),
];

// ── Row 4: the promise about the user's own files (Task 100 §3) ────────
//
// Not `SourceFilesNotDeletedNotice` (describes a state, not a promise) or
// `ModelFilesStayLocal` (a different promise: locality, not safety) --
// verified neither contains any of these retired sentences.

const FILES_PROMISE_FORBIDDEN: &[(Locale, &str)] = &[
    (Locale::En, "Your files are never deleted."),
    (Locale::En, "Your files are untouched."),
    (Locale::En, "Your files stay where they are."),
    (Locale::En, "Your files will not be deleted."),
    (Locale::Ja, "元のファイルが削除されることはありません。"),
    (Locale::Ja, "ファイルはそのままです。"),
    (Locale::Ja, "ファイルはそのまま残ります。"),
    (Locale::Ja, "あなたのファイルは削除されません。"),
];

// ── Row 5: 「フォルダー」, folded in from Task 066 ──────────────────────

const FOLDER_FORBIDDEN: &[(Locale, &str)] = &[(Locale::Ja, "フォルダ")];

const GLOSSARY: &[GlossaryTerm] = &[
    GlossaryTerm {
        concept: "an internal implementation term (RFC-041 §8.2)",
        canonical: &[],
        forbidden: RFC041_FORBIDDEN,
        allowed_if_followed_by: None,
        exemptions: RFC041_EXEMPTIONS,
    },
    GlossaryTerm {
        concept: "the keyword method",
        canonical: &[
            (Locale::En, "keyword search"),
            (Locale::En, "Keyword"),
            (Locale::Ja, "キーワード検索"),
            (Locale::Ja, "キーワード"),
        ],
        forbidden: KEYWORD_METHOD_FORBIDDEN,
        allowed_if_followed_by: None,
        exemptions: KEYWORD_METHOD_EXEMPTIONS,
    },
    GlossaryTerm {
        concept: "the meaning method",
        canonical: &[
            (Locale::En, "search by meaning"),
            (Locale::En, "By meaning"),
            (Locale::Ja, "意味による検索"),
            (Locale::Ja, "意味"),
        ],
        forbidden: MEANING_METHOD_FORBIDDEN,
        allowed_if_followed_by: None,
        exemptions: &[],
    },
    GlossaryTerm {
        concept: "the promise that files are safe",
        canonical: &[
            (Locale::En, "Your files are never changed or deleted."),
            (Locale::Ja, "ファイルは変更も削除もされません。"),
        ],
        forbidden: FILES_PROMISE_FORBIDDEN,
        allowed_if_followed_by: None,
        exemptions: &[],
    },
    GlossaryTerm {
        concept: "the Japanese word for \"folder\" (Task 066)",
        canonical: &[(Locale::Ja, "フォルダー")],
        forbidden: FOLDER_FORBIDDEN,
        allowed_if_followed_by: Some("ー"),
        exemptions: &[],
    },
];

#[test]
fn default_ui_copy_follows_the_glossary() {
    let mut violations = Vec::new();
    for &locale in Locale::ALL {
        for &key in crate::i18n::ALL_KEYS {
            let copy = tr(locale, key);
            for term in GLOSSARY {
                violations.extend(term.violations(locale, key, copy));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "\n{}\n{} violation(s) -- RFC-041 §8.2 / §25 criterion 8, RFC-045 §22 \
         criterion 12, Task 100 §2/§3. Either fix the copy or add a justified \
         entry to the row's `exemptions`.",
        violations.join("\n"),
        violations.len()
    );
}

/// Every glossary exemption must actually match something, in at least one
/// locale -- otherwise it is a permission nobody needs any more.
#[test]
fn every_glossary_exemption_is_load_bearing() {
    for term in GLOSSARY {
        for &(key, forbidden_term, _reason) in term.exemptions {
            let matches_somewhere = Locale::ALL
                .iter()
                .any(|&locale| term.matches(locale, tr(locale, key), forbidden_term));
            assert!(
                matches_somewhere,
                "exemption ({key:?}, {forbidden_term:?}) for \"{concept}\" does not \
                 match any locale's copy any more -- remove it",
                concept = term.concept
            );
        }
    }
}
