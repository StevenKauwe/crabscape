#!/usr/bin/env bash
# Make assets/crab/crab-atlas-oil.png: the crab's puppet parts, oil-painted.
#
#   tools/paint-oil-crab.sh
#
# Run after `cargo run -p crabrig --profile art -- export` (the oil atlas uses the same rectangles,
# so crab-parts.json serves both styles). Takes about 15 seconds (plus a first
# build of the painter).
#
# 1. Export a bigger copy of the parts (1.8x the game's resolution).
# 2. tools/oil-atlas lays them out, well apart, on one painter-sized sheet, each
#    on a halo of its own edge colour.
# 3. The oil painter (crates/painter, `crabpaint`) paints the sheet
#    (--attention off so every part keeps its drawn shape; --ground subject
#    so the ground under skipped bristles is the parts' own colour).
# 4. tools/oil-atlas cuts each painted part back out with the drawing's own
#    outline (grown a pixel, softened), puts back the drawing's broad shading,
#    and writes the atlas.
#
# Needs: only Rust.
set -euo pipefail
cd "$(dirname "$0")/.."
ZOOM=${ZOOM:-1.8}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo build --profile art --quiet -p painter -p oil-atlas -p crabrig
target=${CARGO_TARGET_DIR:-target}
tool=$target/art/oil-atlas
painter=$target/art/crabpaint
rig=$target/art/crabrig
"$rig" export --scale "$ZOOM" --out "$work/big" >/dev/null
"$tool" layout "$work/big/crab-atlas.png" "$work/big/crab-parts.json" "$work"
for sheet in "$work"/sheet-*.png; do
  n=${sheet##*/sheet-}
  echo "painting ${sheet##*/}..."
  "$painter" oilphoto "$sheet" "$work/painted-$n" --attention off --ground subject >/dev/null
done
"$tool" pack "$work/big/crab-parts.json" assets/crab/crab-atlas.png assets/crab/crab-parts.json \
  "$work" assets/crab/crab-atlas-oil.png
