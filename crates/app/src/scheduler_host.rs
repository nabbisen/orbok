//! RFC-056: hosts RFC-036's `Scheduler` in a long-lived background task.
//!
//! Mirrors `download.rs`'s split: a thin [`run`] resolves the active
//! profile's sealed handles and hands off to [`run_with_context`], the core
//! loop tests exercise directly -- the same function `main.rs`'s
//! subscription wires in, per RFC-056's Handoff §4 ("every test exercises
//! the shipped application's path, not a worker invoked directly").
//!
//! **Slice 2** (RFC-056 Handoff §2, RFC-036 §20.1): `GenerateEmbedding`
//! jobs dispatch through a real `EmbeddingWorker`, resolved once at
//! startup via [`crate::bootstrap::embedding_resolution`] (RFC-050 lease
//! held for the loop's whole lifetime). No model configured, or the model
//! fails to load, is not distinguished from any other unavailable-worker
//! case here -- both surface as RFC-008 §15's `model_missing`, matching
//! Slice 1's own terminal-category choice, but now routed through
//! `Scheduler::fail` like every other job kind rather than bypassing it.
//!
//! **Scan routing** (Review 162 §2): discovery is RFC-036 §6.1's own first
//! work category, with its own queue and priority (`JobKind::ScanSource`)
//! that nothing produced before this. `scan_and_index_source` now enqueues
//! a `Scan` job instead of calling `Scanner::scan` inline, so the scan/hash
//! pass -- which scales with source size, not a fixed cost Slice 1's
//! measurement mistook it for -- runs here, off the caller's thread, same
//! as everything else this module hosts.
//!
//! **RFC-057 Slice 1:** the first live path into this task. `[ResourceObservation]`
//! is drained each loop iteration; producers report what they observed, not
//! a `ResourceMode`, and only this module decides what an observation means.
//!
//! **RFC-057 Slice 2 (Amendment 1):** a second source (`OnBattery`) meant a
//! single mutated `ResourceMode` could lose state -- a battery-driven
//! `LowImpact` silently overwritten by the next `UserActive` signal, then
//! never restored when activity stopped, because the old
//! `notify_user_active`/`notify_user_idle` transitions each mutated the mode
//! in isolation. This loop now holds the observation state itself
//! (`last_user_activity`, `on_battery`) and calls
//! `Scheduler::apply_resource_observation` each iteration to *derive* the
//! whole mode fresh, so neither observation can be lost behind the other.

mod battery;

use crate::bootstrap::embedding_resolution::EmbeddingWorkerParts;
use futures::SinkExt as _;
use futures::channel::mpsc::{Receiver, Sender};
use orbok::runtime_context::AllowRuntimePathProbe;
use orbok::runtime_storage::ProfileCache;
use orbok_core::{JobId, JobStatus, JobType, OrbokError, OrbokResult, SourceId};
use orbok_db::Catalog;
use orbok_db::repo::{IndexJobRepository, JobRecord};
use orbok_fs::{ScanRequest, Scanner};
use orbok_ui::state::Message;
use orbok_workers::{
    ChunkAndIndexWorker, EmbeddingWorker, ExtractionWorker, IndexJob, JobKind, JobState, Scheduler,
};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// How long the loop sleeps after finding nothing to do before checking the
/// catalog again for newly-enqueued work. `scan_and_index_source` writes
/// new `index_jobs` rows directly, on the UI's own `Catalog` connection --
/// it does not wake this task, so idle periods are bridged by polling
/// rather than a signal.
const IDLE_POLL: Duration = Duration::from_millis(300);

/// How long since the last `UserActive` observation before the scheduler
/// is told the user has gone idle (RFC-057 §4.2). Search input fires far
/// more often than this while someone is actually typing, so continuous
/// activity never gaps; stopping resets to `Normal` within one interval of
/// the last keystroke or submit.
const USER_IDLE_TIMEOUT: Duration = Duration::from_secs(2);

/// An observation a producer reports about the environment (RFC-057 §4.1)
/// -- not a `ResourceMode` directly. Only [`run_with_context`]'s loop
/// decides what an observation means for scheduling, per RFC-036 §13; a
/// producer needs no knowledge of scheduler internals to send one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResourceObservation {
    /// The user is actively typing or searching (RFC-036 §13.1).
    UserActive,
    /// The machine's power-source state changed (RFC-036 §13.2). Carries
    /// the new state directly, unlike `UserActive` -- a battery source
    /// reports level-triggered state (on battery or not right now), not an
    /// edge, so there is no idle-timeout half to infer the way there is for
    /// activity.
    OnBattery(bool),
}

