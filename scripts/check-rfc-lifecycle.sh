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

# ── Link integrity ─────────────────────────────────────────────────────
# Required before the next RFC folder move (Review 146 §5): a folder move
# silently breaks every inbound cross-reference to the moved file, and
# nothing previously checked for that -- Task 007 Part A's own review
# found two broken links (APPENDIX-C, APPENDIX-D) this script did not
# catch. Shared resolution walk: reads a file from the git index (the same
# source of truth as every other check here), blanks fenced code blocks
# (not removes -- so line numbers in any failure message still match the
# file, and RFC-000's own seven deliberately non-existent illustrative
# example links are never flagged), and for every relative Markdown link
# target on every line, calls $handler with the file, line number, the
# target as written, and the target resolved to a repo-root-relative path.
for_each_md_link() {
  local file="$1"
  local handler="$2"
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
      "$handler" "$file" "$lineno" "$target" "$resolved"
    done < <(grep -oP '(?<=\]\()[^)]+\.md(?=[)#])' <<< "$line" 2>/dev/null || true)
  done <<< "$stripped"
}

# Every relative .md link inside an rfcs/*.md file must resolve, regardless
# of where it points -- README.md, and every state/handoffs/appendices
# subdirectory.
check_link_target() {
  local file="$1" lineno="$2" target="$3" resolved="$4"
  if [ -z "$resolved" ] || ! git show ":$resolved" > /dev/null 2>&1; then
    flag "$file:$lineno: broken relative link -> $target (resolves to ${resolved:-?})"
  fi
}

while read -r file; do
  [ -n "$file" ] || continue
  for_each_md_link "$file" check_link_target
done < <(git ls-files 'rfcs/*.md')

# Every relative link *into* rfcs/ from anywhere else in the tracked tree
# must also resolve (Review 154 §5): the check above only walks rfcs/*.md
# itself, so a stale link into rfcs/ from CHANGELOG.md, README.md, or
# docs/ was invisible to this gate -- exactly how the CHANGELOG's stale
# rfcs/accepted/ -> rfcs/done/ link and a seven-week-old off-by-one in
# docs/src/maintainers/rfcs.md both went uncaught (Review 154 §4-5).
#
# Scope is decided by the target *as written* (does it name an "rfcs/"
# path component), not by where it resolves. A wrong "../" count is
# exactly docs/src/maintainers/rfcs.md's bug: the link says "rfcs/" but
# resolves to "docs/rfcs/..." -- outside rfcs/ entirely. Filtering on the
# resolved path would have missed the very case this check exists for;
# filtering on the written target catches it regardless of where a bad
# resolution lands. Only files outside rfcs/ itself are walked here, since
# rfcs/*.md's own links -- including ones into rfcs/ -- are already fully
# covered by the check above.
check_link_into_rfcs() {
  local file="$1" lineno="$2" target="$3" resolved="$4"
  case "$target" in
    rfcs/*|*/rfcs/*) ;;
    *) return ;;
  esac
  if [ -z "$resolved" ] || ! git show ":$resolved" > /dev/null 2>&1; then
    flag "$file:$lineno: broken relative link into rfcs/ -> $target (resolves to ${resolved:-?})"
  fi
}

while read -r file; do
  [ -n "$file" ] || continue
  for_each_md_link "$file" check_link_into_rfcs
done < <(git ls-files '*.md' | grep -v '^rfcs/')

# ── Closure records (RFC-063 §6, Task 038) ──────────────────────────────
# Every rfcs/done/NNN-slug.md must have a matching rfcs/closures/NNN-slug.md
# (same basename, different directory) naming every one of its RFC's
# numbered acceptance criteria -- unless its id is on the shrink-only
# legacy allowlist, rfcs/closures/LEGACY-ALLOWLIST.txt (see that file's own
# header for why it exists and how "shrink-only" is enforced below).

legacy_allowlist="rfcs/closures/LEGACY-ALLOWLIST.txt"

# Strips both whole-line and trailing '#...' comments, prints one 3-digit
# id per line. $1 is a full git rev-path spec, e.g. ":$legacy_allowlist" or
# "HEAD:$legacy_allowlist".
read_allowlist_ids() {
  git show "$1" 2>/dev/null | sed -E 's/#.*$//; s/[[:space:]]+$//' | grep -E '^[0-9]{3}$' || true
}

current_allowlist_ids="$tmp_dir/allowlist-current.txt"
if git show ":$legacy_allowlist" > /dev/null 2>&1; then
  read_allowlist_ids ":$legacy_allowlist" | sort -u > "$current_allowlist_ids"
else
  : > "$current_allowlist_ids"
fi

# Shrink-only: the staged id set must be a subset of the immediately
# preceding commit's id set -- comparing against any earlier ancestor would
# let a commit that grows the list slip through as long as some later
# commit shrinks it back down, which is not shrink-only, it is "shrink-only
# on average". Comparing against HEAD, on every commit, forever, is what
# makes growth impossible in any single commit across all of history.
#
# No preceding committed version to compare against -- either this is the
# commit that introduces the file, or there is no HEAD yet (an empty
# repository, exercised by this gate's own self-test's very first
# baseline commit) -- passes vacuously: growth cannot be detected without
# a baseline, so there is nothing to flag.
if git rev-parse -q --verify HEAD > /dev/null 2>&1 \
    && git show "HEAD:$legacy_allowlist" > /dev/null 2>&1; then
  previous_allowlist_ids="$tmp_dir/allowlist-previous.txt"
  read_allowlist_ids "HEAD:$legacy_allowlist" | sort -u > "$previous_allowlist_ids"
  grown="$(comm -23 "$current_allowlist_ids" "$previous_allowlist_ids" || true)"
  if [ -n "$grown" ]; then
    while read -r id; do
      [ -n "$id" ] || continue
      flag "$legacy_allowlist grew: $id was not exempt in the previous commit and cannot be added -- see the file's own header"
    done <<< "$grown"
  fi
fi

# Extracts the numbered items directly under an RFC's own
# "## N. Acceptance Criteria" heading (case-insensitive on "Criteria",
# matching both spellings already in this corpus). Anchored on "##
# <digits>. Acceptance" specifically so a section merely discussing
# acceptance criteria in passing (e.g. a "why they decayed" retrospective
# heading) is never mistaken for the criteria list itself.
extract_rfc_criteria_numbers() {
  git show ":$1" 2>/dev/null | awk '
    /^## [0-9]+\. [Aa]cceptance [Cc]riteria/ { in_section = 1; next }
    in_section && /^## / { in_section = 0 }
    in_section && /^[0-9]+\. / { print }
  ' | sed -E 's/^([0-9]+)\..*/\1/'
}

