#!/usr/bin/env bash
# check-migration-integrity.sh — RFC-062 §7: a released migration file must
# never change.
#
# crates/data/db/src/migrations.rs's own doc comment: "New migrations are
# appended here and never reordered or edited after release." That rule was
# broken twice (HANDOFF-062 §1) before anything enforced it. This gate
# fails if any migration file that existed at the last release tag differs
# from its blob there now -- unless its filename is on the shrink-only
# allowlist (crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt, see
# that file's own header).
#
# RFC-062 §9 open question 1: how CI determines the last release tag.
# `git describe --tags --abbrev=0` needs a full clone (`fetch-depth: 0` in
# the workflow job) -- on a shallow checkout it fails confusingly rather
# than silently, which is the reason this was chosen over pinning a tag in
# this script and bumping it at release: a stale pin nobody bumps fails
# silently (the gate would just stop checking anything past that point),
# which is the worse failure mode of the two.
#
# New migration files are unaffected: only files that already existed at
# the last release tag are protected.

set -euo pipefail

migrations_dir="crates/data/db/migrations"
allowlist="$migrations_dir/EDITED-RELEASED-ALLOWLIST.txt"

fail=0
flag() {
  echo "migration-integrity gate: $1" >&2
  fail=1
}

if ! last_tag="$(git describe --tags --abbrev=0 2>/dev/null)"; then
  echo "migration-integrity gate: no release tag found -- nothing has been released yet, nothing to protect"
  exit 0
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

# Strips '#'-prefixed comment lines and trailing whitespace; blank lines
# dropped. $1 is a git rev-path spec, e.g. ":$allowlist" or "HEAD:$allowlist".
read_allowlist() {
  git show "$1" 2>/dev/null | sed -E 's/^[[:space:]]*#.*$//; s/[[:space:]]+$//' | grep -v '^[[:space:]]*$' || true
}

current_allowlist="$tmp_dir/allowlist-current.txt"
if git show ":$allowlist" > /dev/null 2>&1; then
  read_allowlist ":$allowlist" | sort -u > "$current_allowlist"
else
  : > "$current_allowlist"
fi

is_allowlisted() {
  grep -qxF "$1" "$current_allowlist"
}

# Shrink-only, the same mechanism rfcs/closures/LEGACY-ALLOWLIST.txt already
# uses and self-tests (check-rfc-lifecycle.sh): the staged id set must be a
# subset of the immediately preceding commit's id set. Comparing against
# HEAD on every commit, rather than some earlier ancestor, is what makes
# growth impossible in any single commit across all of history -- an
# earlier-ancestor comparison would let a commit that grows the list slip
# through as long as some later commit shrinks it back down, which is
# "shrink-only on average", not shrink-only.
if git rev-parse -q --verify HEAD > /dev/null 2>&1 \
    && git show "HEAD:$allowlist" > /dev/null 2>&1; then
  previous_allowlist="$tmp_dir/allowlist-previous.txt"
  read_allowlist "HEAD:$allowlist" | sort -u > "$previous_allowlist"
  grown="$(comm -23 "$current_allowlist" "$previous_allowlist" || true)"
  if [ -n "$grown" ]; then
    while read -r name; do
      [ -n "$name" ] || continue
      flag "$allowlist grew: $name was not exempt in the previous commit and cannot be added -- see the file's own header"
    done <<< "$grown"
  fi
fi

# The integrity check itself, per RFC-062 §7's own given shape: every
# migration file that changed since $last_tag and already existed there is
# a released file that was edited -- unless allowlisted. The allowlist
# file's own path is excluded here (its shrink-only check above is the
# rule that applies to it; it is expected to change over time).
while read -r f; do
  [ -n "$f" ] || continue
  [ "$f" = "$allowlist" ] && continue
  if git cat-file -e "${last_tag}:${f}" 2>/dev/null; then
    name="$(basename "$f")"
    if is_allowlisted "$name"; then
      continue
    fi
    flag "released migration edited since $last_tag: $f (not on the allowlist -- $allowlist)"
  fi
done < <(git diff --name-only "${last_tag}..HEAD" -- "$migrations_dir/" 2>/dev/null || true)

if [ "$fail" -eq 0 ]; then
  echo "migration-integrity gate: ok"
  exit 0
fi
echo "migration-integrity gate: failed"
exit 1
