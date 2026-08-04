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
# Also covers the 5-folder variant's accepted/ state (Task 007 Part A):
# the same staged/working-tree gap, reproduced for a proposed -> accepted
# move, proving the new folder is checked with the same rigor as done/.
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

mkdir -p rfcs/done rfcs/proposed rfcs/accepted rfcs/archive scripts
cp "$script_under_test" scripts/check-rfc-lifecycle.sh

cat > rfcs/proposed/001-test.md <<'EOF'
# RFC-001: Test

**Status:** Proposed
EOF

cat > rfcs/proposed/002-test.md <<'EOF'
# RFC-002: Test Two

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
| 002 | [Test Two](proposed/002-test.md) | Proposed |

## Accepted

| ID | Title | Status |
|---|---|---|

## Archive

| ID | Title | Reason |
|---|---|---|
EOF

git add -A
git commit -q -m "initial: RFC-001 and RFC-002 proposed"

# Baseline: the freshly committed, consistent state passes.
baseline_result="$(run_gate)"
check "clean initial commit passes" "pass" "$baseline_result"

# ── proposed -> done, staged/working-tree mismatch (the original incident) ─

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

| ID | Title | Status |
|---|---|---|
| 002 | [Test Two](proposed/002-test.md) | Proposed |

## Accepted

| ID | Title | Status |
|---|---|---|

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

# ── proposed -> accepted, the same staged/working-tree mismatch (Task 007) ─
# Same reproduction, applied to the 5-folder variant's new state, proving
# accepted/ is checked with the same rigor as done/ rather than merely
# added to the folder-existence check.

git mv rfcs/proposed/002-test.md rfcs/accepted/002-test.md

cat > rfcs/README.md <<'EOF'
# orbok RFC Index

## Implemented

| ID | Title | Release |
|---|---|---|
| 001 | [Test](done/001-test.md) | main at `deadbeef`; release pending |

## Proposed

None. All RFCs through RFC-002 have graduated to Accepted or Implemented.

## Accepted

| ID | Title | Status |
|---|---|---|
| 002 | [Test Two](accepted/002-test.md) | Accepted |

## Archive

| ID | Title | Reason |
|---|---|---|
EOF
git add rfcs/README.md

cat > rfcs/accepted/002-test.md <<'EOF'
# RFC-002: Test Two

**Status:** Accepted
EOF
# Deliberately not staged, mirroring the RFC-001 case above.

accepted_mismatch_result="$(run_gate)"
check "staged-Proposed / working-tree-Accepted mismatch is caught" "fail" "$accepted_mismatch_result"
if [ "$accepted_mismatch_result" = "fail" ]; then
  if grep -q "status does not match folder" "$tmp_repo/.gate-output"; then
    echo "ok: accepted/ failure reason names the status mismatch"
  else
    echo "FAIL: gate failed, but not for the status-mismatch reason:" >&2
    cat "$tmp_repo/.gate-output" >&2
    fail=1
  fi
fi

# Now stage the correction and confirm the gate agrees the commit is valid.
git add rfcs/accepted/002-test.md
accepted_fixed_result="$(run_gate)"
check "staging the accepted/ correction makes the gate pass again" "pass" "$accepted_fixed_result"

# ── Link integrity: a broken relative link is caught; a fenced example ────
# link is not (Review 146 §5). Reuses the now-consistent RFC-001/RFC-002
# state above as the baseline.

cat > rfcs/done/001-test.md <<'EOF'
# RFC-001: Test

**Status:** Implemented (main at `deadbeef`; release pending)

See [nonexistent](../done/999-does-not-exist.md) for details.

Illustrative only, not a real cross-reference -- must not be flagged:

```markdown
See [RFC 010](../done/010-revoke-tokens.md) for the prior work.
```
EOF
git add rfcs/done/001-test.md

broken_link_result="$(run_gate)"
check "a broken relative .md link is caught" "fail" "$broken_link_result"
if [ "$broken_link_result" = "fail" ]; then
  if grep -q "broken relative link" "$tmp_repo/.gate-output"; then
    echo "ok: failure reason names the broken link"
  else
    echo "FAIL: gate failed, but not for the broken-link reason:" >&2
    cat "$tmp_repo/.gate-output" >&2
    fail=1
  fi
  if grep -q "010-revoke-tokens" "$tmp_repo/.gate-output"; then
    echo "FAIL: the fenced illustrative link was flagged; fence-skipping is broken" >&2
    fail=1
  else
    echo "ok: the fenced illustrative link is not flagged"
  fi
fi

# Remove the broken link (keep the fenced illustrative one) and confirm
# the gate agrees the commit is valid again.
cat > rfcs/done/001-test.md <<'EOF'
# RFC-001: Test

**Status:** Implemented (main at `deadbeef`; release pending)

Illustrative only, not a real cross-reference -- must not be flagged:

```markdown
See [RFC 010](../done/010-revoke-tokens.md) for the prior work.
```
EOF
git add rfcs/done/001-test.md
link_fixed_result="$(run_gate)"
check "removing the broken link makes the gate pass again" "pass" "$link_fixed_result"

if [ "$fail" -ne 0 ]; then
  echo "check-rfc-lifecycle.test.sh: FAILED" >&2
  exit 1
fi
echo "check-rfc-lifecycle.test.sh: ok"
