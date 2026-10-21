//! The oil painter: every shape is covered with bristle strokes that follow its form, each stroke streaked bristle
//! by bristle, picking up the wet paint under it and leaving ridges of thickness; the ridges are then lit from the
//! upper left. `run_photo` (`crabpaint oilphoto`) paints a whole picture this way.
//!
//! `set_zoom`/`set_scene` set the zoom and scene for the next strokes; `run_photo`'s switches also come as a
//! `PhotoOptions` (or from the same environment variables).

use std::sync::atomic::{AtomicBool, Ordering};
/// Mix picked-up paint as pigment (Kubelka-Munk, 38 bands) instead of averaging RGB.
pub static PIGMENT: AtomicBool = AtomicBool::new(true);
/// Everything in watercolour, with the ported brush (to check it against the original engine).
pub static WATER: AtomicBool = AtomicBool::new(false);
/// Mixed media: the scenery in watercolour on paper, the cast in oils over it.
pub static MIXED: AtomicBool = AtomicBool::new(false);
/// Scene lighting and a point of attention (see `Scene`).
pub static LIGHT: AtomicBool = AtomicBool::new(true);

pub const WI: usize = 1920;
pub const HI: usize = 1080;
/// The smallest brush, in pixels. Nothing in the picture is finer than this.
pub const SMALLEST: f32 = 7.0;
/// How much of the wet paint under a stroke its brush carries along. What is under a stroke changes from frame to
/// frame as things move, so in a film this is kept modest or the colours flicker.
pub const STEADY_PICKUP: f32 = 0.18;
/// How much the paint stands up (ridges, lips, gloss). 1 = thick impasto; lower = paint as a means to an end.
pub const BODY: f32 = 0.4;
/// How much extra pigment a wash leaves along its edge (see `Mask::composite`).
pub const EDGE_DEPOSIT: f64 = 2.5;

pub fn h32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}
pub fn hf(a: u32, b: u32) -> f32 {
    (h32(a.wrapping_mul(0x9E37_79B1) ^ h32(b.wrapping_add(0x85EB_CA6B))) >> 8) as f32 / 16_777_216.0
}
/// Smooth noise in [0, 1).
pub fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (tx, ty) = (x - ix, y - iy);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let g = |dx: i32, dy: i32| hf((ix as i32 + dx) as u32 ^ seed.wrapping_mul(977), (iy as i32 + dy) as u32);
    let a = g(0, 0) + (g(1, 0) - g(0, 0)) * sx;
    let b = g(0, 1) + (g(1, 1) - g(0, 1)) * sx;
    a + (b - a) * sy
}
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct Painting {
    pub col: Vec<[f32; 3]>,
    /// The picture in plain colour, shape by shape, with no brushwork. STEADINESS: a brush picks up "what is
    /// under it" from here, not from the painted canvas, whose exact state depends on every stroke before and
    /// would make one stroke's small change spread through all the strokes over it.
    pub flat: Vec<[f32; 3]>,
    /// Paint thickness.
    pub thick: Vec<f32>,
    pub strokes: u32,
    /// The camera's zoom on the picture as a whole (the paint's relief is lit as if seen from that close).
    pub view: f32,
}

/// The painter's tools. Each job gets the one a painter would reach for.
#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    /// A wide flat: square ends, crisp sides, strong bristle marks. Backgrounds, and (on its chisel edge) cutting
    /// one colour in against another.
    Flat,
    /// A filbert: rounded ends, softer sides. Modelling the forms of the subjects.
    Filbert,
    /// A small round: a few fine hairs, comes to a point, flows. Lines, outlines, small accents.
    Round,
    /// A palette knife: no bristles, a flat slab of thick paint with hard edges that breaks over what is under it.
    /// Highlights.
    Knife,
}

#[derive(Clone, Copy)]
pub struct Stroke {
    pub tool: Tool,
    /// What the brush picked up where the whole stroke landed (pieces of one sweep share it).
    pub picked: Option<[f32; 3]>,
    /// How far along its whole stroke this piece starts, in pixels (so the bristles' marks carry on).
    pub along: f32,
    pub c: [f32; 2],
    pub angle: f32,
    pub len: f32,
    pub width: f32,
    pub bend: f32,
    pub col: [f32; 3],
    /// A second colour some bristles carry (broken colour).
    pub alt: [f32; 3],
    pub opacity: f32,
    /// How much wet paint the brush picks up from the canvas.
    pub pickup: f32,
    pub load: f32,
    /// Part of a longer stroke: the bristles it shares with the rest, and whether the brush lands / lifts here.
    pub part: Option<(u32, bool, bool)>,
    /// Where this piece sits along the whole stroke it belongs to (0..1 at its two ends); (0, 1) on its own.
    pub span: (f32, f32),
}

impl Painting {
    pub fn new() -> Painting {
        // a warm toned ground
        let mut col = vec![[0.0; 3]; WI * HI];
        for y in 0..HI {
            for x in 0..WI {
                let n = noise(x as f32 / 140.0, y as f32 / 140.0, 7) * 0.06 - 0.03;
                col[y * WI + x] = if MIXED.load(Ordering::Relaxed) || WATER.load(Ordering::Relaxed) { [0.955 + n * 0.3, 0.93 + n * 0.3, 0.875 + n * 0.3] } else { [0.60 + n, 0.50 + n, 0.37 + n] };
            }
        }
        Painting { flat: col.clone(), col, thick: vec![0.0; WI * HI], strokes: 0, view: 1.0 }
    }

