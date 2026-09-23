//! Catalog connection management (RFC-002 §5).
//!
//! The catalog is opened with foreign keys ON, WAL journal, NORMAL
//! synchronous, and in-memory temp store. Writes are serialized through
//! a single mutex-guarded connection (RFC-002 §5 "one serialized writer
//! path"); v1 keeps reads on the same connection for simplicity.

use crate::migrations;
use orbok_core::{OrbokError, OrbokResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

/// How long a connection waits on `SQLITE_BUSY` before giving up (RFC-061
/// §5). One `Catalog` per process (Slice 1) makes contention from this
/// process alone impossible, but the scheduler task and the UI still meet
/// at the SQLite level through WAL as two separate connections to the same
/// file -- this is what turns that contention into a bounded wait instead
/// of an immediate error.
///
/// **Checked, not assumed, and the RFC's premise was wrong**: RFC-061 §1
/// says this is `0` in production today. It is not — `rusqlite` 0.39's
/// `Connection::open_with_flags` already calls `sqlite3_busy_timeout(db,
/// 5000)` unconditionally for every connection it opens
/// (`inner_connection.rs:118`, confirmed by reading the vendored source and
/// by a standalone reproduction: a bare `Connection::open_in_memory()`
/// already reports `PRAGMA busy_timeout` = 5000). So this call is currently
/// a no-op, not a fix. Kept anyway, explicitly: the requirement should not
/// depend on an undocumented default in a dependency that could change
/// without notice on an upgrade, and this makes the 5-second figure this
/// project's own decision rather than an inherited accident.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// File name of the authoritative catalog database (Appendix A §3).
pub const CATALOG_FILE_NAME: &str = "orbok-catalog.sqlite3";

/// File name of the localcache-managed payload database (Appendix A §3).
pub const CACHE_FILE_NAME: &str = "orbok-cache.sqlite3";

/// The authoritative orbok catalog.
pub struct Catalog {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Catalog {
    /// Open (or create) the catalog at `path`, apply pragmas, and run
    /// pending migrations. Migration failure aborts startup (RFC-002
    /// §6.2).
    pub fn open(path: impl AsRef<Path>) -> OrbokResult<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path).map_err(db_err)?;
        Self::from_connection(conn, path)
    }

    /// Open an in-memory catalog (tests).
    pub fn open_in_memory() -> OrbokResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        Self::from_connection(conn, PathBuf::from(":memory:"))
    }

    fn from_connection(conn: Connection, path: PathBuf) -> OrbokResult<Self> {
        conn.busy_timeout(BUSY_TIMEOUT).map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;
        // WAL is unsupported for in-memory databases; ignore that case.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(db_err)?;
        conn.pragma_update(None, "temp_store", "MEMORY")
            .map_err(db_err)?;

        let catalog = Self {
            conn: Mutex::new(conn),
            path,
        };
        migrations::run_pending(&catalog)?;

        // RFC-062 §6: the schema-downgrade guard, moved here from
        // `run_check` -- the headless `--check` diagnostic already refused
        // a catalog from a newer version; the actual application did not.
        // Checked after `run_pending`, not before: `run_pending` only ever
        // adds rows for migrations *this* binary knows about, so on a
        // catalog written by a newer orbok it is a no-op (nothing in
        // `MIGRATIONS` is still unapplied) and `schema_version()` still
        // reflects the newer binary's higher stamp afterward -- exactly the
        // condition this guard exists to catch.
        //
        // Also reached by `open_in_memory` (tests): a fresh in-memory
        // catalog always starts at `stored = 0` and `run_pending` always
        // brings it to `latest_version()`, so `stored > supported` never
        // holds there and this is a no-op in practice, not a special case
        // — a choice, not an accident (HANDOFF-062 §4/§5 Q3).
        let stored = catalog.schema_version()?;
        let supported = migrations::latest_version();
        if stored > supported {
            return Err(OrbokError::SchemaVersionUnsupported { stored, supported });
        }

        Ok(catalog)
    }

    /// Acquire the serialized connection. Repositories use this; the
    /// guard scope is kept short.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .expect("catalog connection mutex poisoned — a repository panicked mid-write")
    }

    /// Path of the catalog database file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Compact the file via `VACUUM` (Task 095: a reset gives the space
    /// back). Plain `DELETE` never shrinks a SQLite file -- freed pages
    /// stay allocated until something rebuilds it (RFC-059 §10 criterion
    /// 6). Safe against the current connection outside any transaction:
    /// `VACUUM` cannot run inside one, and every caller of this (`Reset`'s
    /// own transaction in `CleanupExecutor::run_reset_catalog`) has
    /// already committed by the time this runs. The caller is expected to
    /// have already checked there is enough free space -- this only
    /// issues the statement.
    ///
    /// **`VACUUM` alone does not give the space back under this catalog's
    /// own settings** -- confirmed empirically, not assumed: this
    /// connection runs WAL (`from_connection`), and `VACUUM`'s rebuilt
    /// content lands in the WAL like any other write rather than in the
    /// main file, so the file this project measures (and the user sees on
    /// disk) stayed exactly its pre-`VACUUM` size in a real run until a
    /// checkpoint moved that content back. `wal_checkpoint(TRUNCATE)`
    /// forces that checkpoint and truncates the WAL file itself back down
    /// -- without it, `VACUUM` still shrinks the *logical* database but
    /// not the bytes actually occupying the disk, which is the entire
    /// point of this task.
    ///
    /// **The checkpoint itself can be refused, silently, and Task 095's
    /// review caught it**: `PRAGMA wal_checkpoint(TRUNCATE)` returns a row
    /// -- `(busy, log_frames, checkpointed_frames)` -- rather than erroring
    /// when another connection still holds an open read against the WAL;
    /// `busy` is then non-zero and the WAL is not truncated. A running
    /// profile has more than one connection to this file (the router's and
    /// the scheduler host's own, `scheduler_host.rs`), so this is read and
    /// logged rather than discarded via `execute_batch` as before.
    pub fn vacuum(&self) -> OrbokResult<()> {
        let conn = self.lock();
        conn.execute_batch("VACUUM;").map_err(db_err)?;
        let (busy, log_frames, checkpointed_frames): (i64, i64, i64) = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE);", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(db_err)?;
        if busy != 0 {
            tracing::warn!(
                log_frames,
                checkpointed_frames,
                "wal checkpoint could not fully truncate after VACUUM -- \
                 another connection was still reading, so the catalog file \
                 may not have shrunk on disk"
            );
        }
        Ok(())
    }

    /// Current schema version (0 when no migration has been applied).
    pub fn schema_version(&self) -> OrbokResult<i64> {
        let conn = self.lock();
        let version = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        Ok(version)
    }
}

