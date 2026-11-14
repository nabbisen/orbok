//! Task 148 §1: a layout-check helper that renders a page across the
//! text-size x language x window-size matrix orbok's manual screenshot
//! passes used to cover by hand (Task 143/146), and asserts, for each
//! combination, what a screenshot only showed a human: nothing overflows
//! the window, every named element (a sidebar/tab-bar label, a choice
//! option, a primary button, ...) is present and not clipped, and --
//! where checked -- a control's hit area meets the house minimum (Task
//! 140's 44 px rule; the generic check this helper exposes is what Task
//! 140 will reuse per control once it lands).
//!
//! **The matrix:**
//! - **Text size:** [`TextScale::ALL`] -- today's three; Task 140 adding
//!   a fourth (`Largest`, x2.0) grows every page's matrix with it, no
//!   change needed here.
//! - **Language:** [`Locale::ALL`] (en, ja).
//! - **Window size:** [`NARROW`] (the narrowest width this crate's own
//!   tests already exercise -- `task106_narrow_window.rs`,
//!   `task127_text_size_reaches_chrome.rs`) and [`REFERENCE`] (the GUI
//!   rule file's own manual-capture default, so the matrix also covers
//!   the size real screenshots were taken at).
//! - **Theme: deliberately not varied here.** `Theme::tokens` differs
//!   only in `Tokens::light()`/`dark()`/`high_contrast_*`'s *color*
//!   fields (read from `theme.rs`); typography, spacing and padding --
//!   the only things layout reads -- are identical across every theme.
//!   "Follow system" resolving to the right concrete theme is a state
//!   question, not a layout one, and is already covered, green, by
//!   `task138_follow_system_follows_the_system.rs` (confirmed, not
//!   duplicated here, per this task's own §1 instruction to "confirm"
//!   it).
//!
//! **Every combination renders through the real shell** ([`OrbokApp::view`]),
//! not a bare page-view function: that is what puts the sidebar/tab-bar
//! labels Task 143/146 screenshotted by hand into the same assertion as
//! the page body, and is required for the notice (rendered only by the
//! shell, above the page -- Task 064) and correctly excludes the sidebar
//! for the wizard (which replaces the shell entirely while active --
//! `shell.rs`'s own `view`, "Startup wizard takes priority").
//!
//! **Two passes per combination (Review 325 §4.1).** The named list
//! above (`Expect`) checks presence and full visibility -- it is how a
//! specific control (a switch's hit area, say) gets checked by more than
//! its text. But in iced, a row given less space than it needs is not
//! pushed past the window (confirmed by mutation-testing real production
//! rows during this task's own development: a child is squeezed to
//! width 0, or clipped, never rendered outside the bounds it was given).
//! So the bounds check that catches real regressions is the *clipping*
//! one, and named-only coverage means it only runs on whatever the list
//! happens to name -- a squeezed "Exact" or "Conceptual" option, a
//! folder path, a result's snippet, would all go unseen if nothing
//! named them. The second pass in `assert_matrix` below walks *every*
//! rendered text (a selector that always returns `None` still visits
//! the whole tree -- `iced_selector::Selector`'s blanket impl for
//! `FnMut(Candidate) -> Option<T>`) and checks the same two properties
//! on each one, named or not.

use crate::i18n::{Locale, MessageKey, tr};
use crate::shell::OrbokApp;
use crate::state::{
    AppState, Message, ResultTrustDisplay, SearchResultDisplay, SourceCard, ViewId, WizardState,
};
use crate::tests::iced_test_guard;
use crate::theme::TextScale;
use iced::{Rectangle, Size};
use iced_test::Simulator;
use iced_test::selector::Candidate;
use std::sync::{Arc, Mutex};

