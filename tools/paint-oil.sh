#!/usr/bin/env bash
# Make the oil-painted atlases: the puppets' parts, oil-painted.
#
#   tools/paint-oil.sh                    the crab and every creature
#   tools/paint-oil.sh crab mantis-shrimp only these
#
# Writes assets/crab/crab-atlas-oil.png and assets/creatures/<creature>-atlas-oil.png.
# Run after `cargo run -p crabrig --profile art -- export [--creature C]` (an oil
# atlas uses the same rectangles as its ink atlas, so <name>-parts.json serves both
# styles). Takes about 15 seconds per puppet (plus a first build of the painter).
# Deterministic: the same inputs give the same bytes.
#
# For each puppet:
# 1. Export a bigger copy of the parts: 1.8x the game's resolution, 4x for the two
#    shrimps (their parts are so small that at 1.8x the strokes paint over their
#    segment lines, spots and the club's shine). ZOOM=<x> overrides it for all.
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
if [ $# -eq 0 ]; then
  set -- crab snapping-shrimp mantis-shrimp snowflake-moray common-octopus
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cargo build --profile art --quiet -p painter -p oil-atlas -p crabrig
target=${CARGO_TARGET_DIR:-target}
tool=$target/art/oil-atlas
painter=$target/art/crabpaint
rig=$target/art/crabrig
for name in "$@"; do
  out=$work/$name
  mkdir -p "$out"
  case $name in
    snapping-shrimp | mantis-shrimp) zoom=${ZOOM:-4} ;;
    *) zoom=${ZOOM:-1.8} ;;
  esac
  if [ "$name" = crab ]; then
    dir=assets/crab
    "$rig" export --scale "$zoom" --out "$out/big" >/dev/null
  else
    dir=assets/creatures
    "$rig" export --creature "$name" --scale "$zoom" --out "$out/big" >/dev/null
  fi
  "$tool" layout "$out/big/$name-atlas.png" "$out/big/$name-parts.json" "$out"
  for sheet in "$out"/sheet-*.png; do
    n=${sheet##*/sheet-}
    echo "painting $name ${sheet##*/}..."
    "$painter" oilphoto "$sheet" "$out/painted-$n" --attention off --ground subject >/dev/null
  done
  "$tool" pack "$out/big/$name-parts.json" "$dir/$name-atlas.png" "$dir/$name-parts.json" \
    "$out" "$dir/$name-atlas-oil.png"
done
