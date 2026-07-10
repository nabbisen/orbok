# RFC-007: Keyword Search Engine Selection

**Project:** orbok  
**RFC:** 007  
**Title:** Keyword Search Engine Selection  
**Status:** Implemented (v0.2.0)
**Target Milestone:** M6  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the keyword search strategy for `orbok`.

The recommended initial decision is:

> Use SQLite FTS5 as the first keyword search backend for development efficiency and catalog simplicity, behind a `KeywordSearchEngine` abstraction that allows Tantivy or a specialized engine to replace or supplement it later.

Keyword search must work without AI models. It is the baseline search capability and the fallback when embedding or reranking is unavailable.

---

## 2. Motivation

Dense vector search is valuable for conceptual queries, but it can miss exact identifiers, error messages, code symbols, product numbers, file paths, and names.

`orbok` must provide robust keyword search for:

- exact terms;
- identifiers;
- model numbers;
- filenames;
- code symbols;
- logs;
- Japanese and mixed-language text.

A keyword search engine is also important because `orbok` must remain useful before local AI models are installed.

---

## 3. Goals

- Provide local keyword search.
- Support exact and partial term retrieval.
- Avoid permanent full extracted-text storage by default.
- Support rebuildable keyword index lifecycle.
- Enable RRF fusion with vector search.
- Preserve backend flexibility.
- Provide a realistic path for Japanese/mixed-language improvement.

---

## 4. Non-Goals

- This RFC does not define embedding search.
- This RFC does not define RRF fusion.
- This RFC does not finalize Japanese tokenization.
- This RFC does not require server-grade search scaling in v1.
- This RFC does not require Boolean query language in v1.

---

## 5. Candidate Engines

## 5.1. SQLite FTS5

Pros:

- simple deployment;
- no additional service;
- integrates with SQLite catalog;
- good development speed;
- supports BM25-like ranking;
- works well for many text search cases;
- can use contentless or external-content tables.

Cons:

- Japanese tokenization is limited without extensions;
- advanced analyzers are less flexible;
- large-scale indexing may be slower than dedicated engines;
- external tokenizer support may complicate distribution.

## 5.2. Tantivy

Pros:

- Rust-native search engine;
- more search-engine-like architecture;
- analyzers/tokenizers can be customized;
- scalable index segments;
- strong query features.

Cons:

- additional index storage separate from SQLite;
- more implementation complexity;
- lifecycle coordination required;
- Japanese tokenizer integration still needs design.

## 5.3. Custom Lightweight Inverted Index

Pros:

- full control;
- narrow fit for orbok;
- storage can be optimized.

Cons:

- high implementation burden;
- ranking quality risk;
- not worth early complexity.

---

## 6. Decision

Adopt SQLite FTS5 for the initial keyword search MVP, behind an abstraction.

Reason:

- faster implementation;
- aligns with local SQLite catalog;
- enough for MVP exact search;
- allows later replacement if Japanese/mixed-language quality requires it.

The engine abstraction must avoid locking the app to FTS5 forever.

---

## 7. KeywordSearchEngine Abstraction

Conceptual interface:

```rust
pub trait KeywordSearchEngine {
    fn index_chunk(&self, input: KeywordIndexInput) -> Result<()>;
    fn delete_chunk(&self, chunk_id: &ChunkId) -> Result<()>;
    fn search(&self, query: KeywordQuery) -> Result<Vec<KeywordCandidate>>;
    fn rebuild(&self, scope: RebuildScope) -> Result<()>;
}
```

## 7.1. KeywordIndexInput

```rust
pub struct KeywordIndexInput {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub text: String,
    pub title: Option<String>,
    pub heading_path: Option<String>,
    pub language_hint: Option<String>,
}
```

## 7.2. KeywordCandidate

```rust
pub struct KeywordCandidate {
    pub chunk_id: ChunkId,
    pub rank: u32,
    pub score: f64,
    pub matched_terms: Vec<String>,
}
```

---

## 8. SQLite FTS5 Schema

## 8.1. Contentless Table Option

Recommended initial approach:

```sql
CREATE VIRTUAL TABLE chunk_fts USING fts5(
    chunk_id UNINDEXED,
    file_id UNINDEXED,
    title,
    heading_path,
    normalized_text,
    tokenize = 'unicode61',
    content = ''
);
```

This keeps a searchable index but does not expose FTS as the authoritative text store.

Important caveat:

- Depending on SQLite FTS5 configuration, contentless FTS limits snippet generation.
- `orbok` should generate display snippets by reading source files through chunk locations.

## 8.2. Keyword Index Records

```sql
CREATE TABLE keyword_index_records (
    chunk_id TEXT PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE,
    index_engine TEXT NOT NULL,
    tokenizer_name TEXT NOT NULL,
    tokenizer_version TEXT NOT NULL,
    language_hint TEXT,
    indexed_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('active', 'stale', 'deleted', 'failed')
    )
);

CREATE INDEX idx_keyword_index_status ON keyword_index_records(status);
```

