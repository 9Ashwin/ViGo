#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
mkdir -p .local/icon-build build
work=$(mktemp -d .local/icon-build/ViGo.XXXXXX)
iconset="$work/ViGo.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  /usr/bin/sips -z "$size" "$size" assets/app-icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  /usr/bin/sips -z "$double" "$double" assets/app-icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
/usr/bin/iconutil --convert icns --output build/icon.icns "$iconset"
python3 tools/native/verify-icon.py build/icon.icns

# The mounted disk only needs a smaller Finder icon; avoid duplicating the 1024px payload.
volume="$work/Volume.iconset"
mkdir -p "$volume" release
for file in "$iconset"/*.png; do
  case "$file" in *icon_512x512@2x.png) continue ;; esac
  cp "$file" "$volume/"
done
/usr/bin/iconutil --convert icns --output release/dmg-volume.icns "$volume"
