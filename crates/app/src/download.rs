//! GUI adapter for RFC-050 serialized trusted model delivery.

use futures::channel::mpsc::Sender;
use futures::{SinkExt as _, StreamExt as _};
use orbok_models::ManagedModelStore;
use orbok_ui::state::{Message, ModelArtifact, ModelDeliveryFailure};
use orbok_workers::{
    ModelDeliveryError, ModelDeliveryEvent, ModelDeliveryOutcome, install_default_model,
};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

type InstallerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelDeliveryOutcome, ModelDeliveryError>> + Send + 'a>>;

/// Run the reviewed production entry and translate its typed events. The
/// binding to `install_default_model` is deliberately direct and reviewable.
pub async fn run(models_root: PathBuf, catalog_path: PathBuf, mut tx: Sender<Message>) {
    let catalog = match orbok_db::Catalog::open(catalog_path) {
        Ok(catalog) => catalog,
        Err(_) => {
            let _ = tx
                .send(Message::DownloadFailed(
                    ModelDeliveryFailure::StoreUnavailable,
                ))
                .await;
            return;
        }
    };
    let store = ManagedModelStore::default_embedding(models_root);
    let _ = run_with_installer(tx, |events| {
        Box::pin(install_default_model(&catalog, &store, events))
    })
    .await;
}

/// Drive one authoritative installer through quiescence, drain all admitted
/// progress, then select one terminal UI outcome.
pub(crate) async fn run_with_installer<'a>(
    mut ui_tx: Sender<Message>,
    installer: impl FnOnce(Sender<ModelDeliveryEvent>) -> InstallerFuture<'a>,
) -> Message {
    let (event_tx, mut event_rx) = futures::channel::mpsc::channel(64);
    let mut install = installer(event_tx);
    let mut ui_open = true;
    let mut events_open = true;

    let outcome = loop {
        tokio::select! {
            result = install.as_mut() => break result,
            event = event_rx.next(), if events_open => {
                match event {
                    Some(event) => forward_event(&mut ui_tx, &mut ui_open, event).await,
                    None => events_open = false,
                }
            }
        }
    };

    // Installer resolution is the worker quiescence boundary. Closing the
    // receiver prevents a contract-violating detached sender from racing the
    // terminal message while preserving already queued events for this drain.
    event_rx.close();
    while let Some(event) = event_rx.next().await {
        forward_event(&mut ui_tx, &mut ui_open, event).await;
    }

    let terminal = match outcome {
        Ok(outcome) => Message::DownloadAllComplete {
            dest_dir: outcome.generation_dir.to_string_lossy().into_owned(),
        },
        Err(error) => {
            tracing::warn!(category = %error, "trusted model delivery failed");
            Message::DownloadFailed(map_delivery_error(&error))
        }
    };
    if ui_open {
        let _ = ui_tx.send(terminal.clone()).await;
    }
    terminal
}

async fn forward_event(ui_tx: &mut Sender<Message>, ui_open: &mut bool, event: ModelDeliveryEvent) {
    let ModelDeliveryEvent::FileProgress {
        logical_name,
        bytes,
        total,
        files_done,
        files_total,
    } = event;
    let Some(artifact) = map_artifact(logical_name) else {
        tracing::warn!(
            category = "internal-state",
            "unknown model delivery artifact"
        );
        return;
    };
    if *ui_open
        && ui_tx
            .send(Message::DownloadFileProgress {
                artifact,
                bytes,
                total,
                files_done,
                files_total,
            })
            .await
            .is_err()
    {
        *ui_open = false;
    }
}

fn map_artifact(logical_name: &str) -> Option<ModelArtifact> {
    match logical_name {
        "tokenizer" => Some(ModelArtifact::Tokenizer),
        "onnx-model" => Some(ModelArtifact::OnnxModel),
        _ => None,
    }
}