---

## 9. Indexing Text Construction

The indexed text should include useful searchable context:

```text
title
heading path
chunk text
file name
selected path tokens
```

But be careful:

- do not overboost file paths by default;
- do not store complete source file text as a separate catalog field;
- keep text construction deterministic and versioned.

Recommended version:

```text
keyword_text_builder_version = "kw-text-v1"
```

---

## 10. Query Normalization

Initial query normalization should include:

- Unicode normalization;
- case folding for Latin text;
- full-width/half-width normalization where safe;
- trimming;
- preserving symbols for exact/code searches.

Do not aggressively remove punctuation in exact mode.

---

## 11. Search Modes

Keyword engine should support different query intents.

| Mode | Behavior |
|---|---|
| Auto | Normal keyword query |
| Exact | Preserve quoted terms and symbols as much as possible |
| Prefix | Optional future mode |
| Filename | Search path/title fields |
| Code/Log | Preserve punctuation-heavy terms |

For v1, support at least Auto and Exact behavior.

---

## 12. Japanese and Mixed-Language Strategy

Japanese support requires a dedicated RFC, but RFC-007 must leave room for it.

Initial options:

1. FTS5 unicode tokenization plus supplemental n-gram table;
2. FTS5 trigram tokenizer if distribution supports it;
3. Tantivy backend with Japanese tokenizer;
4. custom n-gram tokenizer for Japanese fields.

Recommended v1 compromise:

- implement FTS5 baseline;
- add test corpus for Japanese queries;
- create RFC-014 before claiming high-quality Japanese keyword retrieval.

Do not overstate Japanese search quality until validated.

---

## 13. Snippet Strategy

Keyword search engine should not be the authoritative snippet provider.

Snippet loading should use:

```text
chunk_id -> chunk_locations -> source file -> dynamic snippet
```

FTS may provide matched terms, but source display comes from source loader.

This preserves the no-full-source-copy principle.

---

## 14. Ranking

SQLite FTS5 BM25 may provide initial keyword score.

The keyword engine returns:

```text
chunk_id
rank
score
matched terms if available
```

Hybrid fusion uses rank, not raw score, as the primary input.

---

## 15. Lifecycle

Keyword index entries are rebuildable.

They become stale when:

- chunk becomes stale;
- tokenizer version changes;
- keyword text builder version changes;
- keyword engine version changes;
- normalization version changes.

Cleanup may remove stale keyword entries after replacement or explicit confirmation.

---

## 16. localcache Relationship

`localcache` is not the keyword search engine.

However, localcache may accelerate keyword indexing by caching:

- extracted segment bundles;
- chunk bundles;
- normalized text bundles if enabled.

The authoritative keyword index remains FTS5/Tantivy/etc.

Never perform keyword retrieval only by scanning localcache payloads.

---

## 17. API Impact

Keyword search result object:

```json
{
  "chunk_id": "chunk_...",
  "rank": 4,
  "score": 8.72,
  "matched_terms": ["token", "expiry"],
  "engine": "sqlite_fts5"
}
```

Hybrid search will later convert these into candidate records for RRF.

---

## 18. UI Impact

Default UI labels:

| Technical Term | User-Facing Label |
|---|---|
| FTS/BM25 | Exact search |
| keyword index | Exact search index |
| tokenizer | Text matching mode |
| stale keyword index | Exact search index needs rebuild |

Search result badge:

```text
[Exact match]
```

not:

```text
[FTS5]
```

---

## 19. Acceptance Criteria

- Keyword search works without embedding model.
- SQLite FTS5 backend is hidden behind `KeywordSearchEngine`.
- Chunks can be indexed and deleted.
- Stale keyword records are represented.
- Query results include rank and score.
- Snippets are loaded from source, not from permanent full-text storage.
- Exact identifiers and filenames are searchable.
- Tests cover mixed alphanumeric strings.
- Japanese search limitations are documented until RFC-014.

---

## 20. Testing Requirements

Required tests:

1. Index simple text chunk.
2. Search exact term.
3. Search filename/title.
4. Search model number like `ABC-1234`.
5. Search code-like token like `refresh_token`.
6. Delete chunk removes keyword hit.
7. Stale chunk is excluded or marked.
8. Query normalization does not destroy exact mode.
9. Source snippet loads from file location.
10. FTS index rebuild restores results.

Japanese tests should be included as expected-quality probes even before final tokenizer selection.

---

## 21. Unresolved Questions

- Should FTS5 use contentless or external-content mode?
- Is FTS5 trigram tokenizer available across target platforms?
- Should Tantivy be adopted earlier for Japanese support?
- Should phrase search be supported in v1?
- Should file path tokens be indexed separately?
- How should source-code symbols be tokenized?

---

## 22. Decision

Use SQLite FTS5 for the first keyword search backend, behind a backend abstraction.

Plan a later Japanese/mixed-language RFC before claiming strong Japanese keyword search quality.
