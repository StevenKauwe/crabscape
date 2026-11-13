//! The ground is drawn: at home, at desktop and phone size, the terrain shaders compile and the pixels below the
//! crab are painted ground, not water.
//!
//! Main once shipped green while the ground drew as plain water: a terrain shader used a word WGSL reserves
//! (`patch`), the browser refused to compile it, and nothing else in CI draws the world and looks. This loads the
//! game, records any shader the page fails to compile, and samples the picture where the ground must be (just
//! under the crab at home). It also caught the second fault: the 12-layer tile sheet read as a cube array (black).
//!
//! Usage: `terrain_drawn WEB_DIRECTORY [SCREENSHOT_DIRECTORY]`.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

/// Records every console line that says a shader failed to compile, from the page's first script on.
const HOOK: &str = r#"(() => {
  window.__shaderErrors = [];
  for (const k of ["log", "error", "warn"]) {
    const base = console[k].bind(console);
    console[k] = (...args) => {
      const text = args.map(String).join(" ");
      if (/failed to create shader module|ShaderModule|parsing error/i.test(text)) window.__shaderErrors.push(text.slice(0, 400));
      base(...args);
    };
  }
})()"#;

/// An 8-bit RGB or RGBA PNG (what Chrome's screenshots are) as `(width, height, rgb pixels)`.
fn decode_png(png: &[u8]) -> Result<(usize, usize, Vec<[u8; 3]>)> {
    if png.get(..8) != Some(&[137, 80, 78, 71, 13, 10, 26, 10][..]) {
        return Err("not a PNG".into());
    }
    let (mut at, mut w, mut h, mut channels, mut data) = (8, 0, 0, 0, Vec::new());
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap_or_default()) as usize;
        let kind = &png[at + 4..at + 8];
        let body = png.get(at + 8..at + 8 + len).ok_or("a cut PNG")?;
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes(body[0..4].try_into().unwrap_or_default()) as usize;
                h = u32::from_be_bytes(body[4..8].try_into().unwrap_or_default()) as usize;
                if body[8] != 8 || body[12] != 0 {
                    return Err("only 8-bit, non-interlaced PNGs".into());
                }
                channels = match body[9] {
                    2 => 3,
                    6 => 4,
                    t => return Err(format!("PNG colour type {t}")),
                };
            }
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data).map_err(|e| format!("{e:?}"))?;
    let stride = w * channels;
    let mut out = vec![0u8; h * stride];
    for y in 0..h {
        let line = raw
            .get(y * (stride + 1)..(y + 1) * (stride + 1))
            .ok_or("short PNG data")?;
        let filter = line[0];
        for x in 0..stride {
            let a = if x >= channels {
                out[y * stride + x - channels]
            } else {
                0
            };
            let b = if y > 0 { out[(y - 1) * stride + x] } else { 0 };
            let c = if y > 0 && x >= channels {
                out[(y - 1) * stride + x - channels]
            } else {
                0
            };
            let v = line[1 + x];
            out[y * stride + x] = match filter {
                0 => v,
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(b),
                3 => v.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => {
                    let p = i16::from(a) + i16::from(b) - i16::from(c);
                    let (pa, pb, pc) = (
                        (p - i16::from(a)).abs(),
                        (p - i16::from(b)).abs(),
                        (p - i16::from(c)).abs(),
                    );
                    v.wrapping_add(if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    })
                }
                f => return Err(format!("PNG filter {f}")),
            };
        }
    }
    let pixels = out.chunks(channels).map(|p| [p[0], p[1], p[2]]).collect();
    Ok((w, h, pixels))
}

/// Of the pixels in the box (fractions of the screen), the share that read as painted ground: warmer than the
/// water (red at least blue less a little) and not black, or lit like sand and rock. The water is a dark
/// blue-green; an unread tile sheet draws black.
fn ground_share(w: usize, h: usize, px: &[[u8; 3]], x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    let (mut ground, mut all) = (0usize, 0usize);
    for y in (h as f64 * y0) as usize..(h as f64 * y1) as usize {
        for x in (w as f64 * x0) as usize..(w as f64 * x1) as usize {
            let [r, g, b] = px[y * w + x].map(i32::from);
            all += 1;
            if (r + 8 >= b && r + g + b > 150) || r + g + b > 330 {
                ground += 1;
            }
        }
    }
    ground as f64 / all.max(1) as f64
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if !(2..=3).contains(&args.len()) {
        return Err("usage: terrain_drawn WEB_DIRECTORY [SCREENSHOT_DIRECTORY]".into());
    }
    let shots = args.get(2).map(Path::new);
    if let Some(dir) = shots {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let base = serve::start(Path::new(&args[1]), 1280, 800).map_err(|e| e.to_string())?;
    let profile = std::env::temp_dir().join(format!("crab-terrain-{}", std::process::id()));
    let mut b = Chrome::start_without_clipboard(
        "terrain-drawn",
        &profile,
        Options {
            fake_mic: false,
            slow: 1.0,
        },
    )?;
    b.install_script(HOOK)?;
    let mut failed = Vec::new();
    for (name, w, h, dpr) in [("desktop", 1280, 800, 1.0), ("phone", 390, 844, 3.0)] {
        b.device_metrics(w, h, dpr)?;
        b.go(&format!("{base}/?boring"))?;
        let began = Instant::now();
        while b.attr("data-crab").is_none() {
            if let Some(e) = b.trouble() {
                return Err(format!("{name}: {e}"));
            }
            if began.elapsed() > Duration::from_secs(90) {
                return Err(format!("{name}: the game did not start within 90 s"));
            }
            sleep(Duration::from_millis(200));
        }
        // Let the ground's pipelines compile and a few frames draw.
        sleep(Duration::from_secs(6));
        let errors = b.eval("JSON.stringify(window.__shaderErrors || [])")?;
        let errors = errors.as_str().unwrap_or("[]").to_string();
        let path =
            std::env::temp_dir().join(format!("crab-terrain-{name}-{}.png", std::process::id()));
        b.screenshot(&path)?;
        let png = std::fs::read(&path).map_err(|e| e.to_string())?;
        if let Some(dir) = shots {
            std::fs::write(dir.join(format!("terrain-{name}.png")), &png)
                .map_err(|e| e.to_string())?;
        }
        let (pw, ph, px) = decode_png(&png)?;
        // Just under the crab at home (the view frames it a little below the middle at both sizes), left of where its
        // memory fades out to the east and above the keys card: the home slope's sand.
        let share = ground_share(pw, ph, &px, 0.15, 0.45, 0.59, 0.65);
        println!(
            "{name} {w}x{h} at {dpr}x: ground under the crab {:.0}%, shader errors {errors}",
            share * 100.0
        );
        if errors != "[]" {
            failed.push(format!("{name}: a shader failed to compile: {errors}"));
        }
        if share < 0.6 {
            failed.push(format!(
                "{name}: only {:.0}% of the band under the crab at home is painted ground; the rest reads as water or black",
                share * 100.0
            ));
        }
    }
    if !failed.is_empty() {
        return Err(failed.join("\n"));
    }
    println!("PASS the ground is drawn at home, at desktop and phone size");
    Ok(())
}
