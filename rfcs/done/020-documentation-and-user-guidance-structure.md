# RFC-020: Documentation and User Guidance Structure

**Project:** orbok  
**RFC:** 020  
**Title:** Documentation and User Guidance Structure  
**Status:** Implemented (v0.6.0)
**Target Milestone:** M13  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the documentation structure for `orbok`.

The central decision is:

> Documentation must serve multiple audiences separately: ordinary users, privacy-conscious users, developers, and release maintainers. README must stay concise, while detailed guides live in structured docs.

---

## 2. Motivation

`orbok` has several concepts that can confuse users:

- local-only search;
- derived indexes;
- embeddings as sensitive data;
- model installation;
- source registration;
- stale results;
- cleanup/rebuild behavior;
- Japanese search limitations;
- keyword vs semantic search;
- temporary sources.

Good documentation reduces support burden and prevents privacy misunderstandings.

---

## 3. Goals

- Define documentation structure.
- Keep README focused.
- Explain local-first behavior clearly.
- Explain what is stored and what is not.
- Explain model setup.
- Explain storage cleanup.
- Explain search modes.
- Explain limitations honestly.
- Provide developer architecture docs.
- Provide release and troubleshooting docs.

---

## 4. Non-Goals

- This RFC does not write final website copy.
- This RFC does not define marketing pages.
- This RFC does not define API reference generation in detail.
- This RFC does not require translations in v1.

---

## 5. Audience Types

| Audience | Needs |
|---|---|
| Ordinary user | install, add sources, search, cleanup |
| Privacy-conscious user | what is stored, what is uploaded, how to delete data |
| Power user | search modes, models, storage modes |
| Developer | architecture, RFCs, crates, testing |
| Release maintainer | packaging, migration, release checklist |
| Troubleshooter | diagnostics, repair, logs |

---

## 6. Recommended Docs Structure

```text
docs/
├── README.md
├── user-guide/
│   ├── getting-started.md
│   ├── adding-sources.md
│   ├── searching.md
│   ├── search-modes.md
│   ├── models.md
│   ├── storage-and-cleanup.md
│   ├── privacy-and-local-data.md
│   ├── japanese-and-mixed-language-search.md
│   └── troubleshooting.md
├── developer-guide/
│   ├── architecture.md
│   ├── data-lifecycle.md
│   ├── database.md
│   ├── indexing-pipeline.md
│   ├── retrieval-pipeline.md
│   ├── localcache-integration.md
│   ├── security-model.md
│   └── testing.md
├── release/
│   ├── packaging.md
│   ├── release-checklist.md
│   ├── migration-policy.md
│   └── benchmark-report-template.md
└── rfcs/
```

---

## 7. README Scope

README should include only:

- what orbok is;
- local-first promise;
- core features;
- quick install/run;
- basic usage;
- current status;
- links to docs;
- license.

README should not contain:

- full schema design;
- all RFC details;
- exhaustive troubleshooting;
- marketing overclaims;
- long model comparison tables.

---

## 8. Required User Docs

## 8.1. Getting Started

Must explain:

- install app;
- launch app;
- add first source;
- index files;
- search;
- open source result.

## 8.2. Adding Sources

Must explain:

- persistent vs temporary sources;
- hidden-file policy;
- symlink policy;
- sensitive directory warnings;
- source removal does not delete files.

## 8.3. Searching

Must explain:

- exact search;
- semantic search;
- hybrid search;
- result badges;
- stale/missing source states;
- result preview.

## 8.4. Models

Must explain:

- why models are needed;
- keyword search works without models;
- installing/downloading model files;
- locating existing models;
- model changes and reindexing.

## 8.5. Storage and Cleanup

Must explain:

- what orbok stores;
- persistent catalog;
- exact index;
- semantic index;
- cache;
- model files;
- safe cleanup;
- rebuildable index deletion;
- reset catalog.

