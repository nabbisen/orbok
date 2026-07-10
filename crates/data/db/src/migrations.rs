//! Migration runner (RFC-002 §6).
//!
//! Migrations are append-only, numbered, and named. The runner is
//! idempotent: applied versions are skipped. Each migration runs inside
//! a transaction; failure rolls back and aborts startup with a typed
//! error. Test databases run all migrations from the empty state.

use crate::catalog::{Catalog, db_err};
use orbok_core::{OrbokError, OrbokResult, now_iso8601};

/// A single schema migration.
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

/// The append-only migration list. New migrations are appended here and
/// never reordered or edited after release.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "baseline",
        sql: include_str!("../migrations/0001_baseline.sql"),
    },
    Migration {
        version: 2,
        name: "trigram_index",
        sql: include_str!("../migrations/0002_trigram_index.sql"),
    },
    Migration {
        version: 3,
        name: "scheduler",
        sql: include_str!("../migrations/0003_scheduler.sql"),
    },
    Migration {
        version: 4,
        name: "search_history",
        sql: include_str!("../migrations/0004_search_history.sql"),
    },
    Migration {
        version: 5,
        name: "keyword_rowid_indexes",
        sql: include_str!("../migrations/0005_keyword_rowid_indexes.sql"),
    },
];

/// Apply all pending migrations. Called from `Catalog::open` before any
/// application service touches the database.
pub fn run_pending(catalog: &Catalog) -> OrbokResult<()> {
    let mut conn = catalog.lock();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(db_err)?;

    for migration in MIGRATIONS {
        let applied: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [migration.version],
                |row| row.get(0),
            )
            .map_err(db_err)?;
        if applied {
            continue;
        }

        let tx = conn.transaction().map_err(db_err)?;
        tx.execute_batch(migration.sql)
            .map_err(|e| OrbokError::MigrationFailed {
                version: migration.version,
                message: e.to_string(),
            })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![migration.version, migration.name, now_iso8601()],
        )
        .map_err(|e| OrbokError::MigrationFailed {
            version: migration.version,
            message: e.to_string(),
        })?;
        tx.commit().map_err(|e| OrbokError::MigrationFailed {
            version: migration.version,
            message: e.to_string(),
        })?;

        tracing::info!(
            version = migration.version,
            name = migration.name,
            "applied migration"
        );
    }

    Ok(())
}

/// Latest known migration version (for startup verification).
pub fn latest_version() -> i64 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}
