#!/usr/bin/env bash
# check-i18n-literals.test.sh — regression test for check-i18n-literals.sh.
#
# Sources the checker to reuse its check_file()/is_allowlisted() functions
# directly against fixture files (scripts/fixtures/i18n-literals/), rather
# than the real repository's discovery roots. Proves both directions: the
# clean fixture produces zero findings, and the violation fixture produces
# exactly one finding per planted pattern plus zero for the allowlisted
# case (proving suppression, not just pattern-matching).
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# shellcheck disable=SC1091
source scripts/check-i18n-literals.sh

fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $desc (expected $expected, got $actual)" >&2
    fail=1
  else
    echo "ok: $desc"
  fi
}

fixture_allowlist="scripts/fixtures/i18n-literals/allowlist.txt"

# check_file() prints one "i18n-literal gate: ..." line per finding (via
# flag(), a boolean not a counter) — count findings from stdout, not $fail.

# ── Clean fixture: zero findings ──────────────────────────────────────────
clean_out="$(check_file "$fixture_allowlist" "scripts/fixtures/i18n-literals/clean.rs")"
clean_findings="$(echo "$clean_out" | grep -c "^i18n-literal gate:" || true)"
check "clean fixture produces zero findings" "0" "$clean_findings"
if [ "$clean_findings" != "0" ]; then
  echo "$clean_out" >&2
fi

# ── Violation fixture: exactly one finding per planted pattern ───────────
violation_out="$(check_file "$fixture_allowlist" "scripts/fixtures/i18n-literals/violation.rs")"
violation_findings="$(echo "$violation_out" | grep -c "^i18n-literal gate:" || true)"
check "violation fixture produces exactly seven findings (one per planted pattern)" "7" "$violation_findings"
if [ "$violation_findings" != "7" ]; then
  echo "$violation_out" >&2
fi

for expected_substr in \
  'text-rendering call: "Searching' \
  'text-rendering call: "Or type a path manually' \
  '.set_title(): "Select folder to search"' \
  'job_progress(): "Indexing' \
  'unwrap_or fallback: "(source unavailable)"' \
  'multi-word literal: "Search did not finish. Please try again."' \
  '"{}: {value}" label/value concatenation'
do
  if echo "$violation_out" | grep -qF "$expected_substr"; then
    echo "ok: violation fixture reports $expected_substr"
  else
    echo "FAIL: violation fixture did not report $expected_substr" >&2
    fail=1
  fi
done

# The allowlisted literal must NOT appear as a finding.
if echo "$violation_out" | grep -q "reviewed default-model file sizes"; then
  echo "FAIL: allowlisted literal was flagged despite being in the allowlist" >&2
  fail=1
else
  echo "ok: allowlisted literal is correctly suppressed"
fi

# ── Discovery/classification exhaustiveness (RFC-052 §6, Review 141 §3(c)) ─
# classify_file() is a pure function of SCAN_FILES/EXCLUDED_FILES, so these
# exercise the exact gap the review found without needing a throwaway git
# repository or a real planted file.

check "known scan file classifies as scan" "scan" "$(classify_file crates/ui/src/views.rs)"
check "known excluded file classifies as exclude" "exclude" "$(classify_file crates/ui/src/theme.rs)"
check "a hypothetical new top-level ui file classifies as unclassified" \
  "unclassified" "$(classify_file crates/ui/src/panels.rs)"
check "a hypothetical new top-level app file classifies as unclassified" \
  "unclassified" "$(classify_file crates/app/src/tray.rs)"
check "a file under a non-designated subdirectory classifies as unclassified" \
  "unclassified" "$(classify_file crates/app/src/bootstrap/search.rs)"

# ── Discovery exhaustiveness against the real repository ─────────────────
# Every file discover_files() actually returns today must be classified —
# proves SCAN_FILES + EXCLUDED_FILES cover the live discovery set exactly,
# not just that classify_file() has correct logic for cases chosen by hand.
discovered_files="$(discover_files)"
unclassified_count=0
while IFS= read -r f; do
  [ -n "$f" ] || continue
  if [ "$(classify_file "$f")" = "unclassified" ]; then
    echo "FAIL: discovered file has no classification: $f" >&2
    unclassified_count=$((unclassified_count + 1))
  fi
done <<<"$discovered_files"
check "every file discover_files() returns today is classified" "0" "$unclassified_count"

# Sanity check the other direction too: SCAN_FILES/EXCLUDED_FILES must not
# silently reference a path discovery no longer returns (a deleted or
# renamed file drifting out of sync in the other direction).
stale_count=0
for f in "${SCAN_FILES[@]}" "${!EXCLUDED_FILES[@]}"; do
  if ! echo "$discovered_files" | grep -qxF "$f"; then
    echo "FAIL: classified file no longer exists / not discovered: $f" >&2
    stale_count=$((stale_count + 1))
  fi
done
check "every classified file is still discovered (no stale entries)" "0" "$stale_count"

if [ "$fail" -ne 0 ]; then
  echo "check-i18n-literals.test.sh: FAILED" >&2
  exit 1
fi
echo "check-i18n-literals.test.sh: ok"
