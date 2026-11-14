//! Task 140 §3 (RFC-066 §3.4): orbok's own controls (`icon_primary`,
//! `icon_secondary`, the result card's click area, the folder card's
//! actions, `components::choice` and `components::switch`) each measure
//! at least 44x44 px at Default. One test per control kind, each
//! rendering it through `iced_test` and asserting the real bounds.
//!
//! `iced::widget::button` has no `.id()` (confirmed: not in
//! `iced_widget::button`'s own API), so a plain button's *own* padded
//! bounds cannot be selected directly the way `components::switch_id`
//! lets the switch's container be. `icon_primary`, `icon_secondary` and
//! `components::choice` are each measured per Review 320 §4.1: click the
//! four corners of a 44x44 square centered on the control's own label
//! and assert a message arrives from every corner -- "that is what 'a
//! 44 px hit area' means, whatever the button is built from," not a
//! padding constant added to the label's own bounds (which missed a
//! choice-padding regression, Review 320's own mutation T5).
//!
//! snora's own controls (button, chip, tab, sidebar item) get the same
//! floor from `tokens.targets.min` (Task 140 §1a.2, `theme::Theme::tokens`)
//! and are snora's own responsibility to measure, not orbok's -- RFC-066
//! §3.4 draws that line, and `crates/ui/src/a11y.rs`'s own module
//! comment draws the same one for contrast ("a role we do not render is
//! not ours to audit").

use crate::components;
use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message, ViewId};
use crate::tests::iced_test_guard;
use crate::theme::TextScale;
use iced::Size;
use iced_test::Simulator;

const WINDOW: Size = Size::new(1280.0, 720.0);
const MIN: f32 = 44.0;

fn assert_at_least_44(what: &str, width: f32, height: f32) {
    assert!(
        width >= MIN - 0.5 && height >= MIN - 0.5,
        "{what}: {width}x{height} is smaller than the house 44x44 minimum"
    );
}

/// Clicks the four corners of a 44x44 square centered on `label`'s own
/// bounds-center and returns every message the clicks produced. A
/// corner that lands outside the control's real (padded) hit area
/// produces no message there, so fewer than four messages back means
/// the control is smaller than 44x44 somewhere along that square --
/// whatever padding or sizing the control is actually built from.
///
/// `label` is matched by content, but a page can show the same text
/// twice (the Search page's own heading uses `NavSearch`, which happens
/// to read "Search" in both English and Japanese, the same text as
/// `SearchButton`) -- `Simulator::find` would silently take whichever
/// one its depth-first walk meets first, which is the heading, not the
/// button. So this walks every candidate itself (the same
/// never-signal-done pattern `a_folder_card_action_is_at_least_44_square`
/// already uses below), and for each occurrence of `label` finds the
/// smallest `Container` that contains it -- a plain `button` reports
/// its own full padded bounds as a `Container` (confirmed in
/// `iced_widget::button::operate`), and `icon_primary`/`icon_secondary`
/// build symmetric padding on both axes (`control_padding`), so that
/// container's center is the button's own center, exactly. A page
/// heading has no such tightly-wrapping container (the smallest one
/// around it is the whole content column), so picking the occurrence
/// with the overall smallest wrapping container -- not just "the
/// first" -- lands on the button every time, not the heading.
fn messages_from_44_square_corners(view: iced::Element<'_, Message>, label: &str) -> Vec<Message> {
    use iced_test::selector::Candidate;
    use std::sync::{Arc, Mutex};

    fn contains(outer: &iced::Rectangle, inner: &iced::Rectangle) -> bool {
        outer.x <= inner.x + 0.5
            && outer.y <= inner.y + 0.5
            && outer.x + outer.width >= inner.x + inner.width - 0.5
            && outer.y + outer.height >= inner.y + inner.height - 0.5
    }
    fn area(r: &iced::Rectangle) -> f32 {
        r.width * r.height
    }

    let mut ui = Simulator::with_size(iced::Settings::default(), WINDOW, view);
    let texts: Arc<Mutex<Vec<iced::Rectangle>>> = Arc::default();
    let containers: Arc<Mutex<Vec<iced::Rectangle>>> = Arc::default();
    let texts_sink = texts.clone();
    let containers_sink = containers.clone();
    let _ = ui.find(move |c: Candidate<'_>| -> Option<()> {
        match c {
            Candidate::Text {
                bounds, content, ..
            } if content == label => {
                texts_sink.lock().unwrap().push(bounds);
            }
            Candidate::Container { bounds, .. } => {
                containers_sink.lock().unwrap().push(bounds);
            }
            _ => {}
        }
        None
    });
    let containers = containers.lock().unwrap();
    let bounds = texts
        .lock()
        .unwrap()
        .iter()
        .filter_map(|t| {
            containers
                .iter()
                .filter(|c| contains(c, t))
                .min_by(|a, b| area(a).total_cmp(&area(b)))
                .copied()
        })
        .min_by(|a, b| area(a).total_cmp(&area(b)))
        .unwrap_or_else(|| panic!("no control tightly wraps the label {label:?}"));

    let cx = bounds.x + bounds.width / 2.0;
    let cy = bounds.y + bounds.height / 2.0;
    let half = MIN / 2.0;
    for (dx, dy) in [(-half, -half), (half, -half), (-half, half), (half, half)] {
        ui.point_at(iced::Point::new(cx + dx, cy + dy));
        ui.simulate(iced_test::simulator::click());
    }
    ui.into_messages().collect()
}

