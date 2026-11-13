#!/usr/bin/env bash
# Make the oil-painted atlases: the puppets' parts, oil-painted.
#
#   tools/paint-oil.sh                    the crab and every creature
#   tools/paint-oil.sh crab mantis-shrimp only these
#
# Writes assets/crab/crab-atlas-oil.png and assets/creatures/<creature>-atlas-oil.png, with their
# <name>-parts.json (the oil atlas uses the rectangles of the drawn atlas the rig exports, which is made in a
# scratch folder and not kept: the game shows only the oil paint). Takes about 15 seconds per puppet (plus a
# first build of the painter).
# Deterministic: the same inputs give the same bytes.
#
# For each puppet:
# 1. Export a bigger copy of the parts: 1.8x the game's resolution; 4x for the small
#    animals with fine detail (the shrimps, the monkey, the hermit crab, the bobbit worm,
#    the amphipods, the viperfish, the lobster, the shore crab, the giant isopod, the hagfish:
#    their parts are so small that at 1.8x the strokes paint over their segment lines,
#    spots and shine); 1.5x for the goliath grouper (its body is the widest part of any
#    puppet, and must fit the painter's 1920-wide sheet). ZOOM=<x> overrides it for all.
#    How to choose for a new animal: docs/design/painted-creatures.md.
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
  set -- crab snapping-shrimp mantis-shrimp snowflake-moray common-octopus space-monkey pizza-crab queen-triggerfish \
    alarm-jelly loggerhead-turtle humboldt-squid viperfish goliath-grouper american-lobster hermit-crab shore-crab bobbit-worm \
    giant-isopod hagfish amphipod-swarm
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
    snapping-shrimp | mantis-shrimp | space-monkey | hermit-crab | shore-crab | bobbit-worm | amphipod-swarm | viperfish | \
      american-lobster | giant-isopod | hagfish)
      zoom=${ZOOM:-4}
      ;;
    goliath-grouper) zoom=${ZOOM:-1.5} ;;
    *) zoom=${ZOOM:-1.8} ;;
  esac
  if [ "$name" = crab ]; then
    dir=assets/crab
    who=()
  else
    dir=assets/creatures
    who=(--creature "$name")
  fi
  "$rig" export ${who[@]+"${who[@]}"} --scale "$zoom" --out "$out/big" >/dev/null
  "$rig" export ${who[@]+"${who[@]}"} --out "$out/game" >/dev/null
  cp "$out/game/$name-parts.json" "$dir/$name-parts.json"
  "$tool" layout "$out/big/$name-atlas.png" "$out/big/$name-parts.json" "$out"
  for sheet in "$out"/sheet-*.png; do
    n=${sheet##*/sheet-}
    echo "painting $name ${sheet##*/}..."
    "$painter" oilphoto "$sheet" "$out/painted-$n" --attention off --ground subject >/dev/null
  done
  "$tool" pack "$out/big/$name-parts.json" "$out/game/$name-atlas.png" "$dir/$name-parts.json" \
    "$out" "$dir/$name-atlas-oil.png"
done
