#!/usr/bin/env bash
# Make assets/crab/crab-atlas-oil.png: the crab's puppet parts, oil-painted.
#
#   tools/paint-oil-crab.sh
#
# Run after tools/export-crab-parts.py (the oil atlas uses the same rectangles,
# so crab-parts.json serves both styles). Takes about 15 seconds.
#
# 1. Export a bigger copy of the parts (1.8x the game's resolution).
# 2. tools/oil-atlas lays them out, well apart, on one painter-sized sheet, each
#    on a halo of its own edge colour.
# 3. The oil painter from the music-to-movie-magic studio paints the sheet
#    (OIL_ATTENTION=off so every part keeps its drawn shape; OIL_GROUND=subject
#    so the ground under skipped bristles is the parts' own colour).
# 4. tools/oil-atlas cuts each painted part back out with the drawing's own
#    outline (grown a pixel, softened), puts back the drawing's broad shading,
#    and writes the atlas.
#
# Needs: Rust, rsvg-convert, and the painter (set PAINTER to its path).
set -euo pipefail
cd "$(dirname "$0")/.."
PAINTER=${PAINTER:-$HOME/Work/music-to-movie-magic/studio/target-local/release/film-oh-brassica}
ZOOM=${ZOOM:-1.8}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo build --release --quiet --manifest-path tools/oil-atlas/Cargo.toml
tool=tools/oil-atlas/target/release/oil-atlas
python3 tools/export-crab-parts.py --scale "$ZOOM" --out "$work/big"
"$tool" layout "$work/big/crab-atlas.png" "$work/big/crab-parts.json" "$work"
for sheet in "$work"/sheet-*.png; do
  n=${sheet##*/sheet-}
  echo "painting ${sheet##*/}..."
  OIL_ATTENTION=off OIL_GROUND=subject "$PAINTER" oilphoto "$sheet" "$work/painted-$n" >/dev/null
done
"$tool" pack "$work/big/crab-parts.json" assets/crab/crab-atlas.png assets/crab/crab-parts.json \
  "$work" assets/crab/crab-atlas-oil.png
