# The painted world (terrain and far background)

The ground and the far ridges are oil-painted by our own painter, in the same look as the creatures: a dark warm ink
edge, each shape lit on its own (lit top-left), visible brush strokes, true material colours pushed a little. Never a
full-frame filter, overlay, tint or vignette: every pixel belongs to a shape.

## What is painted
- `assets/world/terrain-tiles.png`: six materials (sand, silt, clay, pebble, rubble, rock), each a seamless 96 px
  tile, painted lit (the lantern or the sun on it) then unlit (the same ground in the dark): twelve stacked layers.
- `assets/world/ridges.png`: the far and near rock ridges (RGBA, repeat sideways, faded at their own foot), lit then
  unlit: four stacked layers.
- Both are baked into the game with `include_bytes!` (`src/painted_world.rs`); the shaders read them as 2D arrays.
- Made by `tools/paint-world.sh` (`tools/oil-world` draws swatches, `crabpaint` paints them, `oil-world` cuts the
  middle repeat, makes it seamless and stacks). Deterministic on one machine; rerun it after changing a palette.

## How they are drawn
- Cells (`src/cellular.wgsl`): the cell field is rounded between cell centres (never more than 2 units from the
  simulation's edge), each fragment reads its material's tile in **world space** (a slow warp and a second, turned,
  larger reading break repeats; a tiny per-cell and slow-patch variation on top), blends lit and unlit from the light
  at the place (daylight by depth, plus the lantern), and the ground's own edge is inked, lit where water lies up or
  left, deep where it lies down or right. The simulation has sand, mud, rock and pebble; silt-or-clay and
  bare-or-rubble rock are broad world patches chosen in the shader (presentation only).
- Floor and rocks (`src/memory.wgsl`): the same code (a test keeps the shared block equal), with the distance below
  the shape's top edge carried in the mesh's UV. A painted shape's colour has an alpha below 1 that names its tile.
- Mips: a pixel that covers several texels reads a smaller mip, so a zoomed-out view sees the tile's average.
- Ridges (`src/ridge.wgsl`, `view::move_parallax`): see the parallax below.

## Parallax
A layer with factor f follows the camera by f, so on screen a feature moves (1 - f) of the camera's move: far layers
(0.7) hardly move, near ones (0.45) move more. The strip repeats every 1,600 units and the layer's root moves by whole
periods to keep it centred on the view (`view::layer_root`, tested), so it never ends at any zoom. Vertically the layer
stands on the eased height of the floor under the camera, so it neither floats up a dive nor hangs in open water.
Lit and unlit blend from the daylight at the camera (a ridge is beyond the lantern); each layer sinks into the water
colour by distance, and further when zoomed out: the shape's own colour, never an overlay.

## Photographing a place
`cargo run --release -- --cells --place "home" --zoom 3 --photo target/paint-shots/home` (also `--at x,y`): see
`src/photo.rs`. Set `BEVY_ASSET_ROOT` to the repo when running the release binary from `target/`.
