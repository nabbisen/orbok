//! Task 071: the window shown when orbok cannot start.
//!
//! A startup failure used to reach only stderr, which a desktop launch never
//! shows -- on Windows, with no console at all, the app simply did not
//! appear. This is a separate, minimal iced application: no `AppState`, no
//! catalog, no subscriptions beyond Escape. Its only action is Close; "start
//! again" means relaunching, so nothing is ever half-open.

use crate::i18n::{Locale, MessageKey, startup_failed_data_folder_body, tr};
use crate::theme::{self, TextScale, Theme};
use iced::keyboard::Key;
use iced::keyboard::key::Named;
use iced::widget::{button, column, container, text};
use iced::{Element, Length, Padding};
use snora::design::Tokens;

/// Why startup failed, as the window explains it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupFailureCause {
    /// The data folder could not be created, authorised or opened.
    DataFolder { path: String },
    /// The data was written by a newer orbok.
    NewerData,
    /// Anything else.
    Other,
}

/// The window's only message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupFailureMessage {
    /// Close, Escape: exit.
    Close,
}

/// Everything the window renders. Tokens are passed in explicitly, since
/// there is no `AppState`.
#[derive(Debug, Clone)]
pub struct StartupFailureScreen {
    pub cause: StartupFailureCause,
    pub locale: Locale,
    pub theme: Theme,
    pub tokens: Tokens,
}

impl StartupFailureScreen {
    pub fn title(&self) -> String {
        tr(self.locale, MessageKey::StartupFailedTitle).to_string()
    }

    /// The body for this cause, in this locale.
    pub fn body(&self) -> String {
        match &self.cause {
            StartupFailureCause::DataFolder { path } => {
                startup_failed_data_folder_body(self.locale, path)
            }
            StartupFailureCause::NewerData => {
                tr(self.locale, MessageKey::StartupFailedNewerDataBody).to_string()
            }
            StartupFailureCause::Other => {
                tr(self.locale, MessageKey::StartupFailedOtherBody).to_string()
            }
        }
    }

    pub fn iced_theme(&self) -> iced::Theme {
        crate::shell::iced_theme_for(self.theme, &self.tokens)
    }

    pub fn view(&self) -> Element<'_, StartupFailureMessage> {
        let tokens = &self.tokens;
        let sc = TextScale::default();
        let content = column![
            text(self.title()).size(theme::title_s(tokens, sc)),
            text(self.body())
                .size(theme::body_s(tokens, sc))
                .line_height(theme::body_lh(tokens)),
            button(
                text(tr(self.locale, MessageKey::StartupFailedClose))
                    .size(theme::body_s(tokens, sc))
            )
            .on_press(StartupFailureMessage::Close),
        ]
        .spacing(tokens.spacing.md);
        container(content)
            .padding(Padding::from([tokens.spacing.xl, tokens.spacing.xl]))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// The window's keyboard: Escape closes, like the button.
pub fn startup_failure_key(key: &Key) -> Option<StartupFailureMessage> {
    match key {
        Key::Named(Named::Escape) => Some(StartupFailureMessage::Close),
        _ => None,
    }
}