/// Resolve the active profile's sealed handles and run the loop forever.
/// Wired into `main.rs`'s `Subscription::run_with`; returns only if the
/// profile's catalog or cache cannot be opened at all -- a running profile
/// never sees this return.
///
/// `resource_signal_tx` is a clone of the same sender `main.rs`'s `update`
/// closure holds (RFC-057 §4.1's "one channel, many producers"): this
/// function spawns the battery poller (RFC-057 §4.3d) as the second
/// producer feeding it, alongside `main.rs`'s own `UserActive` producer.
pub async fn run(
    portable: bool,
    resource_signals: Receiver<ResourceObservation>,
    resource_signal_tx: Sender<ResourceObservation>,
    output: Sender<Message>,
) {
    let Ok(runtime) = crate::bootstrap::resolve_runtime_context(portable) else {
        return;
    };
    let Ok(catalog) = crate::bootstrap::open_catalog(&runtime) else {
        return;
    };
    let Ok(cache) = crate::bootstrap::cache_service(&runtime) else {
        return;
    };
    // Best-effort: a settings load failure falls back to defaults for both
    // values derived below -- `None` for the embedding model, which the
    // loop treats as RFC-008 §15 `model_missing` for any `GenerateEmbedding`
    // job it dispatches (the same fail-closed shape `bootstrap::search`'s
    // hybrid-search fallback already uses for the same resolution), and
    // `true` for `background_indexing`, `OrbokSettings::default()`'s own
    // value.
    let settings = crate::bootstrap::load_runtime_settings(&runtime).ok();
    let embedding_parts = settings.as_ref().and_then(|settings| {
        crate::bootstrap::embedding_resolution::resolve_embedding_worker_parts(
            &runtime,
            &AllowRuntimePathProbe,
            &catalog,
            settings,
        )
    });
    // RFC-036 §12 / RFC-019: `background_indexing` is a standing
    // preference, not the interactive per-session Pause/Resume control
    // RFC-036 §12.1/§14.3 describe (that is Slice 4's UI half) -- honored
    // once at startup, the same way `embedding_parts` itself is resolved
    // once here rather than reactively. There is no live control surface
    // yet that could change this setting mid-session.
    let background_indexing_enabled = settings
        .as_ref()
        .map(|settings| settings.background_indexing)
        .unwrap_or(true);
    // RFC-057 §4.4: whether an `OnBattery` observation is allowed to
    // affect scheduling at all -- the detector always runs and always
    // reports (below), same as `background_indexing`'s own
    // resolve-once-at-startup shape; this only gates the derivation's use
    // of what it reports, in `run_with_context`.
    let pause_embedding_on_battery_enabled = settings
        .as_ref()
        .map(|settings| settings.pause_embedding_on_battery)
        .unwrap_or(true);
    // RFC-057 §4.3d: the battery poller runs for the process's whole
    // lifetime, the same as the scheduler loop itself -- there is no
    // narrower scope to bound it to, and nothing here ever joins it
    // (it is dropped, not aborted, when the process exits).
    tokio::spawn(battery::watch_battery(
        battery::SystemBatterySource::new(),
        battery::BATTERY_POLL_INTERVAL,
        resource_signal_tx,
    ));
    run_with_context(
        catalog,
        cache,
        embedding_parts,
        background_indexing_enabled,
        pause_embedding_on_battery_enabled,
        resource_signals,
        output,
        None,
    )
    .await
}

