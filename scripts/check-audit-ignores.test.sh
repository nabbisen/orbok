#!/usr/bin/env bash
# check-audit-ignores.test.sh — regression test for check-audit-ignores.sh
# (Task 044 §2). Runs against copies of the real audit.toml, ci.yml and
# Cargo.lock in a temp directory; touches nothing in the real repository.
# Needs cargo-audit and a local advisory DB (the gate runs --no-fetch).
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

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

reset_fixture() {
  rm -rf "$tmp/work"
  mkdir -p "$tmp/work/.cargo" "$tmp/work/.github/workflows" "$tmp/work/scripts"
  cp "$repo_root/.cargo/audit.toml" "$tmp/work/.cargo/audit.toml"
  cp "$repo_root/.github/workflows/ci.yml" "$tmp/work/.github/workflows/ci.yml"
  cp "$repo_root/Cargo.lock" "$tmp/work/Cargo.lock"
  cp "${GATE_UNDER_TEST:-$repo_root/scripts/check-audit-ignores.sh}" "$tmp/work/scripts/check-audit-ignores.sh"
}

run_gate() {
  if (cd "$tmp/work" && bash scripts/check-audit-ignores.sh) >"$tmp/out" 2>&1; then
    echo "pass"
  else
    echo "fail"
  fi
}

add_ignore() {
  awk -v id="$1" '/^\]/ { print "    \"" id "\","; } { print }' \
    "$tmp/work/.cargo/audit.toml" >"$tmp/audit.toml" \
    && mv "$tmp/audit.toml" "$tmp/work/.cargo/audit.toml"
}

expect_message() {
  grep -qF -- "$1" "$tmp/out" \
    || { echo "FAIL: gate output must mention '$1'; got:" >&2; cat "$tmp/out" >&2; fail=1; }
}

# (a) The real list passes.
reset_fixture
check "(a) the real ignore list passes" "pass" "$(run_gate)"

# (b) A made-up advisory id fails, naming it.
reset_fixture
add_ignore "RUSTSEC-2099-0001"
check "(b) an ignore for a made-up id fails" "fail" "$(run_gate)"
expect_message "RUSTSEC-2099-0001"

# (c) A real advisory whose crate is not in the lockfile fails, naming it.
reset_fixture
grep -q '^name = "rustybuzz"' "$tmp/work/Cargo.lock" \
  && { echo "FAIL: (c) premise broken -- rustybuzz is now in Cargo.lock; pick another absent-crate advisory" >&2; fail=1; }
add_ignore "RUSTSEC-2026-0206"
check "(c) an ignore for a real advisory on an absent crate fails" "fail" "$(run_gate)"
expect_message "RUSTSEC-2026-0206"

# (d) ci.yml without --deny warnings fails.
reset_fixture
sed -i 's/cargo audit --deny warnings/cargo audit/' "$tmp/work/.github/workflows/ci.yml"
grep -q -- '--deny warnings' "$tmp/work/.github/workflows/ci.yml" \
  && { echo "FAIL: (d) fixture still carries --deny warnings" >&2; fail=1; }
check "(d) the audit step without --deny warnings fails" "fail" "$(run_gate)"
expect_message "--deny warnings"

# (e) A narrowed informational_warnings fails.
reset_fixture
sed -i 's/^\[advisories\]$/[advisories]\ninformational_warnings = ["unmaintained"]/' "$tmp/work/.cargo/audit.toml"
check "(e) informational_warnings without \"unsound\" fails" "fail" "$(run_gate)"
expect_message "unsound"

# Self-check: a gutted gate must pass every case above, proving the
# 'fail' results were the real gate firing, not this harness.
if [ -z "${GATE_UNDER_TEST:-}" ]; then
  gutted="$tmp/gutted.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$gutted"
  if GATE_UNDER_TEST="$gutted" bash "$0" >"$tmp/gutted-out" 2>&1; then
    echo "FAIL: this self-test passed against a gutted gate -- its checks are vacuous" >&2
    fail=1
  else
    echo "ok: the self-test goes red against a gutted gate"
  fi
fi

if [ "$fail" -eq 0 ]; then
  echo "check-audit-ignores.test.sh: ok"
  exit 0
fi
echo "check-audit-ignores.test.sh: failed" >&2
exit 1