# Extracts every criterion number a closure record actually names: a
# "### N. ..." heading (the prose shape rfcs/closures/037-...md
# established as the worked example) or a "| N | ..." table row (RFC-063
# §6.1's original sketch) -- either counts as "named".
extract_closure_criteria_numbers() {
  git show ":$1" 2>/dev/null \
    | grep -oE '^(### *[0-9]+\.|\| *[0-9]+ *\|)' \
    | grep -oE '[0-9]+'
}

while read -r done_file; do
  [ -n "$done_file" ] || continue
  rfc_id="$(basename "$done_file" | cut -d- -f1)"
  if grep -qxF "$rfc_id" "$current_allowlist_ids"; then
    continue
  fi
  closure_file="rfcs/closures/$(basename "$done_file")"
  if ! git show ":$closure_file" > /dev/null 2>&1; then
    flag "$done_file has no closure record ($closure_file) and is not on $legacy_allowlist (RFC-063 §6, Task 038)"
    continue
  fi
  # comm requires its inputs sorted in the plain byte/collating order it
  # checks them against -- not numeric order. `sort -n`'s 1,2,...,9,10,11
  # is *not* byte-sorted ("10" < "9" lexicographically), so any RFC with
  # 10+ criteria (e.g. RFC-045's 13) silently broke comm's comparison here
  # until this was caught by mutation-testing this exact check against a
  # 13-criterion closure record.
  rfc_criteria="$tmp_dir/rfc-criteria-$rfc_id.txt"
  extract_rfc_criteria_numbers "$done_file" | sort -u > "$rfc_criteria"
  if [ ! -s "$rfc_criteria" ]; then
    flag "$done_file: could not find a '## N. Acceptance Criteria' section to check $closure_file against"
    continue
  fi
  closure_criteria="$tmp_dir/closure-criteria-$rfc_id.txt"
  extract_closure_criteria_numbers "$closure_file" | sort -u > "$closure_criteria"
  missing="$(comm -23 "$rfc_criteria" "$closure_criteria" || true)"
  if [ -n "$missing" ]; then
    # Numeric order for the message only -- comm's own inputs above stay
    # byte-sorted; this is purely for a readable "missing: 2 8 12" list.
    flag "$closure_file does not name every acceptance criterion in $done_file -- missing: $(sort -n <<< "$missing" | tr '\n' ' ')"
  fi
done < <(git ls-files 'rfcs/done/*.md')

if [ "$fail" -ne 0 ]; then
  echo "rfc lifecycle gate: failed" >&2
  exit 1
fi

echo "rfc lifecycle gate: ok"