/// The hosting loop's core logic (RFC-056 §4.1): pulls jobs from
/// `Scheduler::tick()`, executes them through the existing extract/chunk
/// workers, and reports progress. Owns its `Catalog`/`ProfileCache` --
/// both are `Send + 'static` sealed handles (§4.2) -- and never returns.
// Eight parameters, one over clippy's default: `event_count_probe` is
// test-only observability (Task 020), not a behavioral option like the
// other seven -- folding it into a config struct with the rest would
// blur that distinction rather than clarify it.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_with_context(
    catalog: Catalog,
    cache: ProfileCache,
    embedding_parts: Option<EmbeddingWorkerParts>,
    background_indexing_enabled: bool,
    pause_embedding_on_battery_enabled: bool,
    mut resource_signals: Receiver<ResourceObservation>,
    mut output: Sender<Message>,
    // Task 020: test-only observation seam. Production always passes
    // `None` (zero-cost: one branch on a `None`). `Scheduler` lives
    // entirely inside this function's stack frame with no external
    // handle, so a boundedness test needs something to read from outside
    // the running loop -- `fetch_max` records the worst retained count
    // seen across the whole run, which is what "does not scale with the
    // work done" actually asserts, not just the value at one arbitrary
    // instant.
    event_count_probe: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
) {
    let mut scheduler = Scheduler::with_defaults();
    if background_indexing_enabled {
        // RFC-056 §9 criterion 4: turning `background_indexing` back on
        // must resume work a prior session left `paused`, not just leave
        // the setting honest for the off case. Always called, not gated
        // on any local state -- `resume` itself always runs its catalog
        // fix-up now (a fresh `Scheduler`, which every real restart
        // constructs, has no way to know a *previous* session paused
        // anything), so this is cheap on the normal path (matches zero
        // rows) and correct on the restart-after-off path (matches the
        // rows that previous session paused).
        let _ = scheduler.resume(&catalog);
    } else {
        // RFC-036 §12.2 Safe Pause, applied before any job has been
        // dispatched: "finish the current small unit" is vacuously true
        // here (there is none yet), "stop taking new work" is `tick()`'s
        // own paused-mode short-circuit, and "persist progress" is
        // `pause`'s catalog update -- any `queued` row already present
        // (e.g. left over from a prior session) is marked `paused` too,
        // not just future ones.
        let _ = scheduler.pause(&catalog);
    }
    let mut known: HashSet<JobId> = HashSet::new();
    let cache_service = cache.service();
    let extract = ExtractionWorker::new(&catalog, cache_service);
    let chunk = ChunkAndIndexWorker::new(&catalog, cache_service);
    // `embedding_parts`' RFC-050 lease guard (if any) is held inside this
    // `EmbeddingWorker` for the loop's entire lifetime -- the loop never
    // returns, so nothing needs to `drop` it explicitly (the same pattern
    // `bootstrap/tests/embedding_blocking_measurement.rs` established for
    // a shorter-lived scan/index pass).
    let embed = embedding_parts.map(|parts| {
        EmbeddingWorker::with_model(&catalog, cache_service, parts.model, parts.model_id)
    });
    // RFC-057 §4.2: when this was last set, `USER_IDLE_TIMEOUT` since then
    // with no further `UserActive` observation means the user has stopped
    // -- producers report only that activity happened, not when it ends,
    // so the loop infers the end itself each iteration below.
    let mut last_user_activity: Option<Instant> = None;
    // RFC-057 §4.3d: level-triggered, unlike `last_user_activity` -- the
    // battery source reports current state directly, so this is simply the
    // most recent `OnBattery` observation with no timeout half needed.
    let mut on_battery = false;

    loop {
        // RFC-057 §4.1: drain every observation queued since the last
        // iteration before deciding what to dispatch, so `resource_mode`
        // reflects reality before `tick()` reads it below. `try_recv`
        // never blocks: `Err(Empty)`/`Err(Closed)` both mean "nothing more
        // right now," either way the loop moves on.
        while let Ok(observation) = resource_signals.try_recv() {
            match observation {
                ResourceObservation::UserActive => {
                    last_user_activity = Some(Instant::now());
                }
                ResourceObservation::OnBattery(state) => {
                    on_battery = state;
                }
            }
        }
        let user_active = last_user_activity.is_some_and(|last| last.elapsed() < USER_IDLE_TIMEOUT);
        if last_user_activity.is_some() && !user_active {
            last_user_activity = None;
        }
        // RFC-057 §4.4: `pause_embedding_on_battery_enabled = false` means
        // the user opted out of the reduction -- the detector still runs
        // and `on_battery` still tracks reality, but the derivation below
        // must never see it as `true`, the same "gate the effect, not the
        // signal" shape `background_indexing` itself doesn't need (it has
        // no live source to gate; this does).
        let effective_on_battery = on_battery && pause_embedding_on_battery_enabled;
        // RFC-057 §4.3c: derive the mode from held observation state every
        // iteration rather than mutating it per source -- `apply_resource_observation`
        // already refuses to override `Paused` (RFC-057 §3.2/HANDOFF §3.2),
        // the same guarantee `notify_user_active` gave the single-source
        // Slice 1 path.
        scheduler.apply_resource_observation(user_active, effective_on_battery);

        // Task 020 / Review 179 §4: `Scheduler::drain_events()` had zero
        // production callers, so `SchedulerEvent`s accumulated in
        // `self.events` for the process's entire lifetime -- ~4
        // events/job, unbounded on a large source (measured: 1,000 jobs
        // -> 4,000 events). Nothing needs them: every UI-facing need
        // RFC-036 §14 describes is already served from the catalog
        // (`Message::HealthUpdated`/`IndexHealth` below,
        // `index_jobs.last_error_kind`), not this stream, and forwarding
        // it would trade a memory problem for a responsiveness one
        // (~4 events/job into iced's update loop on a 5,000-file source).
        // Draining and discarding once per iteration here bounds the
        // buffer to at most one iteration's worth -- see `drain_events`'s
        // own doc comment before adding a real consumer.
        if let Some(probe) = &event_count_probe {
            probe.fetch_max(
                scheduler.pending_event_count(),
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        let _ = scheduler.drain_events();

        // `rehydrate` is a full `queued`-rows scan (RFC-036's queues have
        // no cheap "anything new?" signal) -- calling it every iteration
        // turns this into an O(n^2) crawl over a few hundred files (traced
        // during Slice 1 development: fine at 10 files, never finished 300
        // files). Only rehydrate when the in-memory queue has genuinely run
        // dry, so its cost is paid a handful of times per run (once per
        // queue-kind transition), not once per job.
        let job = match scheduler.tick() {
            Some(job) => job,
            None => {
                rehydrate(&mut scheduler, &catalog, &mut known);
                match scheduler.tick() {
                    Some(job) => job,
                    None => {
                        tokio::time::sleep(IDLE_POLL).await;
                        continue;
                    }
                }
            }
        };

        // RFC-036 §12.3 (source removal cancels queued work):
        // `bootstrap::remove_source` cascade-deletes `index_jobs` rows for
        // the removed source (the FK on `sources` does this) without
        // reaching into this task's live in-memory queue -- there is no
        // channel from the UI's synchronous call into a spawned task's
        // state, and none is needed: the catalog is the source of truth
        // (RFC-036 §16), and this check is what keeps the in-memory queue
        // honest against it. A job popped here whose catalog row is no
        // longer `queued` -- deleted by that cascade, or canceled by any
        // future direct catalog mutation -- is stale: skip it without
        // dispatching, completing, or failing it, since there is nothing
        // left to update either way.
        if !job_is_still_queued(&catalog, &job.id) {
            known.remove(&job.id);
            continue;
        }

        let file_id = job.file_id.clone();
        let result = match (job.kind, &file_id) {
            (JobKind::ScanSource, _) => run_scan(&catalog, job.source_id.clone()),
            (JobKind::ExtractFile, Some(file_id)) => extract.run(file_id),
            (JobKind::ChunkFile, Some(file_id)) | (JobKind::UpdateKeywordIndex, Some(file_id)) => {
                chunk.run(file_id)
            }
            (JobKind::GenerateEmbedding, Some(file_id)) => match &embed {
                Some(embed) => embed.run(file_id),
                // No model resolved at startup (RFC-008 §15) -- terminal,
                // via the same `OrbokError::Embedding` categorization path
                // every other embedding failure now goes through.
                None => Err(OrbokError::Embedding {
                    category: "model_missing",
                    message: "no embedding model configured".to_string(),
                }),
            },
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                let _ = scheduler.complete(&job.id, &catalog);
                known.remove(&job.id);
            }
            Err(error) => {
                tracing::warn!(job = job.id.as_str(), error = %error, "indexing job failed");
                // Not removed from `known`: `Scheduler::fail` either
                // re-queues the job in-memory (retry -- still correctly
                // tracked there) or permanently fails it in the catalog,
                // which then never reappears in `list_queued`. Removing it
                // here would risk `rehydrate` loading a second in-memory
                // copy of a job `fail`'s own retry path already re-queued.
                let error_kind = error_kind_for(&error);
                let _ = scheduler.fail(job, error_kind, Some(&error.to_string()), &catalog);
            }
        }
        report_health(&catalog, &mut output).await;
    }
}

/// Discover files under `source_id`, hash them, and enqueue the resulting
/// `Extract`/`Chunk`/`Embedding` jobs (RFC-004 change detection makes
/// re-scanning an unchanged source a cheap no-op, verified by Task 013's
/// tests and carried through unchanged here). A fresh, unshared cancel
/// token: cancelling a scan mid-flight is RFC-036 §12.3/§18.6, Slice 3's
/// scope, not wired here.
fn run_scan(catalog: &Catalog, source_id: SourceId) -> OrbokResult<()> {
    Scanner::new(catalog)
        .scan(
            &ScanRequest {
                source_id,
                force_hash: false,
                enqueue_index_jobs: true,
            },
            &AtomicBool::new(false),
        )
        .map(|_summary| ())
}

async fn report_health(catalog: &Catalog, output: &mut Sender<Message>) {
    let health = crate::bootstrap::get_health(catalog);
    // A dropped UI receiver (window closed) must not corrupt job state or
    // stop indexing -- it only silences progress reporting, matching
    // `download.rs`'s `ui_open` idiom.
    let _ = output.send(Message::HealthUpdated(health)).await;
}

/// Load every catalog `queued` row the in-memory scheduler doesn't already
/// know about. Catches jobs written directly by `IndexJobRepository::enqueue`
/// (the scanner, extraction, chunking -- all bypass `Scheduler::enqueue`),
/// and jobs recovered from a previous session: RFC-018's
/// `reset_interrupted_jobs` already runs at startup and puts any
/// interrupted `running` job back to `queued` before this loop ever starts,
/// so rehydration is the only piece still needed to complete RFC-036 §16
/// for the in-memory queue.
fn rehydrate(scheduler: &mut Scheduler, catalog: &Catalog, known: &mut HashSet<JobId>) {
    let jobs = IndexJobRepository::new(catalog);
    let Ok(records) = jobs.list_queued(u32::MAX) else {
        return;
    };
    for record in records {
        if known.contains(&record.job_id) {
            continue;
        }
        let Some(job) = index_job_from_record(&record) else {
            continue;
        };
        known.insert(record.job_id);
        scheduler.load_persisted(job);
    }

    // RFC-036 §20.2: `Scheduler::fail`'s retry branch marks a job `blocked`
    // (rather than `queued`) precisely when its in-memory push was skipped
    // for lack of room -- it has no in-memory copy by construction, so
    // `known` (which exists to avoid double-adding a row that might still
    // be correctly tracked) must not gate it here. On a successful reload
    // the catalog status is corrected back to `queued`; if there is still
    // no room, it stays `blocked` for the next rehydration pass.
    let Ok(blocked) = jobs.list_blocked(u32::MAX) else {
        return;
    };
    for record in blocked {
        let id = record.job_id.clone();
        let Some(job) = index_job_from_record(&record) else {
            continue;
        };
        if scheduler.load_persisted(job) {
            known.insert(id.clone());
            let _ = jobs.set_status(&id, JobStatus::Queued);
        }
    }
}

/// Build the in-memory `IndexJob` a `rehydrate` pass loads from a
/// persisted `JobRecord`, or `None` for a source-less row -- defensive:
/// no current enqueue path produces one.
fn index_job_from_record(record: &JobRecord) -> Option<IndexJob> {
    let source_id = record.source_id.clone()?;
    let kind = job_kind_for(record.job_type);
    Some(IndexJob {
        id: record.job_id.clone(),
        file_id: record.file_id.clone(),
        source_id,
        kind,
        priority: kind.default_priority(),
        state: JobState::Pending,
        attempt_count: 0,
        last_error_kind: None,
    })
}

/// Whether a just-popped job's catalog row is still `queued` -- `false`
/// covers both a row deleted out from under it (RFC-036 §12.3: source
/// removal cascade-deletes `index_jobs` via the FK on `sources`) and a row
/// whose status changed to anything else. `status_of` returning `Err` is
/// treated the same as "not queued" -- fail-closed: a catalog read error
/// is a reason to skip dispatch, not a reason to guess.
fn job_is_still_queued(catalog: &Catalog, id: &JobId) -> bool {
    matches!(
        IndexJobRepository::new(catalog).status_of(id),
        Ok(Some(JobStatus::Queued))
    )
}

/// The category `Scheduler::fail` (RFC-036 §20.1) matches on to decide
/// retry vs. terminal. An `OrbokError::Embedding` carries its own RFC-008
/// §15 category directly; every other error kind -- scan, extract, chunk
/// failures -- keeps the pre-Slice-2 `"worker_error"` label, which is not
/// in `is_terminal_category`'s set, so those jobs retry exactly as they
/// always have.
fn error_kind_for(error: &OrbokError) -> &'static str {
    match error {
        OrbokError::Embedding { category, .. } => category,
        _ => "worker_error",
    }
}

