//! Pure application controller for RFC-050 model lifecycle messages.

use orbok_models::SearchCapability;
use orbok_ui::state::{
    AppState, Message, ModelDeliveryFailure, ModelPersistenceResult, ModelPersistenceState,
    ModelProvenance, PersistenceAttemptId, ReadyId, WizardFileCheck, WizardState,
};
use orbok_workers::VerifyOutcome;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModelFlowEffect {
    None,
    StartManagedDownload,
    PersistReady {
        ready_id: ReadyId,
        persistence_attempt_id: PersistenceAttemptId,
        model_dir: String,
        provenance: ModelProvenance,
    },
}

/// Apply a model-lifecycle message. `None` means the message is not owned by
/// this controller; `Some(ModelFlowEffect::None)` means it was handled without
/// backend work.
pub(crate) fn reduce(state: &mut AppState, message: &Message) -> Option<ModelFlowEffect> {
    match message {
        Message::ConfirmModelDownload => {
            let Some(WizardState::DownloadConsent {
                presentation,
                return_to,
            }) = state.wizard.clone()
            else {
                return Some(ModelFlowEffect::None);
            };
            let Some(reserved_ready_id) = state.model_flow_ids.allocate_ready() else {
                tracing::error!("model Ready identity space exhausted");
                state.wizard = Some(WizardState::DownloadFailed {
                    presentation,
                    return_to,
                    failure: ModelDeliveryFailure::InternalState,
                });
                return Some(ModelFlowEffect::None);
            };
            state.wizard = Some(WizardState::Downloading {
                reserved_ready_id,
                dest_dir: presentation.destination.clone(),
                presentation,
                return_to,
                current_artifact: None,
                bytes: 0,
                total: 0,
                files_done: 0,
                files_total: 0,
            });
            Some(ModelFlowEffect::StartManagedDownload)
        }
        Message::WizardChecked {
            model_dir,
            checks,
            all_ok,
        } => {
            if *all_ok {
                if let Some(ready_id) = state.model_flow_ids.allocate_ready() {
                    state.wizard = Some(WizardState::Ready {
                        ready_id,
                        model_dir: model_dir.clone(),
                        provenance: ModelProvenance::UserSupplied,
                        persistence: ModelPersistenceState::Idle,
                    });
                } else {
                    tracing::error!("model Ready identity space exhausted");
                    state.wizard = Some(WizardState::Checked {
                        model_dir: model_dir.clone(),
                        checks: checks.clone(),
                        all_ok: true,
                    });
                }
            } else {
                state.wizard = Some(WizardState::Checked {
                    model_dir: model_dir.clone(),
                    checks: checks.clone(),
                    all_ok: false,
                });
            }
            Some(ModelFlowEffect::None)
        }
        Message::WizardAccept => Some(begin_persistence(state)),
        Message::ModelPersistenceCompleted {
            ready_id,
            persistence_attempt_id,
            model_dir,
            provenance,
            result,
        } => {
            apply_persistence_result(
                state,
                *ready_id,
                *persistence_attempt_id,
                model_dir,
                *provenance,
                *result,
            );
            Some(ModelFlowEffect::None)
        }
        Message::DownloadFileProgress {
            artifact,
            bytes,
            total,
            files_done,
            files_total,
        } => {
            if let Some(WizardState::Downloading {
                current_artifact,
                bytes: current_bytes,
                total: current_total,
                files_done: current_files_done,
                files_total: current_files_total,
                ..
            }) = state.wizard.as_mut()
            {
                *current_artifact = Some(*artifact);
                *current_bytes = *bytes;
                *current_total = *total;
                *current_files_done = *files_done;
                *current_files_total = *files_total;
            }
            Some(ModelFlowEffect::None)
        }
        Message::DownloadAllComplete { dest_dir } => {
            let prior = state.wizard.take();
            match prior {
                Some(WizardState::Downloading {
                    reserved_ready_id, ..
                }) => {
                    state.wizard = Some(WizardState::Ready {
                        ready_id: reserved_ready_id,
                        model_dir: dest_dir.clone(),
                        provenance: ModelProvenance::AppManaged,
                        persistence: ModelPersistenceState::Idle,
                    });
                }
                other => state.wizard = other,
            }
            Some(ModelFlowEffect::None)
        }
        Message::DownloadFailed(failure) => {
            let prior = state.wizard.take();
            state.wizard = match prior {
                Some(WizardState::Downloading {
                    presentation,
                    return_to,
                    ..
                }) => Some(WizardState::DownloadFailed {
                    presentation,
                    return_to,
                    failure: *failure,
                }),
                other => other,
            };
            Some(ModelFlowEffect::None)
        }
        Message::RetryModelDownload => {
            let prior = state.wizard.take();
            state.wizard = match prior {
                Some(WizardState::DownloadFailed {
                    presentation,
                    return_to,
                    ..
                }) => Some(WizardState::DownloadConsent {
                    presentation,
                    return_to,
                }),
                other => other,
            };
            Some(ModelFlowEffect::None)
        }
        _ => None,
    }
}

