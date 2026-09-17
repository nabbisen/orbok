//! Task 071 §4 tests 2-3: the startup-failure window's copy, and Close.

use crate::i18n::Locale;
use crate::tests::iced_test_guard;
use crate::theme::Theme;
use crate::views::startup_failure::{
    StartupFailureCause, StartupFailureMessage, StartupFailureScreen, startup_failure_key,
};
use iced::keyboard::Key;
use iced::keyboard::key::Named;
use iced_test::simulator;

const PATH: &str = "/media/backup/orbok-data";

fn screen(cause: StartupFailureCause, locale: Locale) -> StartupFailureScreen {
    StartupFailureScreen {
        cause,
        locale,
        theme: Theme::Light,
        tokens: Theme::Light.tokens(),
    }
}

/// §4 test 2: exact title, body and Close for each cause, both locales;
/// the data-folder body names the path.
#[test]
fn each_cause_renders_its_approved_copy() {
    let _guard = iced_test_guard();
    let cases = [
        (
            Locale::En,
            "orbok could not start",
            "Close",
            [
                format!(
                    "orbok could not use its data folder: {PATH}. Check that the drive is connected and that you can open the folder, then start orbok again."
                ),
                "This data was created by a newer version of orbok. Update orbok, then start it again.".to_string(),
                "orbok could not open its data. Try starting it again.".to_string(),
            ],
        ),
        (
            Locale::Ja,
            "orbok を起動できませんでした",
            "閉じる",
            [
                format!(
                    "orbok のデータフォルダーを使用できませんでした: {PATH}。ドライブが接続されていて、フォルダーを開けることを確認してから、もう一度起動してください。"
                ),
                "このデータは新しいバージョンの orbok で作成されています。orbok を更新してから、もう一度起動してください。".to_string(),
                "orbok のデータを開けませんでした。もう一度起動してみてください。".to_string(),
            ],
        ),
    ];
    for (locale, title, close, [data_folder, newer, other]) in cases {
        for (cause, body) in [
            (
                StartupFailureCause::DataFolder { path: PATH.into() },
                data_folder,
            ),
            (StartupFailureCause::NewerData, newer),
            (StartupFailureCause::Other, other),
        ] {
            let screen = screen(cause.clone(), locale);
            assert_eq!(screen.title(), title, "{locale:?} {cause:?}");
            let mut ui = simulator(screen.view());
            for expected in [title, body.as_str(), close] {
                assert!(
                    ui.find(expected).is_ok(),
                    "{locale:?} {cause:?}: {expected:?} renders"
                );
            }
        }
    }
}

/// §4 test 3: Close sends the exit message, and Escape maps to the same.
#[test]
fn close_and_escape_both_exit() {
    let _guard = iced_test_guard();
    for locale in [Locale::En, Locale::Ja] {
        let screen = screen(StartupFailureCause::Other, locale);
        let mut ui = simulator(screen.view());
        let close = match locale {
            Locale::En => "Close",
            Locale::Ja => "閉じる",
        };
        let _ = ui.click(close);
        let messages: Vec<_> = ui.into_messages().collect();
        assert_eq!(messages, vec![StartupFailureMessage::Close], "{locale:?}");
    }
    assert_eq!(
        startup_failure_key(&Key::Named(Named::Escape)),
        Some(StartupFailureMessage::Close)
    );
    assert_eq!(startup_failure_key(&Key::Named(Named::Enter)), None);
}