/// `icon_primary` (the Search button): all four corners of a 44x44
/// square centered on its own label must land on the button.
#[test]
fn icon_primary_is_at_least_44_square() {
    let _guard = iced_test_guard();
    let mut state = AppState::default();
    state.sources.push(crate::state::SourceCard {
        display_name: "Documents".into(),
        display_path: "/home/user/Documents".into(),
        indexed: 1,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders: true,
    });
    let messages = messages_from_44_square_corners(
        crate::views::search_view(&state),
        tr(Locale::En, MessageKey::SearchButton),
    );
    assert_eq!(
        messages.len(),
        4,
        "icon_primary (Search button): not every corner of a 44x44 square around \
         the label landed on the button, got {messages:?}"
    );
    assert!(
        messages.iter().all(|m| matches!(m, Message::SubmitSearch)),
        "icon_primary (Search button) sent something other than SubmitSearch: {messages:?}"
    );
}

/// `icon_secondary` (the Folders page's "Add folder" button) -- same
/// four-corner check as `icon_primary`.
#[test]
fn icon_secondary_is_at_least_44_square() {
    let _guard = iced_test_guard();
    let state = AppState {
        active_view: ViewId::Sources,
        ..AppState::default()
    };
    let messages = messages_from_44_square_corners(
        crate::views::sources_view(&state),
        tr(Locale::En, MessageKey::SourcesAddFolder),
    );
    assert_eq!(
        messages.len(),
        4,
        "icon_secondary (Add folder button): not every corner of a 44x44 square \
         around the label landed on the button, got {messages:?}"
    );
    assert!(
        messages
            .iter()
            .all(|m| matches!(m, Message::RequestAddSource)),
        "icon_secondary (Add folder button) sent something other than RequestAddSource: {messages:?}"
    );
}

/// `components::choice` (a Settings text-size option) -- same
/// four-corner check as `icon_primary`.
#[test]
fn a_choice_option_is_at_least_44_square() {
    let _guard = iced_test_guard();
    let mut state = AppState::default();
    state.update(&Message::SetTextScale(TextScale::Default));
    let messages = messages_from_44_square_corners(
        crate::views::settings_view(&state),
        tr(Locale::En, MessageKey::TextScaleDefault),
    );
    assert_eq!(
        messages.len(),
        4,
        "components::choice (the \"Default\" text-size option): not every corner \
         of a 44x44 square around the label landed on the option, got {messages:?}"
    );
    assert!(
        messages.iter().all(|m| matches!(m, Message::AlreadyChosen)),
        "the \"Default\" text-size option (already chosen) sent something other \
         than AlreadyChosen: {messages:?}"
    );
}

/// `components::switch` -- measured directly via `switch_id`, which
/// already selects the control's own full container, not its inner
/// text (the same `Target`, not `target::Text`, Task 118's own tests
/// already rely on for the same reason).
#[test]
fn a_switch_is_at_least_44_square() {
    let _guard = iced_test_guard();
    let state = AppState::default();
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        WINDOW,
        crate::views::settings_view(&state),
    );
    let bounds = ui
        .find(components::switch_id("reduce-motion"))
        .expect("the reduce-motion switch")
        .bounds();
    assert_at_least_44(
        "components::switch (reduce-motion)",
        bounds.width,
        bounds.height,
    );
}

