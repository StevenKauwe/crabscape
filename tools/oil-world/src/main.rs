//! The painted world's material tiles (`docs/design/painted-world.md`).
//!
//!     oil-world swatch <dir>            writes <material>-<lit|unlit>.png: a 3 x 3 repeat of a tileable
//!                                       swatch of each material, for `crabpaint oilphoto` to paint
//!     oil-world cut <dir> <out.png>     cuts the middle tile out of each painted swatch, makes it seamless,
//!                                       and stacks them (lit, then unlit, in `MATERIALS` order) into one PNG
//!     oil-world ridges <dir>            writes ridge-<far|near>-<lit|unlit>.png: three repeats of a far rock
//!                                       ridge (spires on an ink ground) for the painter
//!     oil-world cut-ridges <dir> <out.png>   cuts the middle repeat of each painted ridge, makes it repeat
//!                                       sideways, gives it its silhouette as alpha, and stacks them (far lit,
//!                                       far unlit, near lit, near unlit)
//!
//! Deterministic: the same inputs give the same bytes. The painter runs between the two steps
//! (`tools/paint-world.sh`).

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Layer order in the stacked PNG: the six lit tiles, then the six unlit ones. The shaders read it.
const MATERIALS: [&str; 6] = ["sand", "silt", "clay", "pebble", "rubble", "rock"];
/// One swatch tile's side in pixels (the painter paints 3 x 3 of them).
const SWATCH: usize = 192;
/// The finished tile's side in pixels.
const TILE: usize = 96;

type Rgb = [f32; 3];

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x85eb_ca6b)
        ^ (y as u32).wrapping_mul(0xc2b2_ae35)
        ^ seed.wrapping_mul(0x27d4_eb2f);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Value noise that repeats every `period` lattice cells.
fn pnoise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let at = |dx: i32, dy: i32| {
        hash(
            (xi as i32 + dx).rem_euclid(period),
            (yi as i32 + dy).rem_euclid(period),
            seed,
        )
    };
    let a = at(0, 0) + (at(1, 0) - at(0, 0)) * ux;
    let b = at(0, 1) + (at(1, 1) - at(0, 1)) * ux;
    a + (b - a) * uy
}

