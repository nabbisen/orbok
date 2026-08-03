#!/usr/bin/env bash
# scripts/package.sh — build a reproducible release archive for orbok
# (RFC-051).
#
# Usage:
#   ./scripts/package.sh <version>
#
# Example:
#   ./scripts/package.sh 0.17.0
#
# Output (written to dist/ at the repository root, never into the source tree):
#   dist/orbok-v0.17.0.tar.gz
#   dist/orbok-v0.17.0.tar.gz.sha256
#
# Archive layout (flat — one `./` root entry, then `./<repo-relative-path>`
# entries, canonical naming, no parent directory beyond the root):
#   ./
#   ./Cargo.toml
#   ./crates/
#   ./rfcs/
#   ...
#
# Source of truth (RFC-051 §4): git-tracked files at HEAD, filtered
# through scripts/release-path-policy.sh — never the ambient working
# directory. Packaging fails if any tracked file is dirty (modified,
# staged, or deleted relative to HEAD); untracked and ignored files are
# structurally absent from the input, never a reason to fail.
#
# Determinism (RFC-051 §5): normalized path order, uid/gid, owner/group,
# mode (from git's own tracked mode — 100755 stays executable, everything
# else gets 644), mtime (the release commit's own committer timestamp),
# and gzip metadata. Two clean builds of the same commit on the same
# toolchain produce byte-identical output — verified by
# scripts/package.test.sh, not merely asserted.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# shellcheck source=./release-path-policy.sh
source "${SCRIPT_DIR}/release-path-policy.sh"

# ── Argument validation ────────────────────────────────────────────────
if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <version>" >&2
    exit 1
fi
VERSION="$1"
ARCHIVE="orbok-v${VERSION}.tar.gz"
DIST_DIR="${REPO_ROOT}/dist"
mkdir -p "${DIST_DIR}"

COMMIT="$(git rev-parse HEAD)"
echo "Packaging orbok-v${VERSION} from commit ${COMMIT} ..."

# ── Dirty-tracked-content check (RFC-051 §4) ───────────────────────────
# Untracked/ignored files are structurally absent from a git-tree-derived
# input set and are never a reason to fail. A *tracked* file that differs
# from HEAD (modified, staged, or deleted) means the working tree and the
# commit being packaged have silently diverged — fail rather than package
# a state nobody reviewed.
DIRTY="$(git status --porcelain --untracked-files=no -- . || true)"
if [ -n "$DIRTY" ]; then
  echo "error: tracked content is dirty relative to HEAD — commit or stash before packaging:" >&2
  echo "$DIRTY" >&2
  exit 1
fi

# ── Reject tracked symlinks (RFC-051 §4) ───────────────────────────────
# git ls-tree mode 120000 is a symlink. The initial policy has no
# exceptions. Split on the first TAB only (not awk field-splitting) so a
# path containing spaces is not broken apart.
SYMLINKS="$(git ls-tree -r "${COMMIT}" | while IFS=$'\t' read -r meta path; do
  [ "${meta%% *}" = "120000" ] && echo "$path"
  true
done)"
if [ -n "$SYMLINKS" ]; then
  echo "error: tracked symlinks are rejected by the initial release policy (RFC-051 §4):" >&2
  echo "$SYMLINKS" >&2
  exit 1
fi

# ── Build the filtered tracked-path set ─────────────────────────────────
ALL_TRACKED="$(git ls-tree -r --name-only "${COMMIT}")"
FILTERED_PATHS=()
while IFS= read -r path; do
  [ -n "$path" ] || continue
  if ! release_path_excluded "$path"; then
    FILTERED_PATHS+=("$path")
  fi
done <<< "$ALL_TRACKED"

if [ "${#FILTERED_PATHS[@]}" -lt 8 ]; then
  echo "error: filtered tracked-path set has only ${#FILTERED_PATHS[@]} entries — the discovery or policy is almost certainly wrong" >&2
  exit 1
fi

for required in "${RELEASE_REQUIRED_PATHS[@]}"; do
  found=0
  for path in "${FILTERED_PATHS[@]}"; do
    [ "$path" = "$required" ] && { found=1; break; }
  done
  if [ "$found" -ne 1 ]; then
    echo "error: required path missing from the filtered tracked set: $required" >&2
    exit 1
  fi
done

for root in "${RELEASE_REQUIRED_ROOTS[@]}"; do
  found=0
  for path in "${FILTERED_PATHS[@]}"; do
    case "$path" in
      "$root"/*) found=1; break ;;
    esac
  done
  if [ "$found" -ne 1 ]; then
    echo "error: required root has no entries in the filtered tracked set: $root/" >&2
    exit 1
  fi
done

# ── Populate a clean staging directory from git, not the working tree ──
# git archive reads from the commit's tree, never the filesystem, so
# staging can never pick up untracked or ignored content regardless of
# what sits alongside the repository on disk. Verified separately
# (review-request evidence) that extraction preserves git's tracked mode
# bits exactly (100644 -> 644, 100755 -> 755) despite `tar tvf`'s summary
# display looking otherwise for the raw git-archive stream.
STAGING="$(mktemp -d "${REPO_ROOT}/.release-staging.XXXXXX")"
trap 'rm -rf "${STAGING}"' EXIT

git archive --format=tar "${COMMIT}" | tar -x -C "${STAGING}"

# Remove anything the policy excludes but that `git archive` (deliberately
# unfiltered — it archives the whole tree) still populated. Logged, not
# silent: every removal is an explicit, auditable line. A `while read`
# loop over NUL-delimited entries, not `for x in $(...)`, so a path
# containing spaces is not word-split.
while IFS= read -r -d '' path; do
  path="${path#./}"
  [ -n "$path" ] || continue
  if release_path_excluded "$path"; then
    echo "excluding per release-path-policy.sh: $path"
    rm -rf "${STAGING:?}/${path}"
  fi
done < <(cd "${STAGING}" && find . -mindepth 1 -print0)

# ── Build the archive: PAX format, fully normalized metadata ────────────
MTIME_EPOCH="$(git log -1 --format=%ct "${COMMIT}")"
TAR_VERSION="$(tar --version | head -1)"
GZIP_VERSION="$(gzip --version | head -1)"

TMPARCHIVE="$(mktemp "${DIST_DIR}/orbok-pkg-XXXXXX.tar.gz")"
tar --format=pax \
    --sort=name \
    --owner=0 --group=0 --numeric-owner \
    --mtime="@${MTIME_EPOCH}" \
    --pax-option=delete=atime,delete=ctime \
    -C "${STAGING}" \
    -cf - . \
  | gzip -n -9 > "${TMPARCHIVE}"
mv "${TMPARCHIVE}" "${DIST_DIR}/${ARCHIVE}"

# ── Checksum ───────────────────────────────────────────────────────────
cd "${DIST_DIR}"
sha256sum "${ARCHIVE}" > "${ARCHIVE}.sha256"

# ── Summary ────────────────────────────────────────────────────────────
echo ""
echo "Created:"
echo "  dist/${ARCHIVE}"
echo "  dist/${ARCHIVE}.sha256"
echo "  SHA-256: $(awk '{print $1}' "${ARCHIVE}.sha256")"
echo ""
echo "Source commit: ${COMMIT}"
echo "Toolchain (RFC-051 §5 — recorded, not pinned): ${TAR_VERSION} / ${GZIP_VERSION}"
echo ""
echo "Archive layout (first 8 entries — flat, canonical ./ naming):"
tar tf "${ARCHIVE}" | awk 'NR <= 8 { print }'
echo "  ..."
echo "  ($(tar tf "${ARCHIVE}" | wc -l) entries total)"
