#!/usr/bin/env bash
# check-rfc-lifecycle.test.sh — regression test for check-rfc-lifecycle.sh.
#
# Reproduces the exact incident that motivated reading from the git index
# instead of the working tree (Review 126 §4): stage a rename of an RFC
# into rfcs/done/ while its Status field, as staged, still reads Proposed,
# then edit the working-tree copy to say Implemented *without* re-staging
# it. A script that reads the working tree passes here; a script that
# reads the index correctly fails, because that is what would actually be
# committed.
#
# Runs entirely inside a throwaway git repository under a temp directory;
# touches nothing in the real repository.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
script_under_test="$repo_root/scripts/check-rfc-lifecycle.sh"

tmp_repo="$(mktemp -d)"
trap 'rm -rf "$tmp_repo"' EXIT

fail=0
check() {
  local desc="$1"
  local expected="$2" # "pass" or "fail"
  local actual="$3"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $desc (expected $expected, got $actual)" >&2
    fail=1
  else
    echo "ok: $desc"
  fi
}

run_gate() {
  # Runs the script under test against $tmp_repo and echoes "pass"/"fail".
  if (cd "$tmp_repo" && bash "$tmp_repo/scripts/check-rfc-lifecycle.sh") \
      >"$tmp_repo/.gate-output" 2>&1; then
    echo "pass"
  else
    echo "fail"
  fi
}

cd "$tmp_repo"
git init -q
git config user.email "test@example.invalid"
git config user.name "Test"

mkdir -p rfcs/done rfcs/proposed rfcs/archive scripts
cp "$script_under_test" scripts/check-rfc-lifecycle.sh

cat > rfcs/proposed/001-test.md <<'EOF'
# RFC-001: Test

**Status:** Proposed
EOF

cat > rfcs/README.md <<'EOF'
# orbok RFC Index

## Implemented

| ID | Title | Release |
|---|---|---|

## Proposed

| ID | Title | Status |
|---|---|---|
| 001 | [Test](proposed/001-test.md) | Proposed |

## Archive

| ID | Title | Reason |
|---|---|---|
EOF

git add -A
git commit -q -m "initial: RFC-001 proposed"

# Baseline: the freshly committed, consistent state passes.
baseline_result="$(run_gate)"
check "clean initial commit passes" "pass" "$baseline_result"

# Reproduce the incident: stage the move (old content), then edit the
# working tree only.
git mv rfcs/proposed/001-test.md rfcs/done/001-test.md

cat > rfcs/README.md <<'EOF'
# orbok RFC Index

## Implemented

| ID | Title | Release |
|---|---|---|
| 001 | [Test](done/001-test.md) | main at `deadbeef`; release pending |

## Proposed

None. All RFCs through RFC-001 have graduated to Implemented.

## Archive

| ID | Title | Reason |
|---|---|---|
EOF
git add rfcs/README.md

cat > rfcs/done/001-test.md <<'EOF'
# RFC-001: Test

**Status:** Implemented (main at `deadbeef`; release pending)
EOF
# Deliberately not staged: this is the exact gap that produced afc70d5.

mismatch_result="$(run_gate)"
check "staged-Proposed / working-tree-Implemented mismatch is caught" "fail" "$mismatch_result"
if [ "$mismatch_result" = "fail" ]; then
  if grep -q "status does not match folder" "$tmp_repo/.gate-output"; then
    echo "ok: failure reason names the status mismatch"
  else
    echo "FAIL: gate failed, but not for the status-mismatch reason:" >&2
    cat "$tmp_repo/.gate-output" >&2
    fail=1
  fi
fi

# Now stage the correction and confirm the gate agrees the commit is valid.
git add rfcs/done/001-test.md
fixed_result="$(run_gate)"
check "staging the correction makes the gate pass again" "pass" "$fixed_result"

if [ "$fail" -ne 0 ]; then
  echo "check-rfc-lifecycle.test.sh: FAILED" >&2
  exit 1
fi
echo "check-rfc-lifecycle.test.sh: ok"