/// The narrowest width this crate's tests use (`task106_narrow_window.rs`,
/// `task127_text_size_reaches_chrome.rs`) -- tall enough that vertical
/// scrolling, which every page here already allows, never hides a label.
const NARROW: Size = Size::new(450.0, 1400.0);
/// The GUI rule file's own manual-capture default
/// (`.git-exclude/rules/project-instructions-gui-common.md`).
const REFERENCE: Size = Size::new(1280.0, 720.0);

fn window_sizes() -> [Size; 2] {
    [NARROW, REFERENCE]
}

/// One named element a combination must show: present, inside the
/// window, and not clipped. `min` additionally requires the element's
/// own bounds to be at least that large (Task 140's house 44 px hit-area
/// rule) -- `None` for most elements, since only a control's hit area
/// needs it.
struct Expect {
    target: Target,
    min: Option<(f32, f32)>,
}

/// `iced_test`'s `Selector` is implemented separately for `&str`/`String`
/// (text search, used for every ordinary label) and for `widget::Id`
/// (used for a switch, whose own label is inside the widget and so is
/// not a text a selector finds -- the same reason Task 118's own tests
/// address it by id). The two have different `Selector::Output` types,
/// so this enum, not a generic selector, is what lets one `Expect` list
/// mix both.
enum Target {
    Text(String),
    Switch(&'static str),
}

impl Target {
    fn describe(&self) -> String {
        match self {
            Target::Text(t) => format!("{t:?}"),
            Target::Switch(name) => format!("the {name} switch"),
        }
    }
}

fn label(text: impl Into<String>) -> Expect {
    Expect {
        target: Target::Text(text.into()),
        min: None,
    }
}

fn label_min(text: impl Into<String>, w: f32, h: f32) -> Expect {
    Expect {
        target: Target::Text(text.into()),
        min: Some((w, h)),
    }
}

fn switch(name: &'static str) -> Expect {
    Expect {
        target: Target::Switch(name),
        min: None,
    }
}

fn switch_min(name: &'static str, w: f32, h: f32) -> Expect {
    Expect {
        target: Target::Switch(name),
        min: Some((w, h)),
    }
}

/// The tab bar's own labels for `view`'s group (Task 124's one list the
/// real tab bar is built from) -- the generalization, across the whole
/// matrix, of what Task 127's `the_grown_tab_bar_stays_inside_a_narrow_window`
/// checked only at one scale/width, and of what Task 143/146 screenshotted
/// by hand.
fn tab_bar_labels(locale: Locale, view: ViewId) -> Vec<Expect> {
    ViewId::pages(view.group())
        .iter()
        .map(|p| label(tr(locale, p.label_key())))
        .collect()
}

/// The sidebar's three top-level group names (Task 143/snora 0.57: the
/// labelled rail, the engine's default, draws each item's own name on
/// the rail itself, not only in a hover tooltip -- `shell.rs`'s own
/// `SideBarItem::tooltip` field, read by a labelled rail, so this is a
/// real, visible text, not the inaccessible kind Task 127's own
/// `the_sidebar_and_its_tooltip_take_the_scaled_tokens` names). Add to
/// every page that renders through the shell -- not the wizard, which
/// replaces the shell entirely while active (Review 325 §4.1: the
/// matrix never checked these; a planted missing name survived until
/// now).
fn sidebar_group_labels(locale: Locale) -> [Expect; 3] {
    [
        label(tr(locale, MessageKey::NavSearch)),
        label(tr(locale, MessageKey::NavAi)),
        label(tr(locale, MessageKey::NavSettings)),
    ]
}

/// Renders `view` at `window` and asserts every one of `expect` is
/// present, lies inside `window`, is not clipped, and (where `min` is
/// set) is at least that large. `what` names the failing combination in
/// the panic. Takes a plain `Element`, not `&OrbokApp`, so this file's
/// own red-first proof tests can hand it a synthetic, deliberately
/// broken one without touching production code.
fn assert_matrix(view: iced::Element<'_, Message>, window: Size, what: &str, expect: &[Expect]) {
    let mut ui = Simulator::with_size(iced::Settings::default(), window, view);
    for e in expect {
        let describe = e.target.describe();
        let (bounds, visible) = match &e.target {
            Target::Text(t) => {
                let found = ui
                    .find(t.as_str())
                    .unwrap_or_else(|_| panic!("{what}: {describe} is not on the page"));
                (found.bounds(), found.visible_bounds())
            }
            Target::Switch(name) => {
                let found = ui
                    .find(crate::components::switch_id(name))
                    .unwrap_or_else(|_| panic!("{what}: {describe} is not on the page"));
                (found.bounds(), found.visible_bounds())
            }
        };
        assert!(
            bounds.x >= -0.5 && bounds.x + bounds.width <= window.width + 0.5,
            "{what}: {describe} is pushed outside the {}x{} window: x {} .. {}",
            window.width,
            window.height,
            bounds.x,
            bounds.x + bounds.width
        );
        assert!(
            visible.is_some_and(|v| {
                (v.width - bounds.width).abs() < 0.5 && (v.height - bounds.height).abs() < 0.5
            }),
            "{what}: {describe} is clipped: bounds {bounds:?}, visible {visible:?}"
        );
        if let Some((min_w, min_h)) = e.min {
            assert!(
                bounds.width >= min_w - 0.5 && bounds.height >= min_h - 0.5,
                "{what}: {describe}'s hit area is {}x{}, smaller than the {min_w}x{min_h} house minimum",
                bounds.width,
                bounds.height
            );
        }
    }

    // Review 325 §4.1: every rendered text, not only the named ones --
    // see this file's own module comment for why the clipping check,
    // not the window-bounds one, is what a real regression trips. A
    // selector that always returns `None` never matches, so `find`
    // walks the whole tree without stopping; the closure records every
    // `Candidate::Text` it passes on the way.
    type SeenTexts = Arc<Mutex<Vec<(String, Rectangle, Option<Rectangle>)>>>;
    let seen: SeenTexts = Arc::default();
    let sink = seen.clone();
    let _ = ui.find(move |c: Candidate<'_>| -> Option<()> {
        if let Candidate::Text {
            bounds,
            visible_bounds,
            content,
            ..
        } = c
        {
            sink.lock()
                .unwrap()
                .push((content.to_string(), bounds, visible_bounds));
        }
        None
    });
    for (content, bounds, visible) in seen.lock().unwrap().iter() {
        if content.is_empty() {
            continue;
        }
        assert!(
            bounds.x >= -0.5 && bounds.x + bounds.width <= window.width + 0.5,
            "{what}: text {content:?} is pushed outside the {}x{} window: x {} .. {}",
            window.width,
            window.height,
            bounds.x,
            bounds.x + bounds.width
        );
        assert!(
            bounds.width > 0.5,
            "{what}: text {content:?} is squeezed to width {}",
            bounds.width
        );
        match visible {
            Some(v) => assert!(
                (v.width - bounds.width).abs() < 0.5,
                "{what}: text {content:?} is clipped sideways: bounds {bounds:?}, visible {visible:?}"
            ),
            None => assert!(
                bounds.y >= window.height - 0.5,
                "{what}: text {content:?} is fully hidden, but not because it scrolled below \
                 the window's bottom: bounds {bounds:?}"
            ),
        }
    }
}

