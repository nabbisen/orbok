#!/usr/bin/env bash
# check-rfc-lifecycle.sh — RFC index/folder/status consistency gate.
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
require_dir rfcs/archive

check_status_prefix() {
  local file="$1"
  local expected="$2"
  local status
  status="$(grep -m1 '^\*\*Status:\*\*' "$file" || true)"
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

for file in rfcs/done/*.md; do
  [ -e "$file" ] || continue
  check_id_prefix "$file"
  check_status_prefix "$file" "**Status:** Implemented"
done

for file in rfcs/proposed/*.md; do
  [ -e "$file" ] || continue
  check_id_prefix "$file"
  check_status_prefix "$file" "**Status:** Proposed"
done

for file in rfcs/archive/*.md; do
  [ -e "$file" ] || continue
  check_id_prefix "$file"
  status="$(grep -m1 '^\*\*Status:\*\*' "$file" || true)"
  if [ -z "$status" ]; then
    flag "$file has no Status field"
  elif [[ "$status" != "**Status:** Withdrawn"* && "$status" != "**Status:** Superseded"* ]]; then
    flag "$file status does not match archive folder: $status"
  fi
done

mkdir -p target
tmp_dir="$(mktemp -d target/rfc-lifecycle.XXXXXX)"
trap 'rm -rf "$tmp_dir"' EXIT

{ grep -E '^\| [0-9]{3} \| .*]\(done/[^)]+\.md\) \|' rfcs/README.md || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((done\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-done.txt"
{ grep -E '^\| [0-9]{3} \| .*]\(proposed/[^)]+\.md\) \|' rfcs/README.md || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((proposed\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-proposed.txt"
{ grep -E '^\| [0-9]{3} \| .*]\(archive/[^)]+\.md\) \|' rfcs/README.md || true; } \
  | sed -E 's/^\| ([0-9]{3}) \| .*]\((archive\/[^)]+\.md)\) \|.*$/\1 \2/' \
  > "$tmp_dir/index-archive.txt"

check_index_entries() {
  local list="$1"
  local file_id
  while read -r id path; do
    [ -n "${id:-}" ] || continue
    if [ ! -f "rfcs/$path" ]; then
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
check_index_entries "$tmp_dir/index-archive.txt"

index_paths="$tmp_dir/index-paths.txt"
cut -d' ' -f2 "$tmp_dir/index-done.txt" > "$index_paths"
cut -d' ' -f2 "$tmp_dir/index-proposed.txt" >> "$index_paths"
cut -d' ' -f2 "$tmp_dir/index-archive.txt" >> "$index_paths"
sort "$index_paths" > "$tmp_dir/index-paths.sorted"

tracked_rfc_paths="$tmp_dir/tracked-rfc-paths.txt"
{
  git ls-files 'rfcs/done/*.md'
  git ls-files 'rfcs/proposed/*.md'
  git ls-files 'rfcs/archive/*.md'
} | sed 's#^rfcs/##' | sort > "$tracked_rfc_paths"

while read -r path; do
  [ -n "$path" ] || continue
  if ! grep -qxF "$path" "$tmp_dir/index-paths.sorted"; then
    flag "tracked RFC file missing from rfcs/README.md: $path"
  fi
done < "$tracked_rfc_paths"

file_ids="$tmp_dir/file-ids.txt"
git ls-files 'rfcs/done/*.md' 'rfcs/proposed/*.md' 'rfcs/archive/*.md' \
  | xargs -r -n1 basename \
  | cut -d- -f1 \
  | grep -E '^[0-9]{3}$' \
  | sort > "$file_ids"

index_ids="$tmp_dir/index-ids.txt"
{
  cut -d' ' -f1 "$tmp_dir/index-done.txt"
  cut -d' ' -f1 "$tmp_dir/index-proposed.txt"
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

if find rfcs/proposed -maxdepth 1 -type f -name '*.md' | grep -q .; then
  if grep -q '^None\. All RFCs through' rfcs/README.md; then
    flag "rfcs/README.md says Proposed is None but proposed RFC files exist"
  fi
  if [ ! -s "$tmp_dir/index-proposed.txt" ]; then
    flag "proposed RFC files exist but rfcs/README.md has no proposed RFC index rows"
  fi
else
  if ! grep -q '^None\. All RFCs through' rfcs/README.md; then
    flag "rfcs/README.md Proposed section does not say None"
  fi
  if [ -s "$tmp_dir/index-proposed.txt" ]; then
    flag "rfcs/README.md has proposed RFC index rows but no proposed RFC files exist"
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "rfc lifecycle gate: failed" >&2
  exit 1
fi

echo "rfc lifecycle gate: ok"
