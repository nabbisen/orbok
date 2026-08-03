#!/usr/bin/env bash
# check-design-tokens.test.sh — regression test for check-design-tokens.sh.
#
# Sources the checker to reuse check_tokens() directly against fixture
# files (scripts/fixtures/design-tokens/), proving both directions: the
# clean fixture produces zero findings, and the violation fixture produces
# exactly one finding per RFC-052 §5 category (font size, padding, array
# padding, spacing, radius, colour).
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# shellcheck disable=SC1091
source scripts/check-design-tokens.sh

test_fail=0
check() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $desc (expected $expected, got $actual)" >&2
    test_fail=1
  else
    echo "ok: $desc"
  fi
}

clean_out="$(check_tokens scripts/fixtures/design-tokens/clean.rs || true)"
clean_findings="$(echo "$clean_out" | grep -c "^design-token gate:" || true)"
check "clean fixture produces zero findings" "0" "$clean_findings"
if [ "$clean_findings" != "0" ]; then
  echo "$clean_out" >&2
fi

violation_out="$(check_tokens scripts/fixtures/design-tokens/violation.rs || true)"
violation_findings="$(echo "$violation_out" | grep -c "^design-token gate:" || true)"
check "violation fixture produces exactly six findings (one per category)" "6" "$violation_findings"
if [ "$violation_findings" != "6" ]; then
  echo "$violation_out" >&2
fi

for expected_substr in \
  'literal text size' \
  'literal padding' \
  'literal array padding' \
  'literal spacing' \
  'literal radius' \
  'literal colour'
do
  if echo "$violation_out" | grep -qF "$expected_substr"; then
    echo "ok: violation fixture reports $expected_substr"
  else
    echo "FAIL: violation fixture did not report $expected_substr" >&2
    test_fail=1
  fi
done

if [ "$test_fail" -ne 0 ]; then
  echo "check-design-tokens.test.sh: FAILED" >&2
  exit 1
fi
echo "check-design-tokens.test.sh: ok"