// ── Search, with a result ───────────────────────────────────────────────

fn search_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        active_view: ViewId::Search,
        show_advanced: true,
        ..AppState::default()
    };
    // `search_view` renders the "no folders yet" empty state whenever
    // `state.sources` is empty, before it ever looks at `search_results`
    // -- a source must be seeded for a result to render at all.
    state.sources.push(SourceCard {
        display_name: "Documents".into(),
        display_path: "/home/user/Documents".into(),
        indexed: 3,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders: true,
    });
    state.update(&Message::SetTextScale(scale));
    state.update(&Message::QueryChanged("report".into()));
    state.update(&Message::SubmitSearch);
    state.update(&Message::SearchResultsReady(vec![SearchResultDisplay {
        display_path: "/docs/report.md".into(),
        canonical_path: "/docs/report.md".into(),
        title: Some("Quarterly report".into()),
        heading_path: None,
        snippet: Some("a snippet".into()),
        keyword_rank: 1,
        badges: vec![],
        trust: ResultTrustDisplay::default(),
    }]));
    OrbokApp::with_state(state)
}

#[test]
fn the_search_page_with_a_result_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = search_app(locale, scale);
            let mut expect = tab_bar_labels(locale, ViewId::Search);
            expect.extend(sidebar_group_labels(locale));
            expect.push(label(tr(locale, MessageKey::SearchButton)));
            expect.push(label(tr(locale, MessageKey::SearchModeAuto)));
            expect.push(label("Quarterly report"));
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!("search+result, {locale:?}, {scale:?}, {window:?}"),
                    &expect,
                );
            }
        }
    }
}

