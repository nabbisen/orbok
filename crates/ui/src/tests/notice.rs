//! Task 037 §2: `UserNotice::is_problem`'s own doc claims the view "never
//! relies on colour alone" — checkable, and, until now, never checked. snora
//! withdrew the equivalent claim about its own prefab `notice`/`toast`
//! widgets in 0.41.1 (RFC-089): both vary only background/accent colour by
//! tone, identical text otherwise. orbok renders `snora::design::notice::Notice`
//! but supplies its own per-variant `title`/`body`, so the claim here rests on
//! a different foundation — this test is what proves that foundation holds,
//! rather than asserting it in a comment nothing checks.

use crate::i18n::Locale;
use crate::notice::UserNotice;
use std::collections::HashSet;

/// Generates `ALL` (every `UserNotice` variant) and `assert_exhaustive` (a
/// match over the same variant list, no wildcard arm) from one token list,
/// the same way `crate::i18n::message_keys!` generates `MessageKey` and
/// `ALL_KEYS` together so they cannot drift apart (Review 134 §4). Applied
/// here in the test file rather than in `notice.rs` itself, since
/// `UserNotice` is a hand-written enum and generating it from this same
/// macro would be the restructuring Task 037 §6 asks to stop and report
/// rather than do quietly -- this achieves the identical non-drift
/// guarantee without touching `notice.rs` at all. If `UserNotice` gains a
/// variant not named in the invocation below, `assert_exhaustive` fails to
/// compile (E0004); `ALL` is generated from the exact same list, so it
/// cannot silently omit a variant either. (An earlier version of this test
/// tried to prove `ALL`'s completeness by iterating `ALL` itself and
/// checking membership -- that is tautological, since anything drawn from
/// `ALL` is trivially "in `ALL`"; only generating both from one source, as
/// here, actually closes the gap.)
///
/// Task 113: two notices carry the names of the user's folders, so a variant
/// is either a unit (`Name`) or has data (`Name => sample`), the sample being
/// the value `all()` returns for it. `ALL` became `all()` because a sample
/// owns strings.
macro_rules! every_user_notice {
    ($($variant:ident),+ ; $($data:ident => $sample:expr),+ $(,)?) => {
        pub(crate) fn all() -> Vec<UserNotice> {
            vec![$(UserNotice::$variant),+ , $($sample),+]
        }

        #[allow(dead_code)]
        fn assert_exhaustive(n: &UserNotice) {
            match n {
                $(UserNotice::$variant => {})+
                $(UserNotice::$data { .. } => {})+
            }
        }
    };
}

every_user_notice! {
    FolderCouldNotBeAdded,
    SearchDidNotFinish,
    FileCouldNotBeFound,
    FileCouldNotBeOpened,
    FileNotAllowed,
    FileCheckFailed,
    FolderAdded,
    FolderAlreadyAdded,
    SearchReady,
    PreviewsCleared,
    SearchCacheCleared,
    ExtractedTextCleared,
    ReplacedDataRemoved,
    DiagnosticsFileCreated,
    DiagnosticsFileFailed,
    RecentSearchesCleared,
    RecentSearchFilterDropped,
    SettingCouldNotBeSaved,
    CatalogResetFailed,
    SourceCouldNotBeRemoved,
    RecentSearchesNotCleared,
    RecentSearchNotRemoved,
    CleanupDidNotFinish,
    FolderNotChecked,
    StorageUnavailable,
    IndexingCouldNotStart,
    ModelCouldNotBeLoaded
    ;
    FolderAlreadyIncluded => UserNotice::FolderAlreadyIncluded {
        folder: "Notes".into(),
        parent: "Docs".into(),
    },
    FoldersCombined => UserNotice::FoldersCombined {
        folders: vec!["Notes".into(), "Drafts".into()],
        parent: "Docs".into(),
    },
}

/// The invariant `notice.rs:37` claims: every notice is distinguishable by
/// its own words, in both locales, regardless of tone. Two properties, over
/// every variant:
///
/// 1. `title`/`body` are non-empty in both locales — a notice that renders
///    no text would make tone the *only* visible signal by omission.
/// 2. No two variants share the same `(title, body)` pair in a given locale
///    — if they did, tone would be the only thing telling them apart.
#[test]
fn user_notice_text_never_relies_on_colour_alone() {
    for &locale in Locale::ALL {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for notice in &all() {
            let title = notice.title(locale).to_string();
            let body = notice.body(locale);
            assert!(
                !title.is_empty(),
                "{notice:?} must have a non-empty title in {locale:?}"
            );
            assert!(
                !body.is_empty(),
                "{notice:?} must have a non-empty body in {locale:?}"
            );
            assert!(
                seen.insert((title, body.clone())),
                "{notice:?} shares its (title, body) pair with another \
                 variant in {locale:?} -- tone would be the only thing \
                 distinguishing them"
            );
        }
    }
}

/// RFC-059 §10 criterion 5 (Amendment 2): no Safe-cleanup notice reports an
/// amount of space freed -- the reclaim figure is a convention, not a
/// measurement.
#[test]
fn cleanup_notices_never_claim_space_was_freed() {
    const CLEANUP: [UserNotice; 4] = [
        UserNotice::PreviewsCleared,
        UserNotice::SearchCacheCleared,
        UserNotice::ExtractedTextCleared,
        UserNotice::ReplacedDataRemoved,
    ];
    const SPACE_CLAIMS: &[&str] = &[
        "free", "space", "byte", "kb", "mb", "gb", "空き", "容量", "解放",
    ];
    for &locale in Locale::ALL {
        for notice in &CLEANUP {
            let text = format!("{} {}", notice.title(locale), notice.body(locale)).to_lowercase();
            for claim in SPACE_CLAIMS {
                assert!(
                    !text.contains(claim),
                    "{notice:?} in {locale:?} claims space was freed ({claim:?} in {text:?})"
                );
            }
        }
    }
}
