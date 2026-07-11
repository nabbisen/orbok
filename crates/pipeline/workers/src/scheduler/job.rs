//! Job model for the resource-aware scheduler (RFC-036 §11, §15).
//!
//! Types here are the in-memory representation used by the scheduler.
//! Persistence uses `orbok_core::{JobType, JobStatus}` and the
//! `index_jobs` catalog table (RFC-002 §7.9); the scheduler maps
//! between the two representations.

use orbok_core::{FileId, JobId, JobType, SourceId};
use serde::{Deserialize, Serialize};

// ── Priority ──────────────────────────────────────────────────────────────

/// Work priority levels (RFC-036 §8.1).
///
/// Higher variants are dispatched first. `Ord` is derived so that
/// `UserBlocking > UserVisible > NormalBackground > LowBackground >
/// Maintenance`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkPriority {
    Maintenance = 0,
    LowBackground = 1,
    #[default]
    NormalBackground = 2,
    UserVisible = 3,
    UserBlocking = 4,
}

impl WorkPriority {
    /// Catalog integer stored in `index_jobs.priority`.
    ///
    /// Note: the baseline schema used `DEFAULT 0` for priority before
    /// RFC-036. Existing rows with priority=0 map to `Maintenance`.
    /// New jobs use `NormalBackground (2)` or higher by default.
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(v: i64) -> Self {
        match v {
            4 => Self::UserBlocking,
            3 => Self::UserVisible,
            2 => Self::NormalBackground,
            1 => Self::LowBackground,
            _ => Self::Maintenance,
        }
    }
}

// ── Job state ─────────────────────────────────────────────────────────────

/// Scheduler job state (RFC-036 §11; extends `orbok_core::JobStatus`).
///
/// This is the in-memory view. The catalog stores a subset via
/// `JobStatus` in `orbok-core`; `Paused` and `WaitingForDependency`
/// are new with RFC-036 and are added to `JobStatus` in `status.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    WaitingForDependency,
}

// ── Job kind ──────────────────────────────────────────────────────────────

/// Job kind labels used by the scheduler (RFC-036 §11).
///
/// Maps 1-to-1 with `orbok_core::JobType`; kept separate so the
/// scheduler can add kinds (e.g. `Repair`) without changing the
/// catalog schema until needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ScanSource,
    ExtractFile,
    ChunkFile,
    UpdateKeywordIndex,
    GenerateEmbedding,
    Cleanup,
    Repair,
}

impl JobKind {
    /// Map to the catalog `JobType` for persistence.
    pub fn as_job_type(self) -> JobType {
        match self {
            JobKind::ScanSource => JobType::Scan,
            JobKind::ExtractFile => JobType::Extract,
            JobKind::ChunkFile => JobType::Chunk,
            JobKind::UpdateKeywordIndex => JobType::KeywordIndex,
            JobKind::GenerateEmbedding => JobType::Embedding,
            JobKind::Cleanup => JobType::DeleteStale,
            JobKind::Repair => JobType::Rebuild,
        }
    }

    /// Natural priority for this kind of work (RFC-036 §8).
    pub fn default_priority(self) -> WorkPriority {
        match self {
            JobKind::ScanSource => WorkPriority::NormalBackground,
            JobKind::ExtractFile => WorkPriority::NormalBackground,
            JobKind::ChunkFile => WorkPriority::NormalBackground,
            JobKind::UpdateKeywordIndex => WorkPriority::NormalBackground,
            JobKind::GenerateEmbedding => WorkPriority::LowBackground,
            JobKind::Cleanup => WorkPriority::Maintenance,
            JobKind::Repair => WorkPriority::Maintenance,
        }
    }
}

// ── IndexJob ──────────────────────────────────────────────────────────────

/// An in-memory scheduler job (RFC-036 §11).
#[derive(Debug, Clone)]
pub struct IndexJob {
    pub id: JobId,
    pub file_id: Option<FileId>,
    pub source_id: SourceId,
    pub kind: JobKind,
    pub priority: WorkPriority,
    pub state: JobState,
    pub attempt_count: u32,
    pub last_error_kind: Option<String>,
}

impl IndexJob {
    pub fn new(source_id: SourceId, kind: JobKind) -> Self {
        Self {
            id: JobId::generate(),
            file_id: None,
            source_id,
            priority: kind.default_priority(),
            kind,
            state: JobState::Pending,
            attempt_count: 0,
            last_error_kind: None,
        }
    }

    pub fn with_file(mut self, file_id: FileId) -> Self {
        self.file_id = Some(file_id);
        self
    }

    pub fn with_priority(mut self, priority: WorkPriority) -> Self {
        self.priority = priority;
        self
    }
}

// ── Scheduler events ──────────────────────────────────────────────────────

/// Events emitted by the scheduler (RFC-036 §15).
///
/// The app layer listens to these to update the Indexing view.
/// Use plain-language copy in the UI layer; never expose these
/// enum names directly to users.
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    JobQueued(JobId),
    JobStarted(JobId),
    JobPaused(JobId),
    JobResumed(JobId),
    JobCompleted(JobId),
    JobFailed {
        id: JobId,
        error_kind: String,
    },
    JobCancelled(JobId),
    QueueBackpressureApplied(QueueKind),
    QueueBackpressureReleased(QueueKind),
    UserActivityDetected,
    ResourceModeChanged(ResourceMode),
    PartialReadinessChanged {
        ready_count: u64,
        pending_count: u64,
    },
}

/// Which queue triggered backpressure (RFC-036 §10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Scan,
    Extract,
    Chunk,
    Keyword,
    Embedding,
    Maintenance,
}

// ── Resource mode ─────────────────────────────────────────────────────────

/// Current resource/activity mode of the scheduler (RFC-036 §15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResourceMode {
    /// Normal background operation.
    #[default]
    Normal,
    /// User is actively searching or typing — reduce background work.
    UserActive,
    /// Running in low-impact mode (battery/thermal policy).
    LowImpact,
    /// All background work is paused by user request.
    Paused,
}
