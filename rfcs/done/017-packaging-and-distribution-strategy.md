# RFC-017: Packaging and Distribution Strategy

**Project:** orbok  
**RFC:** 017  
**Title:** Packaging and Distribution Strategy  
**Status:** Implemented (v0.5.0)
**Target Milestone:** M13  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the packaging and distribution strategy for `orbok`.

The central decision is:

> `orbok` should be distributed as a local desktop application with a Rust backend, clear local-data directories, optional local model installation, and reproducible release artifacts for Linux, Windows, and macOS.

---

## 2. Motivation

`orbok` is a local-first app that accesses files, stores local indexes, and may use local models. Packaging must therefore address:

- cross-platform file locations;
- model storage;
- database migration;
- app updates;
- release artifact trust;
- platform-specific installation expectations;
- GPU/CPU feature differences;
- WebView/native frontend dependency if applicable.

Packaging should not be an afterthought because data directory layout and model installation affect architecture.

---

## 3. Goals

- Define target platforms.
- Define release artifact types.
- Define local data directory layout.
- Define model packaging policy.
- Define CPU/GPU packaging strategy.
- Define update/migration expectations.
- Keep source files outside app-managed storage.
- Avoid silent model downloads.

---

## 4. Non-Goals

- This RFC does not define commercial code signing policy.
- This RFC does not require app store distribution.
- This RFC does not implement auto-update in v1.
- This RFC does not bundle every model.
- This RFC does not define final UI framework choice.

---

## 5. Target Platforms

Initial target platforms:

```text
Linux x86_64
Windows x86_64
macOS Apple Silicon
macOS x86_64 if practical
```

Future:

```text
Linux aarch64
portable CLI-only mode
```

---

## 6. Artifact Types

Recommended artifacts:

| Platform | Artifact |
|---|---|
| Linux | `.tar.gz`, AppImage or distro package later |
| Windows | `.zip` portable package, installer later |
| macOS | `.tar.gz` or `.dmg` later |
| Source | source archive |
| Checksums | SHA256SUMS |
| Docs | packaged docs or link to docs |

Early releases should prefer simple portable archives.

---

## 7. Data Directory Layout

Use platform-appropriate directories.

Logical layout:

```text
orbok/
├── orbok-catalog.sqlite3
├── orbok-cache.sqlite3
├── models/
├── vector-index/
├── keyword-index/
├── logs/
├── diagnostics/
├── backups/
└── tmp/
```

Rules:

- source files remain outside this directory;
- app cleanup must not delete source files;
- cache DB and catalog DB are separate;
- large vector/index files may use subdirectories;
- temporary files must be cleanable.

---

## 8. Configuration Directory

Use a standard directory resolver crate or platform conventions.

Examples:

| Platform | Typical Location |
|---|---|
| Linux | `$XDG_DATA_HOME/orbok` or `~/.local/share/orbok` |
| Windows | `%APPDATA%/orbok` or `%LOCALAPPDATA%/orbok` |
| macOS | `~/Library/Application Support/orbok` |

The exact crate and path policy should be implemented consistently.

---

## 9. Model Packaging Policy

Recommended v1 policy:

- do not bundle large models by default unless size is acceptable;
- allow users to locate existing models;
- optionally offer explicit model download;
- show model size and license summary;
- store models under `models/`;
- verify checksum where available.

No silent model download.

---

## 10. CPU/GPU Strategy

Initial packaging should assume CPU works everywhere.

GPU acceleration may be feature-gated or distributed separately later.

Possible artifact variants:

```text
orbok-linux-x86_64-cpu.tar.gz
orbok-windows-x86_64-cpu.zip
orbok-macos-aarch64-cpu.tar.gz
```

Future:

```text
orbok-linux-x86_64-cuda.tar.gz
orbok-windows-x86_64-cuda.zip
orbok-macos-aarch64-metal.tar.gz
```

Do not let GPU packaging block CPU release.

---

## 11. Frontend Packaging

If using Tauri/Svelte/WebView:

- bundle frontend assets;
- restrict external navigation;
- ensure backend commands are allowlisted;
- avoid requiring development server in release build.

If using native Rust GUI:

- package native assets;
- ensure fonts/icons licensing is clean;
- avoid relying on system resources unexpectedly.

---

## 12. Database Migration on Upgrade

On app startup:

1. locate data directory;
2. open catalog database;
3. verify schema version;
4. run migrations if safe;
5. backup catalog before destructive migration if needed;
6. verify localcache database state;
7. recover/rebuild cache if needed.

Release notes must mention schema changes.

---

## 13. Portable Mode

A future portable mode may use:

```text
./orbok-data/
```

near the executable.

This is useful for testing and portable installs, but must be explicit.

Do not accidentally create indexes in the source code directory during development without clear configuration.

---

## 14. Checksums and Release Integrity

Each release should provide:

```text
SHA256SUMS
SHA256SUMS.sig if signing is available
```

Even without paid code signing, checksums improve integrity.

---

## 15. License and Notices

Release package must include:

- app license;
- third-party dependency notices where practical;
- model license notices for bundled models;
- privacy statement;
- documentation link.

---

## 16. Acceptance Criteria

- Release artifacts are reproducible enough for development.
- App uses platform-appropriate data directory.
- Catalog and cache database paths are predictable.
- Source files are never stored under app data unless user explicitly chooses such source.
- Model installation is explicit.
- CPU package works without GPU.
- Startup migration path exists.
- Release artifacts include checksums.
- Packaged app does not require dev server.
- Logs and diagnostics are stored under app data directory.

---

## 17. Testing Requirements

Required tests:

1. Fresh install starts.
2. Existing data directory opens.
3. Migration from previous schema succeeds.
4. Cache DB missing is recreated.
5. Model directory missing is handled.
6. App starts without GPU.
7. Packaged frontend assets load.
8. Source files outside app data are not copied.
9. Cleanup does not remove source files.
10. Checksums generated for artifacts.

---

## 18. Unresolved Questions

- Final frontend stack: Tauri/Svelte, local web UI, or native GUI?
- Should macOS notarization be required for release?
- Should Windows installer be created early?
- Should Linux AppImage be supported?
- Should auto-update exist?
- Should GPU variants be distributed separately?

---

## 19. Decision

Start with simple portable CPU-first release archives for Linux, Windows, and macOS where practical.

Keep model installation explicit and separate from the core app package unless model size and license make bundling clearly acceptable.
