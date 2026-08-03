#!/usr/bin/env bash
# scripts/verify-release-archive.sh — independent CI verification of a
# release archive built by scripts/package.sh (RFC-051 §6, HANDOFF-051 §3).
#
# Usage:
#   ./scripts/verify-release-archive.sh <archive.tar.gz> [commit]
#
# Independence (RFC-051 §6, "the verifier must not consume the producer's
# emitted input/path list"): this script re-derives the expected file set
# from `git ls-tree` plus scripts/release-path-policy.sh itself — the same
# *policy data*, but never package.sh's own file list, log output, or any
# other producer-emitted artifact. A planted producer bug (an extra file
# slipped into the archive, or a required file silently dropped) is
# invisible to a verifier that trusts the producer's own accounting; this
# one only trusts `git ls-tree` and re-runs the exclusion logic itself.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# shellcheck source=./release-path-policy.sh
source "${SCRIPT_DIR}/release-path-policy.sh"

fail=0
flag() { echo "verify-release-archive: $1" >&2; fail=1; }

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "Usage: $0 <archive.tar.gz> [commit]" >&2
  exit 1
fi
ARCHIVE="$1"
COMMIT="${2:-HEAD}"
COMMIT="$(git rev-parse "${COMMIT}")"

if [ ! -f "$ARCHIVE" ]; then
  flag "archive not found: $ARCHIVE"
  exit 1
fi

# ── Independently derive the expected set ────────────────────────────────
EXPECTED_FILES=()
while IFS= read -r path; do
  [ -n "$path" ] || continue
  if ! release_path_excluded "$path"; then
    EXPECTED_FILES+=("$path")
  fi
done < <(git ls-tree -r --name-only "${COMMIT}")

if [ "${#EXPECTED_FILES[@]}" -lt 8 ]; then
  flag "independently-derived expected set has only ${#EXPECTED_FILES[@]} entries — discovery or policy is almost certainly wrong"
  exit 1
fi

# ── Read the archive's actual entries ────────────────────────────────────
ARCHIVE_ENTRIES="$(tar tf "$ARCHIVE")"

# Reject malformed entry names before anything else: must start with `./`,
# no absolute paths, no `..` traversal, no repeated slashes.
while IFS= read -r entry; do
  [ -n "$entry" ] || continue
  case "$entry" in
    ./) continue ;;  # the one canonical root entry
    ./*) : ;;
    /*) flag "absolute path entry: $entry" ;;
    *) flag "entry missing canonical ./ prefix: $entry" ;;
  esac
  case "$entry" in
    */../*|../*|*/..) flag "traversal (..) component in entry: $entry" ;;
  esac
  case "$entry" in
    *//*) flag "repeated-slash entry: $entry" ;;
  esac
done <<< "$ARCHIVE_ENTRIES"

# Build the actual file set (directories excluded — expected set is files
# only, from git ls-tree).
ACTUAL_FILES="$(echo "$ARCHIVE_ENTRIES" | grep -v '/$' | grep -v '^\./$' | sed 's#^\./##' | sort)"
EXPECTED_SORTED="$(printf '%s\n' "${EXPECTED_FILES[@]}" | sort)"

# ── Exact set and multiplicity equality ───────────────────────────────────
MISSING="$(comm -23 <(echo "$EXPECTED_SORTED") <(echo "$ACTUAL_FILES") || true)"
UNEXPECTED="$(comm -13 <(echo "$EXPECTED_SORTED") <(echo "$ACTUAL_FILES") || true)"
DUPLICATES="$(echo "$ACTUAL_FILES" | sort | uniq -d || true)"

if [ -n "$MISSING" ]; then
  flag "expected files missing from archive:"
  echo "$MISSING" | sed 's/^/  /' >&2
fi
if [ -n "$UNEXPECTED" ]; then
  flag "unexpected files present in archive (not in the reviewed tracked set):"
  echo "$UNEXPECTED" | sed 's/^/  /' >&2
fi
if [ -n "$DUPLICATES" ]; then
  flag "duplicate entries in archive:"
  echo "$DUPLICATES" | sed 's/^/  /' >&2
fi

# ── Required roots/files (independently, not trusting producer) ──────────
for required in "${RELEASE_REQUIRED_PATHS[@]}"; do
  echo "$ACTUAL_FILES" | grep -qxF "$required" || flag "required file absent from archive: $required"
done
for root in "${RELEASE_REQUIRED_ROOTS[@]}"; do
  echo "$ACTUAL_FILES" | grep -q "^${root}/" || flag "required root has no entries in archive: $root/"
done

# ── Unpack and validate the lockfile ──────────────────────────────────────
if [ "$fail" -eq 0 ]; then
  UNPACK_DIR="$(mktemp -d)"
  trap 'rm -rf "${UNPACK_DIR}"' EXIT
  tar xf "$ARCHIVE" -C "$UNPACK_DIR"
  if ! (cd "$UNPACK_DIR" && cargo metadata --locked --no-deps --format-version 1 >/dev/null); then
    flag "cargo metadata --locked failed against the unpacked archive — Cargo.lock is not coherent with Cargo.toml"
  fi
fi

# ── Checksum ───────────────────────────────────────────────────────────
if [ -f "${ARCHIVE}.sha256" ]; then
  if ! (cd "$(dirname "$ARCHIVE")" && sha256sum -c "$(basename "${ARCHIVE}.sha256")" >/dev/null); then
    flag "checksum verification failed for $ARCHIVE"
  fi
else
  flag "no checksum file found: ${ARCHIVE}.sha256"
fi

if [ "$fail" -ne 0 ]; then
  echo "verify-release-archive: failed" >&2
  exit 1
fi
echo "verify-release-archive: ok (${#EXPECTED_FILES[@]} files, commit ${COMMIT})"
