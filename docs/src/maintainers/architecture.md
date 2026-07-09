# Architecture Overview

orbok is a Rust workspace of twelve crates, grouped under `crates/` by domain.

## Crate Map

```
crates/
├── app/          orbok            — binary: bootstrap, --check mode, GUI launch
├── bench/        orbok-bench  — benchmark harness
├── core/         orbok-core   — typed IDs, error types, data-lifecycle classes
├── data/
│   ├── cache/    orbok-cache  — localcache wrapper for derived-data payloads
│   ├── db/       orbok-db     — SQLite catalog: migrations, repositories (RFC-002)
│   └── fs/       orbok-fs     — safe file access boundary, source policies, scanner (RFC-003/004)
├── pipeline/
│   ├── extract/  orbok-extract — extractor trait, text/PDF/DOCX extractors, chunker (RFC-005)
│   └── workers/  orbok-workers — indexing pipeline, cleanup, recovery (RFC-011/018)
├── search/
│   ├── embed/    orbok-embed  — inference backends: mock, ONNX/tract (RFC-021)
│   ├── engine/   orbok-search — keyword (FTS5) + vector + hybrid RRF (RFC-007/009)
│   └── models/   orbok-models — model traits, capability vocabulary (RFC-012)
└── ui/           orbok-ui     — snora/iced shell, views, i18n, state (RFC-027/031)
```

## Key Design Rules

1. **orbok-ui never accesses the filesystem** (RFC-027).
2. **Every file read goes through PathGuard** (RFC-003 §8).
3. **The catalog is authoritative**; localcache payloads live in a separate DB (Appendix A §3).
4. **Cleanup runs only from a validated CleanupPlan** (RFC-001 §14).
5. **All user-visible strings live in orbok-ui/src/i18n** — backend crates have no display strings (RFC-031).
6. **orbok-ui uses the Snora Design system** (`snora` `design` feature) for
   accessible, WCAG-AA-verified surfaces. `AppState.tokens` holds the active
   `snora::design::Tokens` preset; the high-contrast toggle in Settings swaps
   between `Tokens::light()` and `Tokens::high_contrast_light()`. The
   `friendly_notice` view renders via `snora::design::notice::Notice`, with the
   `UserNotice` domain enum still owning semantics and i18n.
