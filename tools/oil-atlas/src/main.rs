//! Oil-paint the crab's puppet parts, keeping each part's exact outline.
//!
//! The painter (`crabpaint oilphoto`, crates/painter) paints one landscape picture at a
//! fixed size. A puppet part can't be painted alone (too small) or in place on the assembled crab (parts overlap,
//! so a part's hidden end would be painted with its neighbour's colours). So:
//!
//! 1. `layout`: take a big copy of the atlas (`tools/export-crab-parts.py --scale 2.5 --out DIR`), spread the parts
//!    over painter-sized sheets with plenty of room around each, and surround each part with a halo of its own edge
//!    colours fading into a dark neutral ground, so the painter's edge strokes have something sensible to pull.
//! 2. Paint every sheet (see `tools/paint-oil-crab.sh`).
//! 3. `pack`: cut each painted part back out with the game atlas's own alpha (grown by about a pixel and softened,
//!    so the painted edge survives), shrink it to the game atlas's size, and write `crab-atlas-oil.png` with exactly
//!    the same rectangles as `crab-atlas.png`, so `crab-parts.json` serves both.
//!
//! Usage:
//!
//!     oil-atlas layout <big-atlas.png> <big-parts.json> <workdir>
//!     oil-atlas pack <big-parts.json> <game-atlas.png> <game-parts.json> <workdir> <out.png>

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use serde_json::{Value, json};

/// The painter's canvas.
const SHEET_W: usize = 1920;
const SHEET_H: usize = 1080;
/// Room around each part on a sheet (sheet pixels).
const PAD: usize = 34;
/// How far the halo of edge colour reaches before it fades to the ground.
const HALO: usize = 14;
/// The ground between parts: a dark, slightly warm neutral.
const GROUND: [f32; 3] = [0.20, 0.16, 0.14];

struct Img {
    w: usize,
    h: usize,
    /// RGBA, 0..1, straight (not premultiplied) alpha.
    px: Vec<[f32; 4]>,
}

impl Img {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }

    fn load(path: &str) -> Self {
        let decoder = png::Decoder::new(std::io::BufReader::new(
            File::open(path).unwrap_or_else(|e| panic!("{path}: {e}")),
        ));
        let mut reader = decoder.read_info().expect("png header");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("png data");
        assert_eq!(
            info.bit_depth,
            png::BitDepth::Eight,
            "{path}: 8-bit PNG expected"
        );
        let ch = match info.color_type {
            png::ColorType::Rgba => 4,
            png::ColorType::Rgb => 3,
            other => panic!("{path}: unsupported colour type {other:?}"),
        };
        let (w, h) = (info.width as usize, info.height as usize);
        let px = (0..w * h)
            .map(|i| {
                let p = &buf[i * ch..i * ch + ch];
                [
                    p[0] as f32 / 255.0,
                    p[1] as f32 / 255.0,
                    p[2] as f32 / 255.0,
                    if ch == 4 { p[3] as f32 / 255.0 } else { 1.0 },
                ]
            })
            .collect();
        Self { w, h, px }
    }

    fn save(&self, path: &str, alpha: bool) {
        let file = BufWriter::new(File::create(path).unwrap_or_else(|e| panic!("{path}: {e}")));
        let mut enc = png::Encoder::new(file, self.w as u32, self.h as u32);
        enc.set_color(if alpha {
            png::ColorType::Rgba
        } else {
            png::ColorType::Rgb
        });
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::High);
        let mut out = Vec::with_capacity(self.w * self.h * 4);
        for p in &self.px {
            let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out.extend_from_slice(&[q(p[0]), q(p[1]), q(p[2])]);
            if alpha {
                out.push(q(p[3]));
            }
        }
        enc.write_header()
            .expect("png header")
            .write_image_data(&out)
            .expect("png data");
    }

    fn at(&self, x: usize, y: usize) -> [f32; 4] {
        self.px[y * self.w + x]
    }

    /// Area-average of the box [x0, x1) x [y0, y1) (fractional, in pixels), colour only.
    fn area(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> [f32; 3] {
        let mut sum = [0.0f32; 3];
        let mut wsum = 0.0;
        let (ix0, iy0) = (x0.floor().max(0.0) as usize, y0.floor().max(0.0) as usize);
        let (ix1, iy1) = (
            (x1.ceil() as usize).min(self.w),
            (y1.ceil() as usize).min(self.h),
        );
        for y in iy0..iy1 {
            let wy = ((y + 1) as f32).min(y1) - (y as f32).max(y0);
            for x in ix0..ix1 {
                let wx = ((x + 1) as f32).min(x1) - (x as f32).max(x0);
                let wgt = (wx * wy).max(0.0);
                let p = self.at(x, y);
                for k in 0..3 {
                    sum[k] += p[k] * wgt;
                }
                wsum += wgt;
            }
        }
        sum.map(|v| v / wsum.max(1e-6))
    }
}

fn read_json(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}")))
        .expect("json")
}

fn rect(part: &Value) -> [usize; 4] {
    let r = part["rect"].as_array().expect("rect");
    std::array::from_fn(|i| r[i].as_f64().unwrap() as usize)
}

