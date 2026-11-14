//! Task 145 §2: the `CROSS JOIN` change in `fts5::keyword_search_sql` and
//! `multilingual::trigram_search_sql` must not change what a search
//! returns -- same rows, same `bm25` ordering, same `LIMIT` semantics,
//! status and scope filters still applied in SQL (RFC-060 §7). `CROSS
//! JOIN ... ON ...` is semantically identical to a plain `JOIN` in SQLite
//! (it only forbids the optimizer from reordering past it); this test
//! turns that claim into an executable one by running the literal
//! pre-Task-145 query (today's query with `CROSS JOIN` put back to
//! `JOIN`) and the new query side by side, over a seeded catalog with
//! multiple sources, multiple chunks, a paused (non-searchable) source, and
//! a folder-scoped query, asserting byte-identical result rows in the same
//! order for every case -- on both the unicode61 (`chunk_fts`) and trigram
//! (`chunk_fts_trigram`) paths (Review 322 §4.2).

use crate::KeywordSearchEngine;
use crate::fts5::keyword_search_sql;
use crate::multilingual::trigram_search_sql;
use orbok_core::{ChunkId, FolderScope, SearchScope};
use orbok_db::Catalog;
use orbok_db::repo::search_scope::scope_sql;
use rusqlite::params;

/// Two sources (`s1` active, `s2` paused -- non-searchable), three chunks
/// that drive the base cases (two in `s1` sharing the word "alpha", so
/// ordering and multi-row LIMIT both have something to assert; one in `s2`
/// that also contains "alpha" but must never surface while `s2` is
/// paused), plus two chunks for Review 322 §4.2's gap: `c4` is itself
/// `chunk_status = 'stale'` (kills M6: dropping `multilingual.rs`'s
/// `c.chunk_status = 'active'`), and `c5`'s `keyword_index_records` row is
/// `status = 'stale'` (today's code never writes that status -- it deletes
/// the record instead -- but the schema and the filter both allow it, so
/// it's pinned here; kills M3 and M5). Every chunk is indexed into both
/// `chunk_fts` and `chunk_fts_trigram`, so the same fixture drives both
/// paths' tests.
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
        ("f4", "s1", "/s1/four.md"),
        ("f5", "s1", "/s1/five.md"),
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

    for (chunk_id, file_id, text, chunk_status) in [
        ("c1", "f1", "alpha beta common", "active"),
        ("c2", "f2", "alpha alpha gamma common", "active"),
        ("c3", "f3", "alpha delta common", "active"),
        ("c4", "f4", "echo foxglove common", "stale"),
        ("c5", "f5", "foxtrot golf common", "active"),
    ] {
        let conn = catalog.lock();
        conn.execute(
            "INSERT INTO chunks (chunk_id, file_id, extraction_id, chunk_kind, chunk_ordinal, \
             chunk_status, created_at, updated_at) \
             VALUES (?1,?2,?2||'-e','paragraph',0,?3,?4,?4)",
            params![chunk_id, file_id, chunk_status, t],
        )
        .unwrap();
        drop(conn);
        index_both(catalog, chunk_id, file_id, text);
    }

    // `c5`'s own chunk is active, but its `keyword_index_records` row is
    // 'stale': today's code never writes that (it deletes the record
    // instead of marking it), but the schema allows it and both queries
    // filter on it, so it must still be excluded (Review 322 §4.2).
    let conn = catalog.lock();
    conn.execute(
        "UPDATE keyword_index_records SET status = 'stale' WHERE chunk_id = 'c5'",
        [],
    )
    .unwrap();
}