    /// One stroke. `mask` (with its origin and size) limits it softly to a shape.
    pub fn stroke(&mut self, s: &Stroke, mask: Option<(&[f32], [i32; 2], [usize; 2])>) {
        self.strokes += 1;
        let (id, lands, lifts) = match s.part {
            Some((id, a, b)) => (id, a, b),
            None => (self.strokes, true, true),
        };
        let (sn, cs) = s.angle.sin_cos();
        let reach = s.len / 2.0 + s.width;
        let (x0, x1) = (((s.c[0] - reach).floor() as i32).max(0), ((s.c[0] + reach).ceil() as i32).min(WI as i32 - 1));
        let (y0, y1) = (((s.c[1] - reach).floor() as i32).max(0), ((s.c[1] + reach).ceil() as i32).min(HI as i32 - 1));
        if x0 > x1 || y0 > y1 {
            return;
        }
        // the paint already there, where the brush lands
        let start = [s.c[0] - cs * s.len * 0.4, s.c[1] - sn * s.len * 0.4];
        // a stroke that lands off the canvas has nothing to pick up (not whatever happens to be at the canvas edge)
        let under = s.picked.unwrap_or_else(|| flat_under(&self.flat, start[0], start[1], s.width * 0.3, s.col));
        let premix = PREMIX.with(|p| p.get());
        let fade = FADE.with(|f| f.get());
        if fade <= 0.0 {
            return;
        }
        let s = &if premix < 1.0 || fade < 1.0 {
            let tint = |c: [f32; 3]| -> [f32; 3] { std::array::from_fn(|k| under[k] + (c[k] - under[k]) * premix) };
            Stroke { col: tint(s.col), alt: tint(s.alt), load: s.load * (0.35 + 0.65 * premix), opacity: s.opacity * fade, ..*s }
        } else {
            *s
        };
        // what the brush's two colours become with more and more of the picked-up paint worked into them: as
        // pigments (blue into yellow goes green and stays rich), worked out once per stroke
        let pigment = PIGMENT.load(Ordering::Relaxed);
        let mut worked = [[[0.0f32; 3]; 9]; 2];
        if pigment {
            let d = |c: [f32; 3]| [c[0] as f64, c[1] as f64, c[2] as f64];
            let ku = crate::spectral::ks(d(under));
            for (w, own) in [s.col, s.alt].into_iter().enumerate() {
                let ko = crate::spectral::ks(d(own));
                for (i, slot) in worked[w].iter_mut().enumerate() {
                    let f = i as f64 / 8.0 * 0.85;
                    let mixed: [f64; 38] = std::array::from_fn(|b| ko[b] * (1.0 - f) + ku[b] * f);
                    let rgb = crate::spectral::colour(&mixed).0;
                    *slot = [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32];
                }
            }
        }
        let z = zoom();
        let nb = match s.tool {
            Tool::Flat => (s.width / z / 4.5).max(4.0),
            Tool::Filbert => (s.width / z / 5.5).max(3.0),
            Tool::Round => (s.width / z / 3.0).clamp(2.0, 4.0),
            Tool::Knife => 1.0,
        };
        let (marks, lips, edge_soft) = match s.tool {
            // BODY scales the paint's relief: 1 is thick impasto, the default is thin paint that only just shows
            // how it was put on
            Tool::Flat => (0.26 * BODY, 0.6 * BODY, 1.2),
            Tool::Filbert => (0.2 * BODY, 0.5 * BODY, 2.2),
            Tool::Round => (0.07 * BODY, 0.2 * BODY, 1.4),
            Tool::Knife => (0.0, 1.1 * BODY, 0.8),
        };
        // the whole stroke's length (a piece of a sweep knows its share of it), and how far in from its ends the
        // bristles touch down and lift off: about a brush width, less on a touch too short to have that much
        let whole_len = (s.len / (s.span.1 - s.span.0).max(1e-3)).max(1.0);
        let end_w = s.width * (whole_len / (5.0 * s.width)).min(1.0);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = (x as f32 + 0.5 - s.c[0], y as f32 + 0.5 - s.c[1]);
                let u = dx * cs + dy * sn;
                let pos = u / s.len + 0.5;
                if !(0.0..=1.0).contains(&pos) {
                    continue;
                }
                let v = -dx * sn + dy * cs - s.bend * (u * u) / s.len;
                // the brush's footprint: a little narrower where it lands and lifts, ragged at the sides
                // position along the whole stroke: pressure, paint and dryness go by it
                let whole = s.span.0 + (s.span.1 - s.span.0) * pos;
                let arc = (whole * std::f32::consts::PI).sin().max(0.0);
                let taper = match s.tool {
                    Tool::Flat | Tool::Knife => 0.94 + 0.06 * arc,
                    Tool::Filbert => 0.70 + 0.30 * arc.powf(0.5),
                    Tool::Round => 0.35 + 0.65 * arc.powf(0.7),
                };
                let u = u + s.along;
                let half = s.width / 2.0 * taper * (0.94 + 0.12 * noise(u / (9.0 * z), 0.0, id));
                if v.abs() > half {
                    continue;
                }
                let across = (v / s.width + 0.5) * nb;
                let b = across.floor() as i32;
                let (r1, r2, r3, r4) = (hf(id, b as u32), hf(id ^ 0x55, b as u32), hf(id ^ 0xAA, b as u32), hf(id ^ 0x3C, b as u32));
                // The brush meets the canvas at a slant and its bristles touch down and lift off one by one, so a
                // stroke's ends are ragged and oblique, and soft with pressure. (Every bristle used to land within
                // the first 2-6 % of the stroke and the longest ones ran into the hard cut at its end: each stroke
                // began and ended on a straight edge square to its direction. In a sky of level strokes those edges
                // lined up into hard vertical seams all over the film.) No bristle reaches either end of the
                // footprint, so the footprint's own cut never shows.
                // the brush is held at a slant to its path (so its ends are oblique, the same way at both ends) and
                // its loaded edge is rounded (so the middle bristles reach a little further than the outer ones)
                // All of it is measured in brush widths from the stroke's ends (not as a share of its length), so a
                // long line still reaches where it was drawn to and only its last width or so is ragged; a short
                // touch keeps its ends in proportion.
                let t = (across / nb).clamp(0.0, 1.0);
                let slant = ((hf(id, 991) - 0.5) * 1.7 * (t - 0.5) + (hf(id, 992) - 0.5) * 0.25) * end_w;
                let round = 0.38 * (2.0 * t - 1.0).powi(2) * end_w;
                let r5 = hf(id ^ 0x77, b as u32);
                let land_at = ((0.06 + 0.38 * r5) * end_w + slant + round).max(0.0) / whole_len;
                // each bristle runs dry at its own point, and skips over the ground as it does
                let dry_at = 1.0 - ((0.30 + 0.85 * (1.0 - r1)) * end_w - slant + round).max(0.18 * end_w) / whole_len;
                // (each bristle reads the noise at its own phase: sharing the lattice lined every bristle's skips up in
                // columns, a fine vertical grain across the stroke)
                let skip = smooth(dry_at - (0.18f32).min(2.5 * s.width / whole_len), dry_at, whole) * (0.35 + 0.65 * noise(u / (5.0 * z) + 37.0 * r3, b as f32 * 3.7, id ^ 9));
                let landed = smooth(land_at, land_at + (0.25 + 0.35 * r2) * end_w / whole_len, whole) * if lands { 1.0 } else { 1.0 };
                let side = smooth(0.0, edge_soft, half - v.abs());
                let mut a =
                    landed * side * (1.0 - smooth(0.45, 0.8, skip)) * (1.0 - smooth(dry_at - 0.10 * end_w / whole_len, dry_at + 0.12 * end_w / whole_len, whole)) * if lifts { 1.0 } else { 1.0 };
                if s.tool == Tool::Knife {
                    a *= smooth(0.25, 0.45, noise(u / (7.0 * z), v / (2.5 * z), id ^ 77) + 0.35 * (1.0 - whole));
                }
                if let Some((m, o, sz)) = mask {
                    let (mx, my) = (x - o[0], y - o[1]);
                    if mx < 0 || my < 0 || mx as usize >= sz[0] || my as usize >= sz[1] {
                        continue;
                    }
                    a *= m[my as usize * sz[0] + mx as usize];
                }
                if a <= 0.004 {
                    continue;
                }
                // paint: the brush's colour, streaked bristle by bristle with a second colour and with what it picked up
                let ridge = 1.0 - marks + marks * ((across - b as f32) * std::f32::consts::PI).sin();
                // paint squeezed out to the sides of the brush stands up in a lip
                let lip = (-((half - v.abs()) / (0.10 * s.width + z)).powi(2)).exp();
                let streak = if r3 < 0.22 { 0.6 } else { 0.0 } * (0.5 + 0.5 * r4);
                let pick = (s.pickup * STEADY_PICKUP * (0.4 + 1.2 * r2) * (0.5 + whole)).clamp(0.0, 0.85);
                let tone = 1.0 + 0.07 * (r4 - 0.5) + 0.03 * lip;
                let i = y as usize * WI + x as usize;
                let mut c = [0.0f32; 3];
                if pigment {
                    let at = pick / 0.85 * 8.0;
                    let (i0, f) = ((at.floor() as usize).min(7), at - at.floor().min(7.0));
                    for k in 0..3 {
                        let a = worked[0][i0][k] + (worked[0][i0 + 1][k] - worked[0][i0][k]) * f;
                        let b = worked[1][i0][k] + (worked[1][i0 + 1][k] - worked[1][i0][k]) * f;
                        c[k] = ((a + (b - a) * streak) * tone).clamp(0.0, 1.0);
                    }
                } else {
                    for k in 0..3 {
                        let own = s.col[k] + (s.alt[k] - s.col[k]) * streak;
                        c[k] = ((own + (under[k] - own) * pick) * tone).clamp(0.0, 1.0);
                    }
                }
                let cover = a * s.opacity;
                for k in 0..3 {
                    self.col[i][k] += (c[k] - self.col[i][k]) * cover;
                }
                // thickness: most where the loaded brush lands, least where it lifts; a new stroke flattens what it crosses
                let lay = s.load * ((0.6 + 0.4 * r1) * ridge * (1.0 - 0.6 * whole) + lips * lip) * if s.tool == Tool::Knife { 1.6 } else { 1.0 } * (s.width / z / 40.0).clamp(0.3, 1.5);
                // the brush leaves ITS thickness: going over the same place twice in one sweep adds nothing
                self.thick[i] += (lay - self.thick[i]) * cover;
            }
        }
    }

    /// Light the paint's relief from the upper left and add the canvas weave where it is thin.
    pub fn lit(&self) -> Vec<u8> {
        let mut out = vec![0u8; WI * HI * 3];
        if WATER.load(Ordering::Relaxed) {
            // watercolour alone has no relief to light: the paper, with the kit's grain
            let mut s: u64 = 5;
            let mut rnd = || {
                s = (s * 16807) % 2_147_483_647;
                s as f32 / 2_147_483_647.0
            };
            for (i, c) in self.col.iter().enumerate() {
                let g = if rnd() < 0.55 { 1.0 - rnd() * rnd() * 34.0 / 255.0 } else { 1.0 };
                for k in 0..3 {
                    out[i * 3 + k] = ((c[k] * g).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
            return out;
        }
        let l = {
            let v = [-0.52f32, -0.62, 0.59];
            let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            [v[0] / n, v[1] / n, v[2] / n]
        };
        let hv = {
            let v = [l[0], l[1], l[2] + 1.0];
            let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            [v[0] / n, v[1] / n, v[2] / n]
        };
        // wet paint settles: soften the relief a little before lighting it
        let mut soft = vec![0.0f32; WI * HI];
        for y in 0..HI {
            for x in 0..WI {
                let mut acc = 0.0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        acc += self.thick[(y as i32 + dy).clamp(0, HI as i32 - 1) as usize * WI + (x as i32 + dx).clamp(0, WI as i32 - 1) as usize];
                    }
                }
                soft[y * WI + x] = acc / 9.0;
            }
        }
        let hgt = |x: usize, y: usize| -> f32 {
            let (xf, yf) = (x as f32, y as f32);
            let weave = 0.02 * ((xf * 1.55).sin() * (yf * 1.55).sin());
            let t = soft[y.min(HI - 1) * WI + x.min(WI - 1)];
            t + weave * (1.0 - t).clamp(0.15, 1.0)
        };
        for y in 0..HI {
            for x in 0..WI {
                let gx = (hgt(x + 1, y) - hgt(x.saturating_sub(1), y)) * 1.9 * BODY * self.view;
                let gy = (hgt(x, y + 1) - hgt(x, y.saturating_sub(1))) * 1.9 * BODY * self.view;
                let inv = 1.0 / (gx * gx + gy * gy + 1.0).sqrt();
                let n = [-gx * inv, -gy * inv, inv];
                let diff = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]) / l[2];
                let spec = (n[0] * hv[0] + n[1] * hv[1] + n[2] * hv[2]).max(0.0).powf(26.0) - hv[2].powf(26.0);
                let shade = 0.74 + 0.26 * diff;
                let c = self.col[y * WI + x];
                // a little richer, as varnished paint is
                let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
                for k in 0..3 {
                    let v = (grey + (c[k] - grey) * 1.2) * shade + spec.max(0.0) * 0.30 * BODY;
                    out[(y * WI + x) * 3 + k] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        out
    }
}

/// What the painter knows about the scene beyond its shapes: where the light comes from and what is being looked at.
#[derive(Clone, Copy)]
pub struct Scene {
    /// Towards the light, on the canvas (unit).
    pub light: [f32; 2],
    pub focus: [f32; 2],
    pub reach: f32,
}
thread_local! {
    /// The camera's zoom on the shape being painted. STEADINESS under a zoom: a shape's brushwork is planned at the
    /// size the shape has in the film's world and then seen through the camera, like a painting being moved
    /// towards. Brush size, bristle count, stroke spacing and every wobble scale together, so nothing re-plans as
    /// the camera moves in.
    static ZOOM: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
    static PREMIX: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
    static FADE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
    // one frame is painted start to finish on one thread
    static SCENE: std::cell::Cell<Option<Scene>> = const { std::cell::Cell::new(None) };
}
/// How much of what is painted now is laid (1 = all of it). A painter's decisions about a shape go by its size,
/// and a size threshold crossed by a shape that grows or shrinks by a pixel would switch the treatment in one frame:
/// so near every such threshold the two treatments are cross-faded, each laid at its share (`Fade`), and a shape
/// arrives or leaves by degrees. Fades nest (they multiply).
pub struct Fade(f32);
impl Fade {
    pub fn set(k: f32) -> Fade {
        let prev = FADE.with(|f| f.get());
        FADE.with(|f| f.set(prev * k.clamp(0.0, 1.0)));
        Fade(prev)
    }
}
impl Drop for Fade {
    fn drop(&mut self) {
        FADE.with(|f| f.set(self.0));
    }
}

/// The opacity of the wash being painted (1 = as is). Paint is mixed down with what it lands on by this
/// much before it is laid, as a painter mixes a thin tint on the palette instead of laying a transparent stroke.
pub struct Premix;
impl Premix {
    pub fn set(k: f32) -> Premix {
        PREMIX.with(|p| p.set(k.clamp(0.0, 1.0)));
        Premix
    }
}
impl Drop for Premix {
    fn drop(&mut self) {
        PREMIX.with(|p| p.set(1.0));
    }
}

pub fn zoom() -> f32 {
    ZOOM.with(|z| z.get()).max(0.05)
}
/// Set the zoom for the next strokes (1 = no camera).
pub fn set_zoom(z: f32) {
    ZOOM.with(|c| c.set(z));
}
/// Set the scene for the next strokes (`None` = no light, no attention).
pub fn set_scene(sc: Option<Scene>) {
    SCENE.with(|s| s.set(sc));
}
pub fn scene() -> Option<Scene> {
    if LIGHT.load(Ordering::Relaxed) { SCENE.with(|s| s.get()) } else { None }
}
impl Scene {
    pub fn attention(&self, q: [f32; 2]) -> f32 {
        (-((q[0] - self.focus[0]).hypot(q[1] - self.focus[1]) / self.reach).powi(2)).exp()
    }
    /// Restraint: away from what is looked at, colour is greyed and lights and darks are drawn together, a touch
    /// cooler. Full-strength colour is kept for the centre of interest.
    pub fn quiet(&self, c: [f32; 3], q: [f32; 2]) -> [f32; 3] {
        let away = 1.0 - self.attention(q);
        let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
        std::array::from_fn(|k| {
            // only a little: the scenery should recede, not go grey
            let v = grey + (c[k] - grey) * (1.0 - 0.10 * away);
            (0.55 + (v - 0.55) * (1.0 - 0.07 * away) + [-0.004, 0.0, 0.008][k] * away).clamp(0.0, 1.0)
        })
    }
    /// One light, stated plainly: `side` runs from -1 (the side of a form turned to the light) to +1 (the side
    /// turned away). Light side lighter and warmer; shadow side darker, cooler and greyer.
    pub fn turned(&self, c: [f32; 3], side: f32, form: f32) -> [f32; 3] {
        let side = side.clamp(-1.0, 1.0);
        // the form turns away from the light quite suddenly (the shadow edge), is darkest just past that edge, and
        // picks up a little light again at its far rim from whatever it sits on
        let shadow = smooth(-0.05, 0.35, side);
        let core = (-((side - 0.45) / 0.3).powi(2)).exp();
        let lit = smooth(0.1, -0.9, side);
        let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
        let dark = 1.0 - form * (0.34 * shadow + 0.12 * core);
        let warm = form * 0.22 * lit;
        std::array::from_fn(|k| {
            let v = grey + (c[k] - grey) * (1.0 - 0.06 * form * shadow);
            (v * dark * (1.0 + [1.0, 0.8, 0.35][k] * warm) + [0.035, 0.02, 0.0][k] * warm + [0.0, 0.006, 0.04][k] * form * shadow).clamp(0.0, 1.0)
        })
    }
}

/// The painter's palette. The film's colours are cool and even; a painter would warm them, deepen them and let the
/// strong ones sing: more chroma, a tilt towards warm, darks pulled down a little.
pub fn palette(c: [f32; 3]) -> [f32; 3] {
    let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let chroma = 1.0 + RICH * 0.34;
    let deep = grey.powf(1.0 + RICH * 0.18) / grey.max(1e-4);
    std::array::from_fn(|k| ((grey + (c[k] - grey) * chroma) * deep * (1.0 + RICH * [0.07, 0.015, -0.085][k])).clamp(0.0, 1.0))
}
/// How far the palette is pushed (0 = the film's own colours).
pub const RICH: f32 = 0.0;

pub fn shifted(c: [f32; 3], k: f32, warm: f32) -> [f32; 3] {
    [(c[0] * k + warm * 0.04).clamp(0.0, 1.0), (c[1] * k).clamp(0.0, 1.0), (c[2] * k - warm * 0.04).clamp(0.0, 1.0)]
}

/// A shape's soft coverage over its bounding box, grown a little so strokes may cross its edge.
pub fn shape_mask(pts: &[[f32; 2]], grow: f32) -> Option<(Vec<f32>, [i32; 2], [usize; 2])> {
    let pad = grow.ceil() as i32 + 2;
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let o = [(lo[0].floor() as i32 - pad).max(0), (lo[1].floor() as i32 - pad).max(0)];
    let e = [(hi[0].ceil() as i32 + pad).min(WI as i32), (hi[1].ceil() as i32 + pad).min(HI as i32)];
    if e[0] <= o[0] || e[1] <= o[1] {
        return None;
    }
    let sz = [(e[0] - o[0]) as usize, (e[1] - o[1]) as usize];
    let mut m = vec![0.0f32; sz[0] * sz[1]];
    let n = pts.len();
    let mut xs: Vec<f32> = vec![];
    for y in 0..sz[1] {
        let py = (y as i32 + o[1]) as f32 + 0.5;
        xs.clear();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            if (a[1] > py) != (b[1] > py) {
                xs.push(a[0] + (py - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        for pair in xs.chunks(2) {
            if let [a, b] = pair {
                let (xa, xb) = (((*a - o[0] as f32).round() as i32).clamp(0, sz[0] as i32), ((*b - o[0] as f32).round() as i32).clamp(0, sz[0] as i32));
                for x in xa..xb {
                    m[y * sz[0] + x as usize] = 1.0;
                }
            }
        }
    }
    // soften and grow: two box blurs, then a threshold below one half
    let r = (grow.max(1.0)) as usize;
    let mut tmp = vec![0.0f32; m.len()];
    for _ in 0..2 {
        for y in 0..sz[1] {
            let row = &m[y * sz[0]..(y + 1) * sz[0]];
            let mut acc = 0.0;
            let mut cnt = 0usize;
            for x in 0..sz[0].min(r) {
                acc += row[x];
                cnt += 1;
            }
            for x in 0..sz[0] {
                if x + r < sz[0] {
                    acc += row[x + r];
                    cnt += 1;
                }
                if x > r {
                    acc -= row[x - r - 1];
                    cnt -= 1;
                }
                tmp[y * sz[0] + x] = acc / cnt.max(1) as f32;
            }
        }
        for x in 0..sz[0] {
            let mut acc = 0.0;
            let mut cnt = 0usize;
            for y in 0..sz[1].min(r) {
                acc += tmp[y * sz[0] + x];
                cnt += 1;
            }
            for y in 0..sz[1] {
                if y + r < sz[1] {
                    acc += tmp[(y + r) * sz[0] + x];
                    cnt += 1;
                }
                if y > r {
                    acc -= tmp[(y - r - 1) * sz[0] + x];
                    cnt -= 1;
                }
                m[y * sz[0] + x] = acc / cnt.max(1) as f32;
            }
        }
    }
    for v in &mut m {
        *v = smooth(0.22, 0.5, *v);
    }
    Some((m, o, sz))
}

/// A watercolour wash: transparent pigment over whatever is under it, belonging to this one shape. The pigment
/// lies unevenly, in soft clouds of its own; thinner towards one side and in the middle, where the brush lifted it;
/// and it gathers into a darker line just inside the edge as the wash dries (the tide line). Light passes through
/// the pigment twice, so each layer multiplies what is under it: blue over yellow is green, and the paper shows
/// through everything.
/// `fresh`: the shape's own local colour, painted as if on clean paper (it replaces what is under it, as the film's
/// flat colours do). Otherwise a glaze over what is there.
#[allow(clippy::too_many_arguments)]
pub fn wash_shape(p: &mut Painting, pts: &[[f32; 2]], col: [f32; 3], density: f32, seed: u32, soft: f32, texture: f32, fresh: bool) {
    let Some((m, o, sz)) = shape_mask(pts, soft) else { return };
    let n = pts.len() as f32;
    let c = [pts.iter().map(|q| q[0]).sum::<f32>() / n, pts.iter().map(|q| q[1]).sum::<f32>() / n];
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for q in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    let size = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1.0);
    // the mask, softened inwards: where the plain mask stands above it is just inside an edge
    let r = ((size * 0.02).clamp(2.0, 7.0)) as usize;
    let mut inner = m.clone();
    let mut tmp = vec![0.0f32; m.len()];
    for _ in 0..2 {
        for y in 0..sz[1] {
            for x in 0..sz[0] {
                let (a, b) = (x.saturating_sub(r), (x + r).min(sz[0] - 1));
                tmp[y * sz[0] + x] = inner[y * sz[0] + a..=y * sz[0] + b].iter().sum::<f32>() / (b - a + 1) as f32;
            }
        }
        for y in 0..sz[1] {
            let (a, b) = (y.saturating_sub(r), (y + r).min(sz[1] - 1));
            for x in 0..sz[0] {
                let mut acc = 0.0;
                for yy in a..=b {
                    acc += tmp[yy * sz[0] + x];
                }
                inner[y * sz[0] + x] = acc / (b - a + 1) as f32;
            }
        }
    }
    let lean = hf(seed, 21) * std::f32::consts::TAU;
    let (lx, ly) = (lean.cos(), lean.sin());
    let cloud = (size * 0.22).clamp(18.0, 260.0);
    for y in 0..sz[1] {
        for x in 0..sz[0] {
            let cover = m[y * sz[0] + x];
            if cover <= 0.003 {
                continue;
            }
            let (px, py) = ((x as i32 + o[0]) as f32, (y as i32 + o[1]) as f32);
            if px < 0.0 || py < 0.0 || px >= WI as f32 || py >= HI as f32 {
                continue;
            }
            // in the shape's own coordinates, so the wash travels with it
            let (u, v) = (px - c[0], py - c[1]);
            let clouds = 0.62 * noise(u / cloud, v / cloud, seed) + 0.38 * noise(u / (cloud * 0.37), v / (cloud * 0.37), seed ^ 91);
            let side = 1.0 - 0.3 * texture * ((u * lx + v * ly) / (size * 0.5)).clamp(-1.0, 1.0);
            let lifted = 1.0 - 0.22 * texture * (-(u * u + v * v) / (size * size * 0.09)).exp();
            let tide = ((cover - inner[y * sz[0] + x]) * 2.4).clamp(0.0, 1.0) * cover;
            let tooth = 1.0 + 0.07 * (noise(px / 2.3, py / 2.3, 4242) - 0.5) * 2.0;
            let d = density * cover * (1.0 - 0.5 * texture + texture * clouds) * side * lifted * (1.0 + 0.7 * tide) * tooth;
            let i = py as usize * WI + px as usize;
            for k in 0..3 {
                if fresh {
                    // the pigment's own unevenness still shows (d / cover), but the shape covers what was there
                    let on_paper = [0.955, 0.93, 0.875][k] * col[k].clamp(0.02, 1.0).powf((d / cover.max(0.05)).max(0.0));
                    p.col[i][k] += (on_paper - p.col[i][k]) * cover;
                } else {
                    p.col[i][k] *= col[k].clamp(0.02, 1.0).powf(d.max(0.0));
                }
            }
            // a wash has no body: it leaves the paper's surface as it was
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// The strokes a shape is laid with: (slot centre, stroke id, how much of it is laid 0..1). `c` is the shape's
/// centroid, `lo`/`hi` its bounding box clipped to the canvas, `base` the brushwork's direction.
pub fn plan_slots(pts: &[[f32; 2]], c: [f32; 2], lo: [f32; 2], hi: [f32; 2], width: f32, base: f32, seed: u32) -> Vec<([f32; 2], u32, f32)> {
    let vis = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let mid = c;
    let (step_u, step_v) = (width * 1.35, width * 0.42);
    let (sn, cs) = base.sin_cos();
    // a stroke reaches up to about 4.3 brush widths from its slot: every slot that could touch the visible part
    // is visited and kept, so none appears or disappears as the canvas edge slides along the shape
    let reach = 4.5 * width;
    let rad = (hi[0] - lo[0]).hypot(hi[1] - lo[1]) / 2.0 + reach;
    let (du, dv) = ((vis[0] - c[0]) * cs + (vis[1] - c[1]) * sn, -(vis[0] - c[0]) * sn + (vis[1] - c[1]) * cs);
    let (cu, cv) = ((du / step_u).round() as i32, (dv / step_v).round() as i32);
    let (nu, nv) = ((rad / step_u).ceil() as i32 + 1, (rad / step_v).ceil() as i32 + 1);
    // How far inside the shape a slot's centre is decides how much of the stroke is laid, smoothly. (A hard
    // in/out test on the rasterised mask let a stroke whose centre sat on the edge appear and vanish from one
    // frame to the next as the shape or camera moved a fraction of a pixel; with strokes several brush-widths
    // long, that was the "few strokes that flicker while most are perfect".) Strokes are clipped by the mask
    // anyway, so a slot just outside the edge only contributes the part of it that lands inside.
    let (fade_out, fade_in) = (-0.45 * width, 0.15 * width);
    let mut spots: Vec<([f32; 2], u32, f32)> = vec![];
    for iu in cu - nu..=cu + nu {
        for iv in cv - nv..=cv + nv {
            let id = seed.wrapping_mul(31).wrapping_add((iu * 4001 + iv) as u32);
            let (u, v) = ((iu as f32 + hf(id, 1) - 0.5) * step_u, (iv as f32 + hf(id, 2) - 0.5) * step_v);
            let q = [mid[0] + u * cs - v * sn, mid[1] + u * sn + v * cs];
            if q[0] < lo[0] - reach || q[1] < lo[1] - reach || q[0] > hi[0] + reach || q[1] > hi[1] + reach {
                continue;
            }
            let d = signed_depth(pts, q, fade_in);
            if d <= fade_out {
                continue;
            }
            let e = smooth(fade_out, fade_in, d);
            if e <= 0.002 {
                continue;
            }
            spots.push((q, id, e));
        }
    }
    spots
}

/// The nearest distance from `q` to the outline, and the outline's direction there (a line, so modulo pi), blended
/// over the edges that are nearly as close.
pub fn edge_dir(pts: &[[f32; 2]], q: [f32; 2], width: f32) -> (f32, f32) {
    let mut ds: Vec<(f32, f32)> = Vec::with_capacity(pts.len());
    let mut near = f32::MAX;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        if ab[0].abs() + ab[1].abs() < 1e-3 {
            continue;
        }
        let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-6)).clamp(0.0, 1.0);
        let d = (q[0] - a[0] - ab[0] * t).hypot(q[1] - a[1] - ab[1] * t);
        near = near.min(d);
        ds.push((d, ab[1].atan2(ab[0])));
    }
    let along = {
        let sigma = (0.5 * width).max(1.0);
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        for (d, th) in &ds {
            let w = (-((d - near) / sigma).powi(2)).exp();
            sx += w * (2.0 * th).cos();
            sy += w * (2.0 * th).sin();
        }
        0.5 * sy.atan2(sx)
    };
    (near, along)
}

/// How far `q` lies inside the polygon (negative outside), in pixels; exact within `cap` of the edge, and `cap`
/// anywhere deeper (the caller only needs the band near the edge).
pub fn signed_depth(pts: &[[f32; 2]], q: [f32; 2], cap: f32) -> f32 {
    let mut near = f32::MAX;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let ab = [b[0] - a[0], b[1] - a[1]];
        let t = (((q[0] - a[0]) * ab[0] + (q[1] - a[1]) * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-6)).clamp(0.0, 1.0);
        near = near.min((q[0] - a[0] - ab[0] * t).hypot(q[1] - a[1] - ab[1] * t));
    }
    if in_poly(pts, q) { near.min(cap) } else { -near }
}

/// The plain picture's colour around (x, y), averaged over a little patch of radius `r`: what a brush landing
/// there picks up. A single pixel would flip between two colours whenever an edge in the plain picture moved
/// across it by a fraction of a pixel, changing the whole stroke's colour from one frame to the next. Off the
/// canvas there is nothing to pick up, so the result slides to `none` over one patch radius past the edge.
pub fn flat_under(flat: &[[f32; 3]], x: f32, y: f32, r: f32, none: [f32; 3]) -> [f32; 3] {
    let r = r.max(1.0);
    let bil = |x: f32, y: f32| -> [f32; 3] {
        let (x, y) = (x.clamp(0.0, WI as f32 - 1.001), y.clamp(0.0, HI as f32 - 1.001));
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let (a, b, c, d) = (flat[y0 * WI + x0], flat[y0 * WI + x0 + 1], flat[(y0 + 1) * WI + x0], flat[(y0 + 1) * WI + x0 + 1]);
        std::array::from_fn(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy)
    };
    let mut sum = [0.0f32; 3];
    for j in -1..=1 {
        for i in -1..=1 {
            let q = bil(x + i as f32 * r, y + j as f32 * r);
            for k in 0..3 {
                sum[k] += q[k] / 9.0;
            }
        }
    }
    let outside = (-x).max(x - (WI as f32 - 1.0)).max(-y).max(y - (HI as f32 - 1.0)).max(0.0);
    let w = smooth(0.0, r, outside);
    std::array::from_fn(|k| sum[k] + (none[k] - sum[k]) * w)
}

#[allow(clippy::too_many_arguments)]
pub fn paint_body(p: &mut Painting, pts: &[[f32; 2]], col: [f32; 3], alt: [f32; 3], opacity: f32, seed: u32, detail: f32, body: f32, tool: Tool) {
    paint_form(p, pts, col, alt, opacity, seed, detail, body, tool, 0.0)
}

/// `form`: how much the shape is modelled by the scene's light (0 = a flat plane, 1 = a rounded form).
#[allow(clippy::too_many_arguments)]
pub fn paint_form(p: &mut Painting, pts: &[[f32; 2]], col: [f32; 3], alt: [f32; 3], opacity: f32, seed: u32, detail: f32, body: f32, tool: Tool, form: f32) {
    let n = pts.len() as f32;
    let c = [pts.iter().map(|q| q[0]).sum::<f32>() / n, pts.iter().map(|q| q[1]).sum::<f32>() / n];
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for q in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
        let (dx, dy) = (q[0] - c[0], q[1] - c[1]);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    // STEADINESS: everything about how a shape is painted comes from the shape itself, never from where it happens
    // to be on the canvas or what is under it this frame. Its brush is chosen from its own size (whole, even if
    // part is off the canvas), and in steps, so a shape that swells a little keeps the same brush.
    let small = (hi[0] - lo[0]).min(hi[1] - lo[1]);
    let z = zoom();
    // The brush follows the shape's size continuously. (It used to snap to steps of 1.22x: a shape that swelled
    // across a step boundary, or a blinking eye, re-planned every stroke in it in one frame, which read as a pop.)
    let width = (small / z * 0.30 * detail).clamp(5.0, 110.0) * z;
    lo = [lo[0].max(-80.0), lo[1].max(-80.0)];
    hi = [hi[0].min(WI as f32 + 80.0), hi[1].min(HI as f32 + 80.0)];
    if hi[0] <= lo[0] || hi[1] <= lo[1] {
        return;
    }
    // strokes follow a long shape's length; a round one takes the painter's diagonal
    let axis = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let stretch = {
        let t = ((sxx - syy) * (sxx - syy) + 4.0 * sxy * sxy).sqrt();
        ((sxx + syy + t) / (sxx + syy - t).max(1e-3)).sqrt()
    };
    // a round shape takes the painter's diagonal, a long one its own length; in between, part way (no sudden flip
    // as a shape stretches)
    let base = {
        let diag = -0.6 + 1.2 * (hf(seed, 77) - 0.5);
        let mut to = axis - diag;
        while to > std::f32::consts::FRAC_PI_2 {
            to -= std::f32::consts::PI;
        }
        while to < -std::f32::consts::FRAC_PI_2 {
            to += std::f32::consts::PI;
        }
        // Continuous. (It used to snap to steps of 7.5 degrees; a shape whose axis wobbled across a step boundary
        // turned AND re-laid its whole lattice in one frame: the flicker on wings, eyes and beaks.) A turn of a
        // fraction of a degree per frame is the shape's own motion, which the brushwork should follow.
        diag + to * smooth(1.35, 2.1, stretch)
    };
    let Some((mask, o, sz)) = shape_mask(pts, width * 0.22 + 1.5 * z) else { return };
    // the grid of strokes hangs on the shape's own centre, so it travels with the shape (and a big shape half off
    // the canvas keeps its strokes where they were)
    // A slot's identity is its lattice index (iu, iv) counted from the shape's own centroid, so it never depends
    // on which slots are visited. Only the visiting WINDOW follows the visible part of the shape (a big shape
    // half off the canvas, an iris wedge or a whip streak reaching 4000 px out, has its centroid far away; a
    // window centred on the centroid with the clipped radius missed the visible part entirely).
    let mut spots = plan_slots(pts, c, lo, hi, width, base, seed);
    spots.sort_by_key(|(_, id, _)| h32(*id));
    for (q, id, edge_k) in spots {
        // Brushwork within a plane is deliberate: one direction, drifting slowly. Near where the colour ends it
        // turns to run ALONG that edge, so two colours meet with their strokes side by side, never with one
        // hatched across and chopped off.
        let drift = (noise((q[0] - c[0]) / (420.0 * z), (q[1] - c[1]) / (420.0 * z), seed) - 0.5) * 0.5 + (hf(id, 3) - 0.5) * 0.10;
        // The edge's direction is a blend of the nearest edges' directions (as lines: doubled angles), weighted by
        // how nearly as close each is. Taking only THE nearest edge flipped a stroke's direction outright where two
        // edges were equally close (a corner, or the two sides of a narrow trunk): as the shape or camera moved a
        // fraction of a pixel, a long stroke swung several degrees in one frame.
        let (near, along) = edge_dir(pts, q, width);
        // the edge has no "forwards": take whichever way round is closer to the plane's direction
        let mut turn_to = along - (base + drift);
        while turn_to > std::f32::consts::FRAC_PI_2 {
            turn_to -= std::f32::consts::PI;
        }
        while turn_to < -std::f32::consts::FRAC_PI_2 {
            turn_to += std::f32::consts::PI;
        }
        let pull = (-(near / (width * 1.6)).powi(2)).exp();
        let swirl = drift + turn_to * pull;
        let k = 0.94 + 0.12 * hf(id, 4);
        // which side of the form this stroke is on
        let lit_col = match scene() {
            Some(sc) => {
                let across = ((q[0] - c[0]) * -sc.light[0] + (q[1] - c[1]) * -sc.light[1]) / (small * 0.5).max(1.0);
                sc.turned(col, across, form)
            }
            None => col,
        };
        p.stroke(
            &Stroke {
                c: q,
                angle: base + swirl,
                // (a little longer than the paint it lays: its bristles land and lift a way in from its ends)
                len: width * (3.6 + 3.6 * hf(id, 5)) * (1.0 - 0.35 * pull),
                width: width * (0.75 + 0.5 * hf(id, 6)),
                // strokes are arcs from the wrist: mostly bowed the same way, never ruler-straight
                bend: 0.12 + (hf(id, 7) - 0.5) * 0.2,
                col: shifted(lit_col, k, (hf(id, 8) - 0.5) * 2.0),
                alt: shifted(alt, 1.0, 0.0),
                opacity: opacity * (0.86 + 0.12 * hf(id, 9)) * edge_k,
                pickup: 0.22 + 0.3 * hf(id, 10),
                load: (0.6 + 0.4 * hf(id, 11)) * body,
                // the stroke's bristles belong to its place on its shape, not to how many strokes came before it
                part: Some((h32(id) >> 2, true, true)),
                span: (0.0, 1.0),
                tool,
                picked: None,
                along: 0.0,
            },
            Some((&mask, o, sz)),
        );
    }
    // the shape now stands in the plain picture, for whatever is painted over it later (a translucent wash as the
    // tint it was mixed to, not as its full colour)
    let premix = PREMIX.with(|p| p.get());
    for y in 0..sz[1] {
        for x in 0..sz[0] {
            if mask[y * sz[0] + x] >= 0.5 {
                let (px, py) = (x as i32 + o[0], y as i32 + o[1]);
                if px >= 0 && py >= 0 && (px as usize) < WI && (py as usize) < HI {
                    let f = &mut p.flat[py as usize * WI + px as usize];
                    *f = std::array::from_fn(|k| f[k] + (col[k] - f[k]) * premix);
                }
            }
        }
    }
    // the cut-in edge comes in by degrees as a shape grows past the size that has one
    let contour_k = smooth(SMALLEST * 3.5 * z, SMALLEST * 4.5 * z, small);
    if contour_k > 0.001 {
        let _contour = Fade::set(contour_k);
        paint_contour(p, pts, col, alt, (width * 0.45).clamp(SMALLEST * z, 26.0 * z), seed ^ 0xC0, opacity, body);
    }
}

/// How a hand takes a brush along an outline. Not side by side with a ruler:
/// * the brush lifts at a sharp corner and starts again;
/// * between corners it moves in one smooth curve through the points;
/// * one sweep turns through only so much (about 130 degrees) and lasts only one load of paint, then the
///   painter starts again, overlapping the end of the last stroke;
/// * the hand pulls (left to right, top to bottom) rather than pushes;
/// * it runs a little past where it meant to stop, and wanders a little either side of the line.
/// Returns each stroke's path as closely spaced points.
/// Returns each stroke's path, the share of it to lay (1 = all), and its stable identity.
pub fn hand_paths(pts: &[[f32; 2]], closed: bool, width: f32, offset: f32, seed: u32) -> Vec<(Vec<[f32; 2]>, f32, u32)> {
    let n = pts.len();
    if n < 2 {
        return vec![];
    }
    let at = |i: i64| -> [f32; 2] { if closed { pts[i.rem_euclid(n as i64) as usize] } else { pts[i.clamp(0, n as i64 - 1) as usize] } };
    let turn_at = |i: i64| -> f32 {
        let (a, b, c) = (at(i - 1), at(i), at(i + 1));
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        (u[0] * v[1] - u[1] * v[0]).atan2(u[0] * v[0] + u[1] * v[1])
    };
    // pieces between sharp corners
    let corners: Vec<usize> = (0..n).filter(|&i| (closed || (i > 0 && i + 1 < n)) && turn_at(i as i64).abs() > 0.95).collect();
    let mut pieces: Vec<(i64, i64)> = vec![];
    if closed {
        if corners.is_empty() {
            let start = (hf(seed, 1) * n as f32) as i64;
            pieces.push((start, start + n as i64));
        } else {
            for (k, &c) in corners.iter().enumerate() {
                let next = corners[(k + 1) % corners.len()] as i64;
                pieces.push((c as i64, if next > c as i64 { next } else { next + n as i64 }));
            }
        }
    } else {
        let mut from = 0i64;
        for &c in &corners {
            pieces.push((from, c as i64));
            from = c as i64;
        }
        pieces.push((from, n as i64 - 1));
    }
    let side = if closed { (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f32>().signum() } else { 0.0 };
    let step = (width * 0.3).clamp(1.5, 6.0 * zoom().max(1.0));
    let mut out = vec![];
    for (pi, (a, b)) in pieces.into_iter().enumerate() {
        if b <= a {
            continue;
        }
        // one smooth curve through the piece's points (Catmull-Rom), sampled closely
        let mut curve: Vec<[f32; 2]> = vec![];
        for i in a..b {
            let (p0, p1, p2, p3) = (at(if i == a && corners.len() > 0 { i } else { i - 1 }), at(i), at(i + 1), at(if i + 1 == b && corners.len() > 0 { i + 1 } else { i + 2 }));
            let l = (p2[0] - p1[0]).hypot(p2[1] - p1[1]);
            let k = (l / step).ceil().max(1.0) as usize;
            for j in 0..k {
                let (u, u2, u3) = (j as f32 / k as f32, (j as f32 / k as f32).powi(2), (j as f32 / k as f32).powi(3));
                curve.push(std::array::from_fn(|d| {
                    0.5 * (2.0 * p1[d] + (p2[d] - p0[d]) * u + (2.0 * p0[d] - 5.0 * p1[d] + 4.0 * p2[d] - p3[d]) * u2 + (3.0 * p1[d] - p0[d] - 3.0 * p2[d] + p3[d]) * u3)
                }));
            }
        }
        curve.push(at(b));
        // move it inside the shape (cutting in) and let the hand wander
        let m = curve.len();
        let mut walked = 0.0f32;
        let moved: Vec<[f32; 2]> = (0..m)
            .map(|i| {
                let (p, q) = (curve[i.saturating_sub(1)], curve[(i + 1).min(m - 1)]);
                let l = (q[0] - p[0]).hypot(q[1] - p[1]).max(1e-3);
                if i > 0 {
                    walked += (curve[i][0] - curve[i - 1][0]).hypot(curve[i][1] - curve[i - 1][1]);
                }
                let wander = (noise(walked / (width * 8.0), pi as f32 * 9.1, seed) - 0.5) * 0.5 * width.min(14.0 * zoom());
                let nrm = [-(q[1] - p[1]) / l, (q[0] - p[0]) / l];
                let off = offset * side + wander;
                [curve[i][0] + nrm[0] * off, curve[i][1] + nrm[1] * off]
            })
            .collect();
        // Cut into sweeps: so much turning, so much paint. Each point along the piece has a cost (its length in
        // reaches plus its turning in 130-degree turns), and the sweeps end where the running cost passes a
        // fixed series of marks drawn from the seed. The cuts therefore slide with the shape as it flexes, and a
        // sweep is only ever added or lost at the END of a piece, starting from nothing (it is faded in by how far
        // the piece reaches past its mark). (The sweeps used to be cut greedily, "until this much length or this
        // much turn": a shape that flexed a little re-cut every sweep after the change, and since each sweep was
        // seeded by its place in the list, re-seeded them all; the outline of a breathing owl flickered.)
        let unit = 15.5 * width;
        let mut cost = vec![0.0f32; m];
        let mut arc = vec![0.0f32; m];
        for q in 1..m {
            let d = (moved[q][0] - moved[q - 1][0]).hypot(moved[q][1] - moved[q - 1][1]);
            arc[q] = arc[q - 1] + d;
            let turn = if q >= 2 {
                let (e, f) = ([curve[q - 1][0] - curve[q - 2][0], curve[q - 1][1] - curve[q - 2][1]], [curve[q][0] - curve[q - 1][0], curve[q][1] - curve[q - 1][1]]);
                (e[0] * f[1] - e[1] * f[0]).atan2(e[0] * f[0] + e[1] * f[1]).abs()
            } else {
                0.0
            };
            cost[q] = cost[q - 1] + d / unit + turn / 2.3;
        }
        let total = cost[m - 1];
        // the point at running cost c, and the point `back` pixels of path before it
        let at_cost = |c: f32| -> (usize, [f32; 2], f32) {
            let q = cost.partition_point(|&v| v < c).clamp(1, m - 1);
            let f = ((c - cost[q - 1]) / (cost[q] - cost[q - 1]).max(1e-9)).clamp(0.0, 1.0);
            (q, std::array::from_fn(|k| moved[q - 1][k] + (moved[q][k] - moved[q - 1][k]) * f), arc[q - 1] + (arc[q] - arc[q - 1]) * f)
        };
        let at_arc = |a: f32| -> (usize, [f32; 2]) {
            let q = arc.partition_point(|&v| v < a).clamp(1, m - 1);
            let f = ((a - arc[q - 1]) / (arc[q] - arc[q - 1]).max(1e-9)).clamp(0.0, 1.0);
            (q, std::array::from_fn(|k| moved[q - 1][k] + (moved[q][k] - moved[q - 1][k]) * f))
        };
        let piece_seed = seed ^ (pi as u32).wrapping_mul(131);
        let (mut c0, mut sweep) = (0.0f32, 0u32);
        while c0 < total && sweep < 4096 {
            let c1 = c0 + 0.71 + 0.58 * hf(piece_seed, sweep + 5);
            // a sweep that has only just come into being (the piece reaching past its mark by a hair) is laid by degrees
            let born = if sweep == 0 { 1.0 } else { smooth(0.0, 0.15, total - c0) };
            let (_, _, a0) = at_cost(c0);
            // the next sweep starts a little back, over the end of the last one
            let (qs, ps) = at_arc((a0 - if sweep == 0 { 0.0 } else { width * 0.9 }).max(0.0));
            let (qe, pe, _) = at_cost(c1.min(total));
            let mut path: Vec<[f32; 2]> = vec![ps];
            path.extend_from_slice(&moved[qs..qe]);
            path.push(pe);
            path.dedup_by(|a, b| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 1e-3);
            if path.len() >= 2 && born > 0.001 {
                // run on a little past the end, as a hand does
                let (e, f) = (path[path.len() - 2], path[path.len() - 1]);
                let l = (f[0] - e[0]).hypot(f[1] - e[1]).max(1e-3);
                let over = width * (0.25 + 0.5 * hf(seed, sweep * 7 + pi as u32));
                path.push([f[0] + (f[0] - e[0]) / l * over, f[1] + (f[1] - e[1]) / l * over]);
                // Pull, don't push. Which way a sweep runs is one of two, so near the line between them both are
                // laid, each by its share, instead of the whole sweep turning round in one frame.
                let (s0, s1) = (path[0], path[path.len() - 1]);
                let chord = (s1[0] - s0[0]).hypot(s1[1] - s0[1]).max(1e-3);
                let fwd = smooth(-0.2, 0.2, ((s1[0] - s0[0]) * 0.75 + (s1[1] - s0[1])) / (1.25 * chord));
                // a sweep's identity is its piece and its place on that piece, never its place in the list
                let ident = h32(piece_seed ^ sweep.wrapping_mul(0x9E37_79B9));
                if fwd > 0.001 {
                    out.push((path.clone(), born * fwd, ident));
                }
                if fwd < 0.999 {
                    let mut back = path;
                    back.reverse();
                    out.push((back, born * (1.0 - fwd), ident ^ 0x5bd1_e995));
                }
            }
            c0 = c1;
            sweep += 1;
        }
    }
    out
}

/// One sweep of the brush along a path: the same bristles all the way, pressure and paint by how far along it is.
#[allow(clippy::too_many_arguments)]
pub fn sweep(p: &mut Painting, tool: Tool, path: &[[f32; 2]], width: f32, col: [f32; 3], alt: [f32; 3], opacity: f32, pickup: f32, load: f32, seed: u32, weight: &dyn Fn([f32; 2]) -> f32) {
    let total: f32 = path.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum();
    if total < 1.0 {
        return;
    }
    let ident = 0x4000_0000 | (h32(seed) >> 2);
    let picked = flat_under(&p.flat, path[0][0], path[0][1], width * 0.3, col);
    let mut walked = 0.0;
    for (i, w) in path.windows(2).enumerate() {
        let (a, b) = (w[0], w[1]);
        let l = (b[0] - a[0]).hypot(b[1] - a[1]);
        if l < 0.05 {
            continue;
        }
        let state = weight([(b[0] - a[0]) / l, (b[1] - a[1]) / l]);
        if state > 0.04 {
            p.stroke(
                &Stroke {
                    c: [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
                    angle: (b[1] - a[1]).atan2(b[0] - a[0]),
                    len: l + width * 0.12,
                    width: width * (0.6 + 0.4 * state),
                    bend: 0.0,
                    col,
                    alt,
                    opacity: opacity * state,
                    pickup,
                    load,
                    part: Some((ident, i == 0, i + 2 == path.len())),
                    span: (walked / total, (walked + l) / total),
                    tool,
                    picked: Some(picked),
                    along: walked + l / 2.0,
                },
                None,
            );
        }
        walked += l;
    }
}

/// Cutting in: where a colour ends, the brush is drawn along the edge, just inside it.
#[allow(clippy::too_many_arguments)]
pub fn paint_contour(p: &mut Painting, pts: &[[f32; 2]], col: [f32; 3], alt: [f32; 3], width: f32, seed: u32, opacity: f32, body: f32) {
    for (path, share, ident) in hand_paths(pts, true, width, width * 0.42, seed) {
        let id = seed ^ ident;
        sweep(p, Tool::Flat, &path, width, shifted(col, 0.95 + 0.10 * hf(id, 2), (hf(id, 3) - 0.5) * 0.8), alt, opacity * 0.88 * share, 0.34, 0.85 * body, id, &|_| 1.0);
    }
}

/// A drawn line or an outline. `lit`: for a closed outline, how strongly to paint the sides that face the light
/// (the far sides are always painted).
pub fn paint_line(p: &mut Painting, pts: &[[f32; 2]], closed: bool, col: [f32; 3], width: f32, seed: u32, lit: f32) {
    paint_line_with(p, pts, closed, col, width, seed, lit, 0.55, 0.93)
}

/// `load` 0 = ink (no body), `opacity` below 1 lets what is under it show.
#[allow(clippy::too_many_arguments)]
pub fn paint_line_with(p: &mut Painting, pts: &[[f32; 2]], closed: bool, col: [f32; 3], width: f32, seed: u32, lit: f32, load: f32, opacity: f32) {
    let width = width.max(2.5);
    let side = if closed {
        let n = pts.len();
        (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f32>().signum()
    } else {
        0.0
    };
    for (path, share, ident) in hand_paths(pts, closed, width, 0.0, seed) {
        let id = seed ^ ident.rotate_left(7);
        let path = &path;
        // the sweep may have been turned round to be pulled: which side is out depends on its direction
        let flip = if closed && path.len() > 2 {
            let (a, b) = (path[0], path[1]);
            let n = pts.len() as f32;
            let c = [pts.iter().map(|q| q[0]).sum::<f32>() / n, pts.iter().map(|q| q[1]).sum::<f32>() / n];
            let left = [-(b[1] - a[1]), b[0] - a[0]];
            if left[0] * (c[0] - a[0]) + left[1] * (c[1] - a[1]) > 0.0 { -1.0 } else { 1.0 }
        } else {
            side
        };
        // lost and found: an edge is stated on the side away from the light and only hinted on the side towards it
        let weight = |t: [f32; 2]| -> f32 {
            if !closed {
                return 1.0;
            }
            let out = [-t[1] * flip, t[0] * flip];
            lit + (1.0 - lit) * smooth(0.3, 0.6, (out[0] * 0.64 + out[1] * 0.77) * 0.5 + 0.5)
        };
        sweep(p, Tool::Round, path, width, shifted(col, 0.95 + 0.2 * hf(id, 4), 0.2), shifted(col, 1.4, 0.6), opacity * share, 0.16 * load / 0.55, load, id, &weight);
    }
}

// ---------------------------------------------------------------- painting from a picture

pub fn box_blur(src: &[[f32; 3]], r: usize) -> Vec<[f32; 3]> {
    let r = r.max(1) as i32;
    let mut a = src.to_vec();
    let mut b = vec![[0.0f32; 3]; src.len()];
    for _ in 0..2 {
        for y in 0..HI {
            let mut acc = [0.0f32; 3];
            let mut n = 0.0;
            for x in -r..WI as i32 {
                if x + r < WI as i32 {
                    let v = a[y * WI + (x + r) as usize];
                    for k in 0..3 {
                        acc[k] += v[k];
                    }
                    n += 1.0;
                }
                if x - r - 1 >= 0 {
                    let v = a[y * WI + (x - r - 1) as usize];
                    for k in 0..3 {
                        acc[k] -= v[k];
                    }
                    n -= 1.0;
                }
                if x >= 0 {
                    b[y * WI + x as usize] = [acc[0] / n, acc[1] / n, acc[2] / n];
                }
            }
        }
        for x in 0..WI {
            let mut acc = [0.0f32; 3];
            let mut n = 0.0;
            for y in -r..HI as i32 {
                if y + r < HI as i32 {
                    let v = b[(y + r) as usize * WI + x];
                    for k in 0..3 {
                        acc[k] += v[k];
                    }
                    n += 1.0;
                }
                if y - r - 1 >= 0 {
                    let v = b[(y - r - 1) as usize * WI + x];
                    for k in 0..3 {
                        acc[k] -= v[k];
                    }
                    n -= 1.0;
                }
                if y >= 0 {
                    a[y as usize * WI + x] = [acc[0] / n, acc[1] / n, acc[2] / n];
                }
            }
        }
    }
    a
}

/// `oilphoto <in.jpg|png> <out.png>`: paint a picture instead of a film frame. There are no shapes to follow, so the
/// painter works as one does from life: block the whole thing in with the big flat, then go back with smaller
/// brushes only where the canvas is still wrong; every stroke takes its colour from the subject, runs along the
/// subject's edges (never across them) and stops when the colour under it changes.
pub fn run_photo(args: &[String]) -> Result<(), String> {
    run_photo_with(args, &PhotoOptions::from_env())
}

/// MOVED (crabscape): the DEEP-SEA TEST switches of `run_photo`, as options (see each one's comment in
/// `run_photo_with`). `from_env` reads them from `OIL_NOCTURNE`, `OIL_ATTENTION`, `OIL_KEY` and `OIL_GROUND`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhotoOptions {
    /// `OIL_NOCTURNE=g`
    pub nocturne: Option<f32>,
    /// `OIL_ATTENTION=off` sets this to false
    pub attention: bool,
    /// `OIL_KEY` (default 0.56)
    pub key: f32,
    /// `OIL_GROUND=subject`
    pub ground_subject: bool,
}
impl Default for PhotoOptions {
    fn default() -> Self {
        PhotoOptions { nocturne: None, attention: true, key: 0.56, ground_subject: false }
    }
}
impl PhotoOptions {
    pub fn from_env() -> Self {
        PhotoOptions {
            nocturne: std::env::var("OIL_NOCTURNE").ok().and_then(|v| v.parse::<f32>().ok()),
            attention: !std::env::var("OIL_ATTENTION").is_ok_and(|v| v == "off"),
            key: std::env::var("OIL_KEY").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.56),
            ground_subject: std::env::var("OIL_GROUND").is_ok_and(|v| v == "subject"),
        }
    }
}

/// `run_photo` with its switches given rather than read from the environment.
pub fn run_photo_with(args: &[String], opts: &PhotoOptions) -> Result<(), String> {
    let (input, out) = (args.first().ok_or("oilphoto <in> <out.png>")?, args.get(1).ok_or("oilphoto <in> <out.png>")?);
    let img = image::open(input).map_err(|e| e.to_string())?.to_rgb8();
    // the canvas is landscape: a portrait picture is painted on its side and turned back afterwards
    let turned = img.height() > img.width();
    let img = if turned { image::imageops::rotate270(&img) } else { img };
    let scale = (HI as f32 / img.height() as f32).min(WI as f32 / img.width() as f32);
    let (rw, rh) = ((img.width() as f32 * scale) as usize, (img.height() as f32 * scale) as usize);
    let small = image::imageops::resize(&img, rw as u32, rh as u32, image::imageops::FilterType::Triangle);
    let (ox, oy) = ((WI - rw) / 2, (HI - rh) / 2);
    let mut p = Painting::new();
    let mut subject = p.col.clone();
    for y in 0..rh {
        for x in 0..rw {
            let c = small.get_pixel(x as u32, y as u32).0;
            subject[(y + oy) * WI + x + ox] = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
        }
    }
    // DEEP-SEA TEST (crabscape): `OIL_NOCTURNE=g` paints a night picture as a painter does. Their eye adapts to the
    // dark and sees its structure (so here the subject is seen lifted by 1/g, and every judgement of shape, edge and
    // colour is made on that), then they transpose the whole value scale down when mixing (the canvas is mapped
    // back by g just before it is lit), so streaks and warm shifts stay in proportion to the darks they sit in.
    let nocturne = opts.nocturne;
    if let Some(g) = nocturne {
        for c in subject.iter_mut() {
            for v in c.iter_mut() {
                *v = v.max(0.0).powf(1.0 / g);
            }
        }
    }
    // ---- see it as a person does, not as a lens does. A camera gives every part of the scene the same attention
    // through a wide lens. A person looks AT something: that thing is large and clear, and the rest of the room is
    // a few quiet planes. `focus` is where the painter looks (x, y and reach, as fractions of the upright picture).
    let arg = |i: usize, d: f32| args.get(i).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    let (fu, fv, fr) = (arg(2, 0.5), arg(3, 0.4), arg(4, 0.34));
    let focus = if turned { [ox as f32 + fv * rw as f32, oy as f32 + (1.0 - fu) * rh as f32] } else { [ox as f32 + fu * rw as f32, oy as f32 + fv * rh as f32] };
    let reach_px = fr * rw.max(rh) as f32;
    // DEEP-SEA TEST (crabscape): `OIL_ATTENTION=off` paints every part of the picture as if it were looked at: no
    // magnification around the focus, no quieting away from it, small brushes allowed everywhere. For a subject
    // that must keep its exact drawn shape (a game sprite painted on its own).
    let attend = opts.attention;
    let attention = |q: [f32; 2]| if attend { (-(((q[0] - focus[0]).hypot(q[1] - focus[1])) / (1.2 * reach_px)).powi(2)).exp() } else { 1.0 };
    // DEEP-SEA TEST (crabscape): `OIL_KEY` is the value the unattended room relaxes towards (default 0.56, a daylight
    // middle grey). A nocturne's middle value is near black: there the quiet parts must sink, not lift to grey.
    let key = opts.key;
    {
        // what is looked at is seen larger than the lens drew it; the room around it gives way
        let lens = subject.clone();
        let at = |x: f32, y: f32| -> [f32; 3] {
            let (x, y) = (x.clamp(ox as f32, (ox + rw) as f32 - 1.001), y.clamp(oy as f32, (oy + rh) as f32 - 1.001));
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (tx, ty) = (x - x0 as f32, y - y0 as f32);
            std::array::from_fn(|k| {
                let a = lens[y0 * WI + x0][k] + (lens[y0 * WI + x0 + 1][k] - lens[y0 * WI + x0][k]) * tx;
                let b = lens[(y0 + 1) * WI + x0][k] + (lens[(y0 + 1) * WI + x0 + 1][k] - lens[(y0 + 1) * WI + x0][k]) * tx;
                a + (b - a) * ty
            })
        };
        for y in oy..oy + rh {
            for x in ox..ox + rw {
                let d = [x as f32 - focus[0], y as f32 - focus[1]];
                let near = (-(d[0].hypot(d[1]) / reach_px).powi(2) * 1.1).exp();
                let g = if attend { 1.0 - 0.26 * near } else { 1.0 }; // DEEP-SEA TEST (crabscape): see OIL_ATTENTION
                let mut c = at(focus[0] + d[0] * g, focus[1] + d[1] * g);
                // away from it: colour quieter, lights and darks drawn together, a little cooler and lighter
                let away = 1.0 - attention([x as f32, y as f32]);
                let grey = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
                for (k, v) in c.iter_mut().enumerate() {
                    let quiet = grey + (*v - grey) * (1.0 - 0.38 * away);
                    *v = key + (quiet - key) * (1.0 - 0.30 * away) + [-0.012, 0.0, 0.02][k] * away;
                }
                subject[y * WI + x] = c;
            }
        }
    }
    let dist = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let lum = |c: [f32; 3]| 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
    let inside = |q: [f32; 2]| q[0] >= ox as f32 && q[1] >= oy as f32 && q[0] < (ox + rw) as f32 && q[1] < (oy + rh) as f32;
    let px = |q: [f32; 2]| (q[1].clamp(0.0, HI as f32 - 1.0) as usize) * WI + q[0].clamp(0.0, WI as f32 - 1.0) as usize;
    let t0 = std::time::Instant::now();
    // DEEP-SEA TEST (crabscape): `OIL_GROUND=subject` tones the ground with a broad wash of the subject's own colours
    // (an imprimatura laid in to suit the picture) instead of the default warm mid ochre, so a dark picture's
    // skipped bristles show dark ground, not bright brown.
    if opts.ground_subject {
        let wash = box_blur(&subject, 48);
        p.col.copy_from_slice(&wash);
        p.flat.copy_from_slice(&wash);
    }
    // ---- block the whole canvas in with the big flat, squinting
    {
        let seen = box_blur(&subject, 26);
        let w = 70.0f32;
        let mut spots: Vec<([f32; 2], u32)> = vec![];
        for gy in 0..(rh as f32 / (w * 0.5)) as u32 + 1 {
            for gx in 0..(rw as f32 / (w * 0.5)) as u32 + 1 {
                let id = 0x7700_0000 | gy << 12 | gx;
                let q = [ox as f32 + (gx as f32 + hf(id, 1)) * w * 0.5, oy as f32 + (gy as f32 + hf(id, 2)) * w * 0.5];
                if inside(q) {
                    spots.push((q, id));
                }
            }
        }
        spots.sort_by_key(|(_, id)| h32(*id));
        for (q, id) in spots {
            let a = -0.6 + (noise(q[0] / 420.0, q[1] / 420.0, 5) - 0.5) * 0.7;
            let d = [a.cos(), a.sin()];
            let path: Vec<[f32; 2]> = (0..7).map(|i| [q[0] + d[0] * w * 0.45 * (i as f32 - 3.0), q[1] + d[1] * w * 0.45 * (i as f32 - 3.0)]).collect();
            let c = seen[px(q)];
            sweep(&mut p, Tool::Flat, &path, w * (0.85 + 0.3 * hf(id, 4)), c, shifted(c, 1.06, 0.4), 0.97, 0.2, 0.5, id, &|_| 1.0);
        }
    }
    // ---- structure first. A cellular automaton finds the picture's regions: seeds are scattered (thickly where
    // the picture is busy, thinly where it is calm), and on every step each cell may be taken over by a neighbour's
    // region if that region's colour suits it better than its own does. Regions that end up alike are joined.
    let calm = box_blur(&subject, 2);
    let entropy: Vec<f32> = {
        let mean = box_blur(&calm, 6);
        let sq: Vec<[f32; 3]> = calm.iter().map(|c| [c[0] * c[0], c[1] * c[1], c[2] * c[2]]).collect();
        let msq = box_blur(&sq, 6);
        (0..WI * HI).map(|i| (0..3).map(|k| (msq[i][k] - mean[i][k] * mean[i][k]).max(0.0)).sum::<f32>().sqrt()).collect()
    };
    let mut label = vec![0u32; WI * HI];
    let mut strength = vec![0.0f32; WI * HI];
    let mut seed_col: Vec<[f32; 3]> = vec![[0.0; 3]];
    for gy in 0..(rh / 12) as u32 + 1 {
        for gx in 0..(rw / 12) as u32 + 1 {
            let q = [ox as f32 + (gx as f32 + hf(gx, gy * 7 + 1)) * 12.0, oy as f32 + (gy as f32 + hf(gx, gy * 7 + 2)) * 12.0];
            if !inside(q) {
                continue;
            }
            let busy = (entropy[px(q)] / 0.09).clamp(0.0, 1.0) * attention(q);
            let coarse = gx % 5 == 2 && gy % 5 == 2;
            if coarse || hf(gx, gy * 7 + 3) < busy * busy {
                label[px(q)] = seed_col.len() as u32;
                strength[px(q)] = 1.0;
                seed_col.push(calm[px(q)]);
            }
        }
    }
    {
        use rayon::prelude::*;
        for round in 0..2 {
            let reach = if round == 0 { 0.30 } else { 3.0 };
            for _ in 0..400 {
                let (lab0, str0) = (label.clone(), strength.clone());
                let changed: usize = label
                    .par_chunks_mut(WI)
                    .zip(strength.par_chunks_mut(WI))
                    .enumerate()
                    .map(|(y, (lrow, srow))| {
                        let mut n = 0;
                        if y < oy || y >= oy + rh {
                            return 0;
                        }
                        for x in ox..ox + rw {
                            let me = calm[y * WI + x];
                            let (mut best, mut who) = (srow[x], lrow[x]);
                            for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                                let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                                if xx < ox as i32 || yy < oy as i32 || xx >= (ox + rw) as i32 || yy >= (oy + rh) as i32 {
                                    continue;
                                }
                                let j = yy as usize * WI + xx as usize;
                                let l = lab0[j];
                                if l == 0 || l == who {
                                    continue;
                                }
                                let suits = (1.0 - dist(me, seed_col[l as usize]) / reach).max(0.0);
                                let force = str0[j] * suits * if dx != 0 && dy != 0 { 0.978 } else { 0.985 };
                                if force > best + 1e-4 {
                                    (best, who) = (force, l);
                                }
                            }
                            if who != lrow[x] {
                                (lrow[x], srow[x]) = (who, best);
                                n += 1;
                            }
                        }
                        n
                    })
                    .sum();
                if changed == 0 {
                    break;
                }
            }
        }
    }
    // join neighbours that are alike (more readily where the picture is calm): big quiet planes become one shape
    let nl = seed_col.len();
    let (mut sum, mut cnt, mut ent) = (vec![[0.0f64; 3]; nl], vec![0.0f64; nl], vec![0.0f64; nl]);
    let mut looked = vec![0.0f64; nl];
    for y in oy..oy + rh {
        for x in ox..ox + rw {
            let (i, l) = (y * WI + x, label[y * WI + x] as usize);
            for k in 0..3 {
                sum[l][k] += subject[i][k] as f64;
            }
            cnt[l] += 1.0;
            ent[l] += entropy[i] as f64;
            looked[l] += attention([x as f32, y as f32]) as f64;
        }
    }
    let mut pairs: std::collections::HashSet<(u32, u32)> = Default::default();
    for y in oy..oy + rh - 1 {
        for x in ox..ox + rw - 1 {
            let a = label[y * WI + x];
            for b in [label[y * WI + x + 1], label[(y + 1) * WI + x]] {
                if a != b {
                    pairs.insert((a.min(b), a.max(b)));
                }
            }
        }
    }
    let mut parent: Vec<u32> = (0..nl as u32).collect();
    fn root(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    let mean_of = |sum: &[[f64; 3]], cnt: &[f64], l: usize| -> [f32; 3] { [(sum[l][0] / cnt[l].max(1.0)) as f32, (sum[l][1] / cnt[l].max(1.0)) as f32, (sum[l][2] / cnt[l].max(1.0)) as f32] };
    let mut order: Vec<(f32, u32, u32)> = pairs.iter().map(|&(a, b)| (dist(mean_of(&sum, &cnt, a as usize), mean_of(&sum, &cnt, b as usize)), a, b)).collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, a, b) in order {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra == rb {
            continue;
        }
        let busy = (((ent[ra as usize] + ent[rb as usize]) / (cnt[ra as usize] + cnt[rb as usize]).max(1.0)) as f32 / 0.09).clamp(0.0, 1.0);
        let away = 1.0 - ((looked[ra as usize] + looked[rb as usize]) / (cnt[ra as usize] + cnt[rb as usize]).max(1.0)) as f32;
        if dist(mean_of(&sum, &cnt, ra as usize), mean_of(&sum, &cnt, rb as usize)) < 0.10 - 0.055 * busy + 0.11 * away {
            parent[rb as usize] = ra;
            for k in 0..3 {
                sum[ra as usize][k] += sum[rb as usize][k];
            }
            cnt[ra as usize] += cnt[rb as usize];
            ent[ra as usize] += ent[rb as usize];
            looked[ra as usize] += looked[rb as usize];
        }
    }
    for y in oy..oy + rh {
        for x in ox..ox + rw {
            label[y * WI + x] = root(&mut parent, label[y * WI + x]) + 1;
        }
    }
    // ---- regions to shapes: each connected patch of one region, its outline traced and straightened a little
    let mut comp = vec![0u32; WI * HI];
    let mut shapes: Vec<(usize, [f32; 3], f32, Vec<[f32; 2]>)> = vec![];
    let mut stack: Vec<usize> = vec![];
    for y0 in oy..oy + rh {
        for x0 in ox..ox + rw {
            if comp[y0 * WI + x0] != 0 {
                continue;
            }
            let (id, l) = (shapes.len() as u32 + 1, label[y0 * WI + x0]);
            let (mut area, mut col, mut en) = (0usize, [0.0f64; 3], 0.0f64);
            comp[y0 * WI + x0] = id;
            stack.push(y0 * WI + x0);
            while let Some(i) = stack.pop() {
                area += 1;
                en += entropy[i] as f64;
                for k in 0..3 {
                    col[k] += subject[i][k] as f64;
                }
                let (x, y) = (i % WI, i / WI);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                    if xx < ox as i32 || yy < oy as i32 || xx >= (ox + rw) as i32 || yy >= (oy + rh) as i32 {
                        continue;
                    }
                    let j = yy as usize * WI + xx as usize;
                    if comp[j] == 0 && label[j] == l {
                        comp[j] = id;
                        stack.push(j);
                    }
                }
            }
            // walk the outline (the first cell found is the top-left one), keeping the patch on the right hand
            let is = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < WI && (y as usize) < HI && comp[y as usize * WI + x as usize] == id;
            let dirs = [(1i32, 0i32), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
            let (mut cx, mut cy, mut d) = (x0 as i32, y0 as i32, 0usize);
            let mut outline: Vec<[f32; 2]> = vec![];
            for _ in 0..(8 * area + 64).min(400_000) {
                outline.push([cx as f32 + 0.5, cy as f32 + 0.5]);
                let mut moved = false;
                for k in 0..8 {
                    let nd = (d + 6 + k) % 8;
                    if is(cx + dirs[nd].0, cy + dirs[nd].1) {
                        (cx, cy, d, moved) = (cx + dirs[nd].0, cy + dirs[nd].1, nd, true);
                        break;
                    }
                }
                if !moved || (cx == x0 as i32 && cy == y0 as i32 && outline.len() > 2) {
                    break;
                }
            }
            // keep every point that stands more than 1.5 px off the line between the kept ones either side
            let mut kept = vec![false; outline.len()];
            if outline.len() >= 3 {
                let mut todo = vec![(0usize, outline.len() - 1)];
                kept[0] = true;
                let last = outline.len() - 1;
                kept[last] = true;
                while let Some((a, b)) = todo.pop() {
                    let (pa, pb) = (outline[a], outline[b]);
                    let l = (pb[0] - pa[0]).hypot(pb[1] - pa[1]).max(1e-3);
                    let (mut far, mut at) = (0.0f32, a);
                    for i in a + 1..b {
                        let dd = if l < 1.0 {
                            (outline[i][0] - pa[0]).hypot(outline[i][1] - pa[1])
                        } else {
                            ((pb[0] - pa[0]) * (outline[i][1] - pa[1]) - (pb[1] - pa[1]) * (outline[i][0] - pa[0])).abs() / l
                        };
                        if dd > far {
                            (far, at) = (dd, i);
                        }
                    }
                    if far > 1.5 {
                        kept[at] = true;
                        todo.push((a, at));
                        todo.push((at, b));
                    }
                }
            }
            let poly: Vec<[f32; 2]> = outline.iter().zip(&kept).filter(|(_, k)| **k).map(|(q, _)| *q).collect();
            let a = area as f64;
            shapes.push((area, [(col[0] / a) as f32, (col[1] / a) as f32, (col[2] / a) as f32], (en / a) as f32, poly));
        }
    }
    eprintln!("structure: {} regions from {} seeds, {:.1} s", shapes.len(), nl - 1, t0.elapsed().as_secs_f64());
    // ---- paint the shapes as the film's shapes are painted: biggest first, each with the tool for its job
    shapes.sort_by(|a, b| b.0.cmp(&a.0));
    for (k, (area, c, en, poly)) in shapes.iter().enumerate() {
        if poly.len() < 3 {
            continue;
        }
        // how much the painter is looking at this shape: at its middle
        let n = poly.len() as f32;
        let a = attention([poly.iter().map(|q| q[0]).sum::<f32>() / n, poly.iter().map(|q| q[1]).sum::<f32>() / n]);
        // away from the subject, small things are simply not painted: the block-in stands for them
        if (*area as f32) < 40.0 + 1400.0 * (1.0 - a).powi(2) {
            continue;
        }
        let quiet = (*en < 0.035 && *area > 9000) || a < 0.4;
        let alt = shifted(*c, 1.1, 0.5);
        paint_body(&mut p, poly, *c, alt, 0.97, 0x5000 + k as u32, 1.7 - 0.95 * a, 0.5 + 0.65 * a, if quiet { Tool::Flat } else { Tool::Filbert });
    }
    eprintln!("shapes painted: {} strokes, {:.1} s", p.strokes, t0.elapsed().as_secs_f64());
    // ---- then look again, and touch in with the small brushes only where it is still wrong
    for (layer, (w, tool, wrong, load)) in [(13.0f32, Tool::Filbert, 0.11f32, 0.9f32), (SMALLEST, Tool::Round, 0.10, 1.0)].into_iter().enumerate() {
        let layer = layer + 2;
        // squint: see the subject no more finely than this brush can paint it
        let seen = box_blur(&subject, (w * 0.38) as usize);
        let grad = |q: [f32; 2]| -> [f32; 2] {
            let d = (w * 0.3).max(1.5);
            [lum(seen[px([q[0] + d, q[1]])]) - lum(seen[px([q[0] - d, q[1]])]), lum(seen[px([q[0], q[1] + d])]) - lum(seen[px([q[0], q[1] - d])])]
        };
        let step = w * 0.55;
        let mut spots: Vec<([f32; 2], u32)> = vec![];
        for gy in 0..(rh as f32 / step) as u32 + 1 {
            for gx in 0..(rw as f32 / step) as u32 + 1 {
                let id = (layer as u32) << 24 | gy << 12 | gx;
                let q = [ox as f32 + (gx as f32 + hf(id, 1)) * step, oy as f32 + (gy as f32 + hf(id, 2)) * step];
                if !inside(q) {
                    continue;
                }
                // is the canvas still wrong here, as far as this brush can tell?
                let mut err = 0.0;
                for (dx, dy) in [(0.0, 0.0), (0.4, 0.0), (-0.4, 0.0), (0.0, 0.4), (0.0, -0.4)] {
                    let s = [q[0] + dx * step, q[1] + dy * step];
                    err += dist(p.col[px(s)], seen[px(s)]) / 5.0;
                }
                if attention(q) > [0.0, 0.0, 0.42, 0.6][layer.min(3)] && err > wrong {
                    spots.push((q, id));
                }
            }
        }
        spots.sort_by_key(|(_, id)| h32(*id));
        for (q, id) in spots {
            let c = seen[px(q)];
            // along the edge, not across it; where there is no edge, the painter's own diagonal
            let mut dir = {
                let g = grad(q);
                if g[0].hypot(g[1]) > 0.006 {
                    let l = g[0].hypot(g[1]);
                    [-g[1] / l, g[0] / l]
                } else {
                    let a = -0.6 + (noise(q[0] / 380.0, q[1] / 380.0, 5) - 0.5) * 0.9;
                    [a.cos(), a.sin()]
                }
            };
            // pull, don't push
            if dir[0] * 0.75 + dir[1] < 0.0 {
                dir = [-dir[0], -dir[1]];
            }
            let mut path = vec![[q[0] - dir[0] * w * 0.6, q[1] - dir[1] * w * 0.6], q];
            let (seg, most) = (w * 0.45, (7.0 + 9.0 * hf(id, 3)) as usize);
            let mut at = q;
            for i in 0..most {
                let next = [at[0] + dir[0] * seg, at[1] + dir[1] * seg];
                // the stroke ends where its colour stops being right for what is under it
                if !inside(next) || (i >= 2 && dist(seen[px(next)], c) > dist(seen[px(next)], p.col[px(next)]).max(0.06)) {
                    break;
                }
                path.push(next);
                at = next;
                let g = grad(at);
                let l = g[0].hypot(g[1]);
                if l > 0.006 {
                    let mut d = [-g[1] / l, g[0] / l];
                    if d[0] * dir[0] + d[1] * dir[1] < 0.0 {
                        d = [-d[0], -d[1]];
                    }
                    // a wrist turns smoothly
                    let nd = [dir[0] * 0.55 + d[0] * 0.45, dir[1] * 0.55 + d[1] * 0.45];
                    let nl = nd[0].hypot(nd[1]).max(1e-3);
                    dir = [nd[0] / nl, nd[1] / nl];
                }
            }
            let width = w * (0.8 + 0.35 * hf(id, 4));
            sweep(&mut p, tool, &path, width, c, shifted(c, 1.1, 0.5), 0.96, 0.12 + 0.14 * hf(id, 5), load, id, &|_| 1.0);
        }
        eprintln!("brush {w:>4.0}: {} strokes so far, {:.1} s", p.strokes, t0.elapsed().as_secs_f64());
    }
    // DEEP-SEA TEST (crabscape): the nocturne's values transposed down (see `OIL_NOCTURNE` above)
    if let Some(g) = nocturne {
        for c in p.col.iter_mut() {
            for v in c.iter_mut() {
                *v = v.max(0.0).powf(g);
            }
        }
    }
    let lit = p.lit();
    let mut crop = vec![0u8; rw * rh * 3];
    for y in 0..rh {
        crop[y * rw * 3..(y + 1) * rw * 3].copy_from_slice(&lit[((y + oy) * WI + ox) * 3..((y + oy) * WI + ox + rw) * 3]);
    }
    let done = image::RgbImage::from_raw(rw as u32, rh as u32, crop).ok_or("image")?;
    let done = if turned { image::imageops::rotate90(&done) } else { done };
    done.save(out).map_err(|e| e.to_string())
}

/// `oilmix <out.png>`: the same wet-into-wet strokes painted twice. Top: picked-up paint averaged as RGB. Bottom:
/// mixed as pigment. Each column is a wet field of one colour with a second colour dragged through it.
pub fn run_mix(args: &[String]) -> Result<(), String> {
    let out = args.first().ok_or("oilmix <out.png>")?;
    let pairs: [([f32; 3], [f32; 3]); 5] = [
        ([0.96, 0.82, 0.16], [0.10, 0.25, 0.70]),
        ([0.10, 0.25, 0.70], [0.97, 0.96, 0.92]),
        ([0.86, 0.16, 0.14], [0.96, 0.82, 0.16]),
        ([0.16, 0.50, 0.30], [0.80, 0.20, 0.45]),
        ([0.92, 0.50, 0.12], [0.12, 0.45, 0.60]),
    ];
    let mut p = Painting::new();
    for row in 0..2 {
        PIGMENT.store(row == 1, Ordering::Relaxed);
        let (y0, h) = (40.0 + row as f32 * 520.0, 480.0);
        for (k, (field, drag)) in pairs.iter().enumerate() {
            let (x0, w) = (40.0 + k as f32 * 376.0, 336.0);
            let rect = [[x0, y0], [x0 + w, y0], [x0 + w, y0 + h], [x0, y0 + h]];
            paint_body(&mut p, &rect, *field, *field, 0.98, 100 + k as u32, 1.0, 1.0, Tool::Flat);
            // the second colour dragged through, the brush picking up more of the field as it goes
            for j in 0..5 {
                let y = y0 + 60.0 + j as f32 * 88.0;
                let path: Vec<[f32; 2]> = (0..40).map(|i| [x0 + 26.0 + i as f32 * (w - 52.0) / 39.0, y + 14.0 * ((i as f32) * 0.22 + j as f32).sin()]).collect();
                sweep(&mut p, Tool::Filbert, &path, 46.0, *drag, *drag, 0.97, 0.25 + 0.2 * j as f32, 1.0, 900 + (k * 10 + j) as u32, &|_| 1.0);
            }
        }
    }
    image::RgbImage::from_raw(WI as u32, HI as u32, p.lit()).ok_or("image")?.save(out).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- the watercolour brush (after p5.brush)
//
// A port of p5.brush's fill, step for step: the shape is copied about ninety times as thin layers, each copy grown
// outwards a little differently (a new point in the middle of every side, pushed out by a random share of the side's
// length), two of every three cut short by a chord, plus a sparse copy through a few of its points and a darker cut
// copy; every copy gets a faint outline; four times along the way several hundred faint circles are lifted back out
// and what has built up is laid onto the picture as pigment, with a little more wherever its density changes
// sharply (tide lines).

pub struct Roll(u64);
impl Roll {
    pub fn new(seed: u32) -> Roll {
        let mut r = Roll((seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            r.unit();
        }
        r
    }
    pub fn unit(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / 16_777_216.0
    }
    pub fn rr(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }
    pub fn gauss(&mut self, mean: f32, sd: f32) -> f32 {
        let (u, v) = (self.unit().max(1e-7), self.unit());
        mean + sd * (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
    }
}

pub fn in_poly(v: &[[f32; 2]], p: [f32; 2]) -> bool {
    let mut inside = false;
    for i in 0..v.len() {
        let (a, b) = (v[i], v[(i + 1) % v.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
    }
    inside
}

#[derive(Clone)]
pub struct FillPoly {
    pub v: Vec<[f32; 2]>,
    /// How strongly each side grows.
    pub m: Vec<f32>,
    /// Whether a quarter turn of each side points out of the shape.
    pub dir: Vec<bool>,
    pub mid: [f32; 2],
    pub size: [f32; 2],
}

impl FillPoly {
    pub fn trim(&self, f: f32, r: &mut Roll) -> (Vec<[f32; 2]>, Vec<f32>, Vec<bool>) {
        let n = self.v.len();
        if !(0.0..1.0).contains(&f) || n <= 8 {
            return (self.v.clone(), self.m.clone(), self.dir.clone());
        }
        let cut = ((1.0 - f) * n as f32) as usize;
        let s = n / 2 - cut / 2;
        let (a, b) = (self.v[(s + n - 1) % n], self.v[(s + cut) % n]);
        let (ev, len) = ([b[0] - a[0], b[1] - a[1]], (b[0] - a[0]).hypot(b[1] - a[1]));
        let at = if s >= 2 {
            r.rr(0.0, s as f32 - 1.0) as usize
        } else if s + cut < n - 1 {
            s + cut
        } else {
            0
        };
        let spacing = (self.v[(at + 1) % n][0] - self.v[at][0]).hypot(self.v[(at + 1) % n][1] - self.v[at][1]).max(1.0);
        let add = ((len / spacing * 0.05).ceil() as usize).max(2);
        let (mut v, mut m, mut d) = (self.v[..s].to_vec(), self.m[..s].to_vec(), self.dir[..s].to_vec());
        let jitter = len * 0.06;
        for k in 0..add {
            let t = (k + 1) as f32 / (add + 1) as f32;
            v.push([a[0] + ev[0] * t + r.rr(-jitter, jitter), a[1] + ev[1] * t + r.rr(-jitter, jitter)]);
            m.push(r.rr(0.3, 0.5));
            d.push(self.dir[s % self.dir.len()]);
        }
        v.extend_from_slice(&self.v[(s + cut).min(n)..]);
        m.extend_from_slice(&self.m[(s + cut).min(n)..]);
        d.extend_from_slice(&self.dir[(s + cut).min(n)..]);
        (v, m, d)
    }

    /// `f` below 1 trims first; 999 and 997 are the library's two texturing passes (a strong even push, then a mild one).
    pub fn grow(&self, f: f32, bleed: f32, cap: usize, r: &mut Roll) -> FillPoly {
        let (tv, tm, td) = self.trim(f, r);
        let n = tv.len();
        let mut push = if f == 999.0 { r.rr(0.6, 0.8) } else { bleed };
        let pre = if n * 2 > cap { (n * 2).div_ceil(cap) } else { 1 };
        if pre >= 2 && pre % 2 == 0 {
            return FillPoly { v: tv, m: tm, dir: td, mid: self.mid, size: self.size };
        }
        let (mut v, mut m, mut d) = (Vec::with_capacity(n * 2), Vec::with_capacity(n * 2), Vec::with_capacity(n * 2));
        for i in 0..n {
            let (a, b) = (tv[i], tv[(i + 1) % n]);
            if f < 997.0 {
                push = tm[i];
            }
            v.push(a);
            m.push(tm[i]);
            d.push(td[i]);
            let side = [b[0] - a[0], b[1] - a[1]];
            if push < 0.05 {
                v.push([a[0] + side[0] / 2.0, a[1] + side[1] / 2.0]);
                m.push(tm[i]);
                d.push(td[i]);
                continue;
            }
            let ang = (if td[i] { -90.0f32 } else { 90.0 } + r.rr(-1.0, 1.0) * 5.0).to_radians();
            let (s, c) = ang.sin_cos();
            let out = [c * side[0] + s * side[1], c * side[1] - s * side[0]];
            let far = r.gauss(0.5, 0.2) * r.rr(0.65, 1.35) * push;
            v.push([a[0] + side[0] / 2.0 + out[0] * far, a[1] + side[1] / 2.0 + out[1] * far]);
            m.push(tm[i] + r.gauss(0.0, 0.02));
            d.push(td[i]);
        }
        if v.len() > cap {
            let step = v.len().div_ceil(cap);
            v = v.into_iter().step_by(step).collect();
            m = m.into_iter().step_by(step).collect();
            d = d.into_iter().step_by(step).collect();
        }
        FillPoly { v, m, dir: d, mid: self.mid, size: self.size }
    }

    pub fn scatter(&self, ratio: f32, shape: &[[f32; 2]], r: &mut Roll) -> FillPoly {
        let n = self.v.len();
        let keep = ((n as f32 * ratio) as usize).max(3);
        let step = n as f32 / keep as f32;
        let (mut v, mut m, mut d) = (vec![], vec![], vec![]);
        for i in 0..keep {
            let j = ((i as f32 * step + r.rr(0.0, step * 0.8)) as usize) % n;
            let mut q = self.v[j];
            if !in_poly(shape, q) {
                let k = r.rr(0.3, 0.6);
                q = [self.mid[0] + (q[0] - self.mid[0]) * k, self.mid[1] + (q[1] - self.mid[1]) * r.rr(0.3, 0.6)];
            }
            v.push(q);
            m.push(self.m[j]);
            d.push(!self.dir[j]);
        }
        FillPoly { v, m, dir: d, mid: self.mid, size: self.size }
    }

    pub fn flipped(mut self) -> FillPoly {
        for d in &mut self.dir {
            *d = !*d;
        }
        self
    }
}

/// What has been painted since the last composite: alpha over the fill's part of the canvas.
pub struct Mask {
    pub o: [i32; 2],
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
    /// A stroke's own coverage (so its joints are not laid twice), valid where `stamp` is the current stroke.
    pub cov: Vec<f32>,
    pub stamp: Vec<u32>,
    pub stroke: u32,
    pub row: Vec<f32>,
}

impl Mask {
    pub fn new(lo: [f32; 2], hi: [f32; 2]) -> Option<Mask> {
        let o = [(lo[0].floor() as i32).max(0), (lo[1].floor() as i32).max(0)];
        let e = [(hi[0].ceil() as i32).min(WI as i32), (hi[1].ceil() as i32).min(HI as i32)];
        if e[0] <= o[0] || e[1] <= o[1] {
            return None;
        }
        let (w, h) = ((e[0] - o[0]) as usize, (e[1] - o[1]) as usize);
        Some(Mask { o, w, h, a: vec![0.0; w * h], cov: vec![0.0; w * h], stamp: vec![0; w * h], stroke: 0, row: vec![0.0; w] })
    }

    /// A filled polygon (non-zero winding, four sample rows per pixel), laid over what is there.
    pub fn fill(&mut self, v: &[[f32; 2]], alpha: f32) {
        let n = v.len();
        if n < 3 || alpha <= 0.0 {
            return;
        }
        let (mut y0, mut y1) = (f32::MAX, f32::MIN);
        for q in v {
            y0 = y0.min(q[1]);
            y1 = y1.max(q[1]);
        }
        let (ya, yb) = (((y0 - self.o[1] as f32).floor() as i32).max(0), ((y1 - self.o[1] as f32).ceil() as i32).min(self.h as i32));
        let mut xs: Vec<(f32, i32)> = vec![];
        for y in ya..yb {
            self.row.fill(0.0);
            let mut any = false;
            for sub in 0..4 {
                let py = (y + self.o[1]) as f32 + (sub as f32 + 0.5) / 4.0;
                xs.clear();
                for i in 0..n {
                    let (a, b) = (v[i], v[(i + 1) % n]);
                    if (a[1] > py) != (b[1] > py) {
                        xs.push((a[0] + (py - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) - self.o[0] as f32, if b[1] > a[1] { 1 } else { -1 }));
                    }
                }
                if xs.len() < 2 {
                    continue;
                }
                xs.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut wind = 0;
                for k in 0..xs.len() - 1 {
                    wind += xs[k].1;
                    if wind == 0 {
                        continue;
                    }
                    let (xa, xb) = (xs[k].0.max(0.0), xs[k + 1].0.min(self.w as f32));
                    if xb <= xa {
                        continue;
                    }
                    any = true;
                    let (ia, ib) = (xa.floor() as usize, (xb.ceil() as usize).min(self.w));
                    for x in ia..ib {
                        let c = (xb.min(x as f32 + 1.0) - xa.max(x as f32)).max(0.0);
                        self.row[x] += 0.25 * c;
                    }
                }
            }
            if any {
                let base = y as usize * self.w;
                for x in 0..self.w {
                    let c = self.row[x];
                    if c > 0.0 {
                        let a = &mut self.a[base + x];
                        *a += (1.0 - *a) * alpha * c.min(1.0);
                    }
                }
            }
        }
    }

    /// The polygon's outline as one stroke with round ends.
    pub fn outline(&mut self, v: &[[f32; 2]], width: f32, alpha: f32) {
        if width < 0.3 || alpha <= 0.0 {
            return;
        }
        self.stroke += 1;
        let half = width / 2.0;
        let n = v.len();
        for i in 0..n {
            let (a, b) = ([v[i][0] - self.o[0] as f32, v[i][1] - self.o[1] as f32], [v[(i + 1) % n][0] - self.o[0] as f32, v[(i + 1) % n][1] - self.o[1] as f32]);
            let (x0, x1) = (((a[0].min(b[0]) - half - 1.0).floor() as i32).max(0), ((a[0].max(b[0]) + half + 1.0).ceil() as i32).min(self.w as i32));
            let (y0, y1) = (((a[1].min(b[1]) - half - 1.0).floor() as i32).max(0), ((a[1].max(b[1]) + half + 1.0).ceil() as i32).min(self.h as i32));
            let ab = [b[0] - a[0], b[1] - a[1]];
            let l2 = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-6);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = [x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]];
                    let t = ((p[0] * ab[0] + p[1] * ab[1]) / l2).clamp(0.0, 1.0);
                    let c = (half + 0.5 - (p[0] - ab[0] * t).hypot(p[1] - ab[1] * t)).clamp(0.0, 1.0);
                    if c <= 0.0 {
                        continue;
                    }
                    let i = y as usize * self.w + x as usize;
                    let before = if self.stamp[i] == self.stroke { self.cov[i] } else { 0.0 };
                    if c > before {
                        self.stamp[i] = self.stroke;
                        self.cov[i] = c;
                        self.a[i] += (1.0 - self.a[i]) * alpha * (c - before);
                    }
                }
            }
        }
    }

    pub fn lift(&mut self, c: [f32; 2], radius: f32, amount: f32) {
        let (cx, cy) = (c[0] - self.o[0] as f32, c[1] - self.o[1] as f32);
        let (x0, x1) = (((cx - radius - 1.0).floor() as i32).max(0), ((cx + radius + 1.0).ceil() as i32).min(self.w as i32));
        let (y0, y1) = (((cy - radius - 1.0).floor() as i32).max(0), ((cy + radius + 1.0).ceil() as i32).min(self.h as i32));
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = (radius + 0.5 - (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.a[y as usize * self.w + x as usize] *= 1.0 - amount * cov;
                }
            }
        }
    }

    pub fn stamp(&mut self, c: [f32; 2], radius: f32, alpha: f32) {
        let (cx, cy) = (c[0] - self.o[0] as f32, c[1] - self.o[1] as f32);
        let (x0, x1) = (((cx - radius - 1.0).floor() as i32).max(0), ((cx + radius + 1.0).ceil() as i32).min(self.w as i32));
        let (y0, y1) = (((cy - radius - 1.0).floor() as i32).max(0), ((cy + radius + 1.0).ceil() as i32).min(self.h as i32));
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = (radius + 0.5 - (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = &mut self.a[y as usize * self.w + x as usize];
                    *a += (1.0 - *a) * alpha * cov;
                }
            }
        }
    }

    /// As `composite`, for brush strokes: no tide lines, and where the tips have piled up the pigment goes darker.
    pub fn composite_stroke(&mut self, p: &mut Painting, pigment: [f32; 3]) {
        let ks_of = |c: [f32; 3]| crate::spectral::ks([c[0] as f64, c[1] as f64, c[2] as f64]);
        // the pigment at each strength of piling-up, worked out once
        let table: Vec<([f64; 38], f64)> = (0..=8)
            .map(|i| {
                let black = 0.5 * 0.3 * i as f32 / 8.0;
                let c = pigment.map(|v| (v * (1.0 - black) - 0.5 * black).max(0.0));
                let k = ks_of(c);
                let l = crate::spectral::colour(&k).1;
                (k, l)
            })
            .collect();
        for y in 0..self.h {
            for x in 0..self.w {
                let m = self.a[y * self.w + x];
                if m <= 0.003 {
                    continue;
                }
                let i = (y + self.o[1] as usize) * WI + x + self.o[0] as usize;
                let (kp, lp) = &table[(((m - 0.7).max(0.0) / 0.3 * 8.0).round() as usize).min(8)];
                let bg = p.col[i];
                let kb = ks_of(bg);
                let lb = crate::spectral::colour(&kb).1;
                let t = m.min(1.0) as f64;
                let (c1, c2) = ((1.0 - t) * (1.0 - t) * lb, t * t * lp);
                let total = (c1 + c2).max(1e-9);
                let mixed: [f64; 38] = std::array::from_fn(|b| (kb[b] * c1 + kp[b] * c2) / total);
                let rgb = crate::spectral::colour(&mixed).0;
                p.col[i] = [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32];
            }
        }
        self.a.fill(0.0);
    }

    /// Lay what has built up onto the picture as pigment, and start again. Kubelka-Munk over 38 bands, each paint's
    /// share going with the square of its strength and with its luminance (as the library mixes), so thin dark
    /// paint on a light ground barely shows and colours mix as paints do.
    pub fn composite(&mut self, p: &mut Painting, pigment: [f32; 3]) {
        use rayon::prelude::*;
        // A careful brush slows where one colour meets another and leaves a little more pigment there. Not in
        // p5.brush: `EDGE_DEPOSIT` is how much more (0 = the library's own faint tide line only).
        let deposit = std::env::var("OIL_EDGE").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(EDGE_DEPOSIT);
        let kp = crate::spectral::ks([pigment[0] as f64, pigment[1] as f64, pigment[2] as f64]);
        let lp = crate::spectral::colour(&kp).1;
        let (w, h, o) = (self.w, self.h, self.o);
        let a = &self.a;
        let at = |x: i32, y: i32| a[(y.clamp(0, h as i32 - 1) as usize) * w + x.clamp(0, w as i32 - 1) as usize];
        let rows: Vec<(usize, Vec<[f32; 3]>)> = (0..h)
            .into_par_iter()
            .filter_map(|y| {
                let base = (y + o[1] as usize) * WI + o[0] as usize;
                let mut out: Vec<[f32; 3]> = vec![];
                let mut touched = false;
                for x in 0..w {
                    let m = a[y * w + x];
                    let bg = p.col[base + x];
                    if m <= 0.0005 {
                        out.push(bg);
                        continue;
                    }
                    let mut tide = 0.0;
                    let mut rim = 0.0f32;
                    // how close this is to the wash's own edge: strongest right at it, gone a dozen pixels in
                    if deposit > 0.0 {
                        let mut out_near = 0.0f32;
                        for (k, reach) in [3i32, 6, 10, 14].into_iter().enumerate() {
                            let mut least = 1.0f32;
                            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)] {
                                least = least.min(at(x as i32 + dx * reach, y as i32 + dy * reach) / m.max(1e-3));
                            }
                            out_near = out_near.max((1.0 - least).clamp(0.0, 1.0) * [1.0, 0.7, 0.4, 0.2][k]);
                        }
                        rim = out_near;
                    }
                    for j in [-2i32, 0, 2] {
                        for i in [-2i32, 0, 2] {
                            let (xx, yy) = (x as i32 + i, y as i32 + j);
                            let g = [(at(xx + 1, yy) - at(xx - 1, yy)) * 7.5, (at(xx, yy + 1) - at(xx, yy - 1)) * 7.5];
                            tide += smooth(0.05, 0.35, g[0].hypot(g[1]));
                        }
                    }
                    let t = ((m + tide / 9.0 * 0.1).clamp(0.0, 1.0)) as f64;
                    let kb = crate::spectral::ks([bg[0] as f64, bg[1] as f64, bg[2] as f64]);
                    let lb = crate::spectral::colour(&kb).1;
                    let (c1, c2) = ((1.0 - t) * (1.0 - t) * lb, t * t * lp);
                    let total = (c1 + c2).max(1e-9);
                    // more pigment in the same place: its absorption goes up in proportion
                    let more = 1.0 + deposit * (rim * m) as f64;
                    let mixed: [f64; 38] = std::array::from_fn(|b| (kb[b] * c1 + kp[b] * c2 * more) / total);
                    let rgb = crate::spectral::colour(&mixed).0;
                    out.push([rgb[0] as f32, rgb[1] as f32, rgb[2] as f32]);
                    touched = true;
                }
                touched.then_some((y, out))
            })
            .collect();
        for (y, out) in rows {
            let base = (y + o[1] as usize) * WI + o[0] as usize;
            p.col[base..base + w].copy_from_slice(&out);
        }
        self.a.fill(0.0);
    }
}