fn begin_persistence(state: &mut AppState) -> ModelFlowEffect {
    let Some(WizardState::Ready {
        ready_id,
        model_dir,
        provenance,
        persistence,
    }) = state.wizard.as_ref()
    else {
        return ModelFlowEffect::None;
    };
    if matches!(persistence, ModelPersistenceState::InFlight(_)) {
        return ModelFlowEffect::None;
    }
    let (ready_id, model_dir, provenance) = (*ready_id, model_dir.clone(), *provenance);
    let Some(persistence_attempt_id) = state.model_flow_ids.allocate_persistence_attempt() else {
        tracing::error!("model persistence identity space exhausted");
        if let Some(WizardState::Ready { persistence, .. }) = state.wizard.as_mut() {
            *persistence = ModelPersistenceState::Failed;
        }
        return ModelFlowEffect::None;
    };
    if let Some(WizardState::Ready { persistence, .. }) = state.wizard.as_mut() {
        *persistence = ModelPersistenceState::InFlight(persistence_attempt_id);
    }
    ModelFlowEffect::PersistReady {
        ready_id,
        persistence_attempt_id,
        model_dir,
        provenance,
    }
}

fn apply_persistence_result(
    state: &mut AppState,
    ready_id: ReadyId,
    persistence_attempt_id: PersistenceAttemptId,
    model_dir: &str,
    provenance: ModelProvenance,
    result: ModelPersistenceResult,
) {
    let matches_active = matches!(
        state.wizard.as_ref(),
        Some(WizardState::Ready {
            ready_id: active_ready,
            model_dir: active_dir,
            provenance: active_provenance,
            persistence: ModelPersistenceState::InFlight(active_attempt),
        }) if *active_ready == ready_id
            && *active_attempt == persistence_attempt_id
            && active_dir == model_dir
            && *active_provenance == provenance
    );
    if !matches_active {
        return;
    }
    match result {
        ModelPersistenceResult::Saved => {
            state.capability = SearchCapability::Hybrid;
            state.active_model_provenance = Some(provenance);
            state.wizard = None;
            state.wizard_path_input.clear();
        }
        ModelPersistenceResult::Failed => {
            if let Some(WizardState::Ready { persistence, .. }) = state.wizard.as_mut() {
                *persistence = ModelPersistenceState::Failed;
            }
        }
    }
}

pub(crate) struct StartupProjection {
    pub capability: SearchCapability,
    pub wizard: Option<WizardState>,
    pub active_provenance: Option<ModelProvenance>,
}

pub(crate) fn project_startup(
    outcome: VerifyOutcome,
    resolved_provenance: Option<ModelProvenance>,
) -> StartupProjection {
    match outcome {
        VerifyOutcome::Ready => StartupProjection {
            capability: SearchCapability::Hybrid,
            wizard: None,
            active_provenance: resolved_provenance,
        },
        VerifyOutcome::NotConfigured => StartupProjection {
            capability: SearchCapability::KeywordOnly,
            wizard: Some(WizardState::NotConfigured),
            active_provenance: None,
        },
        VerifyOutcome::FilesInvalid { model_dir, issues } => {
            let checks = orbok_workers::model_verifier::REQUIRED_MODEL_FILES
                .iter()
                .map(|relative_path| WizardFileCheck {
                    relative_path: (*relative_path).to_string(),
                    found: !issues
                        .iter()
                        .any(|issue| issue.relative_path == *relative_path),
                    size_mb: None,
                })
                .collect();
            StartupProjection {
                capability: SearchCapability::KeywordOnly,
                wizard: Some(WizardState::FileMissing {
                    previous_dir: model_dir,
                    checks,
                }),
                active_provenance: None,
            }
        }
    }
}

pub(crate) trait ModelPreferenceStore {
    fn accept_user_supplied(&self, model_dir: &str) -> Result<(), ()>;
    fn accept_app_managed(&self) -> Result<(), ()>;
}

