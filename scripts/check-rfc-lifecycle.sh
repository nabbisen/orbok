#!/usr/bin/env bash
# check-rfc-lifecycle.sh — RFC index/folder/status consistency gate.
#
# Source of truth: the git INDEX, not the working tree. Every file this
# script inspects is read via `git show ":<path>"` (the staged blob), and
# every enumeration walks `git ls-files`, not a shell glob over disk. This
# is deliberate: the point of this gate is "does the state about to be
# committed satisfy the invariants", and a working-tree read can pass on
# content that was edited but never re-staged (an uncommitted rename can
# carry stale content this way — see the incident this comment was added
# for). Reading from the index also means the gate can catch a problem
# before commit, at `git add` time, rather than only in CI after the fact.
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0
flag() {
  echo "rfc lifecycle gate: $1" >&2
  fail=1
}

require_dir() {
  if [ ! -d "$1" ]; then
    flag "missing directory: $1"
  fi
}

require_dir rfcs/done
require_dir rfcs/proposed
require_dir rfcs/accepted
require_dir rfcs/archive

read_status() {
  local path="$1"
  git show ":$path" 2>/dev/null | grep -m1 '^\*\*Status:\*\*' || true
}

# rfcs/README.md itself is read from the index too, for the same reason.
readme_index="$(git show ":rfcs/README.md" 2>/dev/null || true)"

check_status_prefix() {
  local file="$1"
  local expected="$2"
  local status
  status="$(read_status "$file")"
  if [ -z "$status" ]; then
    flag "$file has no Status field"
    return
  fi
  if [[ "$status" != "$expected"* ]]; then
    flag "$file status does not match folder: $status"
  fi
}

check_id_prefix() {
  local file="$1"
  local id
  id="$(basename "$file" | cut -d- -f1)"
  if [[ ! "$id" =~ ^[0-9]{3}$ ]]; then
    flag "$file does not start with a three-digit RFC id"
  fi
}

while read -r file; do
  [ -n "$file" ] || continue
  check_id_prefix "$file"
  check_status_prefix "$file" "**Status:** Implemented"
done < <(git ls-files 'rfcs/done/*.md')

while read -r file; do
  [ -n "$file" ] || continue
  check_id_prefix "$file"
  check_status_prefix "$file" "**Status:** Proposed"
done < <(git ls-files 'rfcs/proposed/*.md')

while read -r file; do
  [ -n "$file" ] || continue
  check_id_prefix "$file"
  check_status_prefix "$file" "**Status:** Accepted"
done < <(git ls-files 'rfcs/accepted/*.md')

while read -r file; do
  [ -n "$file" ] || continue
  check_id_prefix "$file"
  status="$(read_status "$file")"
  if [ -z "$status" ]; then
    flag "$file has no Status field"
  elif [[ "$status" != "**Status:** Withdrawn"* && "$status" != "**Status:** Superseded"* ]]; then
    flag "$file status does not match archive folder: $status"
  fi
done < <(git ls-files 'rfcs/archive/*.md')

mkdir -p target
tmp_dir="$(mktemp -d target/rfc-lifecycle.XXXXXX)"
trap 'rm -rf "$tmp_dir"' EXIT

