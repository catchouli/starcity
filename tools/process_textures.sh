#!/usr/bin/env bash
# Downscales source PBR materials (ambientCG, CC0) into small game textures.
# Source sets live outside the repo; set ASSETS to point at them.
# Every texture produced here must be listed in CREDITS.md.
set -euo pipefail

ASSETS="${ASSETS:-$HOME/assets/surfaces}"
OUT="$(dirname "$0")/../assets/textures"
SIZE="${SIZE:-256}"
mkdir -p "$OUT"

# game name : ambientCG material id
MATERIALS=(
  "grass:Grass005"
  "dirt:Ground108"
  "rock:Ground110"
  "gravel:Ground111"
)

for entry in "${MATERIALS[@]}"; do
  name="${entry%%:*}"
  id="${entry##*:}"
  src="$ASSETS/${id}_4K-PNG/${id}_4K-PNG_Color.png"
  echo "$id -> $name.png (${SIZE}px)"
  magick "$src" -filter Lanczos -resize "${SIZE}x${SIZE}" -strip \
    -define png:compression-level=9 "PNG24:$OUT/$name.png"
done
