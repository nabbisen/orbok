//! Application shell (RFC-027): snora `AppLayout` with a two-level navigation:
//! a vertical sidebar for the three top-level groups (Search, AI, Settings) and
//! a horizontal tab bar for the sub-views within each group.
//!
//! RFC-034: [`key_to_message`] is the pure keyboard-map function. It is called
//! from `orbok` via an `iced::keyboard::on_key_press` subscription. Keeping
//! it here (in `orbok-ui`) means it is unit-testable without the iced runtime.

use crate::i18n::{MessageKey, tr};
use crate::state::{AppState, Message, NavGroup, ViewId, WizardKind};
use crate::views;
use iced::Element;
use snora::lucide;
use snora::{
    AppLayout, Icon, LayoutDirection, SideBar, SideBarItem, Tab, TabBar, render,
    widget::{app_side_bar, app_tab_bar},
};

/// Everything [`key_to_message`] needs beyond the raw key event, snapshotted
/// once per frame by `orbok`'s keyboard subscription (RFC-034 §2.1.1 / Task
/// 024).
///
/// Deliberately not `&AppState`: this travels through
/// `iced::Subscription::with`, which requires its payload to be `Hash` (and,
/// separately, cloning the whole app state into every subscription rebuild
/// would be wasteful). Each field is the smallest piece of state that
/// changes *which* keyboard shortcut applies, not the data behind it --
/// `selected_source_id` carries the one string `Enter` would need to build
/// `Message::SourceRemoved`, not the source list itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyboardContext {
    /// Approximates real widget focus (iced 0.14 cannot query it directly);
    /// see [`Message::FocusSearch`]'s own doc comment for why this is an
    /// approximation, not a certainty.
    pub text_input_focused: bool,
    pub active_view: ViewId,
    pub confirm_reset: bool,
    pub confirm_clear_history: bool,
    /// `None` when the startup wizard is not active.
    pub wizard_kind: Option<WizardKind>,
    /// The source `Enter` would remove, if the Sources view has one
    /// selected. `None` either because nothing is selected or because the
    /// Sources view is not active.
    pub selected_source_id: Option<String>,
}

