#!/usr/bin/env bash
# check-pkgbuild.test.sh — regression test for check-pkgbuild.sh. Runs
# against copies of the real Cargo.toml and both PKGBUILD templates in a
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
  mkdir -p "$tmp/work/packaging/linux/orbok-bin" "$tmp/work/packaging/flatpak" "$tmp/work/scripts" "$tmp/work/crates/app/src"
  cp "$repo_root/Cargo.toml" "$tmp/work/Cargo.toml"
  cp "$repo_root/packaging/linux/PKGBUILD" "$tmp/work/packaging/linux/PKGBUILD"
  cp "$repo_root/packaging/linux/orbok-bin/PKGBUILD" "$tmp/work/packaging/linux/orbok-bin/PKGBUILD"
  cp "$repo_root/packaging/flatpak/io.github.nabbisen.orbok.metainfo.xml" "$tmp/work/packaging/flatpak/io.github.nabbisen.orbok.metainfo.xml"
  cp "$repo_root/crates/app/src/main.rs" "$tmp/work/crates/app/src/main.rs"
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

# (a) Both real templates pass together.
reset_fixture
check "(a) the real Cargo.toml and both PKGBUILD templates pass" "pass" "$(run_gate)"

# (b) orbok's pkgver not matching the workspace version fails.
reset_fixture
sed -i -E 's/^pkgver=.*/pkgver=0.0.0/' "$tmp/work/packaging/linux/PKGBUILD"
check "(b) a mismatched orbok pkgver fails" "fail" "$(run_gate)"
expect_message "packaging/linux/PKGBUILD pkgver=0.0.0 does not match the workspace version $workspace_version"

# (c) orbok-bin's pkgver not matching the workspace version fails, and
#     names orbok-bin's own path -- not orbok's.
reset_fixture
sed -i -E 's/^pkgver=.*/pkgver=0.0.0/' "$tmp/work/packaging/linux/orbok-bin/PKGBUILD"
check "(c) a mismatched orbok-bin pkgver fails" "fail" "$(run_gate)"
expect_message "packaging/linux/orbok-bin/PKGBUILD pkgver=0.0.0 does not match the workspace version $workspace_version"

# (d) A workspace version bump without either PKGBUILD fails.
reset_fixture
sed -i -E '/^\[workspace.package\]/,/^\[/ s/^version = "[^"]*"/version = "9.9.9"/' "$tmp/work/Cargo.toml"
check "(d) a workspace version bump alone fails" "fail" "$(run_gate)"
expect_message "workspace version 9.9.9"

# (e) sha256sums with a real hash (not SKIP) fails in orbok's template --
#     it must never reach the repository (aur-prepare.sh writes that only
#     into its own output).
reset_fixture
sed -i -E "s/^sha256sums=\('SKIP'\)\$/sha256sums=('$(printf '0%.0s' {1..64})')/" "$tmp/work/packaging/linux/PKGBUILD"
check "(e) a real hash in orbok's repository copy fails" "fail" "$(run_gate)"
expect_message "packaging/linux/PKGBUILD must keep sha256sums=('SKIP')"

# (f) The same, for orbok-bin's template.
reset_fixture
sed -i -E "s/^sha256sums=\('SKIP'\)\$/sha256sums=('$(printf '0%.0s' {1..64})')/" "$tmp/work/packaging/linux/orbok-bin/PKGBUILD"
check "(f) a real hash in orbok-bin's repository copy fails" "fail" "$(run_gate)"
expect_message "packaging/linux/orbok-bin/PKGBUILD must keep sha256sums=('SKIP')"

# (g) A missing template (e.g. orbok-bin's, if it were ever deleted) fails
#     outright, rather than silently checking only what is left.
reset_fixture
rm "$tmp/work/packaging/linux/orbok-bin/PKGBUILD"
check "(g) a missing orbok-bin template fails" "fail" "$(run_gate)"
expect_message "missing packaging/linux/orbok-bin/PKGBUILD"

# (h) A PKGBUILD that installs a stale, hand-copied desktop entry name
#     (not APP_ID) fails -- Task 137 §1.3's drift guard.
reset_fixture
sed -i 's/io\.github\.nabbisen\.orbok\.desktop/orbok.desktop/g' "$tmp/work/packaging/linux/PKGBUILD"
check "(h) a stale desktop entry name fails" "fail" "$(run_gate)"
expect_message "packaging/linux/PKGBUILD does not install io.github.nabbisen.orbok.desktop"

# (i) The same, for a stale hand-copied icon name.
reset_fixture
sed -i 's/io\.github\.nabbisen\.orbok\.png/orbok.png/g' "$tmp/work/packaging/linux/orbok-bin/PKGBUILD"
check "(i) a stale icon name fails" "fail" "$(run_gate)"
expect_message "packaging/linux/orbok-bin/PKGBUILD does not install io.github.nabbisen.orbok.png"

# (j) If APP_ID itself ever changes in main.rs, a PKGBUILD not yet updated
#     to match fails, rather than passing against its own stale copy.
reset_fixture
sed -i 's/pub(crate) const APP_ID: &str = "[^"]*"/pub(crate) const APP_ID: \&str = "io.github.nabbisen.orbok2"/' "$tmp/work/crates/app/src/main.rs"
check "(j) a PKGBUILD not matching a changed APP_ID fails" "fail" "$(run_gate)"
expect_message "does not install io.github.nabbisen.orbok2.desktop"

# (k) A PKGBUILD that installs a stale, hand-copied metainfo name (not
#     APP_ID) fails.
reset_fixture
sed -i 's/io\.github\.nabbisen\.orbok\.metainfo\.xml/orbok.metainfo.xml/g' "$tmp/work/packaging/linux/PKGBUILD"
check "(k) a stale metainfo name fails" "fail" "$(run_gate)"
expect_message "packaging/linux/PKGBUILD does not install io.github.nabbisen.orbok.metainfo.xml"

# (l) A missing metainfo file fails outright.
reset_fixture
rm "$tmp/work/packaging/flatpak/io.github.nabbisen.orbok.metainfo.xml"
check "(l) a missing metainfo file fails" "fail" "$(run_gate)"
expect_message "missing packaging/flatpak/io.github.nabbisen.orbok.metainfo.xml"

# (m) The metainfo's own <id> not matching APP_ID fails.
reset_fixture
sed -i 's#<id>io.github.nabbisen.orbok</id>#<id>io.github.nabbisen.orbok2</id>#' "$tmp/work/packaging/flatpak/io.github.nabbisen.orbok.metainfo.xml"
check "(m) a mismatched metainfo <id> fails" "fail" "$(run_gate)"
expect_message "<id>io.github.nabbisen.orbok2</id> does not match APP_ID"

# (n) A workspace version bump with no matching <release version=...> entry
#     in the metainfo fails, the same drift check (d) makes for the
#     PKGBUILDs.
reset_fixture
sed -i -E '/^\[workspace.package\]/,/^\[/ s/^version = "[^"]*"/version = "9.9.9"/' "$tmp/work/Cargo.toml"
check "(n) a workspace version bump with no matching metainfo release fails" "fail" "$(run_gate)"
expect_message "does not match the workspace version 9.9.9"

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
