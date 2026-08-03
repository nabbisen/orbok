#!/usr/bin/env bash
# release-path-policy.sh — the one machine-readable release-archive path
# policy (RFC-051 §4), sourced by both the producer (scripts/package.sh)
# and the independent CI verifier (scripts/verify-release-archive.sh) so
# they can never silently diverge.
#
# The archive's input set is the git-tracked file set at the release
# commit, filtered through the exclusions below. Today's tracked tree
# contains nothing these patterns need to exclude in practice — verified
# by `git ls-tree -r --name-only HEAD` against every pattern here at the
# time this policy was written. They exist as defense-in-depth against
# local-only material that might someday be tracked by accident
# (RFC-051 §4: "excludes local-only paths even if accidentally tracked"),
# not because anything currently needs excluding.
#
# `.vscode/` and `.cargo/` are deliberately NOT excluded: both are
# intentionally tracked, reviewed project configuration (recommended
# editor extensions, `cargo audit` waivers), not local-only material —
# confirmed by inspecting their tracked contents, not assumed.

# release_path_excluded <repo-relative-path>
# Returns success (0) if the path should be excluded from the archive.
#
# Matches both a directory itself (e.g. `.git-exclude`, as `find` yields it
# during staging cleanup) and anything under it (e.g. `.git-exclude/notes.md`,
# as `git ls-tree` yields it) — a pattern covering only the latter leaves the
# former as an empty-but-present directory entry in the final archive.
release_path_excluded() {
  local path="$1"
  case "$path" in
    .git-exclude|.git-exclude/*) return 0 ;;
    .agents|.agents/*) return 0 ;;
    .codex|.codex/*) return 0 ;;
    dist|dist/*) return 0 ;;
    docs/book|docs/book/*) return 0 ;;
    *.tar.gz|*.tar.gz.sha256) return 0 ;;
    *) return 1 ;;
  esac
}

# Paths that must be present in the filtered set, or packaging/verification
# fails (HANDOFF-051 §3: "requires Cargo.lock, Cargo.toml, LICENSE, NOTICE,
# source, docs, RFCs, and scripts").
RELEASE_REQUIRED_PATHS=(
  'Cargo.toml'
  'Cargo.lock'
  'LICENSE'
  'NOTICE'
  'README.md'
)

# Top-level directories that must be non-empty in the filtered set.
RELEASE_REQUIRED_ROOTS=(
  'crates'
  'docs'
  'rfcs'
  'scripts'
)
