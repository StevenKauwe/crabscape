//! Exact source Healing painting, except prohibited full-canvas haze/vignette.
use dragon_game::drawing::{Canvas, Paint, circle, draw_star, ell, hex_a, source_rgb, tri};
use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};
const ISLES: [[f64; 3]; 3] = [[0.66, 0.15, 0.05], [0.78, 0.22, 0.06], [0.91, 0.16, 0.07]];
const LANTERNS: [[f64; 2]; 4] = [[0.09, 0.79], [0.15, 0.77], [0.21, 0.78], [0.31, 0.8]];
#[derive(Clone, Copy, Default, PartialEq)]
pub struct HealingCraft {
    pub tea: f64,
    pub tonic: f64,
    pub treats_made: bool,
    pub saddles: bool,
    pub armor: f64,
    pub bellows: f64,
}
fn mix(a: &str, b: &str, t: f64) -> String {
    let a = source_rgb(a);
    let b = source_rgb(b);
    let v: Vec<_> = a
        .iter()
        .zip(b)
        .map(|(a, b)| (*a as f64 + (b - a) as f64 * t + 0.5).floor() as i32)
        .collect();
    format!("rgb({},{},{})", v[0], v[1], v[2])
}
fn lin(c: &Canvas, [w, h]: [f64; 2], [x0, y0, x1, y1]: [f64; 4], stops: &[(f64, String)]) -> Paint {
    c.linear([x0 * w, y0 * h, x1 * w, y1 * h], stops)
}
fn poly(c: &mut Canvas, [w, h]: [f64; 2], points: &[[f64; 2]], fill: impl Into<Paint>) {
    c.fill_style(fill);
    c.begin();
    for (i, [x, y]) in points.iter().enumerate() {
        if i == 0 {
            c.move_to(x * w, y * h)
        } else {
            c.line_to(x * w, y * h)
        }
    }
    c.close();
    c.fill();
}
fn glow(c: &mut Canvas, x: f64, y: f64, r: f64, color: &str, a: f64) {
    let g = c.radial(
        [x, y, 0., x, y, r],
        &[(0., hex_a(color, a)), (1., hex_a(color, 0.))],
    );
    c.fill_style(g);
    circle(c, x, y, r);
}
fn stroke_rect(c: &mut Canvas, x: f64, y: f64, w: f64, h: f64) {
    c.stroke_rect(x, y, w, h);
}
mod painting;
use painting::static_paint;

#[derive(Clone, Copy)]
pub struct HealFlash {
    pub spot: [f64; 2],
    pub started: f64,
}
mod animation;
use animation::animated;

pub struct HealingInput<'a> {
    pub done: &'a BTreeSet<String>,
    pub craft: HealingCraft,
    pub wild: &'a [&'a serde_json::Value],
    pub flash: Option<HealFlash>,
}
#[derive(Default)]
pub struct HealingCache {
    key: Option<([f64; 3], Vec<String>, HealingCraft)>,
    background: Vec<bevy_egui::egui::Shape>,
    canvas: Canvas,
    rng: u32,
}
impl HealingCache {
    fn matches(&self, dimensions: [f64; 3], done: &BTreeSet<String>, craft: HealingCraft) -> bool {
        self.key
            .as_ref()
            .is_some_and(|(d, p, c)| *d == dimensions && p.iter().eq(done.iter()) && *c == craft)
    }
}
/// One bounded static tessellation cache; its key matches the source paintHealing inputs.
pub fn paint_healing_scene(
    painter: &bevy_egui::egui::Painter,
    rect: bevy_egui::egui::Rect,
    cache: &mut HealingCache,
    input: HealingInput<'_>,
    time: f64,
) {
    let size = [rect.width() as f64, rect.height() as f64];
    let dimensions = [size[0], size[1], painter.ctx().pixels_per_point() as f64];
    if !cache.matches(dimensions, input.done, input.craft) {
        cache.canvas.reset();
        let mut rng = dragon_game::random::Mulberry32 { state: cache.rng };
        static_paint(
            &mut cache.canvas,
            size,
            input.done,
            input.craft,
            &mut || rng.next_unit(),
        );
        cache.rng = rng.state;
        cache.background = super::shapes(&cache.canvas);
        cache.key = Some((
            dimensions,
            input.done.iter().cloned().collect(),
            input.craft,
        ));
    }
    let painter = painter.with_clip_rect(rect);
    painter.extend(cache.background.iter().cloned().map(|mut s| {
        s.translate(rect.min.to_vec2());
        s
    }));
    cache.canvas.reset();
    cache.canvas.translate(rect.min.x as f64, rect.min.y as f64);
    animated(
        &mut cache.canvas,
        size,
        input.done,
        input.craft,
        input.wild,
        input.flash,
        time,
    );
    super::paint(&painter, &cache.canvas);
}

#[cfg(test)]
mod tests;
