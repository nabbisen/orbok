//! Repository layer (RFC-002 §8). Each repository owns the SQL for one
//! table family and exposes application-level types.

pub mod chunks;
pub mod cleanup;
pub mod embeddings;
pub mod events;
pub mod files;
pub mod jobs;
pub mod managed_generations;
pub mod models;
pub mod search_history;
pub mod settings;
pub mod sources;
pub mod storage;

pub use chunks::{ChunkRecord, ChunkRepository, ChunkSpec};
pub use cleanup::CleanupExecutor;
pub use embeddings::{EmbeddingRecord, EmbeddingRepository, NewEmbedding};
pub use events::{EventRepository, Severity};
pub use files::{FileRecord, FileRepository, NewFile, ObservedMetadata};
pub use jobs::{IndexJobRepository, JobRecord};
pub use managed_generations::{GenerationCatalogError, ManagedGenerationRepository};
pub use models::{
    ModelRecord, ModelRepository, ModelRole, ModelStatus, NewModel, verify_model_sha256,
};
pub use orbok_core::{ExtractionId, JobStatus, JobType};
pub use search_history::SearchHistoryRepository;
pub use settings::SettingsRepository;
pub use sources::{NewSource, SourceRecord, SourceRepository};
pub use storage::StorageAccountingRepository;