/// Map a key event to a [`Message`], or `None` to let iced handle it normally.
///
/// **Text-input safety:** when `ctx.text_input_focused` is `true`, only
/// global shortcuts that do *not* intercept printable characters are fired
/// (Ctrl/Cmd combos, Escape, and `Tab`, which text inputs also expect to
/// move focus rather than insert). Arrow keys and Enter are suppressed
/// while text input has focus so typing is never hijacked.
///
/// This function is pure and contains no iced runtime state, so it can be
/// called from tests without a display server.
pub fn key_to_message(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    ctx: &KeyboardContext,
) -> Option<Message> {
    use iced::keyboard::Key;
    use iced::keyboard::key::Named;

    match key {
        // Ctrl/Cmd + K  →  focus global search input (works from any view).
        Key::Character(c) if c.as_str() == "k" && modifiers.command() => Some(Message::FocusSearch),
        // Ctrl/Cmd + ,  →  open Settings.
        Key::Character(c) if c.as_str() == "," && modifiers.command() => {
            Some(Message::Switch(ViewId::Settings))
        }
        // Ctrl/Cmd + 1..6  →  jump directly to a view (RFC-034 §2.1.1 /
        // Task 024 §3.2). The same primary-modifier convention as the two
        // shortcuts above, applied to the fixed six-view navigation
        // surface `ViewId::ALL` already enumerates -- not a literal
        // bare-Ctrl binding, for the same cross-platform reason Ctrl+K
        // uses `command()` rather than `Modifiers::CTRL`.
        Key::Character(c) if modifiers.command() && c.as_str() == "1" => {
            Some(Message::Switch(ViewId::Search))
        }
        Key::Character(c) if modifiers.command() && c.as_str() == "2" => {
            Some(Message::Switch(ViewId::Sources))
        }
        Key::Character(c) if modifiers.command() && c.as_str() == "3" => {
            Some(Message::Switch(ViewId::Indexing))
        }
        Key::Character(c) if modifiers.command() && c.as_str() == "4" => {
            Some(Message::Switch(ViewId::Storage))
        }
        Key::Character(c) if modifiers.command() && c.as_str() == "5" => {
            Some(Message::Switch(ViewId::Models))
        }
        Key::Character(c) if modifiers.command() && c.as_str() == "6" => {
            Some(Message::Switch(ViewId::Settings))
        }
        // Tab / Shift+Tab  →  move focus among the widgets iced 0.14 can
        // focus at all: `text_input`/`text_editor` only (RFC-034 §2.1.1 /
        // Task 024 §3.1). Not gated on `text_input_focused` -- moving
        // focus while inside a text input is exactly what Tab is for.
        // Unlike the rest of this map, the actual focus movement is not
        // this function's job: it returns the *intent*, and `orbok` turns
        // it into an `iced::widget::operation::focus_next`/`focus_previous`
        // `Task` (see that call site's own comment).
        Key::Named(Named::Tab) if modifiers.shift() => Some(Message::FocusPrevious),
        Key::Named(Named::Tab) => Some(Message::FocusNext),
        // Escape on the Downloading page  →  request cancellation (Task
        // 027 §3.1, Task 025's Cancel button). Must come before the
        // general Escape arm below: `DismissOverlay` is handled entirely
        // by `AppState::update`, which is pure UI state and cannot issue
        // the backend effect this needs (setting the cancellation flag);
        // binding the `Message` directly here reaches `model_flow::reduce`
        // instead, exactly as the mouse-driven Cancel button already does
        // (see `main.rs:97`'s unconditional `model_flow::reduce` call).
        Key::Named(Named::Escape) if ctx.wizard_kind == Some(WizardKind::Downloading) => {
            Some(Message::CancelDownloadInProgress)
        }
        // Escape  →  close any open overlay / dialog; restore focus to
        // trigger. `AppState::update`'s `DismissOverlay` handler decides
        // what "close" means from the state it already holds (confirm
        // dialogs, the wizard, list selections) -- see that handler's own
        // comment for why the decision lives there and not here.
        Key::Named(Named::Escape) => Some(Message::DismissOverlay),
        // Enter while a text input is (approximately) focused  →  submit
        // search. Unchanged from before Task 024.
        Key::Named(Named::Enter) if ctx.text_input_focused => Some(Message::SubmitSearch),
        // Enter while NOT typing  →  whatever this screen's primary/
        // confirming action is, if any (RFC-034 §2.1.1 / Task 024 §3.4):
        // a confirm dialog's Confirm, the active wizard page's forward
        // action, or removing a selected source. `confirm_message` is the
        // one place that decision is made, so `Enter` and a mouse click
        // on the matching button always dispatch the identical `Message`
        // -- there is no separate "keyboard version" of any of these
        // actions to drift out of sync.
        Key::Named(Named::Enter) => confirm_message(ctx),
        // Arrow keys  →  move the selection the active view owns, only
        // when NOT typing. Search and Sources each have their own list
        // and their own pair of messages (RFC-034 §2.1.1 / Task 024 §3.3)
        // -- gating on `active_view` is what stops, say, Sources' arrows
        // from silently moving Search's selection underneath it.
        Key::Named(Named::ArrowDown)
            if !ctx.text_input_focused && ctx.active_view == ViewId::Search =>
        {
            Some(Message::SelectNextResult)
        }
        Key::Named(Named::ArrowUp)
            if !ctx.text_input_focused && ctx.active_view == ViewId::Search =>
        {
            Some(Message::SelectPrevResult)
        }
        Key::Named(Named::ArrowDown)
            if !ctx.text_input_focused && ctx.active_view == ViewId::Sources =>
        {
            Some(Message::SelectNextSource)
        }
        Key::Named(Named::ArrowUp)
            if !ctx.text_input_focused && ctx.active_view == ViewId::Sources =>
        {
            Some(Message::SelectPrevSource)
        }
        // Everything else: let iced handle it (printable keys, etc.).
        _ => None,
    }
}

