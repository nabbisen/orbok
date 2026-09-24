//! Task 102: a `Chunk` job for an extraction that already has active chunks
//! is an ordinary event, not an error. The keyword rebuild no longer forces
//! a new extraction (and so new chunk ids and new embeddings) to dodge the
//! `UNIQUE(file_id, extraction_id, chunk_ordinal)` constraint.

use crate::{ChunkAndIndexWorker, CleanupService, ExtractionWorker, run_pending};
use orbok_cache::CacheService;
use orbok_core::{
    CleanupAction, CleanupPlan, FileStatus, HiddenFilePolicy, IndexMode, JobType, PersistenceMode,
    SourceType, SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    ChunkRepository, FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata,
    SourceRepository,
};
use orbok_search::SearchService;
use std::path::Path;

/// One column as readable text (a blob is shown by its length).
fn cell(v: rusqlite::types::ValueRef<'_>) -> String {
    use rusqlite::types::ValueRef::*;
    match v {
        Null => "NULL".into(),
        Integer(i) => i.to_string(),
        Real(f) => f.to_string(),
        Text(t) => String::from_utf8_lossy(t).into_owned(),
        Blob(b) => format!("blob({})", b.len()),
    }
}
struct Profile {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    catalog: Catalog,
    cache: CacheService,
    source_id: orbok_core::SourceId,
    files: Vec<orbok_core::FileId>,
}

impl Profile {
    /// A real folder of markdown files, extracted and chunked and keyword-
    /// indexed by the real workers (`run_pending`), with no embedding model.
    fn indexed(bodies: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let catalog = Catalog::open(root.join("catalog.sqlite3")).unwrap();
        let cache = CacheService::new(&root);
        let docs = root.join("docs");
        std::fs::create_dir(&docs).unwrap();
        let docs_str = std::fs::canonicalize(&docs)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let source_id = SourceRepository::new(&catalog)
            .insert(NewSource {
                source_type: SourceType::Directory,
                persistence_mode: PersistenceMode::Persistent,
                display_name: Some("docs".into()),
                original_path: docs_str.clone(),
                canonical_path: docs_str.clone(),
                index_mode: IndexMode::Balanced,
                include_patterns: vec![],
                exclude_patterns: vec![],
                hidden_file_policy: HiddenFilePolicy::Exclude,
                symlink_policy: SymlinkPolicy::Ignore,
                max_file_size_bytes: None,
            })
            .unwrap()
            .source_id;
        let mut files = Vec::new();
        for (name, body) in bodies {
            let path = docs.join(name);
            std::fs::write(&path, body).unwrap();
            let canonical = std::fs::canonicalize(&path)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let record = FileRepository::new(&catalog)
                .insert(NewFile {
                    source_id: source_id.clone(),
                    original_path: canonical.clone(),
                    canonical_path: canonical,
                    display_path: (*name).into(),
                    extension: Some("md".into()),
                    metadata: ObservedMetadata {
                        file_size_bytes: body.len() as u64,
                        modified_at: Some("2026-01-01T00:00:00Z".into()),
                        platform_file_key: None,
                        content_hash: Some(format!("hash-{name}")),
                    },
                    status: FileStatus::Discovered,
                })
                .unwrap();
            IndexJobRepository::new(&catalog)
                .enqueue(JobType::Extract, Some(&source_id), Some(&record.file_id))
                .unwrap();
            files.push(record.file_id);
        }
        let profile = Self {
            _dir: dir,
            root,
            catalog,
            cache,
            source_id,
            files,
        };
        profile.drain();
        profile
    }

    fn drain(&self) {
        let extract = ExtractionWorker::new(&self.catalog, &self.cache);
        let chunk = ChunkAndIndexWorker::new(&self.catalog, &self.cache);
        run_pending(&self.catalog, &extract, &chunk, None, 500).unwrap();
    }

    fn sql_count(&self, sql: &str) -> i64 {
        self.catalog
            .lock()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }

    /// Every row of `sql`'s result, every column, as text.
    fn dump(&self, sql: &str) -> Vec<Vec<String>> {
        let conn = self.catalog.lock();
        let mut stmt = conn.prepare(sql).unwrap();
        let columns = stmt.column_count();
        stmt.query_map([], |row| {
            (0..columns)
                .map(|i| row.get_ref(i).map(cell))
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    fn chunks(&self) -> Vec<Vec<String>> {
        self.dump(
            "SELECT chunk_id, file_id, extraction_id, chunk_ordinal, content_hash, chunk_status \
             FROM chunks ORDER BY file_id, chunk_ordinal, chunk_id",
        )
    }

    fn keyword_rows(&self) -> Vec<Vec<String>> {
        self.dump(
            "SELECT chunk_id, fts_rowid, trigram_fts_rowid, status FROM keyword_index_records \
             ORDER BY chunk_id",
        )
    }

    fn embeddings(&self) -> Vec<Vec<String>> {
        self.dump("SELECT * FROM embeddings ORDER BY embedding_id")
    }

    /// Jobs that ended `failed`, other than the embedding jobs that fail
    /// `model_missing` in every test here (no model is configured).
    fn failed_non_embedding_jobs(&self) -> i64 {
        self.sql_count(
            "SELECT COUNT(*) FROM index_jobs WHERE status = 'failed' AND job_type != 'embedding'",
        )
    }

    fn embedding_jobs(&self) -> i64 {
        self.sql_count("SELECT COUNT(*) FROM index_jobs WHERE job_type = 'embedding'")
    }

    fn finds(&self, query: &str) -> bool {
        !SearchService::new(&self.catalog)
            .search(query, 10)
            .unwrap()
            .is_empty()
    }

    fn invariant_holds(&self, after: &str) {
        let counts = ChunkRepository::new(&self.catalog)
            .keyword_index_counts()
            .unwrap();
        if let Some(violation) = counts.violation() {
            panic!("after {after}: {violation}");
        }
    }

    /// Give every active chunk an embedding row directly -- the rows are
    /// what the tests compare, so no model is needed.
    fn seed_embeddings(&self) {
        let now = "2026-09-24T00:00:00Z";
        let conn = self.catalog.lock();
        conn.execute(
            "INSERT INTO models (model_id, role, model_name, model_version, dimension, status, \
             created_at, updated_at) VALUES ('m','embedding','mock','v1',8,'available',?1,?1)",
            [now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO embeddings (embedding_id, chunk_id, model_id, vector_format, dimension, \
             norm, storage_location, vector_blob, status, created_at, updated_at) \
             SELECT 'e-' || chunk_id, chunk_id, 'm', 'fp32', 8, 'l2', 'sqlite_blob', \
             zeroblob(32), 'active', ?1, ?1 FROM chunks WHERE chunk_status = 'active'",
            [now],
        )
        .unwrap();
    }

    /// The production keyword rebuild: the plan `bootstrap::delete_keyword_index`
    /// builds, run through the same service.
    fn delete_keyword_index(&self) {
        let plan = CleanupPlan::for_action(CleanupAction::DeleteKeywordIndex, 0);
        let cache_db = self.root.join("orbok-cache.sqlite3");
        CleanupService::new(&self.catalog, &self.cache, Path::new(&cache_db))
            .run_safe(&plan)
            .unwrap();
    }
}

const TWO_DOCS: [(&str, &str); 2] = [
    (
        "alpha.md",
        "# Alpha\n\nThe zebrafootnote marker lives in alpha.\n\n## More\n\nSecond section text.\n",
    ),
    (
        "beta.md",
        "# Beta\n\nA quokkaparagraph marker lives in beta.\n",
    ),
];

/// §2.1: a keyword rebuild leaves search by meaning alone -- the same chunk
/// ids, the same embedding rows, no new embedding job -- and keyword search
/// finds the files again.
#[test]
fn a_keyword_rebuild_leaves_chunk_ids_and_embeddings_alone() {
    let p = Profile::indexed(&TWO_DOCS);
    assert!(p.finds("zebrafootnote") && p.finds("quokkaparagraph"));
    p.seed_embeddings();
    let chunks_before = p.chunks();
    let embeddings_before = p.embeddings();
    let embedding_jobs_before = p.embedding_jobs();
    assert!(!embeddings_before.is_empty());

    p.delete_keyword_index();
    assert!(
        !p.finds("zebrafootnote"),
        "control: the keyword index is gone"
    );
    p.drain();

    assert!(p.chunks() == chunks_before, "chunk ids are unchanged");
    assert!(
        p.embeddings() == embeddings_before,
        "embeddings, row for row"
    );
    assert_eq!(
        p.embedding_jobs(),
        embedding_jobs_before,
        "no Embedding job was queued"
    );
    assert!(p.finds("zebrafootnote") && p.finds("quokkaparagraph"));
    assert_eq!(p.failed_non_embedding_jobs(), 0);
    p.invariant_holds("a keyword rebuild");
}

/// §2.2: a duplicated `Chunk` job for a file that is already indexed
/// succeeds and changes no row.
#[test]
fn a_duplicate_chunk_job_is_harmless() {
    let p = Profile::indexed(&TWO_DOCS);
    let chunks_before = p.chunks();
    let keyword_before = p.keyword_rows();
    let files_before = p.dump("SELECT file_id, file_status, last_indexed_at FROM files ORDER BY 1");

    IndexJobRepository::new(&p.catalog)
        .enqueue(JobType::Chunk, Some(&p.source_id), Some(&p.files[0]))
        .unwrap();
    p.drain();

    assert_eq!(
        p.failed_non_embedding_jobs(),
        0,
        "the duplicate job succeeded"
    );
    assert!(p.chunks() == chunks_before, "chunks unchanged");
    assert!(p.keyword_rows() == keyword_before, "keyword rows unchanged");
    assert!(
        p.dump("SELECT file_id, file_status, last_indexed_at FROM files ORDER BY 1")
            == files_before,
        "a healthy file is not touched"
    );
    assert!(p.finds("zebrafootnote"));
    p.invariant_holds("a duplicate chunk job");
}

/// §2.3: Prepare again on a healthy file works.
#[test]
fn prepare_again_on_a_healthy_file_works() {
    let p = Profile::indexed(&TWO_DOCS);
    assert!(
        IndexJobRepository::new(&p.catalog)
            .enqueue_extraction_if_idle(&p.files[0])
            .unwrap()
    );
    p.drain();

    let status: String = p
        .catalog
        .lock()
        .query_row(
            "SELECT file_status FROM files WHERE file_id = ?1",
            [p.files[0].as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "indexed");
    assert!(p.finds("zebrafootnote"));
    assert_eq!(p.failed_non_embedding_jobs(), 0);
    p.invariant_holds("Prepare again");
}

/// §2.4: chunks that differ from the stored ones fall back to a new
/// generation under a new extraction id, without a `UNIQUE` error.
#[test]
fn changed_chunks_fall_back_to_a_new_generation() {
    let p = Profile::indexed(&TWO_DOCS);
    let old_extraction: String = p
        .catalog
        .lock()
        .query_row(
            "SELECT extraction_id FROM chunks WHERE file_id = ?1 AND chunk_status = 'active' \
             LIMIT 1",
            [p.files[0].as_str()],
            |r| r.get(0),
        )
        .unwrap();
    // The chunker "changed its mind" about one chunk of the first file.
    p.catalog
        .lock()
        .execute(
            "UPDATE chunks SET content_hash = 'not-the-same' WHERE file_id = ?1 \
             AND chunk_ordinal = 0",
            [p.files[0].as_str()],
        )
        .unwrap();

    IndexJobRepository::new(&p.catalog)
        .enqueue(JobType::Chunk, Some(&p.source_id), Some(&p.files[0]))
        .unwrap();
    p.drain();

    let new_extraction: String = p
        .catalog
        .lock()
        .query_row(
            "SELECT extraction_id FROM chunks WHERE file_id = ?1 AND chunk_status = 'active' \
             LIMIT 1",
            [p.files[0].as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(new_extraction, old_extraction, "a new generation was made");
    assert_eq!(p.failed_non_embedding_jobs(), 0, "no UNIQUE error");
    assert!(p.finds("zebrafootnote"));
    p.invariant_holds("changed chunks");
}