fn job_kind_for(job_type: JobType) -> JobKind {
    match job_type {
        JobType::Scan => JobKind::ScanSource,
        JobType::Extract => JobKind::ExtractFile,
        JobType::Chunk => JobKind::ChunkFile,
        JobType::KeywordIndex => JobKind::UpdateKeywordIndex,
        JobType::Embedding => JobKind::GenerateEmbedding,
        JobType::DeleteStale => JobKind::Cleanup,
        JobType::Rebuild => JobKind::Repair,
    }
}

/// Identity for `Subscription::run_with` (RFC-057 §4.1): bundles `portable`
/// with the receiving half of the resource-observation channel, plus a
/// clone of the sending half (RFC-057 §4.3d) for [`run`] to hand to the
/// battery poller -- `Sender` is `Clone`, unlike `Receiver`, so this half
/// needs none of `resource_signals`' `Arc<Mutex<Option<..>>>` machinery.
/// Iced requires a plain `fn(&D) -> S` here, not a capturing closure (see
/// `run_stream` below), so this is how both halves -- constructed once in
/// `main.rs`, since `main.rs`'s `update` closure needs its own clone of the
/// sender -- reach the task despite that constraint.
///
/// `Hash` considers only `portable`: `main.rs`'s `.subscription(..)`
/// closure runs on every re-render and clones this fresh each time, but
/// the identity `Subscription::run_with` dedups on must stay exactly what
/// it was pre-RFC-057 (`portable` alone, which never changes after
/// startup) -- otherwise the task would look "new" every frame and never
/// actually run.
#[derive(Clone)]
pub struct SchedulerSubscriptionData {
    pub portable: bool,
    pub resource_signals: std::sync::Arc<std::sync::Mutex<Option<Receiver<ResourceObservation>>>>,
    pub resource_signal_tx: Sender<ResourceObservation>,
}

