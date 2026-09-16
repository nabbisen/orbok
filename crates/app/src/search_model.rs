//! Task 055 §1(c): the embedding model search uses, replaceable during a
//! session.
//!
//! RFC-061 §6 Slice 4 resolved the search model once per process and held it
//! in a plain `Arc`, so a model installed mid-session was never used by
//! search until a restart. This keeps one-model-per-process (a search takes
//! a snapshot; it never resolves) while letting a newly installed model be
//! swapped in.
//!
//! **Why `RwLock<Arc<..>>` and not an atomic-swap cell:** the standard
//! library suffices, and no new dependency is needed for a swap that happens
//! once per install. The lock is only ever held to clone an `Arc` or to
//! assign one -- never across model construction, which happens before
//! [`SearchModel::activate`] takes the write lock -- so a search never waits
//! on a load. A search in flight keeps its own snapshot, so the model it
//! started with (and that model's RFC-050 lease) lives until it finishes.

use crate::bootstrap::embedding_resolution::EmbeddingWorkerParts;
use std::sync::{Arc, RwLock};

pub(crate) struct SearchModel(RwLock<Arc<Option<EmbeddingWorkerParts>>>);

impl SearchModel {
    pub(crate) fn new(initial: Option<EmbeddingWorkerParts>) -> Self {
        Self(RwLock::new(Arc::new(initial)))
    }

    /// The model the next search should use.
    pub(crate) fn current(&self) -> Arc<Option<EmbeddingWorkerParts>> {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Resolve a model and, if that succeeds, make it the current one.
    /// Returns whether it did. The swap has happened by the time this
    /// returns `true`, which is what lets the caller's completion message
    /// -- the only thing that sets `capability = Hybrid` -- be honest.
    pub(crate) fn activate(&self, resolve: impl FnOnce() -> Option<EmbeddingWorkerParts>) -> bool {
        let Some(parts) = resolve() else {
            return false;
        };
        *self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(Some(parts));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbok_core::ModelId;
    use orbok_models::MockEmbeddingModel;

    fn parts() -> EmbeddingWorkerParts {
        EmbeddingWorkerParts::for_test(Box::new(MockEmbeddingModel), ModelId::generate())
    }

    #[test]
    fn a_resolved_model_is_current_by_the_time_activate_returns() {
        let model = SearchModel::new(None);
        assert!(model.current().is_none());
        assert!(model.activate(|| Some(parts())));
        assert!(
            model.current().is_some(),
            "activate reported success, so search must already hold the model"
        );
    }

    #[test]
    fn a_failed_resolution_leaves_the_current_model_and_reports_it() {
        let model = SearchModel::new(None);
        assert!(!model.activate(|| None));
        assert!(model.current().is_none());
    }

    #[test]
    fn a_search_snapshot_taken_before_the_swap_keeps_its_model() {
        let model = SearchModel::new(None);
        let before = model.current();
        assert!(model.activate(|| Some(parts())));
        assert!(
            before.is_none(),
            "an in-flight search is not changed under it"
        );
        assert!(model.current().is_some());
    }
}