/// Fractal noise over the unit tile (u, v in 0..1), repeating.
fn fbm(u: f32, v: f32, base: i32, octaves: i32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
    for o in 0..octaves {
        let period = base << o;
        sum += amp
            * pnoise(
                u * period as f32,
                v * period as f32,
                period,
                seed + o as u32 * 7,
            );
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

/// Nearest and second-nearest jittered point on a repeating grid: (f1, f2, id of the nearest).
fn voronoi(u: f32, v: f32, n: i32, seed: u32) -> (f32, f32, i32, i32) {
    let (x, y) = (u * n as f32, v * n as f32);
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    let (mut f1, mut f2, mut id) = (9.0f32, 9.0f32, (0, 0));
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (gx, gy) = (cx + dx, cy + dy);
            let (wx, wy) = (gx.rem_euclid(n), gy.rem_euclid(n));
            let px = gx as f32 + 0.15 + 0.7 * hash(wx, wy, seed);
            let py = gy as f32 + 0.15 + 0.7 * hash(wx, wy, seed + 1);
            let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
                id = (wx, wy);
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2, id.0, id.1)
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn srgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// One material's colour at a point of its tile, in the lantern's light. The unlit tile is the same ground in the
/// dark: darker, cooler and flatter (`unlit`).
fn material_colour(name: &str, u: f32, v: f32, unlit: bool) -> Rgb {
    let c = match name {
        "sand" => {
            let ripple = fbm(u * 1.0, v * 4.0, 2, 3, 11);
            let grain = hash((u * SWATCH as f32) as i32, (v * SWATCH as f32) as i32, 5);
            let base = mix(
                srgb(122, 102, 72),
                srgb(206, 182, 134),
                fbm(u, v, 3, 4, 3).powf(1.3),
            );
            let c = mix(base, srgb(104, 86, 60), (ripple - 0.45).max(0.0) * 2.0);
            if grain > 0.93 {
                mix(c, srgb(214, 192, 146), 0.7)
            } else if grain < 0.05 {
                mix(c, srgb(110, 92, 66), 0.6)
            } else {
                c
            }
        }
        "silt" => {
            let m = fbm(u, v, 4, 5, 21);
            let c = mix(srgb(84, 86, 72), srgb(116, 112, 92), m);
            mix(c, srgb(66, 72, 64), fbm(u, v, 9, 2, 22) * 0.5)
        }
        "clay" => {
            let (f1, f2, _, _) = voronoi(u, v, 5, 31);
            let band = ((v * 6.0 + fbm(u, v, 3, 3, 33) * 1.4).sin() * 0.5 + 0.5) * 0.45;
            let c = mix(
                srgb(128, 80, 56),
                srgb(170, 112, 78),
                band + fbm(u, v, 6, 3, 32) * 0.3,
            );
            // Shrinkage cracks.
            mix(
                c,
                srgb(70, 40, 30),
                (1.0 - ((f2 - f1) / 0.06)).clamp(0.0, 1.0) * 0.55,
            )
        }
        "pebble" => {
            let (f1, f2, ix, iy) = voronoi(u, v, 6, 41);
            let tone = hash(ix, iy, 43);
            let stone = mix(srgb(92, 88, 80), srgb(158, 146, 124), tone);
            let warm = mix(stone, srgb(150, 110, 78), hash(ix, iy, 44) * 0.5);
            // A rounded stone: lit top left, shaded bottom right (the stone's own light).
            let round = (f1 / 0.5).clamp(0.0, 1.0);
            let c = mix(warm, warm.map(|x| x * 0.62), round * round);
            mix(c, srgb(40, 36, 32), ((f2 - f1) < 0.05) as u8 as f32 * 0.8)
        }
        "rubble" => {
            let (f1, f2, ix, iy) = voronoi(u, v, 7, 51);
            let pale = mix(srgb(176, 164, 146), srgb(212, 196, 172), hash(ix, iy, 53));
            let c = mix(
                pale,
                srgb(196, 150, 124),
                (hash(ix, iy, 54) > 0.85) as u8 as f32 * 0.35,
            );
            let c = mix(c, c.map(|x| x * 0.7), (f1 / 0.45).clamp(0.0, 1.0) * 0.6);
            mix(c, srgb(52, 40, 38), ((f2 - f1) < 0.07) as u8 as f32 * 0.85)
        }
        _ => {
            // Rock: slate, in leaning layers, with cracks.
            let slope = u * 0.6 + v;
            let layer = fbm(u * 0.0 + 1.0, slope.rem_euclid(1.0), 5, 3, 61);
            let c = mix(srgb(48, 58, 68), srgb(94, 104, 112), layer);
            let (f1, f2, _, _) = voronoi(u, v, 4, 62);
            let c = mix(c, srgb(34, 38, 46), ((f2 - f1) < 0.045) as u8 as f32 * 0.7);
            mix(c, srgb(128, 134, 134), fbm(u, v, 12, 2, 63).powi(4) * 0.6)
        }
    };
    if unlit { unlit_of(c) } else { c }
}

/// The same ground with no lantern on it: a third of the light, pulled towards the water's blue-green, and flatter.
fn unlit_of(c: Rgb) -> Rgb {
    let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let flat = mix([grey; 3], c, 0.6);
    [flat[0] * 0.17, flat[1] * 0.24, flat[2] * 0.31]
}

fn write_png(path: &Path, w: usize, h: usize, rgb: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::High);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgb).map_err(|e| e.to_string())
}

fn read_png(path: &Path) -> Result<Picture, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too big")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let channels = info.color_type.samples();
    let (w, h) = (info.width as usize, info.height as usize);
    let rgb = (0..w * h)
        .flat_map(|i| (0..3).map(move |k| (i, k)))
        .map(|(i, k)| buf[i * channels + k.min(channels - 1)])
        .collect();
    Ok(Picture { w, h, rgb })
}

