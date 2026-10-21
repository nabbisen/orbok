#!/usr/bin/env bash
# check-pkgbuild.sh — every Arch PKGBUILD template's pkgver is the
# workspace version, and its sha256sums stays the permanent 'SKIP'
# placeholder.
#
# Checks packaging/linux/PKGBUILD (orbok) and
# packaging/linux/orbok-bin/PKGBUILD (orbok-bin). Each carries its own copy
# of the version, as `pkgver=A.B.C`. A copy drifts unless something checks
# it (the same problem scripts/check-store-manifest.sh already solves for
# the Windows manifest). `sha256sums=('SKIP')` is permanent in both, not a
# gap to close -- the real hash is computed only by
# packaging/linux/aur-prepare.sh, from the released archive's own .sha256,
# and written only into the copy it produces, never into these files (see
# each PKGBUILD's own header comment).
#
# Also checks (Task 137 §1.3) that both templates' package() installs the
# desktop entry and icons under orbok's one application ID
# (`crate::APP_ID` in `crates/app/src/main.rs`), never a hand-copied
# literal that could drift from it.
#
# Run from the release checklist, not CI: a PKGBUILD only needs
# re-checking when a release is about to be cut or published, not on every
# push.
#
# Overridable for the self-test: CARGO_TOML, PKGBUILDS (space-separated),
# APP_ID_SOURCE.
set -euo pipefail

cargo_toml="${CARGO_TOML:-Cargo.toml}"
# shellcheck disable=SC2206  # PKGBUILDS is a caller-supplied path list
pkgbuilds=(${PKGBUILDS:-packaging/linux/PKGBUILD packaging/linux/orbok-bin/PKGBUILD})
app_id_source="${APP_ID_SOURCE:-crates/app/src/main.rs}"

[ -f "$cargo_toml" ] || { echo "check-pkgbuild: missing $cargo_toml" >&2; exit 1; }

# The `version = "..."` line inside [workspace.package].
workspace_version="$(awk '
  /^\[/ { in_section = ($0 == "[workspace.package]") ; next }
  in_section && /^[[:space:]]*version[[:space:]]*=/ {
    line = $0; sub(/^[^"]*"/, "", line); sub(/".*$/, "", line); print line; exit
  }
' "$cargo_toml")"
[ -n "$workspace_version" ] \
  || { echo "check-pkgbuild: no version in [workspace.package] of $cargo_toml" >&2; exit 1; }

[ -f "$app_id_source" ] || { echo "check-pkgbuild: missing $app_id_source" >&2; exit 1; }
app_id="$(sed -n 's/.*pub(crate) const APP_ID: &str = "\([^"]*\)".*/\1/p' "$app_id_source")"
[ -n "$app_id" ] \
  || { echo "check-pkgbuild: no 'pub(crate) const APP_ID: &str = \"...\"' line in $app_id_source" >&2; exit 1; }

fail=0
for pkgbuild in "${pkgbuilds[@]}"; do
  if [ ! -f "$pkgbuild" ]; then
    echo "check-pkgbuild: missing $pkgbuild" >&2
    fail=1
    continue
  fi

  pkgbuild_version="$(awk -F= '/^pkgver=/{print $2; exit}' "$pkgbuild")"
  if [ -z "$pkgbuild_version" ]; then
    echo "check-pkgbuild: no pkgver= line in $pkgbuild" >&2
    fail=1
    continue
  fi

  file_fail=0
  if [ "$pkgbuild_version" != "$workspace_version" ]; then
    echo "check-pkgbuild: $pkgbuild pkgver=$pkgbuild_version does not match the workspace version $workspace_version -- update it in the release commit" >&2
    fail=1
    file_fail=1
  fi
  if ! grep -qF "sha256sums=('SKIP')" "$pkgbuild"; then
    echo "check-pkgbuild: $pkgbuild must keep sha256sums=('SKIP') -- the real hash belongs only in the copy aur-prepare.sh produces, never committed here" >&2
    fail=1
    file_fail=1
  fi
  if ! grep -qF "$app_id.desktop" "$pkgbuild"; then
    echo "check-pkgbuild: $pkgbuild does not install $app_id.desktop -- the desktop entry must be named by APP_ID ($app_id_source), not a hand-copied literal" >&2
    fail=1
    file_fail=1
  fi
  if ! grep -qF "$app_id.png" "$pkgbuild"; then
    echo "check-pkgbuild: $pkgbuild does not install $app_id.png icons -- every hicolor icon must be named by APP_ID ($app_id_source), not a hand-copied literal" >&2
    fail=1
    file_fail=1
  fi
  [ "$file_fail" -eq 0 ] && echo "check-pkgbuild: ok: $pkgbuild (pkgver=$pkgbuild_version, app id $app_id)"
done

exit "$fail"
