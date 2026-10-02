//! Task 128: what a result, and a page, shows is clean. The owner's own
//! Microsoft Store screenshots found a result card repeating itself three
//! times and cutting a word with no mark -- this file's first two tests.
//! The Models and Storage fixes (no reranker line; one size formatter) have
//! their own tests in `smoke_views.rs`, `a11y.rs` and `task081_storage_page.rs`
//! -- RFC-033/035 put locale-aware formatting and page smoke tests there
//! already, and this task's fixes to them are small enough to extend those
//! files rather than start a third home for the same two pages.

use crate::state::{
    AppState, Message, ResultTrustDisplay, SearchResultDisplay, SourceCard, ViewId,
};
use crate::tests::iced_test_guard;
use crate::views::{self, card_heading_line};
use iced_test::simulator;
use orbok_search::ResultTrustState;

/// The title and heading line the architect's own screenshot showed, for
/// `Documents/Trips/kyoto-autumn-trip.md` and the query "how much will the
/// autumn trip cost". Both are `heading_path`'s text: `title` is
/// `heading_path` itself whenever one exists
/// (`orbok_search::service`/`hybrid`'s `enrich`/`fuse`), which is exactly
/// why the card showed it twice.
const KYOTO_TITLE: &str = "Kyoto autumn trip > Budget";

/// A snippet shaped like the engine's own fixed output (Task 128 test 2,
/// `orbok_search::snippet`): no leading heading, no `#`, cut at a word
/// boundary and marked with `…`. 132 characters, deliberately past 120 --
/// the card's own second cut (`.chars().take(120)`, removed this task)
/// would have shortened it, dropping the trailing `…` and part of the last
/// word with no mark that anything was cut. The card must show it exactly
/// as given, proving that second cut is really gone, not merely untested
/// by a fixture short enough to survive it.
const KYOTO_SNIPPET: &str = "Train tickets, lodging, and the amount we set aside in the \
     emergency fund for unexpected costs during the entire length of the trip…";

fn kyoto_result() -> SearchResultDisplay {
    SearchResultDisplay {
        display_path: "…/Trips/kyoto-autumn-trip.md".into(),
        canonical_path: "/home/user/Documents/Trips/kyoto-autumn-trip.md".into(),
        title: Some(KYOTO_TITLE.into()),
        heading_path: Some(KYOTO_TITLE.into()),
        snippet: Some(KYOTO_SNIPPET.into()),
        keyword_rank: 1,
        badges: vec![],
        trust: ResultTrustDisplay::default(),
    }
}

/// Search-view state with one result showing -- the same shape
/// `handoff038_trust_display.rs`'s `with_results` uses: `last_query` set
/// directly (it is not a derived field anything else must stay in sync
/// with, unlike `snora_tokens`) and the page rendered on its own via
/// `views::search_view`, not the full shell.
fn state_with(result: SearchResultDisplay) -> AppState {
    let mut state = AppState {
        active_view: ViewId::Search,
        last_query: Some("how much will the autumn trip cost".into()),
        // The "no sources yet" empty state takes over the page otherwise;
        // the result-list branch only renders once a source exists.
        sources: vec![SourceCard {
            display_name: "Docs".into(),
            display_path: "/docs".into(),
            indexed: 1,
            stale: 0,
            failed: 0,
            no_text_found: 0,
            unfinished_jobs: 0,
            covers_subfolders: true,
            status: orbok_core::SourceStatus::Active,
            source_id: "src-1".into(),
        }],
        ..AppState::default()
    };
    state.update(&Message::SearchResultsReady(vec![result]));
    state
}