/// Map a rusqlite error to the typed orbok error.
pub(crate) fn db_err(e: rusqlite::Error) -> OrbokError {
    OrbokError::Database(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-061 §5: a busy timeout must actually be set on every opened
    /// connection, not just declared as a constant nobody applies.
    ///
    /// **Not mutation-testable against this line specifically, and said so
    /// rather than claimed otherwise**: `rusqlite` 0.39 already applies
    /// `sqlite3_busy_timeout(db, 5000)` internally for every connection
    /// (see `BUSY_TIMEOUT`'s own doc comment), so removing the explicit
    /// `conn.busy_timeout(BUSY_TIMEOUT)` call does not change this
    /// assertion's outcome -- confirmed by actually removing it and
    /// re-running, not assumed. What this guards against is a *different*
    /// regression: a future `rusqlite` upgrade silently changing that
    /// internal default, or a later pragma in `from_connection` resetting
    /// it. The property is real and worth asserting even though this one
    /// line isn't what a mutation of it would currently prove.
    #[test]
    fn busy_timeout_is_set_on_the_connection() {
        let catalog = Catalog::open_in_memory().unwrap();
        let ms: i64 = catalog
            .lock()
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(
            ms,
            BUSY_TIMEOUT.as_millis() as i64,
            "busy_timeout pragma must reflect BUSY_TIMEOUT"
        );
    }

    /// RFC-062 §8 acceptance criterion 4: a catalog whose `schema_version`
    /// is one above `latest_version()` is refused, with an error naming
    /// both versions. A fully-migrated real catalog, closed, then stamped
    /// with one extra `schema_migrations` row one version beyond what this
    /// binary's own `MIGRATIONS` list knows about -- the same situation a
    /// downgrade, a synced profile from a newer machine, or `ORBOK_DATA_DIR`
    /// pointed at a newer install would produce (RFC-049/RFC-054).
    #[test]
    fn schema_version_from_the_future_is_refused_naming_both_versions() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("future.sqlite3");
        let future_version = migrations::latest_version() + 1;
        {
            let catalog = Catalog::open(&path).unwrap();
            catalog
                .lock()
                .execute(
                    "INSERT INTO schema_migrations (version, name, applied_at) \
                     VALUES (?1, 'from_the_future', '2099-01-01T00:00:00Z')",
                    rusqlite::params![future_version],
                )
                .unwrap();
        }

        match Catalog::open(&path) {
            Err(OrbokError::SchemaVersionUnsupported { stored, supported }) => {
                assert_eq!(stored, future_version);
                assert_eq!(supported, migrations::latest_version());
            }
            Err(other) => panic!(
                "expected OrbokError::SchemaVersionUnsupported {{ stored: {future_version}, \
                 supported: {} }}, got a different error: {other}",
                migrations::latest_version()
            ),
            Ok(_) => panic!(
                "a catalog stamped with schema_version {future_version} (one above this \
                 build's {}) must be refused, not opened",
                migrations::latest_version()
            ),
        }
    }

    #[derive(Clone)]
    struct SharedBuf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedBuf {
        type Writer = SharedBuf;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn capture_logs(f: impl FnOnce()) -> String {
        let buf = SharedBuf(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buf.clone())
            .with_ansi(false)
            .without_time()
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        String::from_utf8(buf.0.lock().unwrap().clone()).unwrap()
    }

    /// Task 095 review (Review 273 §3): `wal_checkpoint(TRUNCATE)` does not
    /// error when it cannot finish -- it returns `busy = 1` and leaves the
    /// WAL untruncated, silently, unless the caller reads and reports that.
    /// The real-app shape this guards: a running profile has more than one
    /// connection to the same catalog file (the router's and the scheduler
    /// host's own, `scheduler_host.rs`), so this opens a genuine second
    /// connection to the same file and holds an open read transaction on
    /// it -- not a mock, the actual condition that produces `busy`.
    #[test]
    fn vacuum_logs_a_warning_when_another_connection_blocks_the_checkpoint() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("catalog.sqlite3");
        let catalog = Catalog::open(&path).unwrap();

        let reader = Connection::open(&path).unwrap();
        reader
            .execute_batch("BEGIN; SELECT * FROM schema_migrations LIMIT 1;")
            .unwrap();

        let log = capture_logs(|| {
            catalog
                .vacuum()
                .expect("VACUUM itself must still succeed alongside a concurrent reader")
        });

        reader.execute_batch("COMMIT;").unwrap();

        assert!(
            log.contains("wal checkpoint could not fully truncate"),
            "a blocked checkpoint must be logged, not silently swallowed; got: {log}"
        );
    }
}
