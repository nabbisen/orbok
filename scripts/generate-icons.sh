#!/usr/bin/env bash
# scripts/generate-icons.sh — generate every icon file from the tracked
# masters in packaging/icon/ (Task 130 §1).
#
# Run by hand, never at build time: every output is a committed file, not
# a build artifact. Requires ImageMagick (`magick`).
#
# Usage: ./scripts/generate-icons.sh (from the repository root)

set -euo pipefail

ICON_DIR="packaging/icon"
MASTER="$ICON_DIR/orbok-icon.png"
MASTER_SMALL="$ICON_DIR/orbok-icon-small.png"
MASTER_WIDE="$ICON_DIR/orbok-tile-wide.png"

for f in "$MASTER" "$MASTER_SMALL" "$MASTER_WIDE"; do
  [ -f "$f" ] || { echo "generate-icons: missing $f (run from the repository root)" >&2; exit 1; }
done

resize() {
  local src="$1" size="$2" dest="$3"
  mkdir -p "$(dirname "$dest")"
  magick "$src" -filter Lanczos -resize "${size}" -strip "$dest"
  echo "  wrote $dest"
}

echo "Windows Assets (square, from $MASTER):"
resize "$MASTER" 150x150 packaging/windows/Assets/Square150x150Logo.png
resize "$MASTER" 44x44 packaging/windows/Assets/Square44x44Logo.png
resize "$MASTER" 50x50 packaging/windows/Assets/StoreLogo.png

echo "Windows Assets (wide, from $MASTER_WIDE):"
resize "$MASTER_WIDE" 310x150 packaging/windows/Assets/Wide310x150Logo.png

echo "Linux hicolor icons, 16/32 (from $MASTER_SMALL):"
for size in 16 32; do
  resize "$MASTER_SMALL" "${size}x${size}" "packaging/linux/icons/hicolor/${size}x${size}/apps/orbok.png"
done

echo "Linux hicolor icons, 48 and up (from $MASTER):"
for size in 48 64 128 256 512; do
  resize "$MASTER" "${size}x${size}" "packaging/linux/icons/hicolor/${size}x${size}/apps/orbok.png"
done

echo "Store listing logo (from $MASTER):"
resize "$MASTER" 300x300 packaging/windows/store-listing/logo-300.png

# ── The window icon: raw RGBA, not a PNG, so `include_bytes!` needs no
#    decoding dependency orbok does not already have (Task 130 §1.3's own
#    stop condition) -- `iced::window::icon::from_rgba` takes exactly this
#    shape, width and height as separate constants in the Rust source. ────
echo "Window icon (256px raw RGBA, embedded with include_bytes!):"
mkdir -p crates/app/assets
magick "$MASTER" -filter Lanczos -resize 256x256 -strip -depth 8 \
  "RGBA:crates/app/assets/icon-256.rgba"
echo "  wrote crates/app/assets/icon-256.rgba (256x256, update ICON_SIZE in main.rs if this changes)"

# ── The check this script exists to enforce: no partial transparency
#    inside the plate, and no transparent pixel at all in the wide tile --
#    the defect the owner's original art had (Task 130 §1). ───────────────
echo "Checking opacity..."
fail=0

check_plate_opaque() {
  local f="$1"
  # The plate sits at a 52px margin on the 1024px master, 920px square,
  # corner radius 22% (~202px). Inset by the corner radius on every side
  # so the checked region can never touch a rounded corner's own
  # (legitimately partial) anti-aliased edge.
  local inset=254 extent=516
  local alpha min max
  alpha="$(magick "$f" -crop "${extent}x${extent}+${inset}+${inset}" +repage \
    -alpha extract -format '%[min] %[max]' info:)"
  read -r min max <<<"$alpha"
  if [ "$min" != "65535" ] || [ "$max" != "65535" ]; then
    echo "generate-icons: $f has non-opaque pixels inside its plate (alpha $min..$max)" >&2
    return 1
  fi
}

check_fully_opaque() {
  local f="$1"
  local channels
  channels="$(magick "$f" -format '%[channels]' info:)"
  case "$channels" in
    *a*)
      local alpha min max
      alpha="$(magick "$f" -alpha extract -format '%[min] %[max]' info:)"
      read -r min max <<<"$alpha"
      if [ "$min" != "65535" ]; then
        echo "generate-icons: $f has a transparent pixel (alpha min $min)" >&2
        return 1
      fi
      ;;
  esac
}

check_plate_opaque "$MASTER" || fail=1
check_plate_opaque "$MASTER_SMALL" || fail=1
check_fully_opaque "$MASTER_WIDE" || fail=1

if [ "$fail" -ne 0 ]; then
  echo "generate-icons: opacity check failed -- see above" >&2
  exit 1
fi
echo "generate-icons: ok"
