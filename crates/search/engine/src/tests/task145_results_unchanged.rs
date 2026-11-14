//! Task 145 §2: the `CROSS JOIN` change in `fts5::keyword_search_sql` must
//! not change what a search returns -- same rows, same `bm25` ordering,
//! same `LIMIT` semantics, status and scope filters still applied in SQL
//! (RFC-060 §7). `CROSS JOIN ... ON ...` is semantically identical to a
//! plain `JOIN` in SQLite (it only forbids the optimizer from reordering
//! past it); this test turns that claim into an executable one by running
//! the literal pre-Task-145 query (today's query with `CROSS JOIN` put back
//! to `JOIN`) and the new query side by side, over a seeded catalog with
//! multiple sources, multiple chunks, a paused (non-searchable) source, and
//! a folder-scoped query, asserting byte-identical result rows in the same
//! order for every case.

use crate::KeywordSearchEngine;
use crate::fts5::keyword_search_sql;
use orbok_core::{ChunkId, FolderScope, SearchScope};
use orbok_db::Catalog;
use orbok_db::repo::search_scope::scope_sql;
use rusqlite::params;

/// Two sources (`s1` active, `s2` paused -- non-searchable), three chunks:
/// two in `s1` sharing the word "alpha" (so ordering and multi-row LIMIT
/// both have something to assert), one in `s2` that also contains "alpha"
/// but must never surface while `s2` is paused.
fn seed(catalog: &Catalog) {
    let conn = catalog.lock();
    let t = "2026-01-01T00:00:00Z";
    for (source_id, status) in [("s1", "active"), ("s2", "paused")] {
        conn.execute(
            "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
             canonical_path, status, index_mode, hidden_file_policy, symlink_policy, \
             created_at, updated_at) VALUES \
             (?1,'directory','persistent','/'||?1,'/'||?1,?2,'balanced','exclude','ignore',?3,?3)",
            params![source_id, status, t],
        )
        .unwrap();
    }
    for (file_id, source_id, path) in [
        ("f1", "s1", "/s1/one.md"),
        ("f2", "s1", "/s1/sub/two.md"),
        ("f3", "s2", "/s2/three.md"),
    ] {
        conn.execute(
            "INSERT INTO files (file_id, source_id, original_path, canonical_path, \
             display_path, file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
             VALUES (?1,?2,?3,?3,?3,1,'indexed',?4,?4,?4)",
            params![file_id, source_id, path, t],
        )
        .unwrap();
        let extraction_id = format!("{file_id}-e");
        conn.execute(
            "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
             extractor_version, normalization_version, status, created_at, updated_at) \
             VALUES (?1,?2,'text','v1','norm-v1','succeeded',?3,?3)",
            params![extraction_id, file_id, t],
        )
        .unwrap();
    }
    drop(conn);

    for (chunk_id, file_id, text) in [
        ("c1", "f1", "alpha beta common"),
        ("c2", "f2", "alpha alpha gamma common"),
        ("c3", "f3", "alpha delta common"),
    ] {
        let conn = catalog.lock();
        conn.execute(
            "INSERT INTO chunks (chunk_id, file_id, extraction_id, chunk_kind, chunk_ordinal, \
             chunk_status, created_at, updated_at) \
             VALUES (?1,?2,?2||'-e','paragraph',0,'active',?3,?3)",
            params![chunk_id, file_id, t],
        )
        .unwrap();
        drop(conn);
        crate::Fts5KeywordEngine::new(catalog)
            .index(&[crate::KeywordDocument {
                chunk_id: ChunkId::from_string(chunk_id.to_string()),
                title: Some(file_id.to_string()),
                heading_path: None,
                normalized_text: text.to_string(),
            }])
            .unwrap();
    }
}

/// Runs `sql` with `(match_expr, limit)` plus any scope binds, collecting
/// `(chunk_id, file_id, score)` in the exact row order returned.
fn run(
    catalog: &Catalog,
    sql: &str,
    match_expr: &str,
    limit: u32,
    scope_binds: &[rusqlite::types::Value],
) -> Vec<(String, String, f64)> {
    let conn = catalog.lock();
    let mut stmt = conn.prepare(sql).unwrap();
    let mut binds: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Text(match_expr.to_string()),
        rusqlite::types::Value::Integer(limit as i64),
    ];
    binds.extend(scope_binds.iter().cloned());
    stmt.query_map(rusqlite::params_from_iter(binds), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn assert_old_and_new_agree(catalog: &Catalog, match_expr: &str, limit: u32, scope: &SearchScope) {
    let scope_sql = scope_sql(scope, "f", "s", 3);
    let new_sql = keyword_search_sql(&scope_sql.predicate);
    let old_sql = new_sql.replace("CROSS JOIN", "JOIN");
    let old = run(catalog, &old_sql, match_expr, limit, &scope_sql.binds);
    let new = run(catalog, &new_sql, match_expr, limit, &scope_sql.binds);
    assert_eq!(
        old, new,
        "old (plain JOIN) and new (CROSS JOIN) queries must return the \
         same rows in the same order for match {match_expr:?}, scope {scope:?}"
    );
}

#[test]
fn unscoped_multi_match_query_is_unchanged() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    assert_old_and_new_agree(&catalog, "alpha", 10, &SearchScope::default());

    // Not vacuous: exactly c2 then c1 (both in the active source s1, c2
    // ranked first for its two "alpha" occurrences), never c3 (s2, paused).
    let scope_sql = scope_sql(&SearchScope::default(), "f", "s", 3);
    let rows = run(
        &catalog,
        &keyword_search_sql(&scope_sql.predicate),
        "alpha",
        10,
        &scope_sql.binds,
    );
    let ids: Vec<&str> = rows
        .iter()
        .map(|(chunk_id, _, _)| chunk_id.as_str())
        .collect();
    assert_eq!(ids, vec!["c2", "c1"], "unexpected rows/order: {rows:?}");
}

#[test]
fn a_low_limit_is_unchanged() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    // Exactly one row below the number of matches in s1 alone, so this
    // exercises LIMIT actually truncating a real result set in both forms.
    assert_old_and_new_agree(&catalog, "alpha", 1, &SearchScope::default());
}

#[test]
fn a_folder_scoped_query_is_unchanged() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    let scope = SearchScope {
        extensions: Vec::new(),
        folder: Some(FolderScope {
            source_id: "s1".to_string(),
            limit_path: None,
            include_subfolders: false,
        }),
    };
    assert_old_and_new_agree(&catalog, "alpha", 10, &scope);
}

#[test]
fn a_non_searchable_source_s_chunk_is_excluded_in_both_forms() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    // "delta" only appears in c3, which lives in the paused source s2 --
    // both queries must return nothing, not just the same (wrong) thing.
    let scope_sql = scope_sql(&SearchScope::default(), "f", "s", 3);
    let new_sql = keyword_search_sql(&scope_sql.predicate);
    let new = run(&catalog, &new_sql, "delta", 10, &scope_sql.binds);
    assert!(
        new.is_empty(),
        "a chunk in a paused source must never be returned: {new:?}"
    );
    assert_old_and_new_agree(&catalog, "delta", 10, &SearchScope::default());
}