/// Writes both FTS halves of a chunk directly, the same way
/// `ChunkRepository::write_keyword_rows` does for a real chunk (RFC-014
/// §12) -- `Fts5KeywordEngine::index` only writes the unicode61 half, and
/// this fixture needs both so the trigram-path tests have something to
/// query too.
fn index_both(catalog: &Catalog, chunk_id: &str, file_id: &str, text: &str) {
    crate::Fts5KeywordEngine::new(catalog)
        .index(&[crate::KeywordDocument {
            chunk_id: ChunkId::from_string(chunk_id.to_string()),
            title: Some(file_id.to_string()),
            heading_path: None,
            normalized_text: text.to_string(),
        }])
        .unwrap();
    let conn = catalog.lock();
    conn.execute(
        "INSERT INTO chunk_fts_trigram (title, heading_path, normalized_text) \
         VALUES (?1, NULL, ?2)",
        params![file_id, text],
    )
    .unwrap();
    let trigram_fts_rowid = conn.last_insert_rowid();
    conn.execute(
        "UPDATE keyword_index_records SET trigram_fts_rowid = ?1 WHERE chunk_id = ?2",
        params![trigram_fts_rowid, chunk_id],
    )
    .unwrap();
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

/// The trigram counterpart of `assert_old_and_new_agree`, over
/// `multilingual::trigram_search_sql` instead of `fts5::keyword_search_sql`
/// (Review 322 §4.2: the results-equivalence tests above cover only the
/// unicode61 path, but Task 145 changed the trigram query too).
fn assert_old_and_new_agree_trigram(
    catalog: &Catalog,
    match_expr: &str,
    limit: u32,
    scope: &SearchScope,
) {
    let scope_sql = scope_sql(scope, "f", "s", 3);
    let new_sql = trigram_search_sql(&scope_sql.predicate);
    let old_sql = new_sql.replace("CROSS JOIN", "JOIN");
    let old = run(catalog, &old_sql, match_expr, limit, &scope_sql.binds);
    let new = run(catalog, &new_sql, match_expr, limit, &scope_sql.binds);
    assert_eq!(
        old, new,
        "old (plain JOIN) and new (CROSS JOIN) trigram queries must return \
         the same rows in the same order for match {match_expr:?}, scope {scope:?}"
    );
}

#[test]
fn unscoped_multi_match_query_is_unchanged_on_the_trigram_path() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    assert_old_and_new_agree_trigram(&catalog, "alpha", 10, &SearchScope::default());

    // Same non-vacuous order as the unicode61 case: c2 then c1, never c3
    // (s2, paused).
    let scope_sql = scope_sql(&SearchScope::default(), "f", "s", 3);
    let rows = run(
        &catalog,
        &trigram_search_sql(&scope_sql.predicate),
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
fn a_low_limit_is_unchanged_on_the_trigram_path() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    assert_old_and_new_agree_trigram(&catalog, "alpha", 1, &SearchScope::default());
}

#[test]
fn a_folder_scoped_query_is_unchanged_on_the_trigram_path() {
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
    assert_old_and_new_agree_trigram(&catalog, "alpha", 10, &scope);
}

#[test]
fn a_non_searchable_source_s_chunk_is_excluded_in_both_forms_on_the_trigram_path() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    // "delta" only appears in c3, which lives in the paused source s2.
    let scope_sql = scope_sql(&SearchScope::default(), "f", "s", 3);
    let new_sql = trigram_search_sql(&scope_sql.predicate);
    let new = run(&catalog, &new_sql, "delta", 10, &scope_sql.binds);
    assert!(
        new.is_empty(),
        "a chunk in a paused source must never be returned: {new:?}"
    );
    assert_old_and_new_agree_trigram(&catalog, "delta", 10, &SearchScope::default());
}

/// Review 322 §4.2, M6: dropping `multilingual.rs`'s
/// `c.chunk_status = 'active'` must be caught. `c4` is `chunk_status =
/// 'stale'` and contains "echo" on both paths; old and new queries must
/// agree it's excluded on the unicode61 path (already covered for
/// production by `rfc007_keyword::stale_chunks_are_excluded`, pinned here
/// again only for the equivalence) and, the actual gap, on the trigram
/// path too.
#[test]
fn a_stale_chunk_is_excluded_on_both_paths() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    assert_old_and_new_agree(&catalog, "echo", 10, &SearchScope::default());
    assert_old_and_new_agree_trigram(&catalog, "echo", 10, &SearchScope::default());

    let scope_sql = scope_sql(&SearchScope::default(), "f", "s", 3);
    assert!(
        run(
            &catalog,
            &keyword_search_sql(&scope_sql.predicate),
            "echo",
            10,
            &scope_sql.binds,
        )
        .is_empty(),
        "a stale chunk must never be returned on the unicode61 path"
    );
    assert!(
        run(
            &catalog,
            &trigram_search_sql(&scope_sql.predicate),
            "echo",
            10,
            &scope_sql.binds,
        )
        .is_empty(),
        "a stale chunk must never be returned on the trigram path"
    );
}

/// Review 322 §4.2, M3 and M5: dropping either query's
/// `r.status = 'active'` must be caught. `c5`'s chunk is active, but its
/// `keyword_index_records` row is `status = 'stale'` -- today's code never
/// writes that (it deletes the record instead of marking it stale), but
/// the schema allows it and both queries filter on it, so it's pinned
/// here even though nothing currently produces it.
#[test]
fn a_stale_keyword_index_record_is_excluded_on_both_paths() {
    let catalog = Catalog::open_in_memory().unwrap();
    seed(&catalog);
    assert_old_and_new_agree(&catalog, "foxtrot", 10, &SearchScope::default());
    assert_old_and_new_agree_trigram(&catalog, "foxtrot", 10, &SearchScope::default());

    let scope_sql = scope_sql(&SearchScope::default(), "f", "s", 3);
    assert!(
        run(
            &catalog,
            &keyword_search_sql(&scope_sql.predicate),
            "foxtrot",
            10,
            &scope_sql.binds,
        )
        .is_empty(),
        "a stale keyword_index_records row must never be returned on the unicode61 path"
    );
    assert!(
        run(
            &catalog,
            &trigram_search_sql(&scope_sql.predicate),
            "foxtrot",
            10,
            &scope_sql.binds,
        )
        .is_empty(),
        "a stale keyword_index_records row must never be returned on the trigram path"
    );
}
