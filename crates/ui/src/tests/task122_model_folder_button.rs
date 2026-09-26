//! Task 122: choosing a model folder works like choosing any folder. The page
//! has a "Choose a folder" button (the Folders page's shape) and no Validate
//! button: the check happens when the user chooses or presses Enter.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{AppState, Message, WizardFileCheck, WizardState};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;

/// The catalog string the deleted button showed, per locale.
fn validate_label(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Validate",
        Locale::Ja => "検証",
    }
}

fn setup_state(locale: Locale) -> AppState {
    AppState {
        locale,
        wizard: Some(WizardState::NotConfigured),
        ..AppState::default()
    }
}

fn missing_a_file_state(locale: Locale) -> AppState {
    AppState {
        locale,
        wizard: Some(WizardState::Checked {
            model_dir: "/somewhere".into(),
            checks: vec![WizardFileCheck {
                relative_path: "tokenizer.json".into(),
                found: false,
                size_mb: None,
            }],
            all_ok: false,
        }),
        ..AppState::default()
    }
}

/// §2 test 5: no Validate button, in either locale, on either page that asks for
/// a model folder; and each has "Choose a folder", which sends
/// `WizardChooseFolder` (§2 test 1's view half).
#[test]
fn the_page_has_choose_a_folder_and_no_validate_button() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        for (page, state) in [
            ("setup", setup_state(locale)),
            ("missing a file", missing_a_file_state(locale)),
        ] {
            let mut ui = simulator(views::wizard_view(&state));
            assert!(
                ui.find(validate_label(locale)).is_err(),
                "{locale:?}, {page}: there is no Validate button"
            );
            let choose = tr(locale, MessageKey::SearchChooseFolder);
            assert!(
                ui.find(choose).is_ok(),
                "{locale:?}, {page}: \"{choose}\" is on the page"
            );
            let _ = ui.click(choose);
            let messages: Vec<Message> = ui.into_messages().collect();
            assert!(
                matches!(messages.as_slice(), [Message::WizardChooseFolder]),
                "{locale:?}, {page}: the button opens the picker, got {messages:?}"
            );
        }
    }
}

/// While the picker is open the button does nothing (it is drawn unavailable).
#[test]
fn the_button_is_unavailable_while_the_picker_is_open() {
    let _guard = iced_test_guard();
    let mut state = setup_state(Locale::En);
    state.wizard_picker_in_progress = true;
    let mut ui = simulator(views::wizard_view(&state));
    let label = tr(Locale::En, MessageKey::SearchChooseFolder);
    assert!(ui.find(label).is_ok(), "the button is still on the page");
    let _ = ui.click(label);
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(messages.is_empty(), "no second picker: {messages:?}");
}

/// The picker's answer fills the field; a cancelled picker changes nothing else.
#[test]
fn a_picked_folder_fills_the_field_and_a_cancel_leaves_it() {
    let mut state = setup_state(Locale::En);
    state.wizard_path_input = "typed".into();
    state.update(&Message::WizardChooseFolder);
    state.update(&Message::WizardFolderPickerCancelled);
    assert_eq!(state.wizard_path_input, "typed", "cancel changes nothing");
    assert!(!state.wizard_picker_in_progress);

    state.update(&Message::WizardChooseFolder);
    state.update(&Message::WizardFolderPicked("/models/e5".into()));
    assert_eq!(state.wizard_path_input, "/models/e5");
    assert!(!state.wizard_picker_in_progress);
}