fn bbox(part: &Value) -> [f32; 4] {
    let r = part["bbox"].as_array().expect("bbox");
    std::array::from_fn(|i| r[i].as_f64().unwrap() as f32)
}

/// Spread the parts over painter-sized sheets.
fn layout(big_atlas: &str, big_json: &str, dir: &str) {
    let atlas = Img::load(big_atlas);
    let parts = read_json(big_json);
    let parts = parts["parts"].as_array().expect("parts");
    // Shelf packing, tallest first, with PAD all round.
    let mut order: Vec<&Value> = parts.iter().collect();
    order.sort_by_key(|p| std::cmp::Reverse(rect(p)[3]));
    let mut placed = vec![];
    let (mut sheet, mut x, mut y, mut row_h) = (0usize, PAD, PAD, 0usize);
    for p in order {
        let [_, _, w, h] = rect(p);
        if x + w + PAD > SHEET_W {
            x = PAD;
            y += row_h + PAD;
            row_h = 0;
        }
        if y + h + PAD > SHEET_H {
            sheet += 1;
            x = PAD;
            y = PAD;
            row_h = 0;
        }
        placed.push((p, sheet, x, y));
        x += w + PAD;
        row_h = row_h.max(h);
    }
    let sheets = sheet + 1;
    let mut out = json!({"sheets": sheets, "parts": {}});
    for s in 0..sheets {
        // Paint the parts in, then grow a halo of their edge colours outwards.
        let mut img = Img::new(SHEET_W, SHEET_H);
        for &(p, _, sx, sy) in placed.iter().filter(|e| e.1 == s) {
            let [rx, ry, rw, rh] = rect(p);
            for j in 0..rh {
                for i in 0..rw {
                    img.px[(sy + j) * SHEET_W + sx + i] = atlas.at(rx + i, ry + j);
                }
            }
        }
        // coverage: how much of each pixel is the part (alpha), and its straight colour
        let alpha: Vec<f32> = img.px.iter().map(|p| p[3]).collect();
        let mut col: Vec<[f32; 3]> = img.px.iter().map(|p| [p[0], p[1], p[2]]).collect();
        let mut known: Vec<bool> = alpha.iter().map(|&a| a > 0.5).collect();
        let mut dist = vec![0usize; SHEET_W * SHEET_H];
        for step in 1..=HALO {
            let mut next = known.clone();
            let mut new_col = col.clone();
            for yy in 1..SHEET_H - 1 {
                for xx in 1..SHEET_W - 1 {
                    let i = yy * SHEET_W + xx;
                    if known[i] {
                        continue;
                    }
                    let mut sum = [0.0f32; 3];
                    let mut n = 0.0;
                    for (dx, dy) in [
                        (-1i32, 0i32),
                        (1, 0),
                        (0, -1),
                        (0, 1),
                        (-1, -1),
                        (1, 1),
                        (-1, 1),
                        (1, -1),
                    ] {
                        let k = ((yy as i32 + dy) as usize) * SHEET_W + (xx as i32 + dx) as usize;
                        if known[k] {
                            for c in 0..3 {
                                sum[c] += col[k][c];
                            }
                            n += 1.0;
                        }
                    }
                    if n > 0.0 {
                        new_col[i] = sum.map(|v| v / n);
                        next[i] = true;
                        dist[i] = step;
                    }
                }
            }
            known = next;
            col = new_col;
        }
        for i in 0..SHEET_W * SHEET_H {
            let a = alpha[i];
            let edge = if known[i] { col[i] } else { GROUND };
            // the halo fades towards the ground as it reaches out
            let fade = if a > 0.5 || !known[i] {
                0.0
            } else {
                (dist[i] as f32 / HALO as f32).powf(1.5)
            };
            let halo: [f32; 3] = std::array::from_fn(|k| edge[k] + (GROUND[k] - edge[k]) * fade);
            let c: [f32; 3] = std::array::from_fn(|k| img.px[i][k] * a + halo[k] * (1.0 - a));
            img.px[i] = [c[0], c[1], c[2], 1.0];
        }
        img.save(&format!("{dir}/sheet-{s}.png"), false);
    }
    for (p, s, x, y) in placed {
        out["parts"][p["name"].as_str().unwrap()] = json!({"sheet": s, "x": x, "y": y});
    }
    std::fs::write(
        format!("{dir}/layout.json"),
        serde_json::to_string_pretty(&out).unwrap(),
    )
    .unwrap();
    println!(
        "laid {} parts out on {sheets} sheet(s) in {dir}",
        parts.len()
    );
}