## 8.6. Privacy and Local Data

Must clearly state:

- documents are not uploaded by default;
- source files are not copied by default;
- derived indexes are stored locally;
- embeddings are sensitive derived data;
- extracted text cache may exist depending on settings;
- search history settings;
- logs and diagnostics behavior.

---

## 9. Required Developer Docs

Developer docs must explain:

- crate/module layout;
- backend/frontend boundary;
- database migration policy;
- source access boundary;
- scanner pipeline;
- extraction pipeline;
- chunking;
- keyword search;
- embeddings;
- hybrid search;
- reranking;
- localcache integration;
- storage lifecycle;
- security model;
- testing and benchmarks.

---

## 10. Documentation Language

Initial documentation language:

```text
English
```

Future translations may be added later.

Because Japanese search is a product concern, a Japanese-language quickstart may be considered later, but this is not required for v1.

---

## 11. Copywriting Rules

Use plain language.

Preferred:

```text
Exact search
Semantic search
Deep result refinement
Semantic search index
Source files will not be deleted
```

Avoid in user docs unless advanced section:

```text
BM25
RRF
Cross-Encoder
embedding dimension
SQLite FTS5
```

When technical terms are necessary, explain them briefly.

---

## 12. Privacy Claims

Avoid absolute claims:

Bad:

```text
orbok completely protects your privacy.
```

Preferred:

```text
orbok processes document search locally by default and does not upload document contents unless a future optional feature explicitly says so.
```

Documentation must be honest about:

- local indexes;
- embeddings;
- snippets;
- logs;
- model downloads;
- diagnostics export.

---

## 13. Troubleshooting Structure

Troubleshooting should include:

| Problem | Guide |
|---|---|
| semantic search unavailable | check Models |
| results stale | rescan/reindex |
| source missing | reconnect drive or locate source |
| storage too large | use Storage cleanup |
| model invalid | validate or locate model |
| PDF not searchable | extraction limitations |
| app slow | use Fast mode or reduce indexing |
| database issue | repair tools |
| privacy concern | clear cache/search history |

---

## 14. Release Documentation

Release docs should include:

- release checklist;
- migration notes;
- known limitations;
- benchmark summary;
- supported platforms;
- model compatibility;
- upgrade notes.

Each release should have:

```text
CHANGELOG.md
release notes
checksums
known issues
```

---

## 15. RFC Documentation Policy

RFCs should remain in repository.

Each RFC should include:

- status;
- target milestone;
- summary;
- motivation;
- goals/non-goals;
- design;
- acceptance criteria;
- tests;
- unresolved questions;
- decision.

When implemented, RFC status should be updated:

```text
Draft
Accepted
Implemented
Superseded
Rejected
```

---

## 16. Acceptance Criteria

- README scope is defined.
- User guide structure is defined.
- Developer guide structure is defined.
- Privacy documentation requirements are defined.
- Storage cleanup documentation requirements are defined.
- Model setup documentation requirements are defined.
- Troubleshooting structure is defined.
- Release docs structure is defined.
- RFC status lifecycle is defined.
- Documentation avoids privacy overclaims.

---

## 17. Testing Requirements

Documentation checks:

1. README links to user guide.
2. Privacy guide exists.
3. Storage cleanup guide exists.
4. Model setup guide exists.
5. Troubleshooting guide exists.
6. Developer architecture guide exists.
7. Release checklist exists.
8. Broken links check passes.
9. Privacy claims do not use prohibited absolute wording.
10. RFC index is up to date.

---

## 18. Unresolved Questions

- Should docs be built with mdBook?
- Should screenshots be included in v1 docs?
- Should Japanese quickstart be added?
- Should docs be packaged offline with the app?
- Should release notes include benchmark summaries?

---

## 19. Decision

Adopt structured documentation with a concise README and separate user, developer, release, and RFC documentation.

Documentation is part of release readiness, not an optional afterthought.
