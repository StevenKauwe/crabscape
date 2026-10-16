# painter

The owner's own oil painter (Steven Kauwe), all rights reserved like the rest of the repo. An art tool: the game
never links it.

## What is in it
- **The oil painter** (`oil.rs`): a `Painting` (1920x1080 canvas: colour, a flat copy, paint thickness), the tools a
  painter reaches for (`Tool::Flat`, `Filbert`, `Round`, `Knife`), bristle-by-bristle `Stroke`s that pick up wet
  paint and leave ridges that are lit at the end (`Painting::lit`); shapes painted along their form (`paint_body`,
  `paint_form`, `paint_contour`, `paint_line`, `sweep`); and `run_photo`, which paints a picture as a painter does
  from life (block in, find the regions, paint them biggest first, then touch in with small brushes only where
  the canvas is still wrong).
- **Watercolour / mixed modes** (`WATER`, `MIXED`, `PIGMENT` switches; `wash_shape`).
- **A pure-Rust port of p5.brush**: `brush_fill` (the layered, bleeding watercolour fill), `brush_wash` (one flat
  layer of pigment), `brush_stroke` (the stamped ink line), built on `FillPoly` and `Mask`, mixing as pigment with
  `spectral.rs` (Kubelka-Munk, 38 bands).

## Use
```sh
cargo run -p painter --profile art --bin crabpaint -- oilphoto in.png out.png [fx fy reach] \
    [--attention on|off] [--ground ochre|subject] [--nocturne G] [--key K]
cargo run -p painter --profile art --bin crabpaint -- oilmix out.png    # RGB vs pigment mixing test chart
```
`fx fy reach`: where the painter looks (fractions of the picture; default 0.5 0.4 0.34). Each flag can also be set
by its environment variable (`OIL_ATTENTION=off` = `--attention off`, `OIL_GROUND=subject`, `OIL_NOCTURNE`,
`OIL_KEY`). `OIL_EDGE` (the brush port's edge deposit) is still environment-only. Always use `--profile art`
(or `--release`): unoptimised it is very slow.

As a library: `painter::{Painting, brush_fill, brush_wash, brush_stroke, oil::paint_body, ...}`.

## Tests
`cargo test -p painter` paints the brush port and an oil shape and checks they are deterministic and in place; the
heavier full-photo test is `cargo test -p painter -- --ignored`.