/// p5.brush's `fill`. `opacity` 0..1; the kit's usual bleed 0.1, texture 0.4, border 0.35.
#[allow(clippy::too_many_arguments)]
pub fn brush_fill(p: &mut Painting, pts: &[[f32; 2]], col: [f32; 3], opacity: f32, bleed: f32, tex: f32, border: f32, seed: u32) {
    let n = pts.len();
    if n < 3 {
        return;
    }
    let mut r = Roll::new(seed);
    // the library starts the outline at a random point; the first stretch of it hardly grows
    let roll = r.rr(0.0, 75.0);
    let fluid = (n as f32
        * 0.25
        * if roll < 5.0 {
            1.0
        } else if roll < 15.0 {
            2.0
        } else {
            3.0
        }) as usize;
    let m: Vec<f32> = (0..n).map(|i| if i > fluid { 1.0 } else { 0.3 } * r.rr(0.85, 1.4) * bleed).collect();
    let shift = (r.unit() * n as f32) as usize % n;
    let v: Vec<[f32; 2]> = (0..n).map(|i| pts[(i + shift) % n]).collect();
    let centre = {
        let (mut area, mut cx, mut cy) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..n {
            let (a, b) = (v[i], v[(i + 1) % n]);
            let cr = a[0] * b[1] - b[0] * a[1];
            area += cr;
            cx += (a[0] + b[0]) * cr;
            cy += (a[1] + b[1]) * cr;
        }
        if n < 8 || area.abs() < 1e-3 { [v.iter().map(|q| q[0]).sum::<f32>() / n as f32, v.iter().map(|q| q[1]).sum::<f32>() / n as f32] } else { [cx / (3.0 * area), cy / (3.0 * area)] }
    };
    let size = [v.iter().map(|q| (q[0] - centre[0]).abs()).fold(0.0, f32::max), v.iter().map(|q| (q[1] - centre[1]).abs()).fold(0.0, f32::max)];
    let dir: Vec<bool> = (0..n)
        .map(|i| {
            let (a, b) = (v[i], v[(i + 1) % n]);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]).max(1e-3);
            !in_poly(&v, [(a[0] + b[0]) / 2.0 - (b[1] - a[1]) / l * 0.75, (a[1] + b[1]) / 2.0 + (b[0] - a[0]) / l * 0.75])
        })
        .collect();
    let mid = [centre[0] + r.rr(-0.6, 0.6) * size[0], centre[1] + r.rr(-0.6, 0.6) * size[1]];
    let first = FillPoly { v, m, dir, mid, size };
    let big = size[0].max(size[1]);
    let reach = big * bleed * 1.8 + big / 25.0 + 4.0;
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for q in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k] - reach);
            hi[k] = hi[k].max(q[k] + reach);
        }
    }
    let Some(mut mask) = Mask::new(lo, hi) else { return };
    let texture = tex * 3.0;
    let layer_alpha = 2.0 * opacity * (1.0 + tex / 2.0) / 100.0;
    let cap = (2024.0 * (2.0 * bleed).max(0.2)) as usize;
    let darker = r.rr(0.15, 0.7);
    let mut pol = first.grow(1.0, bleed, cap, &mut r);
    let sparse = first.scatter(0.1, pts, &mut r).grow(1.0, bleed, cap, &mut r).scatter(0.75, pts, &mut r).flipped();
    let mut pols: Vec<FillPoly> = vec![];
    for i in 0..20 {
        if i % 4 == 0 {
            pol = pol.grow(1.0, bleed, cap, &mut r);
        }
        if i % 2 == 0 {
            pols = [1.0, 0.7, 0.4].iter().map(|f| pol.grow(f - 0.0125 * i as f32, bleed, cap, &mut r)).collect();
        }
        let width = (big / 25.0 + (big / 30.0 - big / 25.0) * (i as f32 / 24.0).clamp(0.0, 1.0)) * border;
        let lay = |shape: FillPoly, alpha: f32, mask: &mut Mask| {
            mask.fill(&shape.v, alpha);
            mask.outline(&shape.v, width, border * 0.01);
        };
        for q in &pols {
            lay(q.grow(999.0, bleed, cap, &mut r).grow(997.0, bleed, cap, &mut r), layer_alpha, &mut mask);
        }
        lay(sparse.grow(999.0, bleed, cap, &mut r).flipped().grow(997.0, bleed, cap, &mut r), layer_alpha * texture, &mut mask);
        if i % 2 == 0 {
            lay(pol.grow(darker, bleed, cap, &mut r).grow(999.0, bleed, cap, &mut r), layer_alpha * 2.0, &mut mask);
        }
        if i % 8 == 0 || i == 19 {
            if texture > 0.0 {
                let count = (r.rr(80.0, 110.0) * (2.0 + 1.5 * texture * 3.0)) as usize;
                let least = size[0].min(size[1]) * 1.3;
                let amount = (5.0 - 0.3) * texture * 3.0 / 255.0;
                for k in 0..count {
                    let c = [mid[0] + r.gauss(0.0, size[0] / 1.3), mid[1] + r.gauss(0.0, size[1] / 1.3)];
                    let d = r.rr(0.03 * least, 0.45 * least);
                    if k % 5 != 0 {
                        mask.lift(c, d / 2.0, amount.min(1.0));
                    }
                }
            }
            mask.composite(p, col);
        }
    }
}