/// What `Enter` (while not typing) confirms, if anything -- the single
/// place that decision is made for [`key_to_message`]. Each arm returns
/// the *exact* `Message` the corresponding button's `on_press` already
/// uses, so this never introduces a second, keyboard-only code path for
/// any of these actions; `model_flow::reduce` and the handlers in
/// `crates/app/src/main.rs` that give some of them real backend effects
/// (starting a download, resetting the catalog, clearing history) run
/// identically regardless of whether the message originated from a click
/// or from here.
fn confirm_message(ctx: &KeyboardContext) -> Option<Message> {
    if ctx.confirm_reset {
        return Some(Message::ConfirmResetCatalog);
    }
    if ctx.confirm_clear_history {
        return Some(Message::ConfirmClearRecentSearches);
    }
    if let Some(kind) = ctx.wizard_kind {
        return match kind {
            // Setup's primary action, DownloadModel (Task 027 §3.2/§3.3
            // in the review request -- corrects Review 185 §4's reasoning,
            // verified live before relying on it). Setup and CheckedNotOk
            // each also render a `text_input` with its own
            // `on_submit(WizardValidate)`; the concern when Task 024 first
            // shipped this map was that a global Enter binding here could
            // fire *alongside* that native submit and double-dispatch.
            // That concern does not hold: iced's `text_input` calls
            // `shell.capture_event()` on every Enter it receives while it
            // genuinely has focus (`iced_widget::text_input`, the
            // `Key::Named(Named::Enter)` arm inside its `is_focused` guard
            // -- unconditional on modifiers), and `iced::keyboard::listen()`
            // -- the subscription `key_to_message` runs through -- only
            // ever receives events the widget tree left
            // `Status::Ignored` (`iced_futures::keyboard::listen`'s own
            // filter). A captured Enter never reaches this function at
            // all. Verified against the real running app, not just read
            // from source: with the path input Tab-focused and text
            // typed, Enter reached only `WizardValidate` (the Checked
            // page rendered, not DownloadConsent); with nothing focused,
            // the same Enter reached only this arm (DownloadConsent
            // rendered). No path produced both. `CheckedNotOk`'s own
            // primary action is `WizardValidate` itself -- already
            // reachable through the text input's native submit whenever
            // it has focus -- so it stays unbound here rather than bind a
            // second, redundant path to the same message; not reconsidered
            // further since it was outside this task's scope.
            WizardKind::Setup => Some(Message::DownloadModel),
            WizardKind::CheckedNotOk => None,
            WizardKind::DownloadConsent => Some(Message::ConfirmModelDownload),
            WizardKind::CheckedOk | WizardKind::ReadyIdle | WizardKind::ReadyFailed => {
                Some(Message::WizardAccept)
            }
            WizardKind::DownloadFailed => Some(Message::RetryModelDownload),
            // Downloading's only action is Cancel (Task 025), reachable
            // via Escape above -- not Enter, since Escape is already the
            // convention this map uses for "stop/back out" actions
            // (DownloadConsent's own Cancel mirrors the same key). Ready
            // while a save is in flight has nothing to confirm either.
            WizardKind::Downloading | WizardKind::ReadyInFlight => None,
        };
    }
    if ctx.active_view == ViewId::Sources {
        return ctx.selected_source_id.clone().map(Message::SourceRemoved);
    }
    None
}

fn tab_action_to_msg(action: snora::TabAction<ViewId>) -> Message {
    let snora::TabAction::Pressed(id) = action;
    Message::Switch(id)
}

fn build_tab_bar(tabs: Vec<Tab<ViewId>>, active: ViewId) -> Element<'static, Message> {
    app_tab_bar(
        TabBar { tabs, active },
        &tab_action_to_msg,
        LayoutDirection::Ltr,
    )
}

/// The iced application wrapper around [`AppState`].
#[derive(Default)]
pub struct OrbokApp {
    pub state: AppState,
    /// Whether the global search text input currently holds keyboard focus.
    /// Tracked so [`key_to_message`] can distinguish text entry from navigation.
    pub search_focused: bool,
}

impl OrbokApp {
    pub fn with_state(state: AppState) -> Self {
        Self {
            state,
            search_focused: false,
        }
    }

    pub fn update(&mut self, message: Message) {
        if matches!(message, Message::FocusSearch) {
            self.search_focused = true;
        }
        // Typing in search clears the focus flag (next keypress will be text).
        if matches!(message, Message::QueryChanged(_)) {
            self.search_focused = false;
        }
        self.state.update(&message);
    }

    pub fn view(&self) -> Element<'_, Message> {
        let locale = self.state.locale;

        // ── Startup wizard takes priority ──────────────────────────────
        if self.state.wizard.is_some() {
            return views::wizard_view(&self.state);
        }

