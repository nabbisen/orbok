//! Bounded work queues with backpressure (RFC-036 §7, §10).
//!
//! Each `BoundedQueue<T>` holds at most `capacity` items. Callers must
//! check `is_full` before pushing; the scheduler applies upstream
//! backpressure when any downstream queue reports full.
//!
//! This implementation is synchronous and single-threaded, matching
//! the existing orbok-workers execution model. Async channels can be
//! layered in a future RFC without changing this API.

use crate::scheduler::job::{IndexJob, JobKind, QueueKind};
use std::collections::VecDeque;

/// A single bounded queue of `IndexJob`s, ordered by priority then
/// insertion order (RFC-036 §8).
pub struct BoundedQueue {
    kind: QueueKind,
    capacity: usize,
    items: VecDeque<IndexJob>,
    /// Total jobs ever pushed (for progress reporting).
    total_pushed: u64,
    /// Whether backpressure is currently active for this queue.
    pub backpressure_active: bool,
}

impl BoundedQueue {
    pub fn new(kind: QueueKind, capacity: usize) -> Self {
        Self {
            kind,
            capacity,
            items: VecDeque::new(),
            total_pushed: 0,
            backpressure_active: false,
        }
    }

    pub fn kind(&self) -> QueueKind {
        self.kind
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// `true` when the queue has reached its capacity ceiling.
    /// Callers must not push when this returns `true`.
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.capacity
    }

    /// Remaining capacity.
    pub fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.items.len())
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn total_pushed(&self) -> u64 {
        self.total_pushed
    }

    /// Push a job into the queue, maintaining priority order.
    ///
    /// ## Ordering invariant
    ///
    /// `pop_back` serves the item at the **highest index**. To dispatch
    /// the highest-priority job first, and within equal priority the
    /// *oldest* job first (FIFO), items must be stored with:
    ///
    /// - higher-priority items at higher indices (closer to back), and
    /// - among equal-priority items, older items at higher indices.
    ///
    /// ## Insertion rule
    ///
    /// The new job is inserted at the first index (from the back) where
    /// the existing item has strictly lower priority than the new job.
    /// This places it after (higher index than) all equal-or-higher
    /// priority items already in the queue, satisfying both conditions.
    ///
    /// Panics if the queue is full — callers must check `is_full` first.
    pub fn push(&mut self, job: IndexJob) {
        assert!(
            !self.is_full(),
            "BoundedQueue::push called on a full queue ({:?})",
            self.kind
        );
        // Find insertion point: scan from back to front, find the first
        // item with strictly lower priority. Insert just after it (i.e.
        // at index i+1, which is one step toward the back from i).
        // If all existing items have >= priority (or queue is empty),
        // insert at the front (index 0) so the new job is served last
        // among equal-priority items.
        let len = self.items.len();
        let pos = if len == 0 {
            0
        } else {
            // Start from the back (highest priority end).
            let mut insert_at = 0; // default: insert at front
            for i in (0..len).rev() {
                if self.items[i].priority < job.priority {
                    // items[i] has equal or lower priority than the new
                    // job. The new job goes just after it (toward back).
                    insert_at = i + 1;
                    break;
                }
                // items[i].priority > job.priority: new job must go
                // further toward the front; continue scanning.
            }
            insert_at
        };
        self.items.insert(pos, job);
        self.total_pushed += 1;
    }

    /// Pop the highest-priority job (front of the queue).
    pub fn pop(&mut self) -> Option<IndexJob> {
        self.items.pop_back()
    }

    /// Peek at the highest-priority job without removing it.
    pub fn peek(&self) -> Option<&IndexJob> {
        self.items.back()
    }

    /// Cancel all jobs belonging to `source_id` (RFC-036 §12.3).
    /// Returns the number of jobs removed.
    pub fn cancel_for_source(&mut self, source_id: &orbok_core::SourceId) -> usize {
        let before = self.items.len();
        self.items.retain(|j| &j.source_id != source_id);
        before - self.items.len()
    }

    /// Drain all jobs (e.g. on full scheduler reset).
    pub fn clear(&mut self) -> usize {
        let n = self.items.len();
        self.items.clear();
        n
    }
}