/// p5.brush's `wash`: the shape as one flat layer of pigment.
pub fn brush_wash(p: &mut Painting, pts: &[[f32; 2]], col: [f32; 3], opacity: f32) {
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for q in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k] - 2.0);
            hi[k] = hi[k].max(q[k] + 2.0);
        }
    }
    let Some(mut mask) = Mask::new(lo, hi) else { return };
    mask.fill(pts, opacity.clamp(0.0, 1.0));
    mask.composite(p, col);
}

/// p5.brush's default stroke, as the kit's 'ink' brush uses it: round tips stamped every twenty-fifth of the
/// brush's width along the line, each a little off it (more when the pressure is light), their size going with
/// the square of the pressure, which runs from 1.15 at the start of the stroke to 0.75 at its end. `width` is the
/// stroke's width at pressure 1.
pub fn brush_stroke(p: &mut Painting, pts: &[[f32; 2]], closed: bool, col: [f32; 3], width: f32, seed: u32) {
    let n = pts.len();
    if n < 2 || width <= 0.0 {
        return;
    }
    let reach = width * 1.1 + 3.0;
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for q in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(q[k] - reach);
            hi[k] = hi[k].max(q[k] + reach);
        }
    }
    let Some(mut mask) = Mask::new(lo, hi) else { return };
    let mut r = Roll::new(seed);
    let segs = if closed { n } else { n - 1 };
    let total: f32 = (0..segs).map(|i| (pts[(i + 1) % n][0] - pts[i][0]).hypot(pts[(i + 1) % n][1] - pts[i][1])).sum();
    if total < 0.5 {
        return;
    }
    let (scatter, sharp, noise_k) = (0.25 / 5.0 * width, 0.8, 0.015);
    let alpha = (235.0 / 255.0) * (1.0 + r.gauss(0.0, noise_k));
    // the whole stroke's pressure sits a little higher or lower, and its ramp a little steeper or shallower
    let (lift, tilt) = (r.rr(-0.04, 0.04), r.rr(0.9, 1.1));
    let step = (width * 0.04).max(0.35);
    let mut walked = 0.0f32;
    let mut carry = 0.0f32;
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = (b[0] - a[0]).hypot(b[1] - a[1]);
        if l < 1e-3 {
            continue;
        }
        let (dx, dy) = ((b[0] - a[0]) / l, (b[1] - a[1]) / l);
        let mut at = carry;
        while at < l {
            let t = (walked + at) / total;
            let pressure = (0.95 + (1.15 - 0.95) * tilt + ((0.75 - 1.15) * tilt) * t + lift).max(0.2);
            let vib = scatter * (sharp + (1.0 - sharp) * r.gauss(0.0, 1.0) / pressure);
            let (perp, along) = (vib * r.rr(-1.0, 1.0), 0.3 * vib * r.rr(-1.0, 1.0));
            let c = [a[0] + dx * (at + along) - dy * perp, a[1] + dy * (at + along) + dx * perp];
            let d = pressure * pressure * width * r.rr(0.85, 1.15);
            mask.stamp(c, d / 2.0, (pressure.max(0.9) * alpha * r.rr(0.75, 1.1)).min(1.0));
            at += step;
        }
        carry = at - l;
        walked += l;
    }
    mask.composite_stroke(p, col);
}

