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
if [ "$violation_findings" != "6" ]; then
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

if [ "$fail" -ne 0 ]; then
  echo "check-i18n-literals.test.sh: FAILED" >&2
  exit 1
fi
echo "check-i18n-literals.test.sh: ok"