fn to_byte(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn swatch(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for name in MATERIALS {
        for unlit in [false, true] {
            let side = SWATCH * 3;
            let mut rgb = Vec::with_capacity(side * side * 3);
            for y in 0..side {
                for x in 0..side {
                    let u = (x % SWATCH) as f32 / SWATCH as f32;
                    let v = (y % SWATCH) as f32 / SWATCH as f32;
                    rgb.extend(material_colour(name, u, v, unlit).map(to_byte));
                }
            }
            let file = format!("{name}-{}.png", if unlit { "unlit" } else { "lit" });
            write_png(&dir.join(file), side, side, &rgb)?;
        }
    }
    Ok(())
}

/// A painted picture of any size, read at normalised coordinates with bilinear filtering.
struct Picture {
    w: usize,
    h: usize,
    rgb: Vec<u8>,
}

impl Picture {
    fn texel(&self, x: i32, y: i32, k: usize) -> f32 {
        let x = x.clamp(0, self.w as i32 - 1) as usize;
        let y = y.clamp(0, self.h as i32 - 1) as usize;
        self.rgb[(y * self.w + x) * 3 + k] as f32 / 255.0
    }

    /// Colour at (u, v) in 0..1 across the whole picture.
    fn at(&self, u: f32, v: f32) -> Rgb {
        let (x, y) = (u * self.w as f32 - 0.5, v * self.h as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let mut out = [0.0; 3];
        for (k, o) in out.iter_mut().enumerate() {
            let t = |dx: i32, dy: i32| self.texel(x0 as i32 + dx, y0 as i32 + dy, k);
            let top = t(0, 0) + (t(1, 0) - t(0, 0)) * fx;
            let bottom = t(0, 1) + (t(1, 1) - t(0, 1)) * fx;
            *o = top + (bottom - top) * fy;
        }
        out
    }
}

/// The painted swatch's middle tile at (u, v) in 0..1, made to repeat: blended with a copy shifted half a tile,
/// weighted so the seams fall where the shifted copy is whole. `square`: the swatch repeats down as well as across.
fn seamless(pic: &Picture, u: f32, v: f32, square: bool) -> Rgb {
    let middle = |u: f32, v: f32| {
        if square {
            pic.at((1.0 + u) / 3.0, (1.0 + v) / 3.0)
        } else {
            pic.at((1.0 + u) / 3.0, v)
        }
    };
    let wx = (std::f32::consts::PI * u).sin().powi(2);
    let wy = if square {
        (std::f32::consts::PI * v).sin().powi(2)
    } else {
        1.0
    };
    let w = wx * wy;
    let shifted = middle((u + 0.5) % 1.0, if square { (v + 0.5) % 1.0 } else { v });
    mix(shifted, middle(u, v), w)
}

/// Average of 4 x 4 samples across one destination pixel.
fn pixel(dest_x: usize, dest_y: usize, w: usize, h: usize, f: &dyn Fn(f32, f32) -> Rgb) -> Rgb {
    let mut sum = [0.0; 3];
    for sy in 0..4 {
        for sx in 0..4 {
            let c = f(
                (dest_x as f32 + (sx as f32 + 0.5) / 4.0) / w as f32,
                (dest_y as f32 + (sy as f32 + 0.5) / 4.0) / h as f32,
            );
            for k in 0..3 {
                sum[k] += c[k] / 16.0;
            }
        }
    }
    sum
}

fn cut(dir: &Path, out: &Path) -> Result<(), String> {
    let mut stacked = Vec::new();
    for unlit in [false, true] {
        for name in MATERIALS {
            let file = format!("{name}-{}.png", if unlit { "unlit" } else { "lit" });
            let pic = read_png(&dir.join(&file))?;
            for y in 0..TILE {
                for x in 0..TILE {
                    let c = pixel(x, y, TILE, TILE, &|u, v| seamless(&pic, u, v, true));
                    stacked.extend(c.map(to_byte));
                }
            }
        }
    }
    write_png(out, TILE, TILE * MATERIALS.len() * 2, &stacked)
}

/// Ridge image size: one repeat is `RIDGE_W` wide; the game stretches it over 2,400 world units.
const RIDGE_W: usize = 384;
const RIDGE_H: usize = 224;
const LAYERS: [&str; 2] = ["far", "near"];

/// A repeating ridge of spires: (height 0..1 above the valley floor, which flank (-1 lit, +1 shaded), facet noise).
fn ridge(u: f32, layer: usize) -> (f32, f32, f32) {
    let (count, seed, steep) = if layer == 0 {
        (8, 71, 0.95)
    } else {
        (5, 81, 0.8)
    };
    let (mut best, mut flank) = (0.0f32, -1.0f32);
    for i in 0..count {
        let c = (i as f32 + 0.15 + 0.7 * hash(i, 0, seed)) / count as f32;
        let a = 0.45 + 0.55 * hash(i, 1, seed);
        let w = (0.5 + 0.55 * hash(i, 2, seed)) / count as f32;
        let mut du = u - c;
        du -= du.round();
        let h = a * (1.0 - du.abs() / w).max(0.0).powf(steep);
        if h > best {
            best = h;
            flank = if du < 0.0 { -1.0 } else { 1.0 };
        }
    }
    // Broken, ledged rock: slow swells and a finer jag, and the steep flanks stepped.
    let jag = fbm(u, 0.3, 14, 3, seed + 5) - 0.5;
    let fine = fbm(u, 0.6, 40, 2, seed + 6) - 0.5;
    let stepped = (best * 9.0).floor() / 9.0 * 0.55 + best * 0.45;
    let h = 0.35 + 0.65 * stepped + (jag * 0.10 + fine * 0.035) * (best > 0.02) as u8 as f32;
    (h.clamp(0.0, 1.0), flank, fbm(u, best, 16, 3, seed + 9))
}

/// Is this pixel of the ridge image inside the rock? (Rows count from the top.)
fn ridge_inside(x: usize, y: usize, layer: usize) -> bool {
    let u = (x as f32 + 0.5) / RIDGE_W as f32;
    let (h, _, _) = ridge(u, layer);
    (y as f32 + 0.5) >= RIDGE_H as f32 * (1.0 - h)
}

fn ridge_colour(x: usize, y: usize, layer: usize, unlit: bool) -> Rgb {
    let ink = srgb(26, 20, 18);
    let u = (x as f32 + 0.5) / RIDGE_W as f32;
    let (h, flank, facet) = ridge(u, layer);
    let top = RIDGE_H as f32 * (1.0 - h);
    let depth = (y as f32 + 0.5 - top).max(0.0);
    let (lit_c, base_c, shade_c) = if layer == 0 {
        (srgb(112, 124, 132), srgb(76, 90, 100), srgb(46, 58, 72))
    } else {
        (srgb(124, 122, 112), srgb(86, 88, 84), srgb(52, 56, 64))
    };
    let side = if flank < 0.0 { lit_c } else { shade_c };
    let c = mix(side, base_c, (depth / 90.0).min(1.0) * 0.7 + facet * 0.3);
    // A dark warm ink edge along the silhouette, a little highlight just inside the lit side.
    let edge = (1.0 - depth / 5.0).clamp(0.0, 1.0);
    let c = mix(c, ink, edge * 0.85);
    let c = mix(c, c.map(|v| v * 0.7), (depth / 230.0).clamp(0.0, 1.0));
    if y as f32 + 0.5 < top - 1.0 {
        return ink;
    }
    if unlit { unlit_of(c) } else { c }
}

fn ridges(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for (layer, name) in LAYERS.iter().enumerate() {
        for unlit in [false, true] {
            let w = RIDGE_W * 3;
            let mut rgb = Vec::with_capacity(w * RIDGE_H * 3);
            for y in 0..RIDGE_H {
                for x in 0..w {
                    rgb.extend(ridge_colour(x % RIDGE_W, y, layer, unlit).map(to_byte));
                }
            }
            let file = format!("ridge-{name}-{}.png", if unlit { "unlit" } else { "lit" });
            write_png(&dir.join(file), w, RIDGE_H, &rgb)?;
        }
    }
    Ok(())
}

fn cut_ridges(dir: &Path, out: &Path) -> Result<(), String> {
    let mut stacked = Vec::new();
    for (layer, name) in LAYERS.iter().enumerate() {
        for unlit in [false, true] {
            let file = format!("ridge-{name}-{}.png", if unlit { "unlit" } else { "lit" });
            let pic = read_png(&dir.join(&file))?;
            for y in 0..RIDGE_H {
                // The rock's foot fades away over its lowest third: no edge, whatever lies behind it.
                let foot = {
                    let t = ((RIDGE_H as f32 - y as f32) / (RIDGE_H as f32 * 0.32)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                for x in 0..RIDGE_W {
                    let c = pixel(x, y, RIDGE_W, RIDGE_H, &|u, v| seamless(&pic, u, v, false));
                    // Alpha: the silhouette, softened a pixel.
                    let near = |dx: i32, dy: i32| {
                        let xx = (x as i32 + dx).rem_euclid(RIDGE_W as i32) as usize;
                        let yy = (y as i32 + dy).clamp(0, RIDGE_H as i32 - 1) as usize;
                        ridge_inside(xx, yy, layer) as u8 as f32
                    };
                    let alpha =
                        (near(0, 0) * 4.0 + near(-1, 0) + near(1, 0) + near(0, -1) + near(0, 1))
                            / 8.0;
                    stacked.extend(c.map(to_byte));
                    stacked.push(to_byte(alpha * foot));
                }
            }
        }
    }
    let file = File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), RIDGE_W as u32, (RIDGE_H * 4) as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::High);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&stacked).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["swatch", dir] => swatch(Path::new(dir)),
        ["cut", dir, out] => cut(Path::new(dir), Path::new(out)),
        ["ridges", dir] => ridges(Path::new(dir)),
        ["cut-ridges", dir, out] => cut_ridges(Path::new(dir), Path::new(out)),
        _ => Err("usage: oil-world swatch <dir> | oil-world cut <dir> <out.png>".into()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}