#[cfg(test)]
mod tests {
    //! Steadiness regressions, kept in step with the studio's oil crate (music-to-movie-magic,
    //! studio/crates/oil), which is upstream of this painter. Run with `cargo test -p painter --profile art`.
    use super::*;

    fn bare() -> Painting {
        ZOOM.with(|z| z.set(1.0));
        Painting::new()
    }
    fn band(z: f32, dy: f32) -> Vec<[f32; 2]> {
        // the sky band of the frame that flickered (sky.b#1 at t = 19.125 s): very wide, its top edge on screen,
        // seen through a camera zooming slowly about the frame's centre
        let raw = [[-960.0, 194.0 + dy], [2840.0, 194.0 + dy], [2840.0, 1990.0], [-960.0, 1990.0]];
        raw.iter().map(|q| [960.0 + (q[0] - 960.0) * z, 540.0 + (q[1] - 540.0) * z]).collect()
    }
    /// The largest change of any 24 x 24 block's mean colour between two paintings, within rows y0..y1. A stroke
    /// that pops in or out moves a block by several percent; brushwork that only travels with its shape does not.
    fn block_peak(a: &Painting, b: &Painting, y0: usize, y1: usize) -> f32 {
        let mut peak = 0.0f32;
        for by in (y0..y1 - 23).step_by(24) {
            for bx in (0..WI - 23).step_by(24) {
                let mut d = [0.0f32; 3];
                for y in by..by + 24 {
                    for x in bx..bx + 24 {
                        for k in 0..3 {
                            d[k] += a.col[y * WI + x][k] - b.col[y * WI + x][k];
                        }
                    }
                }
                peak = peak.max(d.iter().map(|v| (v / 576.0).abs()).fold(0.0, f32::max));
            }
        }
        peak
    }