{ grep -E '^\| [0-9]{3} \| .*]\(done/[^)]+\.md\) \|' <<< "$readme_index" || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((done\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-done.txt"
{ grep -E '^\| [0-9]{3} \| .*]\(proposed/[^)]+\.md\) \|' <<< "$readme_index" || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((proposed\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-proposed.txt"
{ grep -E '^\| [0-9]{3} \| .*]\(accepted/[^)]+\.md\) \|' <<< "$readme_index" || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((accepted\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-accepted.txt"
{ grep -E '^\| [0-9]{3} \| .*]\(archive/[^)]+\.md\) \|' <<< "$readme_index" || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((archive\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-archive.txt"

tracked_rfc_paths="$tmp_dir/tracked-rfc-paths.txt"
{
  git ls-files 'rfcs/done/*.md'
  git ls-files 'rfcs/proposed/*.md'
  git ls-files 'rfcs/accepted/*.md'
  git ls-files 'rfcs/archive/*.md'
} | sed 's#^rfcs/##' | sort > "$tracked_rfc_paths"

check_index_entries() {
  local list="$1"
  local file_id
  while read -r id path; do
    [ -n "${id:-}" ] || continue
    if ! grep -qxF "$path" "$tracked_rfc_paths"; then
      flag "rfcs/README.md links missing RFC file: $path"
      continue
    fi
    file_id="$(basename "$path" | cut -d- -f1)"
    if [ "$id" != "$file_id" ]; then
      flag "rfcs/README.md id $id does not match linked file $path"
    fi
  done < "$list"
}

check_index_entries "$tmp_dir/index-done.txt"
check_index_entries "$tmp_dir/index-proposed.txt"
check_index_entries "$tmp_dir/index-accepted.txt"
check_index_entries "$tmp_dir/index-archive.txt"

index_paths="$tmp_dir/index-paths.txt"
cut -d' ' -f2 "$tmp_dir/index-done.txt" > "$index_paths"
cut -d' ' -f2 "$tmp_dir/index-proposed.txt" >> "$index_paths"
cut -d' ' -f2 "$tmp_dir/index-accepted.txt" >> "$index_paths"
cut -d' ' -f2 "$tmp_dir/index-archive.txt" >> "$index_paths"
sort "$index_paths" > "$tmp_dir/index-paths.sorted"

while read -r path; do
  [ -n "$path" ] || continue
  if ! grep -qxF "$path" "$tmp_dir/index-paths.sorted"; then
    flag "tracked RFC file missing from rfcs/README.md: $path"
  fi
done < "$tracked_rfc_paths"

file_ids="$tmp_dir/file-ids.txt"
git ls-files 'rfcs/done/*.md' 'rfcs/proposed/*.md' 'rfcs/accepted/*.md' 'rfcs/archive/*.md' \
  | xargs -r -n1 basename \
  | cut -d- -f1 \
  | grep -E '^[0-9]{3}$' \
  | sort > "$file_ids"

index_ids="$tmp_dir/index-ids.txt"
{
  cut -d' ' -f1 "$tmp_dir/index-done.txt"
  cut -d' ' -f1 "$tmp_dir/index-proposed.txt"
  cut -d' ' -f1 "$tmp_dir/index-accepted.txt"
  cut -d' ' -f1 "$tmp_dir/index-archive.txt"
} | grep -E '^[0-9]{3}$' | sort > "$index_ids"

file_duplicates="$(uniq -d "$file_ids" || true)"
if [ -n "$file_duplicates" ]; then
  while read -r duplicate; do
    [ -n "$duplicate" ] && flag "duplicate RFC file id: $duplicate"
  done <<< "$file_duplicates"
fi

index_duplicates="$(uniq -d "$index_ids" || true)"
if [ -n "$index_duplicates" ]; then
  while read -r duplicate; do
    [ -n "$duplicate" ] && flag "duplicate RFC index id: $duplicate"
  done <<< "$index_duplicates"
fi

# This "None" check exists only for rfcs/proposed/ (Review 146 §3), and
# deliberately has no accepted/done/archive equivalent. It exists because
# the Proposed section, when empty, uses a prose sentinel ("None. All
# RFCs through …") that the table-row parsing above cannot see — an
# empty Proposed table alone can't distinguish "correctly empty" from
# "README just wasn't updated." accepted/ (and every other folder) has
# no such prose sentinel: the bidirectional index<->folder equality
# already checked above (index-accepted.txt vs. tracked-rfc-paths, both
# directions) fully covers accepted/'s empty case the same as its
# non-empty case, so an analogous "None" check here would have nothing
# to guard. Do not add one solely because accepted/ later becomes empty
# in practice (e.g. once 048/052/054 all reach done/) -- emptiness alone
# is not the trigger; the missing prose sentinel is.
if git ls-files 'rfcs/proposed/*.md' | grep -q .; then
  if grep -q '^None\. All RFCs through' <<< "$readme_index"; then
    flag "rfcs/README.md says Proposed is None but proposed RFC files exist"
  fi
  if [ ! -s "$tmp_dir/index-proposed.txt" ]; then
    flag "proposed RFC files exist but rfcs/README.md has no proposed RFC index rows"
  fi
else
  if ! grep -q '^None\. All RFCs through' <<< "$readme_index"; then
    flag "rfcs/README.md Proposed section does not say None"
  fi
  if [ -s "$tmp_dir/index-proposed.txt" ]; then
    flag "rfcs/README.md has proposed RFC index rows but no proposed RFC files exist"
  fi
fi

# ── Link integrity: every relative .md link under rfcs/ must resolve ──────
# Required before the next RFC folder move (Review 146 §5): a folder move
# silently breaks every inbound cross-reference to the moved file, and
# nothing previously checked for that -- Task 007 Part A's own review
# found two broken links (APPENDIX-C, APPENDIX-D) this script did not
# catch. Resolves every relative Markdown link inside every tracked
# rfcs/*.md file (README.md, and every state/handoffs/appendices
# subdirectory) against the git index, the same source of truth as every
# other check here. Fenced code blocks are blanked (not removed, so line
# numbers in any failure message still match the file) before link
# extraction, so RFC-000's own seven deliberately non-existent
# illustrative example links are never flagged.
check_links_in_file() {
  local file="$1"
  local dir
  dir="$(dirname "$file")"
  local content
  content="$(git show ":$file" 2>/dev/null || true)"
  local stripped
  stripped="$(awk '
    /^[[:space:]]*```/ { infence = !infence; print ""; next }
    infence { print ""; next }
    { print }
  ' <<< "$content")"

  local lineno=0
  local line target resolved
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    while read -r target; do
      [ -n "$target" ] || continue
      case "$target" in
        http://*|https://*|mailto:*) continue ;;
      esac
      target="${target%%#*}"
      [ -n "$target" ] || continue
      resolved="$(realpath -m --relative-to=. "$dir/$target" 2>/dev/null || true)"
      if [ -z "$resolved" ] || ! git show ":$resolved" > /dev/null 2>&1; then
        flag "$file:$lineno: broken relative link -> $target (resolves to ${resolved:-?})"
      fi
    done < <(grep -oP '(?<=\]\()[^)]+\.md(?=[)#])' <<< "$line" 2>/dev/null || true)
  done <<< "$stripped"
}

while read -r file; do
  [ -n "$file" ] || continue
  check_links_in_file "$file"
done < <(git ls-files 'rfcs/*.md')

if [ "$fail" -ne 0 ]; then
  echo "rfc lifecycle gate: failed" >&2
  exit 1
fi

echo "rfc lifecycle gate: ok"
