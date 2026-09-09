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

# ── Shrink-only, staged against the *previous commit* -- the same
#    methodology check-rfc-lifecycle.test.sh uses for LEGACY-ALLOWLIST.txt,
#    which HANDOFF-062 §2 names as this gate's own model. Growth is only
#    ever detectable relative to what the *immediately preceding commit*
#    already held (that comparison is what "shrink-only, forever" means);
#    within one not-yet-committed change it shows up as index vs HEAD ──
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

# Growing the allowlist to cover 0004's edit -- staged, not committed, so
# there is a "previous commit" (the one just above) to shrink-check
# against, matching check-rfc-lifecycle.test.sh's own methodology exactly.
cat > crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt <<'EOF'
# test fixture
0001_baseline.sql
0004_fourth.sql
EOF
git add crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt
check "growing the allowlist (staged) is caught even though it 'explains' 0004's edit" "fail" "$(run_gate)"
grep -q "grew" "$tmp_repo/.gate-output" \
  || { echo "FAIL: failure message must name the shrink-only violation" >&2; fail=1; }

# Reverting the staged growth makes the gate pass again (0004's edit is
# still there, still unallowlisted -- back to the state already proven to
# fail above, for the other reason).
cat > crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt <<'EOF'
# test fixture
0001_baseline.sql
EOF
git add crates/data/db/migrations/EDITED-RELEASED-ALLOWLIST.txt
check "reverting the staged growth still fails (0004's edit is still unallowlisted)" "fail" "$(run_gate)"

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