    #[test]
    fn a_slow_zoom_moves_no_stroke_in_or_out_at_an_edge() {
        // Frames 458 -> 459 of the film: the camera zoomed 1.0218 -> 1.0220 and one whole sky stroke (about 600 px
        // long) appeared at the band's edge, because its slot centre crossed the rasterised mask's 0.5 line.
        let (z0, z1) = (1.0218f32, 1.0220f32);
        let col = [0.55, 0.35, 0.6];
        let mut worst = 0.0f32;
        // a few sub-pixel phases, so the test does not hinge on one lucky alignment
        for (k, dy) in [0.0f32, 0.37, 0.71, 1.13].iter().enumerate() {
            let paint = |z: f32| {
                let mut p = bare();
                ZOOM.with(|zz| zz.set(z));
                paint_body(&mut p, &band(z, *dy), col, col, 0.97, 0x8504_0000 ^ k as u32, 1.25, 0.7, Tool::Flat);
                p
            };
            let (a, b) = (paint(z0), paint(z1));
            worst = worst.max(block_peak(&a, &b, 120, 420));
        }
        assert!(worst < 0.02, "a 0.02 % zoom changed a 24 px block of the band's edge by {:.1} %", worst * 100.0);
    }

    #[test]
    fn stroke_slots_fade_in_and_out_at_an_edge_never_pop() {
        // The root cause, at the level of the stroke plan: slide the band's top edge down 120 px in 0.05 px steps
        // (the size of one frame's camera creep) and follow every stroke id. A stroke may only arrive, leave or
        // change its weight a little per step. With the old hard mask test, a slot's weight went 0 -> 1 at once.
        let width = 112.4f32;
        let base = -1.1781f32;
        // a stroke reaches under 4 brush widths from its slot: only those that can reach the canvas count
        let core = |q: [f32; 2]| q[0] > -4.0 * width && q[1] > -4.0 * width && q[0] < WI as f32 + 4.0 * width && q[1] < HI as f32 + 4.0 * width;
        let mut prev: std::collections::HashMap<u32, (f32, [f32; 2])> = Default::default();
        let mut worst = (0.0f32, 0u32, 0.0f32);
        for k in 0..2400 {
            let dy = k as f32 * 0.05;
            let pts = band(1.0, dy);
            let c = [940.0, (194.0 + dy + 1990.0) / 2.0];
            let lo = [-80.0f32, 194.0 + dy];
            let hi = [WI as f32 + 80.0, HI as f32 + 80.0];
            let now: std::collections::HashMap<u32, (f32, [f32; 2])> = plan_slots(&pts, c, lo, hi, width, base, 0x8504_0000).into_iter().map(|(q, id, e)| (id, (e, q))).collect();
            if k > 0 {
                for (id, (e, q)) in &now {
                    let d = match prev.get(id) {
                        Some((e0, _)) => (e - e0).abs(),
                        None if core(*q) => *e,
                        None => 0.0,
                    };
                    if d > worst.0 {
                        worst = (d, *id, dy);
                    }
                }
                for (id, (e, q)) in &prev {
                    if !now.contains_key(id) && core(*q) && *e > worst.0 {
                        worst = (*e, *id, dy);
                    }
                }
            }
            prev = now;
        }
        assert!(worst.0 < 0.02, "stroke {:08x} jumped by {:.2} of its weight at dy = {:.2}", worst.1, worst.0, worst.2);
    }

