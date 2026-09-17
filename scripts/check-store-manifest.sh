#!/usr/bin/env bash
# check-store-manifest.sh — the Microsoft Store manifest's version is the
# workspace version (Task 061 §2).
#
# packaging/windows/AppxManifest.xml carries its own copy of the version,
# as <Identity ... Version="A.B.C.D">. A second copy drifts unless something
# checks it. The Store requires four parts with the fourth 0, so the
# manifest must say exactly "<workspace version>.0".
#
# Overridable for the self-test: CARGO_TOML, MANIFEST.
set -euo pipefail

cargo_toml="${CARGO_TOML:-Cargo.toml}"
manifest="${MANIFEST:-packaging/windows/AppxManifest.xml}"

for f in "$cargo_toml" "$manifest"; do
  [ -f "$f" ] || { echo "check-store-manifest: missing $f" >&2; exit 1; }
done

# The `version = "..."` line inside [workspace.package].
workspace_version="$(awk '
  /^\[/ { in_section = ($0 == "[workspace.package]") ; next }
  in_section && /^[[:space:]]*version[[:space:]]*=/ {
    line = $0; sub(/^[^"]*"/, "", line); sub(/".*$/, "", line); print line; exit
  }
' "$cargo_toml")"
[ -n "$workspace_version" ] \
  || { echo "check-store-manifest: no version in [workspace.package] of $cargo_toml" >&2; exit 1; }

# The Version attribute of the <Identity> element, which may span lines.
manifest_version="$(tr '\n' ' ' <"$manifest" \
  | grep -oE '<Identity[^>]*>' \
  | grep -oE '[[:space:]]Version="[^"]*"' \
  | head -n 1 \
  | sed -E 's/.*Version="([^"]*)"/\1/')"
[ -n "$manifest_version" ] \
  || { echo "check-store-manifest: no Version on <Identity> in $manifest" >&2; exit 1; }

fail=0
if ! [[ "$manifest_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.0$ ]]; then
  echo "check-store-manifest: $manifest Version=\"$manifest_version\" must have four numeric parts with the fourth 0 (a Store requirement)" >&2
  fail=1
fi
if [ "$manifest_version" != "$workspace_version.0" ]; then
  echo "check-store-manifest: $manifest Version=\"$manifest_version\" does not match the workspace version $workspace_version -- set it to \"$workspace_version.0\" in the release commit" >&2
  fail=1
fi
[ "$fail" -eq 0 ] || exit 1
echo "check-store-manifest: ok (Version=\"$manifest_version\")"