// ── Multi-queue set ───────────────────────────────────────────────────────

/// The complete set of bounded queues for the scheduler (RFC-036 §7).
pub struct QueueSet {
    pub scan: BoundedQueue,
    pub extract: BoundedQueue,
    pub chunk: BoundedQueue,
    pub keyword: BoundedQueue,
    pub embedding: BoundedQueue,
    pub maintenance: BoundedQueue,
}

impl QueueSet {
    pub fn new(capacity: &crate::scheduler::limits::QueueCapacity) -> Self {
        Self {
            scan: BoundedQueue::new(QueueKind::Scan, capacity.scan_queue_max),
            extract: BoundedQueue::new(QueueKind::Extract, capacity.extract_queue_max),
            chunk: BoundedQueue::new(QueueKind::Chunk, capacity.chunk_queue_max),
            keyword: BoundedQueue::new(QueueKind::Keyword, capacity.keyword_queue_max),
            embedding: BoundedQueue::new(QueueKind::Embedding, capacity.embedding_queue_max),
            maintenance: BoundedQueue::new(QueueKind::Maintenance, capacity.maintenance_queue_max),
        }
    }

    /// Route a job to its natural queue by kind (RFC-036 §6).
    pub fn queue_for(&mut self, kind: JobKind) -> &mut BoundedQueue {
        match kind {
            JobKind::ScanSource => &mut self.scan,
            JobKind::ExtractFile => &mut self.extract,
            JobKind::ChunkFile => &mut self.chunk,
            JobKind::UpdateKeywordIndex => &mut self.keyword,
            JobKind::GenerateEmbedding => &mut self.embedding,
            JobKind::Cleanup | JobKind::Repair => &mut self.maintenance,
        }
    }

    /// Total pending jobs across all queues.
    pub fn total_pending(&self) -> usize {
        self.scan.len()
            + self.extract.len()
            + self.chunk.len()
            + self.keyword.len()
            + self.embedding.len()
            + self.maintenance.len()
    }

    /// Cancel all queued jobs for a source (RFC-036 §12.3).
    pub fn cancel_source(&mut self, source_id: &orbok_core::SourceId) -> usize {
        self.scan.cancel_for_source(source_id)
            + self.extract.cancel_for_source(source_id)
            + self.chunk.cancel_for_source(source_id)
            + self.keyword.cancel_for_source(source_id)
            + self.embedding.cancel_for_source(source_id)
            + self.maintenance.cancel_for_source(source_id)
    }

    /// Pop the next job to run, respecting resource mode (RFC-036 §8, §13).
    ///
    /// In `UserActive` mode, embedding is skipped so search is never
    /// delayed (RFC-036 §9.2 embedding rule). In `LowImpact` mode, embedding
    /// is skipped for the same reason the concurrency limits can't be lowered
    /// (`SchedulerLimits::default()` is already `1` everywhere): per RFC-048,
    /// embedding is ~99.9% of indexing cost, so skipping it alone *is* RFC-036
    /// §13.2's "reduce background work" (RFC-057 §4.3a) -- extract, chunk and
    /// keyword keep running, so files stay keyword-searchable on battery.
    pub fn pop_next(&mut self, resource_mode: super::job::ResourceMode) -> Option<IndexJob> {
        use super::job::ResourceMode;

        // Priority order: scan → extract → chunk → keyword → embedding →
        // maintenance. Embedding is skipped entirely in UserActive and
        // LowImpact modes.
        let queues: &mut [&mut BoundedQueue] = &mut [
            &mut self.scan,
            &mut self.extract,
            &mut self.chunk,
            &mut self.keyword,
            &mut self.embedding,
            &mut self.maintenance,
        ];

        for q in queues.iter_mut() {
            if q.kind() == QueueKind::Embedding
                && matches!(
                    resource_mode,
                    ResourceMode::UserActive | ResourceMode::LowImpact
                )
            {
                // RFC-036 §9.2/§13.2: yield embedding to active search or
                // battery-reduced background work (RFC-057 §4.3a).
                continue;
            }
            if let Some(job) = q.pop() {
                return Some(job);
            }
        }
        None
    }
}