/// Cut the painted parts back out into an atlas laid out like the game's.
fn pack(big_json: &str, game_atlas: &str, game_json: &str, dir: &str, out_path: &str) {
    let big = read_json(big_json);
    let game = read_json(game_json);
    let lay = read_json(&format!("{dir}/layout.json"));
    let src = Img::load(game_atlas);
    let big_scale = big["px_per_unit"].as_f64().unwrap() as f32;
    let game_scale = game["px_per_unit"].as_f64().unwrap() as f32;
    let sheets: Vec<Img> = (0..lay["sheets"].as_u64().unwrap())
        .map(|s| {
            let img = Img::load(&format!("{dir}/painted-{s}.png"));
            assert!(
                img.w == SHEET_W && img.h <= SHEET_H,
                "painted-{s}.png is {}x{}",
                img.w,
                img.h
            );
            img
        })
        .collect();
    let mut out = Img::new(src.w, src.h);
    // Alpha: the drawn outline, grown by a pixel and softened, so the painted edge isn't shaved off.
    let a = |x: i32, y: i32| -> f32 {
        if x < 0 || y < 0 || x >= src.w as i32 || y >= src.h as i32 {
            0.0
        } else {
            src.at(x as usize, y as usize)[3]
        }
    };
    for part in game["parts"].as_array().unwrap() {
        let name = part["name"].as_str().unwrap();
        let big_part = big["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .expect("part in big atlas");
        let place = &lay["parts"][name];
        let sheet = &sheets[place["sheet"].as_u64().unwrap() as usize];
        // the painter may have cropped the canvas height evenly top and bottom
        let crop_y = (SHEET_H - sheet.h) as f32 / 2.0;
        let (sx, sy) = (
            place["x"].as_f64().unwrap() as f32,
            place["y"].as_f64().unwrap() as f32 - crop_y,
        );
        let [bx, by, _, _] = bbox(part);
        let [bbx, bby, _, _] = bbox(big_part);
        let [rx, ry, rw, rh] = rect(part);
        for j in 0..rh {
            for i in 0..rw {
                let (x, y) = ((rx + i) as i32, (ry + j) as i32);
                let mut grown = 0.0f32;
                let mut soft = 0.0f32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let v = a(x + dx, y + dy);
                        grown = grown.max(v);
                        soft += v * if dx == 0 && dy == 0 {
                            4.0
                        } else if dx == 0 || dy == 0 {
                            2.0
                        } else {
                            1.0
                        };
                    }
                }
                let soft = soft / 16.0;
                let alpha = a(x, y).max(0.5 * grown + 0.5 * soft).min(1.0);
                if alpha <= 0.0 {
                    continue;
                }
                // this game pixel's footprint in art units, then in the painted sheet
                let u0 = bx + i as f32 / game_scale;
                let v0 = by + j as f32 / game_scale;
                let map = |u: f32, v: f32| (sx + (u - bbx) * big_scale, sy + (v - bby) * big_scale);
                let (x0, y0) = map(u0, v0);
                let (x1, y1) = map(u0 + 1.0 / game_scale, v0 + 1.0 / game_scale);
                let c = sheet.area(x0, y0, x1, y1);
                out.px[y as usize * out.w + x as usize] = [c[0], c[1], c[2], alpha];
            }
        }
        // The painter flattens the broad light-to-shade modelling of each part a little (and pushes the reds towards
        // orange). Put the drawing's broad values back, keeping every stroke: add the difference between the two
        // pictures' blurred versions (blur within the part only), at SHADE strength.
        const R: i32 = 5;
        const SHADE: f32 = 0.75;
        let blurred = |img: &Img, x: i32, y: i32| -> Option<[f32; 3]> {
            let (mut sum, mut wsum) = ([0.0f32; 3], 0.0f32);
            for dy in -R..=R {
                for dx in -R..=R {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx < rx as i32
                        || yy < ry as i32
                        || xx >= (rx + rw) as i32
                        || yy >= (ry + rh) as i32
                    {
                        continue;
                    }
                    let a = src.at(xx as usize, yy as usize)[3];
                    let p = img.at(xx as usize, yy as usize);
                    for k in 0..3 {
                        sum[k] += p[k] * a;
                    }
                    wsum += a;
                }
            }
            (wsum > 0.5).then(|| sum.map(|v| v / wsum))
        };
        let mut fixed = vec![];
        for j in 0..rh {
            for i in 0..rw {
                let (x, y) = ((rx + i) as i32, (ry + j) as i32);
                let p = out.at(x as usize, y as usize);
                if p[3] <= 0.0 {
                    continue;
                }
                if let (Some(o), Some(q)) = (blurred(&src, x, y), blurred(&out, x, y)) {
                    let c: [f32; 3] = std::array::from_fn(|k| p[k] + SHADE * (o[k] - q[k]));
                    fixed.push((y as usize * out.w + x as usize, [c[0], c[1], c[2], p[3]]));
                }
            }
        }
        for (i, p) in fixed {
            out.px[i] = p;
        }
    }
    out.save(out_path, true);
    println!("wrote {out_path}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["layout", big_atlas, big_json, dir] => {
            std::fs::create_dir_all(dir).unwrap();
            layout(big_atlas, big_json, dir)
        }
        ["pack", big_json, game_atlas, game_json, dir, out] => {
            assert!(Path::new(dir).is_dir());
            pack(big_json, game_atlas, game_json, dir, out)
        }
        _ => {
            eprintln!(
                "usage:\n  oil-atlas layout <big-atlas.png> <big-parts.json> <workdir>\n  oil-atlas pack <big-parts.json> <game-atlas.png> <game-parts.json> <workdir> <out.png>"
            );
            std::process::exit(2);
        }
    }
}
