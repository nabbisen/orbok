# RFC-051: Reproducible Reviewed-Source Packaging

**Project:** orbok  
**RFC:** 051  
**Title:** Reproducible Reviewed-Source Packaging  
**Status:** Proposed  
**Target milestone:** v1.0.0 release provenance  
**Date:** 2026-07-14  
**Related RFCs:** RFC-017 Packaging and Distribution; RFC-019 Test Matrix and Release Readiness  
**Handoff:** [`HANDOFF-051-reproducible-reviewed-source-packaging.md`](../handoffs/HANDOFF-051-reproducible-reviewed-source-packaging.md)

---

## 1. Summary

This RFC makes the source release archive a deterministic projection of
reviewed repository files rather than a filtered copy of the working directory.
The application workspace's audited `Cargo.lock` is included.

## 2. Triggering Evidence

The v0.24.0 packaging path archives `.` with exclusions. That permits
untracked/ignored material not covered by the exclusion list and deliberately
omits `Cargo.lock`. The observed release archive contained more entries than
the Git-tracked source set. A checksum authenticates that archive after creation
but does not prove reviewed provenance or dependency-lock equivalence.

## 3. Artifact Contract

The source archive must:

1. contain only files selected from a reviewed, version-controlled allowlist;
2. include `Cargo.lock` because orbok is an application workspace and release
   gates/audit run against that lock;
3. remain flat, with files directly under the archive root;
4. exclude `.git`, `.git-exclude`, local agent configuration, build output,
   generated `docs/book`, prior release output, and other local-only material;
5. use normalized ordering, ownership, permissions, and timestamps so the same
   commit and tool contract produce the same bytes;
6. ship with a SHA-256 checksum generated after archive creation.

## 4. Reviewed File Selection

Packaging must start from Git-tracked paths at the exact release commit (or an
equivalent explicit manifest generated and reviewed from that commit). It must
not traverse the ambient working directory.

The release allowlist includes the application source and required build,
license, documentation, RFC, CI, and script files. It excludes local-only paths
even if accidentally tracked; the exact policy is maintained in one
machine-readable place used by packaging and CI.

Packaging from a dirty tree must fail by default. An explicit maintainer-only
override is out of scope for the release gate and must not be used for published
artifacts.

## 5. Determinism Rules

- Sort archive paths bytewise in a documented locale.
- Normalize uid/gid and owner/group names.
- Normalize modification time to a commit-derived or fixed epoch.
- Normalize executable bits from the reviewed Git mode; ordinary files use a
  stable non-executable mode.
- Use stable gzip metadata without a wall-clock filename/timestamp.
- Record the source commit identifier in release evidence, not necessarily as
  a file inside the archive.

## 6. CI Verification

CI must independently derive the expected path list and compare it exactly with
the archive path list. It must also verify:

- required roots and legal files are present;
- `Cargo.lock` is present and `cargo metadata --locked` succeeds after unpack;
- forbidden/local-only paths are absent;
- no unexpected path, duplicate, absolute path, or `..` component exists;
- checksum verification passes;
- two clean builds of the archive produce the same SHA-256 digest.

## 7. Non-Goals

- Binary installers, signing/notarization, or platform package formats.
- Publishing, tagging, or changing the release cadence.
- Vendoring all dependencies.
- Rewriting Git history to normalize file metadata.

## 8. Testing Requirements

Tests must plant an untracked file and prove it is excluded, modify a tracked
file and prove dirty-tree packaging fails, verify `Cargo.lock` inclusion, check
exact path equality, reject traversal/forbidden paths, unpack and run locked
metadata validation, and demonstrate deterministic repeated output.

## 9. Acceptance Criteria

This RFC is accepted when the tracked-file/allowlist model, lockfile policy,
dirty-tree rule, and metadata normalization contract are approved.

It is implemented when packaging and CI share the reviewed selection policy,
the archive exactly matches it, `Cargo.lock` is present, repeated clean builds
are byte-reproducible, release documentation is updated, and a release review
records the source commit plus checksum.