// ── Task 147 §4: the two new search-location states ─────────────────────

/// `pub(crate)`: Task 147's own evidence test (headless snapshots of
/// this same state) reuses this fixture rather than a second copy that
/// could drift from the one the matrix itself checks.
pub(crate) fn search_one_folder_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        active_view: ViewId::Search,
        ..AppState::default()
    };
    state.update(&Message::SetTextScale(scale));
    // Task 147 §1.1: the one usable folder just added becomes the
    // chosen location -- `Message::SourceAdded` is the reducer this
    // runs through, not a struct literal, so the derived selection is
    // real, not asserted by construction.
    state.update(&Message::SourceAdded(SourceCard {
        display_name: "Documents".into(),
        display_path: "/home/user/Documents".into(),
        indexed: 3,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders: true,
    }));
    OrbokApp::with_state(state)
}

#[test]
fn the_search_page_with_one_folder_chosen_by_default_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = search_one_folder_app(locale, scale);
            let mut expect = tab_bar_labels(locale, ViewId::Search);
            expect.extend(sidebar_group_labels(locale));
            expect.push(label("Documents"));
            expect.push(label(tr(locale, MessageKey::SearchScopeOnly)));
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!(
                        "search, one folder chosen by default, {locale:?}, {scale:?}, {window:?}"
                    ),
                    &expect,
                );
            }
        }
    }
}

/// `pub(crate)`: see `search_one_folder_app`'s own doc comment.
pub(crate) fn search_two_folders_no_location_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        active_view: ViewId::Search,
        ..AppState::default()
    };
    state.update(&Message::SetTextScale(scale));
    // `SourcesLoaded`, not two `SourceAdded`s: both folders present from
    // the start (the realistic trigger per Review 324 §3's own origin --
    // folders added in an earlier session), so §1.1's auto-select on add
    // never runs here. Adding them one at a time would auto-select the
    // first (its own test, `the_search_page_with_one_folder_chosen_by_default_matrix`),
    // leaving no way to reach "two folders, none chosen" through
    // `SourceAdded` alone.
    state.update(&Message::SourcesLoaded(
        [("s1", "Documents"), ("s2", "Projects")]
            .into_iter()
            .map(|(id, name)| SourceCard {
                display_name: name.into(),
                display_path: format!("/home/user/{name}"),
                indexed: 3,
                stale: 0,
                failed: 0,
                no_text_found: 0,
                unfinished_jobs: 0,
                status: orbok_core::SourceStatus::Active,
                source_id: id.into(),
                covers_subfolders: true,
            })
            .collect(),
    ));
    // Task 147 §1.2: with two usable folders, nothing was preselected --
    // this models the real app's own gate (`router.rs`'s `SubmitSearch`,
    // `!has_selected() && !sources.is_empty()`), which dispatches this
    // reducer message.
    state.update(&Message::SearchFolderChoicesPending);
    OrbokApp::with_state(state)
}

