#!/usr/bin/env bash
# check-pkgbuild.test.sh — regression test for check-pkgbuild.sh (Task 129
# §1.5). Runs against copies of the real Cargo.toml and PKGBUILD in a temp
# directory; touches nothing in the real repository.
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
  mkdir -p "$tmp/work/packaging/linux" "$tmp/work/scripts"
  cp "$repo_root/Cargo.toml" "$tmp/work/Cargo.toml"
  cp "$repo_root/packaging/linux/PKGBUILD" "$tmp/work/packaging/linux/PKGBUILD"
  cp "${GATE_UNDER_TEST:-$repo_root/scripts/check-pkgbuild.sh}" "$tmp/work/scripts/check-pkgbuild.sh"
}

run_gate() {
  if (cd "$tmp/work" && bash scripts/check-pkgbuild.sh) >"$tmp/out" 2>&1; then
    echo "pass"
  else
    echo "fail"
  fi
}

expect_message() {
  grep -qF -- "$1" "$tmp/out" \
    || { echo "FAIL: gate output must mention '$1'; got:" >&2; cat "$tmp/out" >&2; fail=1; }
}

workspace_version="$(awk '/^\[/ { s = ($0 == "[workspace.package]"); next } s && /^version/ { gsub(/.*"|".*/, ""); print; exit }' "$repo_root/Cargo.toml")"

# (a) The real files pass.
reset_fixture
check "(a) the real Cargo.toml and PKGBUILD pass" "pass" "$(run_gate)"

# (b) A PKGBUILD pkgver that is not the workspace version fails.
reset_fixture
sed -i -E 's/^pkgver=.*/pkgver=0.0.0/' "$tmp/work/packaging/linux/PKGBUILD"
check "(b) a mismatched pkgver fails" "fail" "$(run_gate)"
expect_message "does not match the workspace version $workspace_version"

# (c) A workspace version bump without the PKGBUILD fails.
reset_fixture
sed -i -E '/^\[workspace.package\]/,/^\[/ s/^version = "[^"]*"/version = "9.9.9"/' "$tmp/work/Cargo.toml"
check "(c) a workspace version bump alone fails" "fail" "$(run_gate)"
expect_message "workspace version 9.9.9"

# (d) sha256sums with a real hash (not SKIP) fails -- it must never reach
#     the repository (aur-prepare.sh writes that only into its own output).
reset_fixture
sed -i -E "s/^sha256sums=\('SKIP'\)\$/sha256sums=('$(printf '0%.0s' {1..64})')/" "$tmp/work/packaging/linux/PKGBUILD"
check "(d) a real hash in the repository copy fails" "fail" "$(run_gate)"
expect_message "must keep sha256sums=('SKIP')"

# Self-check: a gutted gate must fail this self-test, proving the 'fail'
# results above were the real gate firing, not this harness.
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
  echo "check-pkgbuild.test.sh: ok"
  exit 0
fi
echo "check-pkgbuild.test.sh: failed" >&2
exit 1
