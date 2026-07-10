# RFC-030: Portable Mode

**Project:** orbok  
**RFC:** 030  
**Title:** Portable Mode  
**Status:** Implemented (v0.8.0)
**Target Timing:** When portable/USB/self-contained distribution becomes a concrete product goal  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will define a portable mode for `orbok`.

Portable mode would keep app data near the executable or in a user-selected directory instead of the platform default application data directory.

## 2. Motivation

Portable mode may be useful for USB drive usage, isolated testing, client/project-specific search workspaces, no-install environments, and reproducible demos.

But portable mode complicates path handling, model storage, permissions, backups, relative paths, external drive disappearance, and update behavior.

## 3. Activation Conditions

Reconsider this RFC when:

1. Standard app data layout is implemented.
2. Packaging strategy exists.
3. Source registration is stable.
4. Storage dashboard works.
5. User demand for portable use is confirmed.

## 4. Candidate Modes

| Mode | Description |
|---|---|
| standard | platform app data directory |
| portable-near-exe | `./orbok-data` near executable |
| portable-explicit | user passes `--data-dir` |
| project workspace | data stored under selected project folder |

Recommended first portable support:

```text
--data-dir <path>
```

This is explicit and useful for development/testing.

## 5. Data Layout

Portable mode layout:

```text
orbok-portable/
├── orbok executable
└── orbok-data/
    ├── orbok-catalog.sqlite3
    ├── orbok-cache.sqlite3
    ├── models/
    ├── vector-index/
    ├── keyword-index/
    ├── logs/
    └── tmp/
```

## 6. Source Path Policy

Portable mode must decide whether source paths are absolute, relative to data dir, relative to portable root, or mixed.

Default should remain absolute unless user explicitly creates a portable workspace.

## 7. Security Considerations

Portable mode risks copied indexes, weaker OS protection, path confusion, and loss of a removable drive.

UI must warn:

```text
Portable mode stores orbok indexes in this folder. These indexes may contain sensitive derived data.
```

## 8. Non-Goals

This future RFC should not replace standard app data mode, block normal packaging, guarantee secure portability, or copy source files into portable data directory by default.

## 9. Expected Decision Output

The activated RFC should produce portable mode trigger, data directory layout, path policy, model storage policy, cleanup behavior, security warning, migration behavior, and packaging implications.

## 10. Acceptance Criteria

- Portable mode is explicit.
- App can run with custom data directory.
- Source files are not copied by default.
- Storage dashboard works in portable mode.
- Cleanup works in portable mode.
- Warning explains sensitive derived data.
- Standard mode remains default.

## 11. Deferred Decision

Do not implement portable mode as a product feature until standard mode is stable.

A development-only `--data-dir` may be implemented earlier if useful.