    #[test]
    fn a_strokes_edge_direction_never_flips_between_two_equally_near_edges() {
        // A narrow trunk whose two sides lean differently (frames 472 -> 473: one long bark stroke swung as the
        // nearest side changed from left to right). Sliding the shape sideways must turn the direction smoothly.
        let mut worst = 0.0f32;
        let mut last: Option<f32> = None;
        for k in 0..400 {
            let dx = k as f32 * 0.05;
            let pts = vec![[300.0 + dx, 100.0], [420.0 + dx, 100.0], [440.0 + dx, 1000.0], [290.0 + dx, 1000.0]];
            let (_, a) = edge_dir(&pts, [365.0 + 10.0, 550.0], 110.0);
            if let Some(b) = last {
                let mut d = (a - b).abs() % std::f32::consts::PI;
                d = d.min(std::f32::consts::PI - d);
                worst = worst.max(d);
            }
            last = Some(a);
        }
        assert!(worst < 0.01, "the edge direction jumped {:.3} rad for a 0.05 px move", worst);
    }

    /// How strongly the picture has straight seams running at right angles to level brushwork. A seam is many
    /// rows stepping in colour at the SAME column: for every run of `run` rows, each column's median horizontal step
    /// is compared with the mean of its neighbours 4-16 px either side; the score is the largest excess, against
    /// the region's mean step. Ragged, oblique stroke ends spread their steps over many columns and score low; a
    /// stroke that starts or stops on a straight edge square to its direction makes one column stand out.
    fn seam_score(p: &Painting, x0: usize, x1: usize, y0: usize, y1: usize, run: usize) -> f32 {
        let l = |x: usize, y: usize| {
            let c = p.col[y * WI + x];
            0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
        };
        let (w, h) = (x1 - x0, y1 - y0);
        let g: Vec<f32> = (0..h).flat_map(|y| (0..w).map(move |x| (y, x))).map(|(y, x)| (l(x0 + x + 1, y0 + y) - l(x0 + x, y0 + y)).abs()).collect();
        let mean = g.iter().sum::<f32>() / g.len() as f32;
        let mut worst = 0.0f32;
        for ys in (0..h - run).step_by(run / 3) {
            // the median over the run: a seam steps in most of its rows, a speck or a glint in one or two
            let col: Vec<f32> = (0..w)
                .map(|x| {
                    let mut v: Vec<f32> = (ys..ys + run).map(|y| g[y * w + x]).collect();
                    v.sort_by(|a, b| a.total_cmp(b));
                    v[run / 2]
                })
                .collect();
            for x in 16..w - 16 {
                let side = (4..=16).map(|d| col[x - d] + col[x + d]).sum::<f32>() / 26.0;
                worst = worst.max(col[x] - side);
            }
        }
        worst / mean.max(1e-6)
    }

