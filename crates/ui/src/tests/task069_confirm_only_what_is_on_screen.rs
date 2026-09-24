//! Task 069: Enter confirms only a confirmation the user can see.

use crate::shell::{OrbokApp, key_to_message};
use crate::state::{
    AppState, FolderAddOrigin, Message, NavGroup, PendingFolderAdd, SourceCard, ViewId, WizardState,
};
use iced::keyboard::{Key, Modifiers, key::Named};

#[derive(Debug, Clone, Copy)]
enum Confirmation {
    Removal,
    /// Task 114: "Stop including subfolders?".
    Narrow,
    Reset,
    ClearHistory,
    /// Task 110: the private-folder question, from either page.
    AddOnFolders,
    AddOnSearch,
}

impl Confirmation {
    const ALL: [Confirmation; 6] = [
        Self::Removal,
        Self::Narrow,
        Self::Reset,
        Self::ClearHistory,
        Self::AddOnFolders,
        Self::AddOnSearch,
    ];

    /// The one view each confirmation renders on.
    fn home(self) -> ViewId {
        match self {
            Self::Removal | Self::Narrow => ViewId::Sources,
            Self::Reset => ViewId::Storage,
            Self::ClearHistory => ViewId::Settings,
            Self::AddOnFolders => ViewId::Sources,
            Self::AddOnSearch => ViewId::Search,
        }
    }

    fn open(self) -> Message {
        match self {
            Self::Removal => Message::AskRemoveSource("src-1".into()),
            Self::Narrow => Message::AskNarrowFolder("src-1".into()),
            Self::Reset => Message::AskResetCatalog,
            Self::ClearHistory => Message::AskClearRecentSearches,
            Self::AddOnFolders => Message::AskAddSensitiveFolder(PendingFolderAdd {
                path: "/home/u/.ssh".into(),
                origin: FolderAddOrigin::FoldersPage,
            }),
            Self::AddOnSearch => Message::AskAddSensitiveFolder(PendingFolderAdd {
                path: "/home/u/.ssh".into(),
                origin: FolderAddOrigin::SearchPage,
            }),
        }
    }

    fn is_confirm(self, message: &Option<Message>) -> bool {
        matches!(
            (self, message),
            (Self::Removal, Some(Message::ConfirmRemoveSource))
                | (Self::Narrow, Some(Message::ConfirmNarrowFolder))
                | (Self::Reset, Some(Message::ConfirmResetCatalog))
                | (
                    Self::ClearHistory,
                    Some(Message::ConfirmClearRecentSearches)
                )
                | (
                    Self::AddOnFolders | Self::AddOnSearch,
                    Some(Message::ConfirmAddSensitiveFolder)
                )
        )
    }

    fn is_open(self, state: &AppState) -> bool {
        match self {
            Self::Removal => state.confirm_remove_source.is_some(),
            Self::Narrow => state.confirm_narrow_source.is_some(),
            Self::Reset => state.confirm_reset,
            Self::ClearHistory => state.confirm_clear_history,
            Self::AddOnFolders | Self::AddOnSearch => state.pending_folder_add.is_some(),
        }
    }
}

fn opened_on_its_view(confirmation: Confirmation) -> AppState {
    let mut state = AppState {
        active_view: confirmation.home(),
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
    state.update(&confirmation.open());
    state
}

/// Enter, with the context built exactly as `orbok` builds it.
fn enter(state: &AppState) -> Option<Message> {
    let app = OrbokApp::with_state(state.clone());
    key_to_message(
        &Key::Named(Named::Enter),
        Modifiers::default(),
        &app.keyboard_context(),
    )
}

/// A switch that leaves `view` -- both the tab bar's `Switch` and the
/// sidebar's `SwitchGroup`.
fn switches_away_from(view: ViewId) -> [Message; 2] {
    let (other_view, other_group) = match view.group() {
        NavGroup::Search => (ViewId::Settings, NavGroup::Settings),
        NavGroup::Ai => (ViewId::Search, NavGroup::Search),
        NavGroup::Settings => (ViewId::Search, NavGroup::Search),
    };
    [
        Message::Switch(other_view),
        Message::SwitchGroup(other_group),
    ]
}

/// §2 test 1: every switching message × every confirmation.
#[test]
fn switching_view_closes_every_confirmation_and_enter_cannot_confirm_it() {
    let mut failures = Vec::new();
    for confirmation in Confirmation::ALL {
        for switch in switches_away_from(confirmation.home()) {
            let mut state = opened_on_its_view(confirmation);
            assert!(confirmation.is_open(&state), "{confirmation:?} opened");
            state.update(&switch);
            let still_open = confirmation.is_open(&state);
            let enter = enter(&state);
            if still_open || confirmation.is_confirm(&enter) {
                failures.push(format!(
                    "{confirmation:?} after {switch:?}: still open={still_open}, Enter -> {enter:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "unseen confirmations:\n{}",
        failures.join("\n")
    );
}

/// §2 test 2: a wizard over an open confirmation takes Enter.
#[test]
fn a_wizard_over_an_open_confirmation_never_confirms_it() {
    let mut state = opened_on_its_view(Confirmation::Reset);
    state.wizard = Some(WizardState::NotConfigured);
    let got = enter(&state);
    assert!(
        !Confirmation::Reset.is_confirm(&got),
        "Enter under a wizard must not reset, got {got:?}"
    );
    assert!(
        matches!(got, Some(Message::DownloadModel)),
        "Enter yields the wizard's action, got {got:?}"
    );
}

/// §2 test 3: on its own view, with no wizard, each is still confirmed.
#[test]
fn on_its_own_view_each_confirmation_is_confirmed_by_enter() {
    for confirmation in Confirmation::ALL {
        let state = opened_on_its_view(confirmation);
        let got = enter(&state);
        assert!(
            confirmation.is_confirm(&got),
            "{confirmation:?} on its own view: Enter -> {got:?}"
        );
    }
}