#[test]
fn the_search_page_with_two_folders_and_no_location_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = search_two_folders_no_location_app(locale, scale);
            let mut expect = tab_bar_labels(locale, ViewId::Search);
            expect.extend(sidebar_group_labels(locale));
            expect.push(label("Documents"));
            expect.push(label("Projects"));
            expect.push(label(tr(locale, MessageKey::SearchFolderNotChosen)));
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!("search, two folders, none chosen, {locale:?}, {scale:?}, {window:?}"),
                    &expect,
                );
            }
        }
    }
}

// ── Folders ──────────────────────────────────────────────────────────────

fn folders_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        active_view: ViewId::Sources,
        ..AppState::default()
    };
    state.update(&Message::SetTextScale(scale));
    state.sources.push(SourceCard {
        display_name: "Documents".into(),
        display_path: "/home/user/Documents".into(),
        indexed: 3,
        stale: 0,
        failed: 0,
        no_text_found: 0,
        unfinished_jobs: 0,
        status: orbok_core::SourceStatus::Active,
        source_id: "s1".into(),
        covers_subfolders: true,
    });
    OrbokApp::with_state(state)
}

#[test]
fn the_folders_page_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = folders_app(locale, scale);
            let mut expect = tab_bar_labels(locale, ViewId::Sources);
            expect.extend(sidebar_group_labels(locale));
            expect.push(label(tr(locale, MessageKey::SourcesAddFolder)));
            expect.push(label(tr(locale, MessageKey::SearchScopeSubfolders)));
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!("folders, {locale:?}, {scale:?}, {window:?}"),
                    &expect,
                );
            }
        }
    }
}

// ── Settings ─────────────────────────────────────────────────────────────

fn settings_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        active_view: ViewId::Settings,
        ..AppState::default()
    };
    state.update(&Message::SetTextScale(scale));
    OrbokApp::with_state(state)
}

#[test]
fn the_settings_page_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = settings_app(locale, scale);
            let mut expect = tab_bar_labels(locale, ViewId::Settings);
            expect.extend(sidebar_group_labels(locale));
            expect.extend(
                crate::theme::Theme::ALL
                    .iter()
                    .map(|t| label(tr(locale, t.label_key()))),
            );
            expect.extend(
                TextScale::ALL
                    .iter()
                    .map(|t| label(tr(locale, t.label_key()))),
            );
            expect.push(label(tr(locale, MessageKey::ClearRecentSearches)));
            // Presence only -- not the 44 px house minimum: measured today
            // (narrow/Default) at 162.75x22.4, well under it. That is Task
            // 140's own job to fix and assert; `switch_min` below already
            // proves this helper's `min` check works, so Task 140 only has
            // to call it once these controls actually meet the floor.
            for name in ["reduce-motion", "remember-recent-searches"] {
                expect.push(switch(name));
            }
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!("settings, {locale:?}, {scale:?}, {window:?}"),
                    &expect,
                );
            }
        }
    }
}

// ── The wizard's first screen ────────────────────────────────────────────

fn wizard_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        wizard: Some(WizardState::NotConfigured),
        ..AppState::default()
    };
    state.update(&Message::SetTextScale(scale));
    OrbokApp::with_state(state)
}

#[test]
fn the_wizard_s_first_screen_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = wizard_app(locale, scale);
            let expect = [
                label(tr(locale, MessageKey::WizardTitleNotConfigured)),
                label(tr(locale, MessageKey::WizardDownloadAction)),
            ];
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!("wizard first screen, {locale:?}, {scale:?}, {window:?}"),
                    &expect,
                );
            }
        }
    }
}

// ── The notice ───────────────────────────────────────────────────────────

