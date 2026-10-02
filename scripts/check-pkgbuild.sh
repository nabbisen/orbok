#!/usr/bin/env bash
# check-pkgbuild.sh — the Arch PKGBUILD template's pkgver is the workspace
# version, and its sha256sums stays the permanent 'SKIP' placeholder
# (Task 129 §1.5).
#
# packaging/linux/PKGBUILD carries its own copy of the version, as
# `pkgver=A.B.C`. A second copy drifts unless something checks it (the same
# problem scripts/check-store-manifest.sh already solves for the Windows
# manifest). `sha256sums=('SKIP')` is permanent here, not a gap to close --
# the real hash is computed only by packaging/linux/aur-prepare.sh, from the
# released archive's own .sha256, and written only into the copy it
# produces, never into this file (see PKGBUILD's own header comment).
#
# Run from the release checklist, not CI (the owner's instruction, Task
# 129): a PKGBUILD only needs re-checking when a release is about to be cut
# or published, not on every push.
#
# Overridable for the self-test: CARGO_TOML, PKGBUILD.
set -euo pipefail

cargo_toml="${CARGO_TOML:-Cargo.toml}"
pkgbuild="${PKGBUILD:-packaging/linux/PKGBUILD}"

for f in "$cargo_toml" "$pkgbuild"; do
  [ -f "$f" ] || { echo "check-pkgbuild: missing $f" >&2; exit 1; }
done

# The `version = "..."` line inside [workspace.package].
workspace_version="$(awk '
  /^\[/ { in_section = ($0 == "[workspace.package]") ; next }
  in_section && /^[[:space:]]*version[[:space:]]*=/ {
    line = $0; sub(/^[^"]*"/, "", line); sub(/".*$/, "", line); print line; exit
  }
' "$cargo_toml")"
[ -n "$workspace_version" ] \
  || { echo "check-pkgbuild: no version in [workspace.package] of $cargo_toml" >&2; exit 1; }

# The `pkgver=...` line (no quotes in a PKGBUILD).
pkgbuild_version="$(awk -F= '/^pkgver=/{print $2; exit}' "$pkgbuild")"
[ -n "$pkgbuild_version" ] \
  || { echo "check-pkgbuild: no pkgver= line in $pkgbuild" >&2; exit 1; }

fail=0
if [ "$pkgbuild_version" != "$workspace_version" ]; then
  echo "check-pkgbuild: $pkgbuild pkgver=$pkgbuild_version does not match the workspace version $workspace_version -- update it in the release commit" >&2
  fail=1
fi
if ! grep -qF "sha256sums=('SKIP')" "$pkgbuild"; then
  echo "check-pkgbuild: $pkgbuild must keep sha256sums=('SKIP') -- the real hash belongs only in the copy aur-prepare.sh produces, never committed here" >&2
  fail=1
fi
[ "$fail" -eq 0 ] || exit 1
echo "check-pkgbuild: ok (pkgver=$pkgbuild_version)"