/// The result card's click area: not given its own id (a raw
/// `button(inner)` with no `.id()` -- see this file's own module
/// comment), so this measures the card's real vertical extent from its
/// first line (the title) to its last (the snippet), which is always
/// present (`card_heading_line`'s own "no snippet" case still keeps the
/// placeholder sentence) -- a multi-line card's own click area is
/// never close to the 44 px floor in practice, confirmed rather than
/// assumed.
#[test]
fn a_result_card_is_at_least_44_square() {
    let _guard = iced_test_guard();
    let mut state = AppState {
        active_view: ViewId::Search,
        last_query: Some("report".into()),
        ..AppState::default()
    };
    state.sources.push(crate::state::SourceCard {
        display_name: "Documents".into(),
        display_path: "/home/user/Documents".into(),
        indexed: 1,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders: true,
    });
    state.search_results = vec![crate::state::SearchResultDisplay {
        display_path: "/docs/report.md".into(),
        canonical_path: "/docs/report.md".into(),
        title: Some("Quarterly report".into()),
        heading_path: None,
        snippet: Some("a snippet".into()),
        keyword_rank: 1,
        badges: vec![],
        trust: Default::default(),
    }];
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        WINDOW,
        crate::views::search_view(&state),
    );
    let title = ui
        .find("Quarterly report")
        .expect("the result's title")
        .bounds();
    let path = ui
        .find("/docs/report.md")
        .expect("the result's display path")
        .bounds();
    let snippet = ui.find("a snippet").expect("the result's snippet").bounds();
    assert!(
        title.width >= MIN - 0.5,
        "a result card's title line alone is already narrower than 44 px: {title:?}"
    );
    let card_height = (snippet.y + snippet.height) - title.y;
    assert!(
        card_height >= MIN - 0.5,
        "a result card (title to snippet) is {card_height} tall, under the house minimum -- \
         title {title:?}, path {path:?}, snippet {snippet:?}"
    );
}

/// A folder card's own action (`source_card`'s "Prepare again" /
/// "Remove from orbok", built from `components::secondary`/`danger` --
/// snora's own buttons, so they get the floor from `tokens.targets.min`
/// (Task 140 §1a.2), not from any padding orbok sets itself). Checked
/// the same way this file checks every other control: measured, not
/// assumed.
#[test]
fn a_folder_card_action_is_at_least_44_square() {
    use iced_test::selector::Candidate;
    use std::sync::{Arc, Mutex};

    let _guard = iced_test_guard();
    let state = AppState {
        active_view: ViewId::Sources,
        sources: vec![crate::state::SourceCard {
            display_name: "Documents".into(),
            display_path: "/home/user/Documents".into(),
            indexed: 1,
            stale: 0,
            failed: 0,
            no_text_found: 0,
            unfinished_jobs: 0,
            status: orbok_core::SourceStatus::Active,
            source_id: "s1".into(),
            covers_subfolders: true,
        }],
        ..AppState::default()
    };
    let mut ui = Simulator::with_size(
        iced::Settings::default(),
        WINDOW,
        crate::views::sources_view(&state),
    );
    let text_bounds = ui
        .find(tr(Locale::En, MessageKey::SourceActionRemoveFromOrbok))
        .expect("the folder card's \"Remove from orbok\" action")
        .bounds();
    // `components::danger` (what the folder card's own "Remove from
    // orbok" action is actually built from, `source_card`'s own body)
    // is a *snora* control (`snora::design::button::danger_maybe`), so
    // per this file's own module comment its floor is snora's
    // responsibility via `tokens.targets.min`, not a padding choice
    // orbok makes -- confirmed here anyway, per Task 140 §3's own
    // "measure them the same way" for snora's controls too. A plain
    // button has no `.id()` (this file's own limitation, stated above),
    // so this walks every `Candidate::Focusable` in the page and takes
    // the one whose bounds contain the action's own text -- that is the
    // button's real, full (padded) bounds, not the text's.
    type Seen = Arc<Mutex<Vec<iced::Rectangle>>>;
    let seen: Seen = Arc::default();
    let sink = seen.clone();
    let _ = ui.find(move |c: Candidate<'_>| -> Option<()> {
        if let Candidate::Focusable { bounds, .. } = c {
            sink.lock().unwrap().push(bounds);
        }
        None
    });
    let button_bounds = seen
        .lock()
        .unwrap()
        .iter()
        .find(|b| {
            b.x <= text_bounds.x + 0.5
                && b.y <= text_bounds.y + 0.5
                && b.x + b.width >= text_bounds.x + text_bounds.width - 0.5
                && b.y + b.height >= text_bounds.y + text_bounds.height - 0.5
        })
        .copied()
        .unwrap_or_else(|| {
            panic!("no Focusable control contains the action's own text {text_bounds:?}")
        });
    assert_at_least_44(
        "the folder card's \"Remove from orbok\" action (components::danger)",
        button_bounds.width,
        button_bounds.height,
    );
}