impl std::hash::Hash for SchedulerSubscriptionData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.portable.hash(state);
    }
}

/// The app-wide subscription that hosts the scheduler for the process's
/// lifetime. `Subscription::run_with` deduplicates by `data`'s `Hash`
/// identity (`portable` never changes after startup), so `main.rs` calling
/// this on every `.subscription()` re-evaluation still spawns [`run`]
/// exactly once, the same long-lived pattern the iced websocket example
/// uses for a perpetual connection -- unlike `download.rs`'s
/// `Task::stream`, which drives a one-shot action to completion.
pub fn subscription(data: SchedulerSubscriptionData) -> iced::Subscription<Message> {
    iced::Subscription::run_with(data, run_stream)
}

// `+ use<>` (edition 2024 precise capturing): the returned stream must not
// depend on `data`'s elided lifetime -- `Subscription::run_with` requires a
// plain `fn(&D) -> S`, not one whose `S` is tied to the borrow's lifetime.
fn run_stream(data: &SchedulerSubscriptionData) -> impl futures::Stream<Item = Message> + use<> {
    let portable = data.portable;
    // `.take()`: `run_stream` itself only ever runs once per process (the
    // `Hash` identity above guarantees `Subscription::run_with` never
    // rebuilds it), so the receiver is moved out exactly once here and the
    // `Arc<Mutex<..>>` wrapper has done its only job -- getting a
    // non-`Clone` `Receiver` through a `fn` pointer that cannot capture it
    // directly.
    let resource_signals = data
        .resource_signals
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .expect("scheduler subscription must only be built once");
    let resource_signal_tx = data.resource_signal_tx.clone();
    iced::stream::channel(64, async move |output| {
        run(portable, resource_signals, resource_signal_tx, output).await;
    })
}

#[cfg(test)]
mod tests;
