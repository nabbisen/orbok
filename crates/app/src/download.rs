//! GUI adapter for RFC-050 serialized trusted model delivery.

use futures::channel::mpsc::Sender;
use futures::{SinkExt as _, StreamExt as _};
use orbok_models::ManagedModelStore;
use orbok_ui::state::{AppState, Message, WizardState};
use orbok_workers::{ModelDeliveryEvent, install_default_model};
use std::path::PathBuf;

/// The app adapter may start network work only from the explicit consent state.
pub(crate) fn consent_allows_start(state: &AppState) -> bool {
    matches!(state.wizard, Some(WizardState::DownloadConsent { .. }))
}

/// Run the reviewed model-delivery worker and translate typed worker events to
/// the existing wizard messages. Local paths are carried only in the success
/// message needed to configure the model; errors remain path-free.
pub async fn run(models_root: PathBuf, catalog_path: PathBuf, mut tx: Sender<Message>) {
    let catalog = match orbok_db::Catalog::open(catalog_path) {
        Ok(catalog) => catalog,
        Err(_) => {
            let _ = tx
                .send(Message::DownloadFailed(
                    "The model catalog could not be opened.".to_string(),
                ))
                .await;
            return;
        }
    };
    let store = ManagedModelStore::default_embedding(models_root);
    let (event_tx, mut event_rx) = futures::channel::mpsc::channel(64);
    let install = install_default_model(&catalog, &store, event_tx);
    tokio::pin!(install);

    let outcome = loop {
        tokio::select! {
            result = &mut install => break result,
            event = event_rx.next() => {
                let Some(ModelDeliveryEvent::FileProgress {
                    logical_name,
                    bytes,
                    total,
                    files_done,
                    files_total,
                }) = event else {
                    continue;
                };
                let _ = tx.send(Message::DownloadFileProgress {
                    file: logical_name.to_string(),
                    bytes,
                    total: Some(total),
                    files_done,
                    files_total,
                }).await;
            }
        }
    };

    match outcome {
        Ok(outcome) => {
            let _ = tx
                .send(Message::DownloadAllComplete {
                    dest_dir: outcome.generation_dir.to_string_lossy().to_string(),
                })
                .await;
        }
        Err(error) => {
            tracing::warn!(category = %error, "trusted model delivery failed");
            let _ = tx.send(Message::DownloadFailed(error.to_string())).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbok_ui::ModelDownloadConsent;

    #[test]
    fn network_start_requires_the_explicit_consent_state() {
        let mut state = AppState {
            wizard: Some(WizardState::NotConfigured),
            model_download_consent: Some(ModelDownloadConsent::trusted_default(
                "/managed/models".into(),
            )),
            ..Default::default()
        };

        assert!(!consent_allows_start(&state));
        state.update(&Message::DownloadModel);
        assert!(consent_allows_start(&state));
        state.update(&Message::CancelModelDownload);
        assert!(!consent_allows_start(&state));
    }
}
