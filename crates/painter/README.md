# painter

The oil painter for CrabScape's art: a thin layer over **brushwork**
(https://github.com/StevenKauwe/brushwork, MIT), which is **upstream**. Fixes to the painting itself go there, get
tagged, and this crate bumps its pin. An art tool: the game never links it.

## What is in it
- `src/lib.rs`: brushwork's painter under the paths CrabScape has always used: `painter::oil::*` (a `Painting`,
  the tools `Tool::Flat`, `Filbert`, `Round`, `Knife`, bristle-by-bristle `Stroke`s, `paint_body`, the p5.brush port
  `brush_fill` / `brush_wash` / `brush_stroke`), `run_photo_with` + `PhotoOptions` (paints a picture as a painter
  does from life), and `painter::spectral` (Kubelka-Munk pigment mixing).
- `src/bin/crabpaint.rs`: the command line.
- The `gpu` feature turns on brushwork's GPU backend (wgpu). It is off by default and in CI.

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

## Bumping brushwork
Change the tag in `Cargo.toml`, then repaint the atlases (`tools/paint-oil.sh`, after `crabrig parts` and
`crabrig export`). The art CI job fails if a committed atlas differs: a brushwork release that changes pixels
comes with the repainted atlases in the same PR.

## Tests
`cargo test -p painter` paints the brush port and an oil shape and checks they are deterministic and in place; the
heavier full-photo test is `cargo test -p painter -- --ignored`. The painter's steadiness tests run in brushwork's
own CI.
