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
    /// Task 101: whether the user guide (`docs/src/users/`) is scanned for
    /// this row's forbidden terms too. Set only on rows that name a
    /// canonical term -- never on row 1, whose jargon ban does not apply to
    /// docs (explaining what an embedding model is, is what user docs are
    /// for). Maintainer docs are never scanned: they discuss the code,
    /// where "semantic" and "exact" are real technical words.
    applies_to_docs: bool,
    /// Task 101: spellings forbidden in the user guide *in addition to*
    /// `forbidden`, for a word another row already owns in the catalogs
    /// (row 1 bans "semantic" there; repeating it in `forbidden` would make
    /// two rows own one word). Only read when `applies_to_docs` is set.
    doc_forbidden: &'static [(Locale, &'static str)],
    /// Task 101: `(file relative to docs/src/users/, term, why)` -- the docs
    /// counterpart of `exemptions`, and covered by the same load-bearing
    /// check.
    doc_exemptions: &'static [(&'static str, &'static str, &'static str)],
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

impl GlossaryTerm {
    fn doc_term_hits(&self, prose: &str, locale: Locale, term: &str) -> bool {
        match (locale, self.allowed_if_followed_by) {
            (Locale::Ja, Some(suffix)) => contains_not_followed_by(prose, term, suffix),
            (Locale::Ja, None) => prose.contains(term),
            (Locale::En, _) => contains_word(prose, term),
        }
    }

    fn doc_is_exempt(&self, file: &str, term: &str) -> bool {
        self.doc_exemptions
            .iter()
            .any(|&(f, t, _)| f == file && t == term)
    }

    /// Task 101: one user-guide file's violations of this row. `prose` is
    /// the file with code spans and fenced blocks already removed
    /// (`prose_only`). English terms match case-sensitively as whole words
    /// -- "Exact" the mode name, not "exactly" or "an exact term" -- since
    /// prose, unlike a catalog value, is full of ordinary words that merely
    /// contain a banned one. Japanese terms keep the catalog's plain
    /// substring rule.
    fn doc_violations(&self, file: &str, prose: &str) -> Vec<String> {
        let mut out = Vec::new();
        if !self.applies_to_docs {
            return out;
        }
        for &(locale, term) in self.forbidden.iter().chain(self.doc_forbidden) {
            if !self.doc_term_hits(prose, locale, term) || self.doc_is_exempt(file, term) {
                continue;
            }
            let canon = self.canonical_forms(locale);
            let replacement = match canon.as_slice() {
                [] => format!("forbidden for \"{}\"", self.concept),
                [long] => format!("forbidden for \"{}\"; use {long:?} instead", self.concept),
                [long, short, ..] => format!(
                    "forbidden for \"{}\"; the term is {long:?} (short: {short:?})",
                    self.concept
                ),
            };
            out.push(format!(
                "docs/src/users/{file} contains {term:?}, {replacement}"
            ));
        }
        out
    }
}

/// Task 101: `markdown` with fenced code blocks and inline code spans
/// removed, so a config value or identifier in backticks is never mistaken
/// for a vocabulary violation. A fence is a line starting with three
/// backticks (toggling); an inline span is text between two single
/// backticks on one line. Removed text is replaced by a space so words on
/// either side do not fuse.
fn prose_only(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut in_span = false;
        for ch in line.chars() {
            if ch == '`' {
                in_span = !in_span;
                out.push(' ');
            } else if !in_span {
                out.push(ch);
            }
        }
        out.push('\n');
    }
    out
}

/// Case-sensitive whole-word match: `term` at a position where the
/// character before and after (if any) is not alphanumeric. Multi-word
/// terms ("Basic search") match as a unit.
fn contains_word(text: &str, term: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = text[from..].find(term) {
        let start = from + rel;
        let end = start + term.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        from = start + term.chars().next().map_or(1, char::len_utf8);
    }
    false
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
        applies_to_docs: false,
        doc_forbidden: &[],
        doc_exemptions: &[],
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
        applies_to_docs: true,
        doc_forbidden: &[],
        doc_exemptions: &[],
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
        applies_to_docs: true,
        // Row 1 bans "semantic" in the catalogs; the user guide is not under
        // row 1, so the meaning row carries it for docs only.
        doc_forbidden: &[(Locale::En, "semantic"), (Locale::En, "Semantic")],
        doc_exemptions: &[],
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
        applies_to_docs: true,
        doc_forbidden: &[],
        doc_exemptions: &[],
    },
    GlossaryTerm {
        concept: "the Japanese word for \"folder\" (Task 066)",
        canonical: &[(Locale::Ja, "フォルダー")],
        forbidden: FOLDER_FORBIDDEN,
        allowed_if_followed_by: Some("ー"),
        exemptions: &[],
        applies_to_docs: true,
        doc_forbidden: &[],
        doc_exemptions: &[],
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

/// Task 101: every Markdown file directly under `docs/src/users/`, as
/// `(file name, prose-only text)`, sorted. Fails loudly if the directory is
/// missing or empty -- a scan that finds no files must not pass by finding
/// nothing.
fn user_docs() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/users");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|x| x == "md"))
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(entry.path())
                .unwrap_or_else(|e| panic!("cannot read {name}: {e}"));
            (name, prose_only(&text))
        })
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "docs/src/users/ holds no Markdown files -- the scan would prove nothing"
    );
    files
}

/// Task 101: the user guide uses the same words as the app. Applies every
/// row with `applies_to_docs`; code spans and fenced blocks are skipped
/// (`prose_only`), and only `docs/src/users/` is read -- maintainer docs and
/// `docs/src/intermediate/settings.md` (which documents the stored `exact`/
/// `conceptual` setting values, not labels) are outside it.
#[test]
fn user_docs_follow_the_glossary() {
    let mut violations = Vec::new();
    for (file, prose) in user_docs() {
        for term in GLOSSARY {
            violations.extend(term.doc_violations(&file, &prose));
        }
    }
    assert!(
        violations.is_empty(),
        "\n{}\n{} violation(s) in the user guide -- Task 101. Fix the sentence, or add a \
         justified entry to the row's `doc_exemptions`.",
        violations.join("\n"),
        violations.len()
    );
}

/// Task 101: a docs exemption that no longer matches anything is a stale
/// permission -- same rule as the catalog exemptions above.
#[test]
fn every_glossary_doc_exemption_is_load_bearing() {
    let docs = user_docs();
    for term in GLOSSARY {
        for &(file, forbidden_term, _reason) in term.doc_exemptions {
            let prose = docs
                .iter()
                .find(|(name, _)| name == file)
                .map(|(_, prose)| prose.as_str())
                .unwrap_or_else(|| {
                    panic!("doc exemption names {file}, which is not in docs/src/users/")
                });
            let matches = term
                .forbidden
                .iter()
                .chain(term.doc_forbidden)
                .any(|&(locale, t)| t == forbidden_term && term.doc_term_hits(prose, locale, t));
            assert!(
                matches,
                "doc exemption ({file:?}, {forbidden_term:?}) for \"{concept}\" does not match \
                 any more -- remove it",
                concept = term.concept
            );
        }
    }
}