    #[test]
    fn level_strokes_end_raggedly_not_on_a_vertical_seam() {
        // The owner's screenshot (the flight shot, t = 71.5 s): a night sky of level flat-brush strokes, cut all
        // over by hard vertical edges where strokes started and stopped square to their direction. Lay level
        // strokes on a flat ground and look, at each end, for one column where most rows step at once.
        let mut worst = 0.0f32;
        for k in 0..8u32 {
            let mut p = bare();
            for c in p.col.iter_mut().chain(p.flat.iter_mut()) {
                *c = [0.10, 0.12, 0.30];
            }
            let (len, width) = (520.0f32, 110.0f32);
            p.stroke(
                &Stroke {
                    c: [960.0, 540.0],
                    angle: 0.0,
                    len,
                    width,
                    bend: 0.0,
                    col: [0.25, 0.35, 0.75],
                    alt: [0.3, 0.4, 0.8],
                    opacity: 0.97,
                    pickup: 0.0,
                    load: 1.0,
                    part: Some((h32(0x5EA0 + k) >> 2, true, true)),
                    span: (0.0, 1.0),
                    tool: Tool::Flat,
                    picked: None,
                    along: 0.0,
                },
                None,
            );
            let (x0, x1) = (960 - (len / 2.0) as usize, 960 + (len / 2.0) as usize);
            let (y0, y1) = (540 - 36, 540 + 36);
            let start = seam_score(&p, x0 - 40, x0 + 140, y0, y1, 60);
            let end = seam_score(&p, x1 - 280, x1 + 20, y0, y1, 60);
            if std::env::var("SEAM_PNG").is_ok() {
                eprintln!("stroke {k}: start {start:.2} end {end:.2}");
            }
            worst = worst.max(start).max(end);
        }
        assert!(worst < 6.0, "a stroke end makes a straight vertical seam ({:.1}x the mean step)", worst);
    }

    #[test]
    fn an_outline_that_flexes_keeps_its_sweeps() {
        // Frames 587 -> 588 -> 589: the owl's head outline went from 9 sweeps to 10 and back as it breathed, and
        // since sweeps were seeded by their place in the list every later sweep was re-seeded. Flex an outline in
        // tiny steps: every sweep keeps its identity, and its share may only change a little per step.
        ZOOM.with(|z| z.set(1.0));
        let shape = |sy: f32| -> Vec<[f32; 2]> {
            (0..40)
                .map(|i| {
                    let a = i as f32 / 40.0 * std::f32::consts::TAU;
                    [960.0 + 160.0 * a.cos() * (1.0 + 0.08 * (3.0 * a).sin()), 540.0 + 130.0 * sy * a.sin()]
                })
                .collect()
        };
        let mut prev: std::collections::HashMap<u32, f32> = Default::default();
        let mut worst = (0.0f32, 0usize);
        for k in 0..600 {
            let sy = 1.0 + k as f32 * 0.0005;
            let now: std::collections::HashMap<u32, f32> = hand_paths(&shape(sy), true, 9.0, 0.0, 0xC0FFEE).into_iter().map(|(_, w, id)| (id, w)).collect();
            if k > 0 {
                for (id, w) in &now {
                    let d = (w - prev.get(id).copied().unwrap_or(0.0)).abs();
                    if d > worst.0 {
                        worst = (d, k);
                    }
                }
                for (id, w) in &prev {
                    if !now.contains_key(id) && *w > worst.0 {
                        worst = (*w, k);
                    }
                }
            }
            prev = now;
        }
        assert!(worst.0 < 0.1, "a sweep's share jumped by {:.2} at step {}", worst.0, worst.1);
    }

    #[test]
    fn same_shape_same_place_paints_identically() {
        let pts: Vec<[f32; 2]> = (0..24)
            .map(|i| {
                let a = i as f32 / 24.0 * std::f32::consts::TAU;
                [900.0 + 210.0 * a.cos(), 520.0 + 150.0 * a.sin()]
            })
            .collect();
        let paint = || {
            let mut p = bare();
            paint_form(&mut p, &pts, [0.8, 0.45, 0.2], [0.9, 0.7, 0.4], 0.97, 0xBEEF, 0.8, 1.15, Tool::Filbert, 1.0);
            p
        };
        let (a, b) = (paint(), paint());
        assert!(a.col == b.col && a.thick == b.thick, "painting the same frame twice must give the same picture");
    }

    #[test]
    fn a_shape_that_swells_a_little_keeps_its_brushwork() {
        // The brush used to snap to steps of 1.22x and the stroke direction to steps of 7.5 degrees: a shape
        // crossing a step boundary re-planned every stroke at once. Swelling by 0.5 % must change little.
        let shape = |s: f32, turn: f32| -> Vec<[f32; 2]> {
            (0..20)
                .map(|i| {
                    let a = i as f32 / 20.0 * std::f32::consts::TAU;
                    let (x, y) = (60.0 * s * a.cos(), 25.0 * s * a.sin());
                    let (sn, cs) = turn.sin_cos();
                    [960.0 + x * cs - y * sn, 540.0 + x * sn + y * cs]
                })
                .collect()
        };
        let mut worst = 0.0f32;
        for k in 0..12 {
            // sweep sizes and turns across where the old steps sat
            let (s, turn) = (0.9 + k as f32 * 0.07, -1.2 + k as f32 * 0.21);
            let paint = |s: f32, turn: f32| {
                let mut p = bare();
                paint_form(&mut p, &shape(s, turn), [0.8, 0.45, 0.2], [0.9, 0.7, 0.4], 0.97, 0xFACE, 0.8, 1.15, Tool::Filbert, 0.0);
                p
            };
            worst = worst.max(block_peak(&paint(s, turn), &paint(s * 1.005, turn + 0.002), 420, 660));
        }
        assert!(worst < 0.06, "0.5 % of swelling changed a block by {:.1} %", worst * 100.0);
    }

    #[test]
    fn a_shape_reaching_far_off_canvas_is_still_painted_where_it_shows() {
        // An iris wedge / whip streak: its centroid is thousands of pixels off screen, its visible part is not.
        let pts = vec![[300.0, 200.0], [700.0, 260.0], [4300.0, 2000.0], [-3000.0, 3800.0]];
        let mut p = bare();
        let col = [0.1, 0.08, 0.2];
        paint_body(&mut p, &pts, col, col, 0.97, 0x1215, 1.25, 0.7, Tool::Flat);
        let (mut inside, mut dark) = (0, 0);
        for y in (300..1080).step_by(7) {
            for x in (200..900).step_by(7) {
                if in_poly(&pts, [x as f32, y as f32]) && signed_depth(&pts, [x as f32, y as f32], 60.0) >= 60.0 {
                    inside += 1;
                    let c = p.col[y * WI + x];
                    dark += (c[0] + c[1] + c[2] < 0.9) as u32;
                }
            }
        }
        assert!(inside > 100 && dark as f32 > 0.9 * inside as f32, "only {dark} of {inside} samples inside the visible part were painted");
    }
}
