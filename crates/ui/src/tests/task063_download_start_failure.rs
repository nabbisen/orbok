//! Task 063 §3 test 3: the page a download that could not start lands on
//! shows the store-unavailable copy, not the network failure copy.

use crate::i18n::{Locale, MessageKey, tr};
use crate::state::{
    AppState, ModelConsentReturn, ModelDeliveryFailure, ModelDownloadConsent, WizardState,
};
use crate::tests::iced_test_guard;
use crate::views;
use iced_test::simulator;

#[test]
fn a_download_that_could_not_start_shows_the_store_copy() {
    let _guard = iced_test_guard();
    for locale in Locale::ALL {
        let state = AppState {
            locale: *locale,
            wizard: Some(WizardState::DownloadFailed {
                presentation: ModelDownloadConsent::trusted_default("/managed/models".into()),
                return_to: ModelConsentReturn::NotConfigured,
                failure: ModelDeliveryFailure::StoreUnavailable,
            }),
            ..AppState::default()
        };
        let mut ui = simulator(views::wizard_view(&state));
        assert!(
            ui.find(tr(*locale, MessageKey::ModelDeliveryStoreUnavailable))
                .is_ok(),
            "{locale:?}: the store-unavailable copy renders"
        );
        assert!(
            ui.find(tr(*locale, MessageKey::ModelDeliveryConnection))
                .is_err(),
            "{locale:?}: not the network failure copy"
        );
    }
}