        // ── Sidebar: three top-level groups ───────────────────────────
        let sidebar_items: Vec<SideBarItem<Message, NavGroup>> = vec![
            SideBarItem {
                view_id: NavGroup::Search,
                icon: Icon::Lucide(lucide::Search),
                tooltip: tr(locale, MessageKey::NavSearch).to_string(),
                on_press: Message::SwitchGroup(NavGroup::Search),
            },
            SideBarItem {
                view_id: NavGroup::Ai,
                icon: Icon::Lucide(lucide::BrainCircuit),
                tooltip: tr(locale, MessageKey::NavAi).to_string(),
                on_press: Message::SwitchGroup(NavGroup::Ai),
            },
            SideBarItem {
                view_id: NavGroup::Settings,
                icon: Icon::Lucide(lucide::Settings),
                tooltip: tr(locale, MessageKey::NavSettings).to_string(),
                on_press: Message::SwitchGroup(NavGroup::Settings),
            },
        ];
        let side_bar = app_side_bar(
            SideBar {
                items: sidebar_items,
                active: self.state.active_view.group(),
            },
            LayoutDirection::Ltr,
        );

        // ── Tab bar: sub-views within the active group ─────────────────
        let tab_bar_el: Option<Element<'_, Message>> = match self.state.active_view.group() {
            NavGroup::Search => Some(build_tab_bar(
                vec![
                    Tab {
                        id: ViewId::Search,
                        label: tr(locale, MessageKey::NavSearch).to_string(),
                        icon: None,
                    },
                    Tab {
                        id: ViewId::Sources,
                        label: tr(locale, MessageKey::NavSources).to_string(),
                        icon: None,
                    },
                ],
                self.state.active_view,
            )),
            NavGroup::Ai => Some(build_tab_bar(
                vec![
                    Tab {
                        id: ViewId::Indexing,
                        label: tr(locale, MessageKey::NavIndexing).to_string(),
                        icon: None,
                    },
                    Tab {
                        id: ViewId::Storage,
                        label: tr(locale, MessageKey::NavStorage).to_string(),
                        icon: None,
                    },
                    Tab {
                        id: ViewId::Models,
                        label: tr(locale, MessageKey::NavModels).to_string(),
                        icon: None,
                    },
                ],
                self.state.active_view,
            )),
            NavGroup::Settings => None,
        };

        // ── Active page body ───────────────────────────────────────────
        let page_body = match self.state.active_view {
            ViewId::Search => views::search_view(&self.state),
            ViewId::Sources => views::sources_view(&self.state),
            ViewId::Indexing => views::indexing_view(&self.state),
            ViewId::Storage => views::storage_view(&self.state),
            ViewId::Models => views::models_view(&self.state),
            ViewId::Settings => views::settings_view(&self.state),
        };

        // Compose: tab bar (if any) stacked above the page body.
        let body: Element<'_, Message> = if let Some(tabs) = tab_bar_el {
            iced::widget::column![tabs, page_body].spacing(0).into()
        } else {
            page_body
        };

        render(AppLayout::new(body).side_bar(side_bar))
    }

    pub fn title(&self) -> String {
        tr(self.state.locale, MessageKey::AppTitle).to_string()
    }

    /// Map the active snora token palette to an `iced::Theme` so iced uses
    /// the correct background, text, and accent colors. Without this, iced
    /// always renders with its built-in Light theme regardless of which snora
    /// preset is active.
    ///
    /// `iced::Theme::Custom` accepts an `iced::theme::Palette` with six roles.
    /// We map the snora palette's semantic roles to those six fields.
    pub fn iced_theme(&self) -> iced::Theme {
        use snora::design::style::color::to_iced_color;
        let p = &self.state.tokens.palette;
        let is_dark = matches!(
            self.state.theme,
            crate::theme::Theme::Dark | crate::theme::Theme::HighContrastDark
        );
        let palette = iced::theme::Palette {
            background: to_iced_color(p.background),
            text: to_iced_color(p.text_primary),
            primary: to_iced_color(p.accent),
            success: to_iced_color(p.success),
            warning: to_iced_color(p.warning),
            danger: to_iced_color(p.danger),
        };
        let name = if is_dark { "orbok-dark" } else { "orbok-light" };
        iced::Theme::custom(name, palette)
    }
}
