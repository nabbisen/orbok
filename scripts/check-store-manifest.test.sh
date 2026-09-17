#!/usr/bin/env bash
# check-store-manifest.test.sh — regression test for check-store-manifest.sh
# (Task 061 §2). Runs against copies of the real Cargo.toml and manifest in a
# temp directory; touches nothing in the real repository.
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
  mkdir -p "$tmp/work/packaging/windows" "$tmp/work/scripts"
  cp "$repo_root/Cargo.toml" "$tmp/work/Cargo.toml"
  cp "$repo_root/packaging/windows/AppxManifest.xml" "$tmp/work/packaging/windows/AppxManifest.xml"
  cp "${GATE_UNDER_TEST:-$repo_root/scripts/check-store-manifest.sh}" "$tmp/work/scripts/check-store-manifest.sh"
}

run_gate() {
  if (cd "$tmp/work" && bash scripts/check-store-manifest.sh) >"$tmp/out" 2>&1; then
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
set_manifest_version() {
  sed -i -E "s/(<Identity[^>]*|^[[:space:]]*)Version=\"[^\"]*\"/\1Version=\"$1\"/" \
    "$tmp/work/packaging/windows/AppxManifest.xml"
  grep -qF "Version=\"$1\"" "$tmp/work/packaging/windows/AppxManifest.xml" \
    || { echo "FAIL: fixture did not take Version=\"$1\"" >&2; fail=1; }
}

# (a) The real files pass.
reset_fixture
check "(a) the real Cargo.toml and manifest pass" "pass" "$(run_gate)"

# (b) A manifest version that is not the workspace version fails.
reset_fixture
set_manifest_version "0.0.0.0"
check "(b) a mismatched version fails" "fail" "$(run_gate)"
expect_message "does not match the workspace version $workspace_version"

# (c) The workspace version with a non-zero fourth part fails.
reset_fixture
set_manifest_version "$workspace_version.1"
check "(c) a non-zero fourth part fails" "fail" "$(run_gate)"
expect_message "fourth 0"

# (d) A workspace version bump without the manifest fails.
reset_fixture
sed -i -E '/^\[workspace.package\]/,/^\[/ s/^version = "[^"]*"/version = "9.9.9"/' "$tmp/work/Cargo.toml"
check "(d) a workspace version bump alone fails" "fail" "$(run_gate)"
expect_message "workspace version 9.9.9"

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
  echo "check-store-manifest.test.sh: ok"
  exit 0
fi
echo "check-store-manifest.test.sh: failed" >&2
exit 1
