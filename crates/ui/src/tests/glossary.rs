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
    /// Task 104: `(formatter function name, term, why)` -- the same shape as
    /// `doc_exemptions`, with the `pub fn` in `i18n.rs` as the `where`. Covered
    /// by the same load-bearing check.
    formatter_exemptions: &'static [(&'static str, &'static str, &'static str)],
    /// Task 104: match `forbidden` as whole words (case-insensitively in the
    /// catalogs and formatters, as the other rows do), not as substrings. For
    /// a short ordinary word -- "stale" -- whose substring would also hit
    /// longer words.
    whole_word: bool,
}

/// Where a checked sentence came from -- a catalog key or a formatter
/// function -- for exemptions and failure messages.
#[derive(Clone, Copy)]
enum Source {
    Key(MessageKey),
    Formatter(&'static str),
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Key(key) => write!(f, "{key:?}"),
            Source::Formatter(name) => write!(f, "{name}()"),
        }
    }
}

impl GlossaryTerm {
    fn canonical_forms(&self, locale: Locale) -> Vec<&'static str> {
        self.canonical
            .iter()
            .filter(|(l, _)| *l == locale)
            .map(|(_, s)| *s)
            .collect()
    }

    fn is_exempt(&self, source: Source, term: &str) -> bool {
        match source {
            Source::Key(key) => self
                .exemptions
                .iter()
                .any(|&(k, t, _)| k == key && t == term),
            Source::Formatter(name) => self
                .formatter_exemptions
                .iter()
                .any(|&(f, t, _)| f == name && t == term),
        }
    }

    fn matches(&self, locale: Locale, copy: &str, term: &str) -> bool {
        match self.allowed_if_followed_by {
            Some(suffix) => contains_not_followed_by(copy, term, suffix),
            None if self.whole_word => contains_word(&copy.to_lowercase(), &term.to_lowercase()),
            None => contains_term(locale, copy, term),
        }
    }

    fn violations(&self, locale: Locale, source: Source, copy: &str) -> Vec<String> {
        let mut out = Vec::new();
        for &(term_locale, term) in self.forbidden {
            if term_locale != locale || !self.matches(locale, copy, term) {
                continue;
            }
            if self.is_exempt(source, term) {
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
                "{locale:?} {source:?} says {copy:?} -- contains {term:?}, {replacement}"
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
        formatter_exemptions: &[],
        whole_word: false,
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
        formatter_exemptions: &[],
        whole_word: false,
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
        formatter_exemptions: &[],
        whole_word: false,
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
        formatter_exemptions: &[],
        whole_word: false,
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
        formatter_exemptions: &[],
        whole_word: false,
    },
    GlossaryTerm {
        concept: "a file that changed since orbok prepared it",
        canonical: &[(Locale::En, "Needs update"), (Locale::Ja, "要更新")],
        forbidden: &[(Locale::En, "stale")],
        allowed_if_followed_by: None,
        exemptions: &[],
        applies_to_docs: true,
        // Docs match case-sensitively as whole words, so the capitalised
        // spelling is listed too.
        doc_forbidden: &[(Locale::En, "Stale")],
        doc_exemptions: &[],
        formatter_exemptions: &[],
        whole_word: true,
    },
];

#[test]
fn default_ui_copy_follows_the_glossary() {
    let mut violations = Vec::new();
    for &locale in Locale::ALL {
        for &key in crate::i18n::ALL_KEYS {
            let copy = tr(locale, key);
            for term in GLOSSARY {
                violations.extend(term.violations(locale, Source::Key(key), copy));
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

// ── Task 104: the formatted sentences ─────────────────────────────────────
//
// `ALL_KEYS` is every fixed string; `i18n.rs` also holds formatter functions
// whose sentences are `format!` literals, which the catalog scan never sees.
// Each is called here with fixed sample arguments, in both locales, and its
// output goes through the same rows.
//
// **User-supplied parts** (a folder name, a query, a path) are sample values
// that break no row ("Docs", "notes", "/data"), so a violation can only come
// from orbok's own words. **Variants** (a singular and a plural, a segment
// present and absent) are each sampled: one formatter can hold several
// sentences.

use crate::state::SearchFolderScope;

type Sampler = fn(Locale) -> Vec<String>;

/// Every `pub fn … -> String` in `i18n.rs`, by name, with its samples. The
/// exhaustiveness test below compares this list with the file itself.
const FORMATTERS: &[(&str, Sampler)] = &[
    ("fmt_label_value", |l| {
        vec![crate::i18n::fmt_label_value(l, "Label", "Value")]
    }),
    ("wizard_file_size_mb", |l| {
        vec![crate::i18n::wizard_file_size_mb(l, 1.5)]
    }),
    ("preparing_folder_for_search", |l| {
        vec![crate::i18n::preparing_folder_for_search(l, "Docs")]
    }),
    ("files_ready_for_search", |l| {
        vec![
            crate::i18n::files_ready_for_search(l, 1),
            crate::i18n::files_ready_for_search(l, 3),
        ]
    }),
    ("startup_failed_data_folder_body", |l| {
        vec![crate::i18n::startup_failed_data_folder_body(l, "/data")]
    }),
    ("model_exact_size", |l| {
        vec![crate::i18n::model_exact_size(l, 1_234_567)]
    }),
    ("model_file_position", |l| {
        vec![
            crate::i18n::model_file_position(l, 1, 3),
            crate::i18n::model_file_position(l, 0, 0),
        ]
    }),
    ("model_transfer_progress", |l| {
        vec![
            crate::i18n::model_transfer_progress(l, 1_000_000, 5_000_000),
            crate::i18n::model_transfer_progress(l, 1_000_000, 0),
        ]
    }),
    ("source_summary", |l| {
        vec![
            crate::i18n::source_summary(l, 12, 0, 0, 0),
            crate::i18n::source_summary(l, 12, 1, 2, 3),
        ]
    }),
    ("search_result_count", |l| {
        vec![
            crate::i18n::search_result_count(l, 1),
            crate::i18n::search_result_count(l, 3),
        ]
    }),
    ("fmt_reset_removes", |l| {
        vec![
            crate::i18n::fmt_reset_removes(l, 2, 3, false),
            crate::i18n::fmt_reset_removes(l, 1, 1, true),
        ]
    }),
    ("fmt_rebuild_prepares", |l| {
        vec![
            crate::i18n::fmt_rebuild_prepares(l, 1),
            crate::i18n::fmt_rebuild_prepares(l, 3),
        ]
    }),
    ("fmt_gib", |l| vec![crate::i18n::fmt_gib(l, 1.5)]),
    ("fmt_mib_bucket", |l| {
        vec![crate::i18n::fmt_mib_bucket(l, "Sample", 1.5)]
    }),
    ("fmt_storage_row", |l| {
        vec![crate::i18n::fmt_storage_row(l, "Sample", 1.5, 3)]
    }),
    ("fmt_remove_source_title", |l| {
        vec![crate::i18n::fmt_remove_source_title(l, "Docs")]
    }),
    ("fmt_query", |l| vec![crate::i18n::fmt_query(l, "notes")]),
    ("search_location_chip", |l| {
        vec![
            crate::i18n::search_location_chip(l, "Docs", SearchFolderScope::FolderAndSubfolders),
            crate::i18n::search_location_chip(l, "Docs", SearchFolderScope::FolderOnly),
        ]
    }),
];

/// The other `pub fn`s in `i18n.rs`, each accounted for by *not* being a
/// formatter: they return a catalog string or a struct of them, which
/// `default_ui_copy_follows_the_glossary` already reads through `ALL_KEYS`.
const NOT_FORMATTERS: &[(&str, &str)] = &[
    ("tr", "the catalog lookup itself"),
    (
        "dialog_title_add_source",
        "returns tr(DialogAddSourceTitle), a catalog key",
    ),
    (
        "dialog_title_choose_search_folder",
        "returns tr(DialogChooseSearchFolderTitle), a catalog key",
    ),
    (
        "diagnostics_bundle_labels",
        "a struct of tr(Diagnostics*) catalog keys",
    ),
];

/// `(name, returns_string)` for every top-level `pub fn` / `pub(crate) fn` in
/// `i18n.rs`, read from the file itself. A signature runs from `fn` to the
/// first `{`; it may span lines.
fn i18n_pub_fns() -> Vec<(String, bool)> {
    let source = include_str!("../i18n.rs");
    let mut out = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line
            .strip_prefix("pub fn ")
            .or_else(|| line.strip_prefix("pub(crate) fn "))
        else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let mut signature = line.to_string();
        while !signature.contains('{') {
            match lines.next() {
                Some(next) => {
                    signature.push(' ');
                    signature.push_str(next.trim());
                }
                None => break,
            }
        }
        let returns_string = signature
            .split_once("->")
            .is_some_and(|(_, ret)| ret.split('{').next().unwrap_or("").trim() == "String");
        out.push((name, returns_string));
    }
    out
}

/// Exhaustiveness is enforced, not trusted: `FORMATTERS` must be exactly the
/// `-> String` functions in `i18n.rs`, and every other `pub fn` must be
/// accounted for in `NOT_FORMATTERS`. Add a formatter without listing it and
/// this fails; delete one and leave its entry and this fails.
#[test]
fn every_formatter_in_i18n_is_in_the_glossary_scan() {
    let found = i18n_pub_fns();
    assert!(
        found.len() >= 20,
        "the signature scan of i18n.rs found only {} pub fns -- the parser is broken",
        found.len()
    );
    let listed: std::collections::BTreeSet<&str> = FORMATTERS.iter().map(|&(n, _)| n).collect();
    let others: std::collections::BTreeSet<&str> = NOT_FORMATTERS.iter().map(|&(n, _)| n).collect();
    let mut problems = Vec::new();
    for (name, returns_string) in &found {
        let n = name.as_str();
        if *returns_string && !listed.contains(n) {
            problems.push(format!(
                "`{n}` returns String but is not in FORMATTERS -- add it with sample arguments"
            ));
        }
        if !*returns_string && !others.contains(n) {
            problems.push(format!(
                "`{n}` does not return String and is not in NOT_FORMATTERS -- say why it is not a formatter"
            ));
        }
    }
    let names: std::collections::BTreeSet<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
    for n in listed.iter().chain(others.iter()) {
        if !names.contains(n) {
            problems.push(format!(
                "`{n}` is listed but is not a pub fn in i18n.rs any more"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Every formatter's sentences, both locales, through every row.
#[test]
fn formatted_sentences_follow_the_glossary() {
    let mut violations = Vec::new();
    for &locale in Locale::ALL {
        for &(name, sample) in FORMATTERS {
            for text in sample(locale) {
                for term in GLOSSARY {
                    violations.extend(term.violations(locale, Source::Formatter(name), &text));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "\n{}\n{} violation(s) in formatted sentences -- Task 104. Either fix the copy or add a \
         justified entry to the row's `formatter_exemptions`.",
        violations.join("\n"),
        violations.len()
    );
}

/// Task 104: a formatter exemption that no longer matches anything is a stale
/// permission -- same rule as the catalog and docs exemptions above. The
/// exempted formatter must exist in `FORMATTERS`, and at least one of its
/// samples, in some locale, must still contain the exempted term.
#[test]
fn every_glossary_formatter_exemption_is_load_bearing() {
    for term in GLOSSARY {
        for &(function, forbidden_term, _reason) in term.formatter_exemptions {
            let sample = FORMATTERS
                .iter()
                .find(|&&(name, _)| name == function)
                .map(|&(_, sample)| sample)
                .unwrap_or_else(|| {
                    panic!("formatter exemption names {function}, which is not in FORMATTERS")
                });
            let matches = Locale::ALL.iter().any(|&locale| {
                sample(locale).iter().any(|text| {
                    term.forbidden.iter().any(|&(term_locale, t)| {
                        term_locale == locale
                            && t == forbidden_term
                            && term.matches(locale, text, t)
                    })
                })
            });
            assert!(
                matches,
                "formatter exemption ({function:?}, {forbidden_term:?}) for \"{concept}\" does not \
                 match any more -- remove it",
                concept = term.concept
            );
        }
    }
}
