//! Task 140 §5 (RFC-066): the pictures headless rendering cannot replace
//! with a bounds check -- "does it read well", and specifically whether
//! the system actually has a usable semibold (or bolder) face for
//! Japanese (§4's own stop condition). Not run by `cargo test` on its
//! own: `cargo test -p orbok-ui --lib task140_evidence -- --ignored`.
//!
//! Per Review 325 §3's decision, these are headless snapshots
//! (`crate::tests::task148_layout_matrix::snapshot`), not real-window
//! captures -- and, checked during this task's own development, the
//! semibold/regular distinction this file exists to show **is already
//! visible in a headless snapshot** on this machine (Noto Sans and Noto
//! Sans CJK both render a visibly heavier "検索"/"Quarterly report" than
//! the surrounding regular text, confirmed by eye against the saved
//! PNGs). Review 325 §3's own framing ("font rendering is the one thing
//! headless can't show") does not hold for *this* distinction on *this*
//! system -- recorded here rather than silently assumed, in case a
//! different font stack (a CI runner, another desktop) renders it
//! differently. If it ever does, that is exactly the "capture shows
//! something the automated checks missed" case the GUI rule file asks
//! for a new check for.

use super::task148_layout_matrix::{
    folders_app, notice_app, search_app, settings_app, snapshot, wizard_app,
};
use crate::i18n::Locale;
use crate::tests::iced_test_guard;
use crate::theme::TextScale;
use iced::Size;

const NARROW: Size = Size::new(450.0, 2400.0);
const REFERENCE: Size = Size::new(1280.0, 720.0);

/// Worst case (×2.0, Japanese, narrow) and baseline (Default, English,
/// 1280x720) for each of the five pages Task 148's own matrix covers.
#[test]
#[ignore]
fn task140_pictures() {
    let _guard = iced_test_guard();
    type PageBuilder = fn(Locale, TextScale) -> crate::shell::OrbokApp;
    let pages: &[(&str, PageBuilder)] = &[
        ("search", search_app),
        ("folders", folders_app),
        ("settings", settings_app),
        ("wizard", wizard_app),
        ("notice", notice_app),
    ];
    for (name, build) in pages {
        snapshot(
            *build,
            Locale::Ja,
            TextScale::Largest,
            NARROW,
            "task140",
            &format!("{name}-worst-case-ja-largest-narrow"),
        );
        snapshot(
            *build,
            Locale::En,
            TextScale::Default,
            REFERENCE,
            "task140",
            &format!("{name}-baseline-en-default-1280x720"),
        );
    }
}

/// §4's own stop condition, specifically: the semibold heading, in both
/// languages, at Default (where it is easiest to compare against
/// regular-weight body text beside it).
#[test]
#[ignore]
fn task140_semibold_headings() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        let name = match locale {
            Locale::En => "en",
            Locale::Ja => "ja",
        };
        snapshot(
            search_app,
            locale,
            TextScale::Default,
            REFERENCE,
            "task140",
            &format!("semibold-heading-{name}"),
        );
    }
}
