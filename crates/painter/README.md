# painter

**Moved from music-to-movie-magic: the owner's own code** (Steven Kauwe). `src/oil.rs` is
`studio/examples/oh-brassica/src/oil.rs` and `src/spectral.rs` is `studio/crates/look/src/spectral.rs`, copied on
2026-10-15 so CrabScape's art pipeline is pure Rust and needs nothing outside this repository. Not under any
licence (all rights reserved, like the rest of the repo).

What changed in the move (behaviour is identical; see "Checked" below):
- left out the film-only parts (`oil` / `oilclip` paint a frame from the studio's recorded shapes; the cast list
  and colour conversion they use);
- items are `pub`, so it works as a library; `set_zoom` / `set_scene` set what the film's frame painter used to;
- `run_photo`'s CrabScape switches (`OIL_KEY`, `OIL_GROUND`, `OIL_NOCTURNE`, `OIL_ATTENTION`) also come as a
  `PhotoOptions`; `run_photo` still reads them from the environment;
- rustfmt (at the owner's wide line length, `rustfmt.toml`) and one unneeded `mut` removed; clippy's style lints
  that the code was not written against are allowed in `lib.rs` rather than the code being reworded.

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
  `spectral.rs` (Kubelka-Munk, 38 bands). It matched the JS library on a test chart in the studio.

## Use
```sh
cargo run -p painter --profile art --bin crabpaint -- oilphoto in.png out.png [fx fy reach] \
    [--attention on|off] [--ground ochre|subject] [--nocturne G] [--key K]
cargo run -p painter --profile art --bin crabpaint -- oilmix out.png    # RGB vs pigment mixing test chart
```
`fx fy reach`: where the painter looks (fractions of the picture; default 0.5 0.4 0.34). Every flag defaults to its
old environment variable (`OIL_ATTENTION=off` = `--attention off`, `OIL_GROUND=subject`, `OIL_NOCTURNE`,
`OIL_KEY`). `OIL_EDGE` (the brush port's edge deposit) is still environment-only. Always use `--profile art`
(or `--release`): unoptimised it is very slow.

As a library: `painter::{Painting, brush_fill, brush_wash, brush_stroke, oil::paint_body, ...}`.

## Checked
The old binary (`film-oh-brassica oilphoto`, built from the studio on 2026-10-15) and `crabpaint oilphoto` give
byte-identical PNGs for: the crab parts sheet (`--attention off --ground subject`), a game screenshot with the
default attention, a nocturne (`OIL_NOCTURNE=2.2 OIL_KEY=0.1`), and `oilmix`.

`cargo test -p painter` paints the brush port and an oil shape and checks they are deterministic and in place; the
heavier full-photo test is `cargo test -p painter -- --ignored`.