struct ProductionModelPreferenceStore {
    data_dir: PathBuf,
}

impl ModelPreferenceStore for ProductionModelPreferenceStore {
    fn accept_user_supplied(&self, model_dir: &str) -> Result<(), ()> {
        crate::bootstrap::persist_model_dir(model_dir).map_err(|error| {
            tracing::error!(category = "settings", %error, "failed to save model preference");
        })
    }

    fn accept_app_managed(&self) -> Result<(), ()> {
        crate::bootstrap::remove_managed_model_dir_setting(&self.data_dir).map_err(|error| {
            tracing::error!(category = "settings", %error, "failed to save model preference");
        })
    }
}

pub(crate) fn execute_production_persistence(
    effect: ModelFlowEffect,
    data_dir: &Path,
) -> Option<Message> {
    let store = ProductionModelPreferenceStore {
        data_dir: data_dir.to_path_buf(),
    };
    execute_persistence(&store, effect)
}

fn execute_persistence(
    store: &impl ModelPreferenceStore,
    effect: ModelFlowEffect,
) -> Option<Message> {
    let ModelFlowEffect::PersistReady {
        ready_id,
        persistence_attempt_id,
        model_dir,
        provenance,
    } = effect
    else {
        return None;
    };
    let saved = match provenance {
        ModelProvenance::UserSupplied => store.accept_user_supplied(&model_dir),
        ModelProvenance::AppManaged => store.accept_app_managed(),
    };
    Some(Message::ModelPersistenceCompleted {
        ready_id,
        persistence_attempt_id,
        model_dir,
        provenance,
        result: if saved.is_ok() {
            ModelPersistenceResult::Saved
        } else {
            ModelPersistenceResult::Failed
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt as _, StreamExt as _};
    use orbok_models::ManagedGenerationId;
    use orbok_ui::{ModelConsentReturn, ModelDownloadConsent};
    use orbok_workers::{ModelDeliveryError, ModelDeliveryEvent, ModelDeliveryOutcome};
    use std::cell::RefCell;

    fn consent_state() -> AppState {
        AppState {
            wizard: Some(WizardState::DownloadConsent {
                presentation: ModelDownloadConsent::trusted_default("/managed/models".into()),
                return_to: ModelConsentReturn::NotConfigured,
            }),
            ..AppState::default()
        }
    }

    #[test]
    fn confirmation_is_synchronous_and_duplicate_safe() {
        let mut state = consent_state();
        assert_eq!(
            reduce(&mut state, &Message::ConfirmModelDownload),
            Some(ModelFlowEffect::StartManagedDownload)
        );
        assert!(matches!(
            state.wizard,
            Some(WizardState::Downloading { .. })
        ));
        assert_eq!(
            reduce(&mut state, &Message::ConfirmModelDownload),
            Some(ModelFlowEffect::None)
        );
    }

    #[test]
    fn wizard_accept_outside_ready_is_defensive() {
        let mut state = AppState {
            wizard: Some(WizardState::NotConfigured),
            ..AppState::default()
        };
        assert_eq!(
            reduce(&mut state, &Message::WizardAccept),
            Some(ModelFlowEffect::None)
        );
        assert_eq!(state.capability, SearchCapability::KeywordOnly);
        assert_eq!(state.wizard, Some(WizardState::NotConfigured));
    }

    fn ready_state() -> AppState {
        let mut state = AppState::default();
        reduce(
            &mut state,
            &Message::WizardChecked {
                model_dir: "/user/model".into(),
                checks: Vec::new(),
                all_ok: true,
            },
        );
        state
    }

    fn persistence_effect(state: &mut AppState) -> ModelFlowEffect {
        reduce(state, &Message::WizardAccept).unwrap()
    }

    fn worker_outcome(path: &str) -> ModelDeliveryOutcome {
        ModelDeliveryOutcome {
            generation_id: ManagedGenerationId::generate(),
            generation_dir: path.into(),
        }
    }

    #[tokio::test]
    async fn compiled_adapter_controller_and_executor_complete_managed_success() {
        let mut state = consent_state();
        assert_eq!(
            reduce(&mut state, &Message::ConfirmModelDownload),
            Some(ModelFlowEffect::StartManagedDownload)
        );
        let reserved_ready_id = match state.wizard.as_ref() {
            Some(WizardState::Downloading {
                reserved_ready_id, ..
            }) => *reserved_ready_id,
            other => panic!("expected Downloading, got {other:?}"),
        };

        let (ui_tx, mut ui_rx) = futures::channel::mpsc::channel(8);
        crate::download::run_with_installer(ui_tx, |mut events| {
            Box::pin(async move {
                events
                    .send(ModelDeliveryEvent::FileProgress {
                        logical_name: "tokenizer",
                        bytes: 5,
                        total: 10,
                        files_done: 0,
                        files_total: 2,
                    })
                    .await
                    .unwrap();
                Ok(worker_outcome("/managed/generation"))
            })
        })
        .await;
        ui_rx.close();
        let messages: Vec<_> = ui_rx.collect().await;
        assert_eq!(messages.len(), 2);

        assert!(matches!(messages[0], Message::DownloadFileProgress { .. }));
        reduce(&mut state, &messages[0]);
        assert!(
            matches!(state.wizard.as_ref(), Some(WizardState::Downloading { .. })),
            "progress must not create Ready"
        );

        assert!(matches!(messages[1], Message::DownloadAllComplete { .. }));
        reduce(&mut state, &messages[1]);
        assert!(matches!(
            state.wizard.as_ref(),
            Some(WizardState::Ready {
                ready_id,
                provenance: ModelProvenance::AppManaged,
                persistence: ModelPersistenceState::Idle,
                ..
            }) if *ready_id == reserved_ready_id
        ));

        let persistence = persistence_effect(&mut state);
        assert!(matches!(
            persistence,
            ModelFlowEffect::PersistReady {
                ready_id,
                provenance: ModelProvenance::AppManaged,
                ..
            } if ready_id == reserved_ready_id
        ));
        let store = RecordingStore(RefCell::new(Vec::new()));
        let completion = execute_persistence(&store, persistence).unwrap();
        reduce(&mut state, &completion);

        assert_eq!(store.0.borrow().as_slice(), ["managed"]);
        assert_eq!(state.capability, SearchCapability::Hybrid);
        assert_eq!(
            state.active_model_provenance,
            Some(ModelProvenance::AppManaged)
        );
        assert!(state.wizard.is_none());
    }

    #[tokio::test]
    async fn compiled_adapter_failure_never_creates_ready() {
        let mut state = consent_state();
        reduce(&mut state, &Message::ConfirmModelDownload);
        let (ui_tx, mut ui_rx) = futures::channel::mpsc::channel(2);
        crate::download::run_with_installer(ui_tx, |_events| {
            Box::pin(async { Err(ModelDeliveryError::Network) })
        })
        .await;
        ui_rx.close();
        let messages: Vec<_> = ui_rx.collect().await;
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0],
            Message::DownloadFailed(ModelDeliveryFailure::Connection)
        ));
        reduce(&mut state, &messages[0]);
        assert!(matches!(
            state.wizard,
            Some(WizardState::DownloadFailed {
                failure: ModelDeliveryFailure::Connection,
                ..
            })
        ));
    }

    #[test]
    fn persistence_requires_an_exact_correlated_completion() {
        let mut state = ready_state();
        let effect = persistence_effect(&mut state);
        let ModelFlowEffect::PersistReady {
            ready_id,
            persistence_attempt_id,
            model_dir,
            provenance,
        } = effect
        else {
            panic!("expected persistence effect")
        };
        let wrong_attempt = state
            .model_flow_ids
            .allocate_persistence_attempt()
            .expect("a distinct test attempt");
        let wrong_ready = state
            .model_flow_ids
            .allocate_ready()
            .expect("a distinct test Ready identity");

        for message in [
            Message::ModelPersistenceCompleted {
                ready_id: wrong_ready,
                persistence_attempt_id,
                model_dir: model_dir.clone(),
                provenance,
                result: ModelPersistenceResult::Saved,
            },
            Message::ModelPersistenceCompleted {
                ready_id,
                persistence_attempt_id: wrong_attempt,
                model_dir: model_dir.clone(),
                provenance,
                result: ModelPersistenceResult::Saved,
            },
            Message::ModelPersistenceCompleted {
                ready_id,
                persistence_attempt_id,
                model_dir: "/wrong".into(),
                provenance,
                result: ModelPersistenceResult::Saved,
            },
            Message::ModelPersistenceCompleted {
                ready_id,
                persistence_attempt_id,
                model_dir: model_dir.clone(),
                provenance: ModelProvenance::AppManaged,
                result: ModelPersistenceResult::Saved,
            },
        ] {
            reduce(&mut state, &message);
            assert!(matches!(state.wizard, Some(WizardState::Ready { .. })));
            assert_eq!(state.capability, SearchCapability::KeywordOnly);
        }

        reduce(
            &mut state,
            &Message::ModelPersistenceCompleted {
                ready_id,
                persistence_attempt_id,
                model_dir,
                provenance,
                result: ModelPersistenceResult::Saved,
            },
        );
        assert!(state.wizard.is_none());
        assert_eq!(state.capability, SearchCapability::Hybrid);
        assert_eq!(state.active_model_provenance, Some(provenance));
        reduce(
            &mut state,
            &Message::ModelPersistenceCompleted {
                ready_id,
                persistence_attempt_id,
                model_dir: "/user/model".into(),
                provenance,
                result: ModelPersistenceResult::Saved,
            },
        );
        assert!(state.wizard.is_none(), "duplicate completion is a no-op");
    }

    #[test]
    fn duplicate_accept_during_persistence_emits_no_second_write() {
        let mut state = ready_state();
        assert!(matches!(
            persistence_effect(&mut state),
            ModelFlowEffect::PersistReady { .. }
        ));
        assert_eq!(
            persistence_effect(&mut state),
            ModelFlowEffect::None,
            "an in-flight write must not be duplicated"
        );
    }

    #[test]
    fn failed_persistence_retries_with_a_fresh_attempt_without_downloading() {
        let mut state = ready_state();
        let first = persistence_effect(&mut state);
        let completion = execute_persistence(&FailingStore, first.clone()).unwrap();
        reduce(&mut state, &completion);
        assert!(matches!(
            state.wizard,
            Some(WizardState::Ready {
                persistence: ModelPersistenceState::Failed,
                ..
            })
        ));

        let second = persistence_effect(&mut state);
        let (
            ModelFlowEffect::PersistReady {
                persistence_attempt_id: first_id,
                ..
            },
            ModelFlowEffect::PersistReady {
                persistence_attempt_id: second_id,
                ..
            },
        ) = (first, second)
        else {
            panic!("expected persistence effects")
        };
        assert_ne!(first_id, second_id);
    }

    #[test]
    fn delivery_failure_retains_consent_and_retry_never_starts_work() {
        let mut state = consent_state();
        reduce(&mut state, &Message::ConfirmModelDownload);
        reduce(
            &mut state,
            &Message::DownloadFailed(ModelDeliveryFailure::Connection),
        );
        assert!(matches!(
            state.wizard,
            Some(WizardState::DownloadFailed {
                failure: ModelDeliveryFailure::Connection,
                ..
            })
        ));
        assert_eq!(
            reduce(&mut state, &Message::RetryModelDownload),
            Some(ModelFlowEffect::None)
        );
        assert!(matches!(
            state.wizard,
            Some(WizardState::DownloadConsent { .. })
        ));
    }

    struct FailingStore;

    impl ModelPreferenceStore for FailingStore {
        fn accept_user_supplied(&self, _model_dir: &str) -> Result<(), ()> {
            Err(())
        }

        fn accept_app_managed(&self) -> Result<(), ()> {
            Err(())
        }
    }

    struct RecordingStore(RefCell<Vec<String>>);

    impl ModelPreferenceStore for RecordingStore {
        fn accept_user_supplied(&self, model_dir: &str) -> Result<(), ()> {
            self.0.borrow_mut().push(model_dir.to_string());
            Ok(())
        }

        fn accept_app_managed(&self) -> Result<(), ()> {
            self.0.borrow_mut().push("managed".into());
            Ok(())
        }
    }

    #[test]
    fn executor_echoes_the_exact_effect_identity() {
        let mut state = ready_state();
        let effect = persistence_effect(&mut state);
        let store = RecordingStore(RefCell::new(Vec::new()));
        let completion = execute_persistence(&store, effect.clone()).unwrap();
        let ModelFlowEffect::PersistReady {
            ready_id,
            persistence_attempt_id,
            model_dir,
            provenance,
        } = effect
        else {
            unreachable!()
        };
        assert!(matches!(
            completion,
            Message::ModelPersistenceCompleted {
                ready_id: actual_ready,
                persistence_attempt_id: actual_attempt,
                model_dir: ref actual_dir,
                provenance: actual_provenance,
                result: ModelPersistenceResult::Saved,
            } if actual_ready == ready_id
                && actual_attempt == persistence_attempt_id
                && actual_dir == &model_dir
                && actual_provenance == provenance
        ));
        assert_eq!(store.0.borrow().as_slice(), ["/user/model"]);
    }
}
