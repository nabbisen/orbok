# RFC-051: Reproducible Reviewed-Source Packaging

**Project:** orbok\
**RFC:** 051\
**Title:** Reproducible Reviewed-Source Packaging\
**Status:** Implemented (main at `56d63a6`; release pending)\
**Target milestone:** v1.0.0 release provenance\
**Date:** 2026-07-14\
**Related RFCs:** RFC-017 Packaging and Distribution; RFC-019 Test Matrix and Release Readiness\
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

Packaging from dirty tracked content must fail by default. Untracked and ignored
files neither enter the commit-derived input set nor cause failure. An explicit
maintainer-only dirty-tracked override is out of scope for the release gate and
must not be used for published artifacts.

Tracked symbolic links are rejected unless a later reviewed exception identifies
a concrete need and proves the target cannot escape the archive on extraction.
The initial policy has no symlink exceptions.

## 5. Determinism Rules

- Sort archive paths bytewise in a documented locale.
- Normalize uid/gid and owner/group names.
- Normalize modification time to a commit-derived or fixed epoch.
- Normalize executable bits from the reviewed Git mode; ordinary files use a
  stable non-executable mode.
- Use stable gzip metadata without a wall-clock filename/timestamp.
- Record the source commit identifier in release evidence, not necessarily as
  a file inside the archive.

Archives are produced in POSIX/PAX tar format with gzip timestamp/name
suppression, and PAX access/change-time keys are deleted. **Release evidence
records the tar and gzip versions actually used**, so the toolchain behind any
published archive is always known after the fact.

### Determinism scope: within-release, not cross-time

**Amended 2026-08-03 by project-owner decision.** An earlier draft of this
section additionally mandated GNU tar 1.35 and gzip 1.12 executed inside a
CI builder image pinned by immutable digest. That requirement is **struck**.

What it would have bought is *cross-time* byte reproducibility — the same commit
producing an identical SHA-256 years later on different infrastructure — so that
a third party could verify a release by hash comparison alone rather than by
unpacking and diffing against the tag. That property has real value, but only to
a verifier who exercises it, and orbok has none today: no distribution packaging,
no external audit, no provenance-attestation consumer.

It was also underspecified in a way that would have blocked implementation: no
image, registry, or digest was ever named, so the pin the text referred to did
not exist. And the two constraints could conflict — tool versions are a
consequence of whichever image is chosen, not a free variable, and gzip 1.12
predates what current base images ship.

What this RFC still guarantees is unchanged: the archive contains exactly the
reviewed tracked files of the release commit, with normalized metadata, verified
independently by CI against `git ls-tree`. That is what the §2 triggering
evidence actually called for — an archive-contents problem, not a
bit-identity one.

**Revisit when any of these becomes true**, at which point this is a small
addition rather than a rework, because the content-determinism work below is the
expensive part and is already done:

- a distribution packages orbok under a reproducibility policy (Nix, Guix,
  Debian);
- the project adopts build-provenance attestation (SLSA, sigstore, GitHub
  artifact attestations) that downstream consumers check;
- a security audit or a platform requires independently rebuildable artifacts;
- the threat model is extended to cover release-infrastructure compromise.

Reintroducing it means naming a concrete image digest, making that digest the
pin, and keeping tool versions *recorded* rather than mandated.

Canonical entry names contain one leading `./`; the archive includes one `./`
root entry and then `./<repository-relative-path>` entries. Verifiers normalize
only this declared representation and reject alternate absolute, repeated-slash,
dot-dot, or duplicate spellings.

## 6. CI Verification

CI must independently run a verifier implementation over `git ls-tree` for the
release commit plus the shared path policy. The verifier must not consume the
producer's emitted input/path list. It canonicalizes names and compares exact
sets and multiplicities with the archive. Planted producer and policy violations
must prove this independence. CI must also verify:

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

Tests must plant untracked and ignored files and prove they are structurally
excluded without blocking packaging; modify a tracked file and prove packaging
fails; add a tracked symlink and prove rejection; verify `Cargo.lock` inclusion;
check exact path/multiplicity equality; reject traversal/forbidden/duplicate
paths; unpack and run locked metadata validation; inject producer/policy errors;
and demonstrate deterministic repeated output on a single unchanged toolchain
(§5 — cross-environment reproducibility is explicitly out of scope).

## 9. Acceptance Criteria

This RFC is accepted when the tracked-file/allowlist model, lockfile policy,
dirty-tree rule, and metadata normalization contract are approved.

It is implemented when packaging and CI share the reviewed selection policy,
the archive exactly matches it, `Cargo.lock` is present, repeated clean builds
are byte-reproducible, release documentation is updated, and a release review
records the source commit plus checksum.