fn notice_app(locale: Locale, scale: TextScale) -> OrbokApp {
    let mut state = AppState {
        locale,
        active_view: ViewId::Search,
        ..AppState::default()
    };
    state.update(&Message::SetTextScale(scale));
    state.update(&Message::ShowNotice(crate::notice::UserNotice::FolderAdded));
    OrbokApp::with_state(state)
}

#[test]
fn the_notice_matrix() {
    let _guard = iced_test_guard();
    for &locale in Locale::ALL {
        for &scale in TextScale::ALL {
            let app = notice_app(locale, scale);
            let mut expect = tab_bar_labels(locale, ViewId::Search);
            expect.extend(sidebar_group_labels(locale));
            expect.push(label(tr(locale, MessageKey::NoticeFolderAddedTitle)));
            expect.push(label(tr(locale, MessageKey::NoticeFolderAddedBody)));
            // Not checked: `NoticeDismiss` is a tooltip on the dismiss X
            // (`friendly_notice`'s `.dismiss_tooltip(..)`), not a text --
            // Task 127's own `the_sidebar_and_its_tooltip_take_the_scaled_tokens`
            // already hit this same `Simulator::find` limitation for a
            // tooltip and documents it the same way.
            for window in window_sizes() {
                assert_matrix(
                    app.view(),
                    window,
                    &format!("notice, {locale:?}, {scale:?}, {window:?}"),
                    &expect,
                );
            }
        }
    }
}

// ── Red-first proof (Task 148 §5: "Red first") ───────────────────────────
//
// Task 148's own stop condition names exactly what this helper must
// catch: "a fixed-width row forced too wide at x1.3 in Japanese", and "a
// missing sidebar label". Both are proved here directly, against
// `assert_matrix` itself, with a synthetic page -- not by editing
// production code and reverting it, which has no equivalent for a
// macro-built `Element` tree the way it does for a SQL string elsewhere
// in this repo.

/// A missing named element is caught, not silently skipped.
#[test]
#[should_panic(expected = "is not on the page")]
fn a_missing_named_element_is_caught() {
    let _guard = iced_test_guard();
    let app = search_app(Locale::En, TextScale::Default);
    assert_matrix(
        app.view(),
        NARROW,
        "red-first proof: missing label",
        &[label("a label this page never shows")],
    );
}

/// The exact failure mode Task 148's own stop condition names: "a
/// fixed-width row forced too wide ... in Japanese", proved against the
/// real search page (not a hand-built stand-in), at a window far
/// narrower than the matrix's own narrow window (450 px -- this
/// codebase has no such bug at either size the matrix actually tests,
/// confirmed by the full matrix passing at both). 80 px forces real
/// rows past their own minimum, where iced can no longer reflow around
/// it, which is exactly the condition `assert_matrix`'s two checks exist
/// to catch:
/// - the mode choice's "Auto" option is pushed to `x = 84` in an 80 px
///   window (confirmed by hand during this task's own development), so
///   the window-bounds check fires;
/// - its `visible_bounds` is `None` (fully clipped), so the clipping
///   check fires too.
#[test]
#[should_panic(expected = "is pushed outside the")]
fn a_real_row_forced_narrower_than_its_minimum_is_caught_overflowing() {
    let _guard = iced_test_guard();
    let app = search_app(Locale::Ja, TextScale::Larger);
    assert_matrix(
        app.view(),
        Size::new(80.0, 1400.0),
        "red-first proof: overflow",
        &[label(tr(Locale::Ja, MessageKey::SearchModeAuto))],
    );
}

/// Proves the `min` check itself fires: the reduce-motion switch, measured
/// during this task's own development at 162.75x22.4 (narrow window,
/// Default scale -- short of 44 px tall), must fail a 44x44 floor. Not
/// asserted as a standing requirement on the Settings page itself (see
/// `the_settings_page_matrix`'s own comment): Task 140 has not landed the
/// house minimum yet, so this is a test of the helper, not of the app.
#[test]
fn the_min_size_check_catches_a_control_under_the_floor() {
    let _guard = iced_test_guard();
    let app = settings_app(Locale::En, TextScale::Default);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_matrix(
            app.view(),
            NARROW,
            "red-first proof: hit area",
            &[switch_min("reduce-motion", 44.0, 44.0)],
        );
    }));
    assert!(
        result.is_err(),
        "a switch shorter than the house minimum must be caught"
    );
}

