//! Search history repository (RFC-042 §8).
//!
//! Stores recent search instructions locally. No snippets, embeddings, or
//! ranking data. Enforces deduplication and a configurable max-entry count.

use crate::catalog::{Catalog, db_err};
use orbok_core::{
    OrbokError, OrbokResult, SearchHistoryEntry, SearchHistoryId, SearchHistorySettings,
    StoredSearchFilter, now_iso8601,
};
use rusqlite::params;

pub struct SearchHistoryRepository<'a> {
    catalog: &'a Catalog,
}

impl<'a> SearchHistoryRepository<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    // ── Queries ───────────────────────────────────────────────────────

    /// All entries, newest first (RFC-042 §8.3).
    pub fn list(&self) -> OrbokResult<Vec<SearchHistoryEntry>> {
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, search_text, filters_json, created_at, last_used_at, \
                 result_count, locale \
                 FROM search_history ORDER BY last_used_at DESC",
            )
            .map_err(db_err)?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(db_err)?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, search_text, filters_json, created_at, last_used_at, result_count, locale) =
                row.map_err(db_err)?;
            let filters: Vec<StoredSearchFilter> =
                serde_json::from_str(&filters_json).map_err(|e| {
                    OrbokError::Database(format!("history filters_json deserialize: {e}"))
                })?;
            entries.push(SearchHistoryEntry {
                id: SearchHistoryId::new(id),
                search_text,
                filters,
                created_at,
                last_used_at,
                previous_result_count: result_count.map(|n| n as usize),
                locale,
            });
        }
        Ok(entries)
    }

    /// Fetch a single entry by id. Returns `None` when not found.
    pub fn get(&self, id: &SearchHistoryId) -> OrbokResult<Option<SearchHistoryEntry>> {
        let conn = self.catalog.lock();
        let result = conn.query_row(
            "SELECT id, search_text, filters_json, created_at, last_used_at, \
             result_count, locale FROM search_history WHERE id = ?1",
            params![id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        );
        match result {
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(db_err(e)),
            Ok((
                id_s,
                search_text,
                filters_json,
                created_at,
                last_used_at,
                result_count,
                locale,
            )) => {
                let filters: Vec<StoredSearchFilter> = serde_json::from_str(&filters_json)
                    .map_err(|e| {
                        OrbokError::Database(format!("history filters_json deserialize: {e}"))
                    })?;
                Ok(Some(SearchHistoryEntry {
                    id: SearchHistoryId::new(id_s),
                    search_text,
                    filters,
                    created_at,
                    last_used_at,
                    previous_result_count: result_count.map(|n| n as usize),
                    locale,
                }))
            }
        }
    }

    // ── Mutations ─────────────────────────────────────────────────────

    /// Create a new entry, or update `last_used_at` + `result_count` if an
    /// identical (search_text, filters) entry already exists (RFC-042 §8.4).
    /// Returns the id of the created or updated entry.
    pub fn upsert(
        &self,
        search_text: &str,
        filters: &[StoredSearchFilter],
        result_count: Option<usize>,
        locale: &str,
        settings: &SearchHistorySettings,
    ) -> OrbokResult<SearchHistoryId> {
        if search_text.trim().is_empty() {
            return Err(OrbokError::Database(
                "history: refusing to store empty search".to_string(),
            ));
        }

        let filters_json = serde_json::to_string(filters)
            .map_err(|e| OrbokError::Database(format!("history filters_json serialize: {e}")))?;
        let now = now_iso8601();

        // Check for duplicate (same text + same filter set).
        if let Some(existing) = self.find_duplicate(search_text, &filters_json)? {
            let conn = self.catalog.lock();
            conn.execute(
                "UPDATE search_history SET last_used_at = ?1, result_count = ?2 WHERE id = ?3",
                params![now, result_count.map(|n| n as i64), existing.as_str()],
            )
            .map_err(db_err)?;
            return Ok(existing);
        }

        // New entry.
        let id = SearchHistoryId::new(uuid_v4());
        {
            let conn = self.catalog.lock();
            conn.execute(
                "INSERT INTO search_history \
                 (id, search_text, filters_json, created_at, last_used_at, result_count, locale) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id.as_str(),
                    search_text,
                    filters_json,
                    now,
                    now,
                    result_count.map(|n| n as i64),
                    locale
                ],
            )
            .map_err(db_err)?;
        }

        // Enforce max_entries: delete oldest entries beyond the limit.
        self.evict_oldest(settings.max_entries)?;

        Ok(id)
    }

    /// Delete a single entry by id.
    pub fn remove(&self, id: &SearchHistoryId) -> OrbokResult<()> {
        let conn = self.catalog.lock();
        conn.execute(
            "DELETE FROM search_history WHERE id = ?1",
            params![id.as_str()],
        )
        .map_err(db_err)?;
        Ok(())
    }

    /// Delete all entries (RFC-042 §13.3).
    pub fn clear(&self) -> OrbokResult<()> {
        let conn = self.catalog.lock();
        conn.execute("DELETE FROM search_history", [])
            .map_err(db_err)?;
        Ok(())
    }

    /// Count of currently stored entries.
    pub fn count(&self) -> OrbokResult<usize> {
        let conn = self.catalog.lock();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM search_history", [], |row| row.get(0))
            .map_err(db_err)?;
        Ok(n as usize)
    }

    // ── Private helpers ───────────────────────────────────────────────

    fn find_duplicate(
        &self,
        search_text: &str,
        filters_json: &str,
    ) -> OrbokResult<Option<SearchHistoryId>> {
        let conn = self.catalog.lock();
        let result = conn.query_row(
            "SELECT id FROM search_history WHERE search_text = ?1 AND filters_json = ?2",
            params![search_text, filters_json],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(id) => Ok(Some(SearchHistoryId::new(id))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(db_err(e)),
        }
    }

    fn evict_oldest(&self, max_entries: usize) -> OrbokResult<()> {
        let conn = self.catalog.lock();
        conn.execute(
            "DELETE FROM search_history WHERE id IN \
             (SELECT id FROM search_history ORDER BY last_used_at DESC LIMIT -1 OFFSET ?1)",
            params![max_entries as i64],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

fn uuid_v4() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("h-{t:016x}-{seq:08x}")
}
