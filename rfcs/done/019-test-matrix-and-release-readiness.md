# RFC-019: Test Matrix and Release Readiness

**Project:** orbok  
**RFC:** 019  
**Title:** Test Matrix and Release Readiness  
**Status:** Implemented (v0.6.0)
**Target Milestone:** M13  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the test matrix and release readiness gates for `orbok`.

The central decision is:

> `orbok` must not be considered release-ready merely because it builds. It must pass lifecycle, security, retrieval, storage, recovery, and packaging gates.

---

## 2. Motivation

`orbok` combines local file access, database lifecycle, document parsing, search indexes, local AI models, cache management, and desktop packaging.

Risks are cross-cutting:

- source boundary failures can leak files;
- cleanup bugs can delete catalog state;
- stale data can produce misleading results;
- model changes can invalidate indexes;
- packaging can break local data paths;
- parser failures can interrupt indexing.

A structured test matrix is necessary.

---

## 3. Goals

- Define test categories.
- Define CI test levels.
- Define release gates.
- Define platform matrix.
- Define smoke tests.
- Define security tests.
- Define retrieval tests.
- Define recovery tests.
- Define manual QA checklist.

---

## 4. Non-Goals

- This RFC does not implement every test.
- This RFC does not require exhaustive formal verification.
- This RFC does not require testing every GPU configuration.
- This RFC does not define commercial certification.

---

## 5. Test Categories

Required categories:

1. Unit tests.
2. Repository/database tests.
3. File-system policy tests.
4. Extraction tests.
5. Chunking tests.
6. Keyword search tests.
7. Embedding/vector tests.
8. Hybrid search tests.
9. Reranking tests.
10. Storage cleanup tests.
11. Security tests.
12. Recovery tests.
13. UI behavior tests.
14. Packaging smoke tests.
15. Benchmark regression tests.

---

## 6. CI Levels

## 6.1. Fast CI

Runs on every commit/PR.

Includes:

- formatting;
- clippy;
- unit tests;
- database migration tests;
- source boundary tests;
- keyword search smoke;
- no-default-document-upload tests;
- minimal extraction fixtures.

## 6.2. Extended CI

Runs on scheduled builds or release branches.

Includes:

- full fixture extraction;
- retrieval benchmark synthetic corpus;
- storage cleanup tests;
- recovery tests;
- packaging smoke;
- cross-platform matrix.

## 6.3. Release CI

Runs before release.

Includes:

- all extended tests;
- benchmark report;
- packaged artifact startup;
- migration from previous release;
- diagnostics export;
- release checklist.

---

## 7. Platform Matrix

Minimum:

| Platform | Fast CI | Release Smoke |
|---|---:|---:|
| Linux x86_64 | Yes | Yes |
| Windows x86_64 | Yes | Yes |
| macOS aarch64 | Preferred | Yes |
| macOS x86_64 | Optional | Optional |

CPU-only tests are mandatory.

GPU tests are optional until GPU package is officially supported.

---

## 8. Security Test Gates

Mandatory before release:

- path traversal rejected;
- symlink escape blocked by default;
- hidden files excluded by default;
- sensitive directory warning exists;
- local API binds to loopback if used;
- state-changing API protected;
- snippets escaped/sanitized;
- logs omit document body;
- cleanup cannot delete source files.

---

## 9. Database and Lifecycle Gates

Mandatory before release:

- migrations from empty DB pass;
- migrations from previous release pass;
- foreign keys enabled;
- safe cleanup preserves persistent sources;
- reset catalog requires explicit confirmation path;
- stale file state represented;
- model change invalidates embeddings;
- reranker change invalidates rerank cache only;
- localcache DB missing/corrupt recoverable.

---

## 10. Retrieval Gates

Mandatory before release:

- keyword-only search works;
- exact identifier search works;
- vector search works when model installed;
- search degrades when model missing;
- RRF deduplicates candidates;
- deleted chunks excluded;
- stale results marked;
- Japanese baseline test exists;
- benchmark report generated.

---

## 11. UI Gates

Mandatory before release:

- user can add source;
- user can search;
- user can view result preview;
- stale/missing state visible;
- storage dashboard shows categories;
- cleanup confirmation works;
- model missing state visible;
- keyboard navigation basics work.

---

## 12. Packaging Gates

Mandatory before release:

- packaged app starts;
- app data directory resolves;
- catalog DB created;
- cache DB created/recreated;
- packaged frontend assets load;
- no dev server dependency;
- checksums generated;
- license files included.

---

## 13. Manual QA Checklist

Manual checks:

```text
Fresh install
Add folder
Index files
Search exact term
Search conceptual query
Open source file
Modify source file
Rescan
Observe stale/reindex behavior
Clear safe cache
Delete semantic index
Rebuild semantic index
Remove source
Export diagnostics
Restart app
Upgrade from previous build
```

---

## 14. Release Readiness Levels

## 14.1. Developer Preview

Allowed limitations:

- rough UI;
- limited file types;
- no reranker;
- CPU-only;
- manual model setup.

Required:

- safe source boundary;
- no document upload;
- keyword search;
- cleanup safety.

## 14.2. Alpha

Required:

- scan/extract/chunk/index pipeline;
- keyword + vector search;
- storage dashboard;
- crash recovery basics;
- package smoke tests.

## 14.3. Beta

Required:

- stable GUI flows;
- benchmark report;
- Japanese baseline;
- migration tests;
- diagnostics export;
- security gates pass.

## 14.4. v1.0

Required:

- release documentation;
- known limitations;
- package integrity;
- cross-platform smoke;
- migration from beta;
- retrieval quality acceptance.

---

## 15. Acceptance Criteria

- Test categories are represented in project plan.
- Fast CI gate is defined.
- Release CI gate is defined.
- Security gates are mandatory.
- Lifecycle tests are mandatory.
- Retrieval benchmark is part of release validation.
- Packaging smoke tests exist.
- Manual QA checklist exists.
- Release readiness levels are documented.

---

## 16. Testing Requirements for This RFC

Meta-tests/process checks:

1. CI workflow includes fast test job.
2. CI workflow includes migration test.
3. CI workflow includes path boundary test.
4. Release checklist is present in repository.
5. Benchmark command is runnable.
6. Packaging smoke script exists.
7. Diagnostics export test exists.
8. Cleanup safety test exists.

---

## 17. Unresolved Questions

- Which CI provider should be primary?
- Should GUI tests be automated with Playwright/WebDriver if WebView-based?
- Should packaged app tests run in virtual machines?
- What retrieval thresholds define beta/v1?
- Should release checklist be enforced by xtask?

---

## 18. Decision

Adopt release gates that cover security, lifecycle, retrieval quality, storage behavior, recovery, and packaging.

Build success alone is not a release criterion.
