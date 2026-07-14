# Implementation Handoff — RFC-051: Reproducible Reviewed-Source Packaging

**Project:** orbok  
**RFC:** 051  
**Lifecycle stage:** Design + handoff  
**Primary owners:** release automation and CI  
**RFC:** [`../proposed/051-reproducible-reviewed-source-packaging.md`](../proposed/051-reproducible-reviewed-source-packaging.md)

> **Scope rule:** The release input is a reviewed commit/file manifest, never
> the ambient working-directory traversal.

## 1. Expected Change Surface

- `scripts/package.sh`
- a single machine-readable release-path policy or helper script
- `.github/workflows/ci.yml`
- `docs/src/maintainers/development.md`
- `docs/src/maintainers/release_readiness.md`
- packaging tests/smoke script

## 2. Program Design

1. Define included/excluded tracked paths in one policy consumed by packaging
   and CI.
2. Fail release packaging on dirty tracked content. Ignore untracked/ignored
   files structurally because they are absent from commit-derived input.
3. Obtain archive inputs from the release commit's tracked file set, filtered
   through the reviewed policy; do not feed `tar` the repository directory.
4. Include `Cargo.lock` and required license/build metadata.
5. Normalize path order, uid/gid, owner/group, mode, mtime, and gzip metadata.
6. Write to a temporary artifact, verify it, then move to the final versioned
   path and generate its checksum.
7. Keep the flat archive layout.
8. Reject tracked symlinks under the initial no-exception policy.
9. Emit exactly one `./` root entry and canonical `./<path>` entries.

Use the RFC-pinned builder image, GNU tar 1.35 POSIX/PAX output, and gzip 1.12.
The script records the builder digest/tool versions and deletes nondeterministic
PAX time keys. Do not substitute host tar/gzip for release evidence.

## 3. Independent CI Verification

CI uses a verifier code path separate from the producer. It derives the expected
list from `git ls-tree` plus the shared policy, never from producer output, and
performs exact canonical set and multiplicity equality. It also:

- rejects absolute, traversal, duplicate, forbidden, and unexpected paths;
- requires `Cargo.lock`, `Cargo.toml`, `LICENSE`, `NOTICE`, source, docs, RFCs,
  and scripts;
- unpacks into a fresh directory and runs `cargo metadata --locked`;
- builds twice from the same clean commit and compares SHA-256 values;
- verifies the published checksum format.

## 4. Adversarial Tests

Plant an untracked file, an ignored file, a forbidden local directory, and a
filename containing spaces; prove only reviewed tracked inputs are packaged and
untracked/ignored files do not block the run. Prove dirty tracked content blocks
packaging. Add a tracked symlink and prove rejection. Test executable-bit
preservation, stable ordinary-file modes, canonical `./` naming, multiplicity,
and planted producer/policy disagreement.

## 5. Validation

- packaging smoke/adversarial script
- two-build deterministic hash comparison
- archive exact-list comparison
- clean unpack plus `cargo metadata --locked --no-deps --format-version 1`
- `bash scripts/check-rfc-lifecycle.sh`
- `mdbook build docs`
- `git diff --check`

## 6. Stop Conditions

Return to design review if reproducibility requires dropping `Cargo.lock`,
including unreviewed generated content, changing the flat-layout contract, or
coupling source packaging to tag/publish operations.

## 7. Definition of Done

Only reviewed tracked files enter the archive, the audited lock is present,
dirty/untracked material cannot leak, repeated clean builds are byte-identical,
CI checks exact contents independently, and maintainer docs match the observed
commands.
