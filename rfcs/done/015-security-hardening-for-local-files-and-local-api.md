# RFC-015: Security Hardening for Local Files and Local API

**Project:** orbok  
**RFC:** 015  
**Title:** Security Hardening for Local Files and Local API  
**Status:** Implemented (v0.5.0)
**Target Milestone:** M13 Supporting  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines security hardening requirements for `orbok`.

The central decision is:

> `orbok` is local-first, but it must still be treated as a security-sensitive local application because it reads private files, stores derived indexes, and may expose a local UI/API boundary.

Local-first reduces cloud privacy risk, but it does not eliminate local security risk.

---

## 2. Motivation

`orbok` may process sensitive local documents. Risks include:

- accidental indexing of secrets;
- symlink escape from approved folders;
- path traversal;
- unsafe HTML preview rendering;
- parser vulnerabilities;
- local HTTP API abuse;
- WebView origin confusion;
- logs leaking snippets or paths;
- cache files containing extracted text;
- embeddings leaking semantic information.

A security model must exist before release.

---

## 3. Goals

- Define threat model.
- Harden local file access.
- Harden local API/WebView boundary.
- Protect logs and diagnostics.
- Treat derived indexes as sensitive local data.
- Enforce safe rendering.
- Require tests for path traversal and source boundary.
- Integrate with lifecycle and cleanup design.
- Keep app usable without overcomplicating v1.

---

## 4. Non-Goals

- This RFC does not provide malware sandboxing.
- This RFC does not guarantee protection from a fully compromised local user account.
- This RFC does not implement enterprise multi-user access control.
- This RFC does not require encrypted indexes in v1.
- This RFC does not implement secure deletion guarantees.

---

## 5. Threat Model

## 5.1. Protected Assets

- source file contents;
- file paths and metadata;
- extracted text;
- snippets;
- embeddings;
- search queries;
- model paths;
- local cache files;
- catalog database;
- logs and diagnostics.

## 5.2. Threats

| Threat | Example |
|---|---|
| Accidental over-indexing | user selects home directory and indexes secrets |
| Path traversal | frontend asks backend to read `../../.ssh/id_rsa` |
| Symlink escape | selected folder contains symlink to secret directory |
| Unsafe preview | indexed HTML runs script in UI |
| Local API abuse | malicious webpage calls local API |
| Log leakage | crash log contains document snippet |
| Cache leakage | extracted text stored in cache without user awareness |
| Parser exploit | malformed PDF crashes extractor |
| Stale source confusion | old index shown as fresh |

---

## 6. File Access Boundary

Backend must enforce all file access.

Rules:

1. canonicalize requested path;
2. verify it belongs to an active source;
3. apply source policy;
4. apply symlink policy;
5. apply hidden-file policy;
6. apply max file size;
7. handle permission errors;
8. never trust frontend-provided path alone.

This applies to:

- scanning;
- extraction;
- snippet loading;
- opening source file;
- localcache payload creation;
- diagnostics.

---

## 7. Path Safety Tests

Required cases:

```text
../ traversal
absolute path outside source
symlink to outside source
hardlink/platform edge cases where applicable
deleted file replaced between check and read
case-insensitive path collision
Windows drive/UNC edge cases
```

The implementation should minimize TOCTOU exposure where practical.

---

## 8. Sensitive Directory Warnings

Warn before indexing:

```text
.ssh
.gnupg
.aws
.azure
.config
browser profiles
password manager exports
system directories
hidden home folders
```

Default policy:

- hidden files excluded;
- symlinks ignored;
- sensitive source warning enabled.

---

## 9. Safe Rendering

Never render indexed HTML as trusted HTML.

Rules:

- snippets are escaped text by default;
- HTML previews are sanitized;
- scripts never execute;
- remote resources are not loaded from indexed HTML;
- Markdown preview, if added, must sanitize HTML;
- PDF preview should not execute active content.

---

## 10. Local API Hardening

If `orbok` uses a local HTTP API:

- bind to loopback only;
- do not expose on LAN by default;
- require app-local token for state-changing calls;
- verify Origin/Referer where applicable;
- use random port or named pipe where practical;
- reject arbitrary file path read endpoints;
- use least-privilege API commands.

State-changing APIs include:

- add source;
- remove source;
- cleanup;
- model install;
- reset catalog;
- open file;
- change settings.

---

## 11. WebView Hardening

If using WebView/Tauri/Svelte:

- disable unnecessary navigation;
- restrict external links;
- enforce CSP if applicable;
- do not expose broad filesystem APIs to frontend;
- use command allowlist;
- validate all command inputs in Rust;
- treat frontend state as untrusted.

---

## 12. Logging and Diagnostics

Default logs must not contain:

- document body;
- snippets;
- extracted text;
- vector values;
- raw search queries when history disabled;
- secrets from paths where avoidable.

Recommended logging levels:

| Level | Behavior |
|---|---|
| minimal | operational events, redacted paths |
| normal | paths allowed, no content |
| debug | more technical details, still no document body by default |

Diagnostics export must show a preview and allow redaction.

---

## 13. Derived Data Sensitivity

The following are sensitive:

- embeddings;
- keyword index;
- extracted segment cache;
- snippet cache;
- search history;
- file paths.

UI should not imply that derived data is harmless.

Storage view must allow cleanup of text-bearing caches and rebuildable indexes.

---

## 14. localcache Security Rules

`localcache` may store extracted text-bearing payloads.

Rules:

- access only through backend wrapper;
- create entries only for approved source files;
- do not expose raw localcache API to UI;
- classify namespaces by lifecycle;
- allow cleanup;
- disable text-bearing payloads in privacy-strict mode;
- do not enable encryption until key management is designed.

If encryption is enabled later, key management must be a separate RFC.

---

## 15. Model Download Security

If model installation downloads files:

- require explicit confirmation;
- show source and size;
- verify checksum/signature where available;
- store under model directory;
- do not execute downloaded code;
- do not upload documents during validation.

---

## 16. Parser Safety

Document parsers should be treated as untrusted input handlers.

Rules:

- enforce file size limits;
- use timeouts/cancellation;
- isolate per-file failure;
- catch parser errors;
- avoid unsafe code where possible;
- avoid executing macros/scripts;
- consider process isolation later for high-risk formats.

PDF and Office extraction deserve special attention.

---

## 17. Privacy Modes

Recommended privacy modes:

| Mode | Behavior |
|---|---|
| Standard | local-only, no document upload, normal cache |
| Strict | no search history, minimal snippets, no extracted-text cache |
| Debug | additional diagnostics, still no document body unless explicit |

Strict mode should disable or minimize:

- raw query storage;
- snippet cache;
- extracted segment cache;
- verbose logs.

---

## 18. Stale and Missing Source Honesty

Security and correctness overlap here.

The UI must not show stale indexed content as if it is current.

Rules:

- stale status visible;
- missing source visible;
- permission denied visible;
- source hash mismatch invalidates snippet cache;
- dynamic snippet loading verifies source status.

---

## 19. Acceptance Criteria

- Backend enforces source allowlist for all file reads.
- Path traversal attempts are rejected.
- Symlink escape is blocked by default.
- Hidden files are excluded by default.
- Sensitive directory warning exists.
- HTML snippets/previews are sanitized or escaped.
- Local API, if used, binds to loopback only.
- State-changing local API calls are protected.
- Logs do not include document contents by default.
- localcache text-bearing caches are cleanable and privacy-aware.
- Embeddings are treated as sensitive derived data.
- Model downloads require explicit consent.

---

## 20. Testing Requirements

Required tests:

1. Path traversal rejected.
2. Symlink outside source rejected.
3. Hidden file excluded by default.
4. Sensitive directory warning triggered.
5. HTML script does not execute in preview.
6. Local API rejects non-loopback or invalid origin where applicable.
7. Cleanup cannot delete source files.
8. Logs omit snippets in normal mode.
9. Search query not stored when history disabled.
10. localcache wrapper rejects non-source path.
11. Modified source invalidates snippet cache.
12. Model install cannot run silently.

---

## 21. Unresolved Questions

- Should parser isolation be process-based in v1?
- Should indexes be optionally encrypted?
- Should path redaction be default in logs?
- Should local API use HTTP, IPC, or Tauri commands?
- Should opening source files require confirmation for untrusted formats?
- Should model file integrity verification be mandatory?

---

## 22. Decision

Treat `orbok` as a security-sensitive local app.

Local-first is necessary but not sufficient; backend-enforced file boundaries, safe rendering, local API hardening, and privacy-aware logging are mandatory.

---

## 23. Amendment (2026-06-06): GUI Framework Decision Impact

RFC-027 (activated) selects a native Rust GUI (iced 0.14 via snora 0.8)
with **no WebView and no local HTTP API in v1**. Consequently:

- All requirements in this RFC concerning loopback binding, local API
  tokens/CSRF, and browser-origin checks are **dormant**: they apply
  only if a local HTTP API or WebView frontend is introduced later, and
  must be re-activated in that RFC.
- The frontend boundary requirement remains in force in a new form: the
  `orbok-ui` crate must not perform direct file-system access; all file
  operations are mediated by `orbok-core` services enforcing RFC-003
  source-membership validation.
- Safe-rendering requirements remain in force: extracted document text
  is rendered as plain text widgets only; HTML is never interpreted;
  remote resources are never loaded.
- Log-hygiene and privacy requirements are unchanged.