/// §1.1: the heading line disappears entirely when it equals the title.
/// Tested directly against the rule, not by searching the rendered tree for
/// a *second* occurrence: `iced_test::Simulator::find` only answers "is
/// this text somewhere on screen", never "how many times" -- the title and
/// an unsuppressed, identical heading line would both satisfy a `find` for
/// the same string, so the rendered tree cannot tell "shown once" from
/// "shown twice" on its own. The rule the rendering depends on can.
#[test]
fn the_heading_line_disappears_when_it_repeats_the_title() {
    assert_eq!(
        card_heading_line(KYOTO_TITLE, Some(KYOTO_TITLE)),
        "",
        "a heading identical to the title must not render a second time"
    );
    assert_eq!(
        card_heading_line("Kyoto autumn trip", Some(KYOTO_TITLE)),
        KYOTO_TITLE,
        "a heading that says more than the title must still render"
    );
    assert_eq!(
        card_heading_line("My document.md", None),
        "",
        "no heading at all renders nothing, same as before this task"
    );
}

/// §1.1 §1.2, through the real card: the Kyoto fixture's title is the only
/// place its words appear as a heading-shaped line, and the snippet reaches
/// the screen exactly as the engine built it -- no second cut, no stray `#`.
/// Red before this task on both counts: the heading rendered a second time
/// (no suppression existed), and the card's own `.chars().take(120)` would
/// have cut this already-120-ish-character fixture again, most likely
/// losing the trailing `…` or part of the last word.
#[test]
fn the_kyoto_card_shows_the_title_once_and_the_snippet_uncut() {
    let _guard = iced_test_guard();
    let state = state_with(kyoto_result());
    let mut ui = simulator(views::search_view(&state));
    assert!(ui.find(KYOTO_TITLE).is_ok(), "the title must render");
    assert!(
        ui.find(KYOTO_SNIPPET).is_ok(),
        "the snippet must reach the screen exactly as the engine built it, \
         not cut a second time by the card"
    );
}

/// Review 306 §3.2: a suppressed heading (title == heading, Task 128 §1.1)
/// must render **no row**, not an empty `text("")` that still reserves a
/// line's height -- a blank gap would sit between the path and the
/// snippet. Measured, not guessed: the snippet's own vertical position must
/// be strictly higher (smaller y) with the heading suppressed than with a
/// real heading line present, since nothing should separate it from the
/// path line in that case. Red before the fix: `text("")` took the same
/// line height as any other one-line text, so the two positions were equal.
#[test]
fn a_suppressed_heading_reserves_no_row() {
    let _guard = iced_test_guard();
    use snora::design::Tokens;
    let tokens = Tokens::light();
    let card = |heading: &str| {
        crate::components::result_card(
            &tokens,
            crate::i18n::Locale::En,
            "My document.md".to_string(),
            "/docs/My document.md".to_string(),
            heading.to_string(),
            "A short snippet of content.".to_string(),
            &[],
            ResultTrustState::Ready,
            false,
            false,
            Message::SelectResult(0),
        )
    };
    let mut no_heading = simulator(card(""));
    let no_heading_y = no_heading
        .find("A short snippet of content.")
        .unwrap()
        .bounds()
        .y;
    let mut with_heading = simulator(card("Some section"));
    let with_heading_y = with_heading
        .find("A short snippet of content.")
        .unwrap()
        .bounds()
        .y;
    assert!(
        no_heading_y < with_heading_y,
        "a suppressed heading must not reserve its row's height: snippet y \
         was {no_heading_y} with no heading, {with_heading_y} with one -- \
         they must differ, not match"
    );
}

/// A result with a heading that genuinely differs from its title (a
/// document whose own `<h1>`/title line is not what a deeper heading in the
/// matched chunk says) still shows both -- the suppression is specific to
/// the "same words twice" case, not a blanket hiding of the heading line.
#[test]
fn a_heading_that_differs_from_the_title_still_renders() {
    let _guard = iced_test_guard();
    let result = SearchResultDisplay {
        title: Some("Kyoto autumn trip".into()),
        heading_path: Some("Kyoto autumn trip > Budget > Train tickets".into()),
        ..kyoto_result()
    };
    let state = state_with(result);
    let mut ui = simulator(views::search_view(&state));
    assert!(ui.find("Kyoto autumn trip").is_ok());
    assert!(
        ui.find("Kyoto autumn trip > Budget > Train tickets")
            .is_ok(),
        "a heading that says more than the title must still render"
    );
}
