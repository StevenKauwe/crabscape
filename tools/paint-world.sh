#!/usr/bin/env bash
# Make the painted world's tiles: the terrain's materials and the far ridges, oil-painted.
#
#   tools/paint-world.sh
#
# Writes assets/world/terrain-tiles.png (six materials, lit then unlit, 96 px tiles stacked, each seamless) and
# assets/world/ridges.png (the far and near rock ridges, lit and unlit, RGBA, repeating sideways).
# Takes a minute or two (plus a first build of the painter). Deterministic: the same inputs give the same bytes.
#
# 1. tools/oil-world draws a swatch of each material (and of each ridge, three repeats side by side), in the
#    lantern's light and in the dark.
# 2. The oil painter (crates/painter, `crabpaint`) paints each one (--attention off so every swatch keeps its
#    layout; --ground subject so the ground under skipped bristles is the swatch's own colour).
# 3. tools/oil-world cuts the middle repeat out of each, makes it seamless, and writes the PNGs.
#
# Needs: only Rust.
set -euo pipefail
cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo build -q -p oil-world -p painter --profile art --bin oil-world --bin crabpaint
art=target/art
mkdir -p "$work/in" "$work/out" assets/world
"$art/oil-world" swatch "$work/in"
"$art/oil-world" ridges "$work/in"
for f in "$work"/in/*.png; do
  OIL_ATTENTION=off "$art/crabpaint" oilphoto "$f" "$work/out/$(basename "$f")" 0.5 0.5 0.5 --attention off --ground subject
done
"$art/oil-world" cut "$work/out" assets/world/terrain-tiles.png
"$art/oil-world" cut-ridges "$work/out" assets/world/ridges.png
ls -l assets/world
