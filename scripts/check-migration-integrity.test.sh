#!/usr/bin/env bash
# check-migration-integrity.test.sh — regression test for
# check-migration-integrity.sh (RFC-062 §7 / §8 acceptance criteria 5, 6).
#
# Runs entirely inside a throwaway git repository under a temp directory;
# touches nothing in the real repository.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
script_under_test="$repo_root/scripts/check-migration-integrity.sh"

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
  if (cd "$tmp_repo" && bash "$tmp_repo/scripts/check-migration-integrity.sh") \
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
# This machine's global config defaults every tag to signed+annotated
# (tag.gpgsign=true), which needs a real key and a message -- irrelevant
# to what this test exercises. Scoped to this throwaway repo only.
git config tag.gpgsign false
git config commit.gpgsign false

mkdir -p crates/data/db/migrations scripts
cp "$script_under_test" scripts/check-migration-integrity.sh

cat > crates/data/db/migrations/0001_baseline.sql <<'EOF'
CREATE TABLE t (id TEXT PRIMARY KEY);
EOF
cat > crates/data/db/migrations/0002_second.sql <<'EOF'
ALTER TABLE t ADD COLUMN a TEXT;
EOF

git add -A
git commit -q -m "initial"

# ── No tag yet: passes vacuously (nothing released, nothing to protect) ──
check "no release tag: passes vacuously" "pass" "$(run_gate)"

git tag v0.1.0

# ── Baseline: nothing has changed since the tag ─────────────────────────
check "unmodified tree right after tagging: passes" "pass" "$(run_gate)"

# ── A brand-new migration: must not be flagged ──────────────────────────
cat > crates/data/db/migrations/0003_third.sql <<'EOF'
ALTER TABLE t ADD COLUMN b TEXT;
EOF
git add -A
git commit -q -m "add 0003"
check "a new migration file: passes" "pass" "$(run_gate)"

# ── Editing a released migration: must be flagged ───────────────────────
cat > crates/data/db/migrations/0001_baseline.sql <<'EOF'
CREATE TABLE t (id TEXT PRIMARY KEY, extra TEXT);
EOF
git add -A
git commit -q -m "edit released 0001"
check "editing a released migration: fails" "fail" "$(run_gate)"
grep -q "released migration edited" "$tmp_repo/.gate-output" \
  || { echo "FAIL: failure message must name the violation" >&2; fail=1; }

# ── Allowlisting the edit clears the failure ────────────────────────────
cat > crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt <<'EOF'
# test fixture
0001_baseline.sql
EOF
git add -A
git commit -q -m "allowlist 0001's edit"
check "an allowlisted edit: passes" "pass" "$(run_gate)"

# ── Shrink-only, checked against HEAD~1, its immediate parent commit --
#    the same mechanism check-rfc-lifecycle.test.sh uses for
#    LEGACY-ALLOWLIST.txt, which HANDOFF-062 §2 names as this gate's own
#    model (both fixed together, Review 212 §4: comparing the index
#    against HEAD, as this used to, is a no-op on a clean CI checkout --
#    index and HEAD are the same tree there, so a *pushed* commit's own
#    growth was never actually caught, only a staged-but-uncommitted local
#    edit was). HEAD is what a push actually publishes, so that is what
#    both the edited-migration check and this shrink check validate now.
cat > crates/data/db/migrations/0004_fourth.sql <<'EOF'
ALTER TABLE t ADD COLUMN c TEXT;
EOF
git add -A
git commit -q -m "add 0004 (new, unreleased -- editing it freely is still fine here)"
git tag v0.2.0

cat > crates/data/db/migrations/0004_fourth.sql <<'EOF'
ALTER TABLE t ADD COLUMN c TEXT NOT NULL DEFAULT '';
EOF
git add -A
git commit -q -m "edit released 0004 without allowlisting it"
check "editing released 0004 with only 0001 allowlisted: fails" "fail" "$(run_gate)"
grep -q "released migration edited" "$tmp_repo/.gate-output" \
  || { echo "FAIL: failure message must name the edited-migration violation" >&2; fail=1; }

# Staging the allowlist growth *without committing* is not enough to clear
# the failure above, and not enough to trip the shrink-only check either --
# this gate only ever looks at HEAD. Documented here as a real, understood
# property of the design (a local `git add` alone gives no signal from
# this gate either way), not an oversight: the check is meant to validate
# what a push publishes, not what is merely staged.
cat > crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt <<'EOF'
# test fixture
0001_baseline.sql
0004_fourth.sql
EOF
git add crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt
check "staging (not committing) the allowlist growth still fails -- HEAD is unchanged" "fail" "$(run_gate)"
grep -q "released migration edited" "$tmp_repo/.gate-output" \
  || { echo "FAIL: staged-only growth must fail for the *edited-migration* reason (HEAD still lacks the entry), not be silently accepted" >&2; fail=1; }

# Committing the growth is what actually trips the shrink-only check --
# this is the case Review 212 found missing entirely: a *pushed* commit
# that grows the allowlist, checked against its own immediate parent.
git commit -q -m "commit the allowlist growth (covers 0004's edit, but grows the list)"
check "committing the allowlist growth: fails (shrink-only, even though it 'explains' 0004's edit)" "fail" "$(run_gate)"
grep -q "grew" "$tmp_repo/.gate-output" \
  || { echo "FAIL: failure message must name the shrink-only violation" >&2; fail=1; }

# Reverting the growth in a *new* commit is a shrink relative to the bad
# commit's parent -- the shrink-only check passes -- but 0004's edit is
# still unallowlisted, so the gate still fails, now for the other reason.
cat > crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt <<'EOF'
# test fixture
0001_baseline.sql
EOF
git add -A
git commit -q -m "revert the allowlist growth (0004's edit is still unallowlisted)"
check "reverting the growth in a new commit: still fails (0004's edit is still unallowlisted)" "fail" "$(run_gate)"
grep -q "released migration edited" "$tmp_repo/.gate-output" \
  || { echo "FAIL: failure message must name the edited-migration violation, not shrink-only (the revert itself is a shrink)" >&2; fail=1; }

# ── Self-check: confirm this test actually distinguishes gate behavior,
#    not merely that every scenario happens to pass ──────────────────────
# Replace the gate with a version that always reports success, and confirm
# the failing scenarios above would have gone undetected -- i.e. that this
# test file's own checks are load-bearing, not vacuous.
cat > scripts/check-migration-integrity.sh <<'EOF'
#!/usr/bin/env bash
echo "migration-integrity gate: ok"
exit 0
EOF
gutted_result="$(run_gate)"
if [ "$gutted_result" = "pass" ]; then
  echo "ok: a gutted gate (always succeeds) reports pass -- confirms the real \
gate's earlier 'fail' results above were the check actually firing, not \
an artifact of this harness"
else
  echo "FAIL: a gutted gate must report pass (it always exits 0) -- got $gutted_result" >&2
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  echo "check-migration-integrity.test.sh: ok"
  exit 0
fi
echo "check-migration-integrity.test.sh: failed" >&2
exit 1