fn map_delivery_error(error: &ModelDeliveryError) -> ModelDeliveryFailure {
    match error {
        ModelDeliveryError::StoreUnavailable | ModelDeliveryError::StoreBusy => {
            ModelDeliveryFailure::StoreUnavailable
        }
        ModelDeliveryError::Network => ModelDeliveryFailure::Connection,
        ModelDeliveryError::TrustPolicy
        | ModelDeliveryError::Plan
        | ModelDeliveryError::TransferLimit
        | ModelDeliveryError::Integrity
        | ModelDeliveryError::FinalCheck => ModelDeliveryFailure::Verification,
        ModelDeliveryError::Filesystem | ModelDeliveryError::Catalog => {
            ModelDeliveryFailure::LocalStorage
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbok_models::ManagedGenerationId;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn outcome(path: &str) -> ModelDeliveryOutcome {
        ModelDeliveryOutcome {
            generation_id: ManagedGenerationId::generate(),
            generation_dir: path.into(),
        }
    }

    #[tokio::test]
    async fn queued_progress_is_drained_before_exactly_one_terminal() {
        let (ui_tx, mut ui_rx) = futures::channel::mpsc::channel(8);
        let terminal = run_with_installer(ui_tx, |mut events| {
            Box::pin(async move {
                for (logical_name, files_done) in [("tokenizer", 0), ("onnx-model", 1)] {
                    events
                        .send(ModelDeliveryEvent::FileProgress {
                            logical_name,
                            bytes: 10,
                            total: 10,
                            files_done,
                            files_total: 2,
                        })
                        .await
                        .unwrap();
                }
                Ok(outcome("/generation"))
            })
        })
        .await;
        drop(terminal);
        ui_rx.close();
        let messages: Vec<_> = ui_rx.collect().await;
        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages[0],
            Message::DownloadFileProgress {
                artifact: ModelArtifact::Tokenizer,
                ..
            }
        ));
        assert!(matches!(
            messages[1],
            Message::DownloadFileProgress {
                artifact: ModelArtifact::OnnxModel,
                ..
            }
        ));
        assert!(matches!(messages[2], Message::DownloadAllComplete { .. }));
    }

    #[tokio::test]
    async fn unknown_event_does_not_cancel_or_override_worker_success() {
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (ui_tx, mut ui_rx) = futures::channel::mpsc::channel(8);
        let adapter = tokio::spawn(run_with_installer(ui_tx, move |mut events| {
            Box::pin(async move {
                events
                    .send(ModelDeliveryEvent::FileProgress {
                        logical_name: "unexpected",
                        bytes: 1,
                        total: 1,
                        files_done: 0,
                        files_total: 2,
                    })
                    .await
                    .unwrap();
                events
                    .send(ModelDeliveryEvent::FileProgress {
                        logical_name: "tokenizer",
                        bytes: 1,
                        total: 2,
                        files_done: 0,
                        files_total: 2,
                    })
                    .await
                    .unwrap();
                release_rx.await.unwrap();
                worker_completed.store(true, Ordering::SeqCst);
                Ok(outcome("/generation"))
            })
        }));

        let progress = ui_rx
            .next()
            .await
            .expect("known progress must pass the suppressed unknown event");
        assert!(matches!(
            progress,
            Message::DownloadFileProgress {
                artifact: ModelArtifact::Tokenizer,
                ..
            }
        ));
        assert!(
            !completed.load(Ordering::SeqCst),
            "the installer must still be active at the adapter checkpoint"
        );
        release_tx.send(()).unwrap();
        let terminal = adapter.await.unwrap();
        assert!(completed.load(Ordering::SeqCst));
        assert!(matches!(terminal, Message::DownloadAllComplete { .. }));
        ui_rx.close();
        let messages: Vec<_> = ui_rx.collect().await;
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0], Message::DownloadAllComplete { .. }));
    }

    #[tokio::test]
    async fn closed_ui_receiver_does_not_cancel_the_worker() {
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let (ui_tx, ui_rx) = futures::channel::mpsc::channel(1);
        drop(ui_rx);
        let terminal = run_with_installer(ui_tx, move |mut events| {
            Box::pin(async move {
                let _ = events
                    .send(ModelDeliveryEvent::FileProgress {
                        logical_name: "tokenizer",
                        bytes: 1,
                        total: 2,
                        files_done: 0,
                        files_total: 2,
                    })
                    .await;
                worker_completed.store(true, Ordering::SeqCst);
                Ok(outcome("/generation"))
            })
        })
        .await;
        assert!(completed.load(Ordering::SeqCst));
        assert!(matches!(terminal, Message::DownloadAllComplete { .. }));
    }

    #[tokio::test]
    async fn worker_failure_selects_one_safe_terminal_category() {
        let (ui_tx, mut ui_rx) = futures::channel::mpsc::channel(2);
        let terminal = run_with_installer(ui_tx, |_events| {
            Box::pin(async { Err(ModelDeliveryError::Integrity) })
        })
        .await;
        assert!(matches!(
            terminal,
            Message::DownloadFailed(ModelDeliveryFailure::Verification)
        ));
        ui_rx.close();
        let messages: Vec<_> = ui_rx.collect().await;
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn worker_error_mapping_is_exhaustive_and_safe() {
        let cases = [
            (
                ModelDeliveryError::StoreUnavailable,
                ModelDeliveryFailure::StoreUnavailable,
            ),
            (
                ModelDeliveryError::StoreBusy,
                ModelDeliveryFailure::StoreUnavailable,
            ),
            (
                ModelDeliveryError::TrustPolicy,
                ModelDeliveryFailure::Verification,
            ),
            (ModelDeliveryError::Plan, ModelDeliveryFailure::Verification),
            (
                ModelDeliveryError::Network,
                ModelDeliveryFailure::Connection,
            ),
            (
                ModelDeliveryError::TransferLimit,
                ModelDeliveryFailure::Verification,
            ),
            (
                ModelDeliveryError::Integrity,
                ModelDeliveryFailure::Verification,
            ),
            (
                ModelDeliveryError::Filesystem,
                ModelDeliveryFailure::LocalStorage,
            ),
            (
                ModelDeliveryError::Catalog,
                ModelDeliveryFailure::LocalStorage,
            ),
            (
                ModelDeliveryError::FinalCheck,
                ModelDeliveryFailure::Verification,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(map_delivery_error(&error), expected);
        }
        assert_eq!(map_artifact("tokenizer"), Some(ModelArtifact::Tokenizer));
        assert_eq!(map_artifact("onnx-model"), Some(ModelArtifact::OnnxModel));
        assert_eq!(map_artifact("/secret/path"), None);
    }
}
