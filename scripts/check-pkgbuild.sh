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
# Run from the release checklist, not CI: a PKGBUILD only needs
# re-checking when a release is about to be cut or published, not on every
# push.
#
# Overridable for the self-test: CARGO_TOML, PKGBUILDS (space-separated).
set -euo pipefail

cargo_toml="${CARGO_TOML:-Cargo.toml}"
# shellcheck disable=SC2206  # PKGBUILDS is a caller-supplied path list
pkgbuilds=(${PKGBUILDS:-packaging/linux/PKGBUILD packaging/linux/orbok-bin/PKGBUILD})

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
  [ "$file_fail" -eq 0 ] && echo "check-pkgbuild: ok: $pkgbuild (pkgver=$pkgbuild_version)"
done

exit "$fail"