/// The same check does not flag a control that already meets the floor
/// it is given -- proves `min` is a real comparison, not a check that
/// always fails.
#[test]
fn the_min_size_check_passes_a_control_that_meets_its_floor() {
    let _guard = iced_test_guard();
    let app = search_app(Locale::En, TextScale::Default);
    assert_matrix(
        app.view(),
        NARROW,
        "red-first proof: hit area (passing case)",
        &[label_min(
            tr(Locale::En, MessageKey::SearchButton),
            10.0,
            10.0,
        )],
    );
}

// ── A headless snapshot helper (Review 325 §4.2) ─────────────────────────
//
// `Simulator::snapshot` renders a frame and `Snapshot::matches_image`
// compares it against a stored PNG -- writing it, the first time, rather
// than failing (a golden-file pattern) -- with no window, no compositor,
// and no host to size one on (the question Owner Task 007 has not
// settled; Review 325 §3.2). This is the headless-first answer to a
// "does it read well" / "does it read in Japanese" capture, which the
// layout matrix above cannot answer (it checks bounds, not pixels).
//
// Not a `#[test]` and not run by CI on its own: a task's own evidence
// test calls it, deliberately, for the specific combinations its own
// review request names (the GUI rule file's "captures are the worst
// case, plus a baseline" -- not every combination, unlike the layout
// matrix above). CI must not write evidence files as a side effect of
// an ordinary test run, so the caller marks its own test `#[ignore]` and
// the evidence is produced by running it by hand with
// `--ignored`, the same convention this crate already uses for a pass
// that needs something outside a plain `cargo test` (RFC013_MODEL_DIR's
// own tests, for example).
//
// The icon font is loaded the same way `orbok`'s own `main.rs` loads it
// (`.font(orbok_ui::LUCIDE_FONT_BYTES)` there; `crate::LUCIDE_FONT_BYTES`
// here, the same constant, re-exported at this crate's own root) --
// without it, every lucide glyph in the snapshot is a placeholder box
// (confirmed during this task's own development).
#[allow(dead_code)]
pub(crate) fn snapshot(
    build: fn(Locale, TextScale) -> OrbokApp,
    locale: Locale,
    scale: TextScale,
    window: Size,
    task: &str,
    name: &str,
) {
    let app = build(locale, scale);
    let settings = iced::Settings {
        fonts: vec![crate::LUCIDE_FONT_BYTES.into()],
        ..iced::Settings::default()
    };
    let mut ui = Simulator::with_size(settings, window, app.view());
    let snap = ui
        .snapshot(&iced::Theme::Light)
        .unwrap_or_else(|e| panic!("{task}/{name}: could not render a snapshot: {e}"));
    // `cargo test`'s own working directory is this crate's manifest
    // directory, not the workspace root -- anchored here so the file
    // lands at `.git-exclude/evidence/<task>/` project-root relative
    // regardless (confirmed during this task's own development: without
    // this, it wrote to `crates/ui/.git-exclude/...` instead).
    let path = format!(
        "{}/../../.git-exclude/evidence/{task}/{name}.png",
        env!("CARGO_MANIFEST_DIR")
    );
    let matches = snap
        .matches_image(&path)
        .unwrap_or_else(|e| panic!("{task}/{name}: could not read/write {path}: {e}"));
    assert!(
        matches,
        "{task}/{name}: the rendered frame no longer matches the stored \
         baseline at {path} -- if this change is correct, delete the file \
         and re-run to record the new baseline"
    );
}
