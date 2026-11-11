//! GPU-only adapter for the original Canvas geometry. No raster assets or gameplay state.
mod healing;
pub(super) use healing::{
    HealFlash, HealingCache, HealingCraft, HealingInput, paint_healing_scene,
};

mod previews;
mod scenes;

pub(super) use scenes::{AdventureScene, paint_adventure_scene};

pub(super) use previews::{builder_preview, companion_preview, roost_preview};

use bevy_egui::egui::{
    self, Color32, Pos2, Shape, Stroke,
    epaint::{Mesh, Vertex, WHITE_UV},
};
use dragon_game::drawing::{Affine, Canvas, Draw, Paint, PathCommand};
use std::f64::consts::TAU;

fn point(t: Affine, x: f64, y: f64) -> Pos2 {
    let p = t.point(x, y);
    Pos2::new(p[0] as f32, p[1] as f32)
}
fn paths(commands: &[PathCommand], t: Affine) -> Vec<(Vec<Pos2>, bool)> {
    let mut out = Vec::new();
    let mut p = Vec::new();
    let mut current = [0., 0.];
    for cmd in commands {
        match *cmd {
            PathCommand::Move(x, y) => {
                if !p.is_empty() {
                    out.push((std::mem::take(&mut p), false));
                }
                current = [x, y];
                p.push(point(t, x, y));
            }
            PathCommand::Line(x, y) => {
                current = [x, y];
                p.push(point(t, x, y));
            }
            PathCommand::Quad(cx, cy, x, y) => {
                let from = current;
                if p.is_empty() {
                    p.push(point(t, cx, cy));
                }
                let a = point(t, from[0], from[1]);
                let b = point(t, cx, cy);
                let d = point(t, x, y);
                let n = ((a.distance(b) + b.distance(d)) / 3.)
                    .ceil()
                    .clamp(4., 128.) as usize;
                for i in 1..=n {
                    let q = i as f64 / n as f64;
                    let z = 1. - q;
                    p.push(point(
                        t,
                        z * z * from[0] + 2. * z * q * cx + q * q * x,
                        z * z * from[1] + 2. * z * q * cy + q * q * y,
                    ));
                }
                current = [x, y];
            }
            PathCommand::Cubic(x1, y1, x2, y2, x, y) => {
                if p.is_empty() {
                    current = [x1, y1];
                    p.push(point(t, x1, y1));
                }
                let from = current;
                let a = point(t, from[0], from[1]);
                let b = point(t, x1, y1);
                let d = point(t, x2, y2);
                let e = point(t, x, y);
                let n = ((a.distance(b) + b.distance(d) + d.distance(e)) / 3.)
                    .ceil()
                    .clamp(8., 192.) as usize;
                for i in 1..=n {
                    let q = i as f64 / n as f64;
                    let z = 1. - q;
                    p.push(point(
                        t,
                        z * z * z * from[0]
                            + 3. * z * z * q * x1
                            + 3. * z * q * q * x2
                            + q * q * q * x,
                        z * z * z * from[1]
                            + 3. * z * z * q * y1
                            + 3. * z * q * q * y2
                            + q * q * q * y,
                    ));
                }
                current = [x, y];
            }
            PathCommand::RoundRect(x, y, w, h, radius) => {
                if !p.is_empty() {
                    out.push((std::mem::take(&mut p), false));
                }
                // Source scenes use positive dimensions and a single uniform radius.
                let r = radius.max(0.).min(w.abs() / 2.).min(h.abs() / 2.);
                let (left, right) = (x.min(x + w), x.max(x + w));
                let (top, bottom) = (y.min(y + h), y.max(y + h));
                let scale = t.0[0].hypot(t.0[1]).max(t.0[2].hypot(t.0[3]));
                let n = (std::f64::consts::FRAC_PI_2 * r * scale / 2.)
                    .ceil()
                    .clamp(4., 64.) as usize;
                for (cx, cy, start) in [
                    (right - r, top + r, -std::f64::consts::FRAC_PI_2),
                    (right - r, bottom - r, 0.),
                    (left + r, bottom - r, std::f64::consts::FRAC_PI_2),
                    (left + r, top + r, std::f64::consts::PI),
                ] {
                    for i in 0..=n {
                        let a = start + std::f64::consts::FRAC_PI_2 * i as f64 / n as f64;
                        p.push(point(t, cx + r * a.cos(), cy + r * a.sin()));
                    }
                }
                current = [left + r, top];
                out.push((std::mem::take(&mut p), true));
            }
            PathCommand::Ellipse(x, y, rx, ry, rot, start, end) => {
                let span = if end - start >= TAU {
                    TAU
                } else {
                    (end - start).rem_euclid(TAU)
                };
                let scale = (t.0[0].hypot(t.0[1])).max(t.0[2].hypot(t.0[3]));
                let n = (span * rx.max(ry) * scale / 2.).ceil().clamp(8., 256.) as usize;
                for i in 0..=n {
                    let a = start + span * i as f64 / n as f64;
                    let dx = rx * a.cos();
                    let dy = ry * a.sin();
                    current = [
                        x + dx * rot.cos() - dy * rot.sin(),
                        y + dx * rot.sin() + dy * rot.cos(),
                    ];
                    p.push(point(t, current[0], current[1]));
                }
            }
            PathCommand::Close => {
                if !p.is_empty() {
                    out.push((std::mem::take(&mut p), true));
                }
            }
        }
    }
    if !p.is_empty() {
        out.push((p, false));
    }
    out
}
fn rgba(s: &str) -> [f64; 4] {
    if let Some(h) = s.strip_prefix('#') {
        let h = if h.len() == 3 {
            h.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            h.into()
        };
        if let Ok(n) = u32::from_str_radix(&h, 16) {
            return [
                ((n >> 16) & 255) as f64,
                ((n >> 8) & 255) as f64,
                (n & 255) as f64,
                1.,
            ];
        }
    }
    if let Some((_, tail)) = s.split_once('(') {
        let v: Vec<f64> = tail
            .trim_end_matches(')')
            .split(',')
            .filter_map(|x| x.trim().parse().ok())
            .collect();
        if v.len() >= 3 {
            return [v[0], v[1], v[2], v.get(3).copied().unwrap_or(1.)];
        }
    }
    [0., 0., 0., 1.]
}
/// Parse each draw's colors once, not for each generated vertex. The source
/// paint stays borrowed so transforms and radial fallback keep their semantics.
struct PreparedPaint<'a> {
    draw: &'a Draw,
    solid: Option<Color32>,
    stops: Vec<(f64, [f64; 4])>,
    linear_bands: bool,
}
impl<'a> PreparedPaint<'a> {
    fn new(draw: &'a Draw) -> Self {
        let stops: Vec<_> = match &draw.paint {
            Paint::Solid(_) => Vec::new(),
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
                stops.iter().map(|(at, color)| (*at, rgba(color))).collect()
            }
        };
        let solid = match &draw.paint {
            Paint::Solid(color) => Some(tint(rgba(color), draw.alpha, draw.additive)),
            _ => None,
        };
        // With constant alpha, color is affine inside each linear stop band.
        // Duplicate/invalid stops and changing alpha retain the old sampler:
        // premultiplying changing RGB and alpha is not affine.
        let linear_bands = match &draw.paint {
            Paint::Linear {
                coords: [x0, y0, x1, y1],
                transform,
                ..
            } => {
                let axis = (x1 - x0).powi(2) + (y1 - y0).powi(2);
                let [a, b, c, d, _, _] = transform.0;
                axis.is_finite()
                    && axis > 0.
                    && transform.0.iter().all(|v| v.is_finite())
                    && (a * d - b * c).is_finite()
                    && (a * d - b * c).abs() >= 1e-15
                    && draw.alpha.is_finite()
                    && !stops.is_empty()
                    && stops.iter().all(|(at, color)| {
                        (0. ..=1.).contains(at)
                            && color.iter().all(|v| v.is_finite())
                            && color[..3].iter().all(|v| (0. ..=255.).contains(v))
                            && (0. ..=1.).contains(&color[3])
                            && color[3] == stops[0].1[3]
                    })
                    && stops.windows(2).all(|s| s[0].0 < s[1].0)
            }
            _ => false,
        };
        Self {
            draw,
            solid,
            stops,
            linear_bands,
        }
    }
    fn value(&self, p: Pos2) -> f64 {
        match &self.draw.paint {
            Paint::Solid(_) => 0.,
            Paint::Linear {
                coords: [x0, y0, x1, y1],
                transform,
                ..
            } => {
                let [x, y] = transform.inverse_point(p.x as f64, p.y as f64);
                let dx = x1 - x0;
                let dy = y1 - y0;
                ((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy)
            }
            Paint::Radial {
                coords: [x0, y0, r0, x1, y1, r1],
                transform,
                ..
            } => {
                let [x, y] = transform.inverse_point(p.x as f64, p.y as f64);
                let dx = x1 - x0;
                let dy = y1 - y0;
                let dr = r1 - r0;
                let px = x - x0;
                let py = y - y0;
                let a = dx * dx + dy * dy - dr * dr;
                let b = -2. * (px * dx + py * dy + r0 * dr);
                let cc = px * px + py * py - r0 * r0;
                let disc = (b * b - 4. * a * cc).max(0.).sqrt();
                if a.abs() < 1e-12 {
                    -cc / b
                } else {
                    let u = (-b - disc) / (2. * a);
                    let v = (-b + disc) / (2. * a);
                    if r0 + u * dr >= 0. { u } else { v }
                }
            }
        }
    }
    fn at(&self, value: f64) -> Color32 {
        if let Some(solid) = self.solid {
            return solid;
        }
        let v = if value.is_finite() {
            value.clamp(0., 1.)
        } else {
            0.
        };
        let mut a = self.stops.first();
        let mut b = self.stops.last();
        for stop in &self.stops {
            if stop.0 <= v {
                a = Some(stop);
            }
            if stop.0 >= v {
                b = Some(stop);
                break;
            }
        }
        if let (Some((ta, a)), Some((tb, b))) = (a, b) {
            let f = if tb > ta { (v - ta) / (tb - ta) } else { 0. };
            tint(
                std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f),
                self.draw.alpha,
                self.draw.additive,
            )
        } else {
            Color32::TRANSPARENT
        }
    }
    fn color(&self, p: Pos2) -> Color32 {
        self.at(self.value(p))
    }
}

#[cfg(test)]
fn color(d: &Draw, p: Pos2) -> Color32 {
    let (v, stops) = match &d.paint {
        Paint::Solid(s) => return tint(rgba(s), d.alpha, d.additive),
        Paint::Linear {
            coords: [x0, y0, x1, y1],
            transform,
            stops,
        } => {
            let [x, y] = transform.inverse_point(p.x as f64, p.y as f64);
            let dx = x1 - x0;
            let dy = y1 - y0;
            (((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy), stops)
        }
        Paint::Radial {
            coords: [x0, y0, r0, x1, y1, r1],
            transform,
            stops,
        } => {
            let [x, y] = transform.inverse_point(p.x as f64, p.y as f64);
            let dx = x1 - x0;
            let dy = y1 - y0;
            let dr = r1 - r0;
            let px = x - x0;
            let py = y - y0;
            let a = dx * dx + dy * dy - dr * dr;
            let b = -2. * (px * dx + py * dy + r0 * dr);
            let cc = px * px + py * py - r0 * r0;
            let disc = (b * b - 4. * a * cc).max(0.).sqrt();
            let v = if a.abs() < 1e-12 {
                -cc / b
            } else {
                let u = (-b - disc) / (2. * a);
                let v = (-b + disc) / (2. * a);
                if r0 + u * dr >= 0. { u } else { v }
            };
            (v, stops)
        }
    };
    let v = if v.is_finite() { v.clamp(0., 1.) } else { 0. };
    let mut a = stops.first();
    let mut b = stops.last();
    for stop in stops {
        if stop.0 <= v {
            a = Some(stop);
        }
        if stop.0 >= v {
            b = Some(stop);
            break;
        }
    }
    if let (Some((ta, ca)), Some((tb, cb))) = (a, b) {
        let a = rgba(ca);
        let b = rgba(cb);
        let f = if tb > ta { (v - ta) / (tb - ta) } else { 0. };
        tint(
            std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f),
            d.alpha,
            d.additive,
        )
    } else {
        Color32::TRANSPARENT
    }
}
fn tint(v: [f64; 4], alpha: f64, additive: bool) -> Color32 {
    let a = (v[3] * alpha).clamp(0., 1.);
    let c = Color32::from_rgba_unmultiplied(
        v[0].clamp(0., 255.) as u8,
        v[1].clamp(0., 255.) as u8,
        v[2].clamp(0., 255.) as u8,
        (a * 255.) as u8,
    );
    if additive {
        Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), 0)
    } else {
        c
    }
}
fn cross(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn emit_triangle(mesh: &mut Mesh, vertices: [(Pos2, Color32); 3]) {
    let i = mesh.vertices.len() as u32;
    mesh.vertices.extend(vertices.map(|(pos, color)| Vertex {
        pos,
        uv: WHITE_UV,
        color,
    }));
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2]);
}

#[derive(Clone, Copy)]
struct BandPoint {
    pos: Pos2,
    value: f64,
}
/// Split an already-triangulated fill at each gradient stop plane. Every
/// resulting polygon is convex. Constant end bands preserve Canvas clamping.
fn linear_triangle(mesh: &mut Mesh, paint: &PreparedPaint<'_>, points: [Pos2; 3]) -> bool {
    let mut remaining: Vec<_> = points
        .into_iter()
        .map(|pos| BandPoint {
            pos,
            value: paint.value(pos),
        })
        .collect();
    if remaining.iter().any(|p| !p.value.is_finite()) {
        return false;
    }
    for (boundary, _) in &paint.stops {
        if remaining.is_empty() {
            break;
        }
        let mut below = Vec::with_capacity(remaining.len() + 1);
        let mut above = Vec::with_capacity(remaining.len() + 1);
        let mut prev = *remaining.last().expect("nonempty");
        for &next in &remaining {
            if (prev.value <= *boundary) != (next.value <= *boundary) {
                let fraction = ((*boundary - prev.value) / (next.value - prev.value)) as f32;
                let at = BandPoint {
                    pos: prev.pos + (next.pos - prev.pos) * fraction,
                    value: *boundary,
                };
                below.push(at);
                above.push(at);
            }
            if next.value <= *boundary {
                below.push(next);
            } else {
                above.push(next);
            }
            prev = next;
        }
        band_polygon(mesh, paint, &below);
        remaining = above;
    }
    band_polygon(mesh, paint, &remaining);
    true
}
fn band_polygon(mesh: &mut Mesh, paint: &PreparedPaint<'_>, points: &[BandPoint]) {
    for pair in points.get(1..).unwrap_or_default().windows(2) {
        let points = [points[0], pair[0], pair[1]];
        if cross(points[0].pos, points[1].pos, points[2].pos).abs() > f32::EPSILON {
            emit_triangle(mesh, points.map(|p| (p.pos, paint.at(p.value))));
        }
    }
}
fn triangle(mesh: &mut Mesh, paint: &PreparedPaint<'_>, a: Pos2, b: Pos2, c: Pos2, depth: u8) {
    if paint.linear_bands && linear_triangle(mesh, paint, [a, b, c]) {
        return;
    }
    // Keep the existing radial/changing-alpha approximation. This fallback has
    // the same geometric bound, but no longer parses colors per vertex.
    if paint.solid.is_none()
        && depth < 7
        && (a.distance(b).max(b.distance(c)).max(c.distance(a)) > 8.)
    {
        let ab = a + (b - a) * 0.5;
        let bc = b + (c - b) * 0.5;
        let ca = c + (a - c) * 0.5;
        for [a, b, c] in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            triangle(mesh, paint, a, b, c, depth + 1);
        }
        return;
    }
    emit_triangle(mesh, [a, b, c].map(|pos| (pos, paint.color(pos))));
}
/// Circular glows with one RGB hue can be drawn as concentric stop rings.
/// Keep the original polygon's boundary samples, rather than changing the art's
/// outline. Every stop gets a ring, including narrow interior peaks. If the
/// polygon's angular resolution cannot bound color error, keep the old fallback.
fn radial_circle(mesh: &mut Mesh, paint: &PreparedPaint<'_>, boundary: &[Pos2]) -> bool {
    let draw = paint.draw;
    let Paint::Radial {
        coords: [x0, y0, r0, x1, y1, r1],
        transform,
        ..
    } = &draw.paint
    else {
        return false;
    };
    let [PathCommand::Ellipse(x, y, rx, ry, rotation, start, end)] = draw.path.as_slice() else {
        return false;
    };
    let dr = r1 - r0;
    let [a, b, c, d, _, _] = transform.0;
    let determinant = a * d - b * c;
    if !transform.0.iter().all(|v| v.is_finite())
        || !determinant.is_finite()
        || determinant.abs() < 1e-15
        || draw.stroke.is_some()
        || !draw.clips.is_empty()
        || draw.transform != *transform
        || x != x0
        || x0 != x1
        || y != y0
        || y0 != y1
        || rx != ry
        || ![x, y, rx, ry, rotation, start, end, r0, r1]
            .iter()
            .all(|v| v.is_finite())
        || *rx <= 0.
        || *r0 < 0.
        || dr <= 0.
        || !dr.is_finite()
        || end - start < TAU
        || !(0. ..=1.).contains(&draw.alpha)
        || paint.stops.is_empty()
        || !paint.stops.iter().all(|(at, color)| {
            (0. ..=1.).contains(at)
                && (0. ..=1.).contains(&color[3])
                && color[..3] == paint.stops[0].1[..3]
                && color[..3].iter().all(|v| (0. ..=255.).contains(v))
        })
        || !paint.stops.windows(2).all(|s| s[0].0 < s[1].0)
    {
        return false;
    }
    let boundary = if boundary.len() > 1
        && boundary[0].distance_sq(*boundary.last().expect("nonempty")) < 0.0001
    {
        &boundary[..boundary.len() - 1]
    } else {
        boundary
    };
    if boundary.len() < 3 || boundary.iter().any(|p| !p.is_finite()) {
        return false;
    }
    // For adjacent angular samples, interpolated radius differs from the true
    // radius by at most R * (1 - cos(pi/n)). Multiplying by the largest alpha
    // slope bounds every channel's unquantized color error, including crossings
    // of stop radii between samples. This rejects unresolved thin rings instead
    // of relying only on vertex/midpoint samples that can miss them completely.
    let slope = paint
        .stops
        .windows(2)
        .map(|s| 255. * draw.alpha * (s[1].1[3] - s[0].1[3]).abs() / ((s[1].0 - s[0].0) * dr))
        .fold(0., f64::max);
    let error = slope * rx * (1. - (std::f64::consts::PI / boundary.len() as f64).cos());
    if !error.is_finite() || error > 0.25 {
        return false;
    }
    let center = point(draw.transform, *x, *y);
    if !center.is_finite() {
        return false;
    }
    let center_index = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos: center,
        uv: WHITE_UV,
        color: paint.at(-r0 / dr),
    });
    let mut inner = None;
    for radius in paint
        .stops
        .iter()
        .map(|(at, _)| r0 + at * dr)
        .filter(|r| *r > 0. && *r < *rx)
        .chain(std::iter::once(*rx))
    {
        let outer = mesh.vertices.len() as u32;
        let color = paint.at((radius - r0) / dr);
        mesh.vertices.extend(boundary.iter().map(|&p| Vertex {
            pos: if radius == *rx {
                p
            } else {
                center + (p - center) * (radius / rx) as f32
            },
            uv: WHITE_UV,
            color,
        }));
        let count = boundary.len() as u32;
        for i in 0..count {
            let next = (i + 1) % count;
            if let Some(inner) = inner {
                mesh.indices.extend_from_slice(&[
                    inner + i,
                    outer + i,
                    outer + next,
                    inner + i,
                    outer + next,
                    inner + next,
                ]);
            } else {
                mesh.indices
                    .extend_from_slice(&[center_index, outer + i, outer + next]);
            }
        }
        inner = Some(outer);
    }
    true
}

fn polygon(mesh: &mut Mesh, paint: &PreparedPaint<'_>, mut p: Vec<Pos2>) {
    p.dedup_by(|a, b| a.distance_sq(*b) < 0.0001);
    if p.len() > 1 && p[0].distance_sq(*p.last().expect("nonempty")) < 0.0001 {
        p.pop();
    }
    if p.len() < 3 {
        return;
    }
    let area: f32 = p
        .iter()
        .zip(p.iter().cycle().skip(1))
        .take(p.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    if area < 0. {
        p.reverse();
    }
    while p.len() > 3 {
        let mut ear = None;
        for i in 0..p.len() {
            let a = p[(i + p.len() - 1) % p.len()];
            let b = p[i];
            let c = p[(i + 1) % p.len()];
            if cross(a, b, c) <= 0.00001 {
                continue;
            }
            if p.iter().enumerate().any(|(j, &q)| {
                j != i
                    && j != (i + p.len() - 1) % p.len()
                    && j != (i + 1) % p.len()
                    && cross(a, b, q) > 0.
                    && cross(b, c, q) > 0.
                    && cross(c, a, q) > 0.
            }) {
                continue;
            }
            ear = Some(i);
            triangle(mesh, paint, a, b, c, 0);
            break;
        }
        if let Some(i) = ear {
            p.remove(i);
        } else {
            return;
        }
    }
    triangle(mesh, paint, p[0], p[1], p[2], 0);
}
fn clip(mut p: Vec<Pos2>, boundary: &[Pos2]) -> Vec<Pos2> {
    let area: f32 = boundary
        .iter()
        .zip(boundary.iter().cycle().skip(1))
        .take(boundary.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    let sign = if area >= 0. { 1. } else { -1. };
    for (&a, &b) in boundary
        .iter()
        .zip(boundary.iter().cycle().skip(1))
        .take(boundary.len())
    {
        let old = std::mem::take(&mut p);
        if old.is_empty() {
            break;
        }
        let mut prev = *old.last().expect("nonempty");
        let mut pd = cross(a, b, prev) * sign;
        for q in old {
            let qd = cross(a, b, q) * sign;
            if (pd >= 0.) != (qd >= 0.) {
                p.push(prev + (q - prev) * (pd / (pd - qd)));
            }
            if qd >= 0. {
                p.push(q);
            }
            prev = q;
            pd = qd;
        }
    }
    p
}
/// Paints recorded paths in source order. Coordinates are egui logical screen pixels.
pub fn paint(painter: &egui::Painter, canvas: &Canvas) {
    painter.extend(shapes(canvas));
}
fn shapes(canvas: &Canvas) -> Vec<Shape> {
    let mut out = Vec::new();
    for d in &canvas.commands {
        let paint = PreparedPaint::new(d);
        let mut mesh = Mesh::default();
        for (mut p, closed) in paths(&d.path, d.transform) {
            if let Some(stroke) = &d.stroke {
                if p.len() < 2 {
                    continue;
                }
                if closed {
                    p.push(p[0]);
                }
                let scale = d.transform.0[0].hypot(d.transform.0[1]) as f32;
                let width = stroke.width as f32 * scale;
                // Egui's path stroker supplies anti-aliased round joins/caps. Original art strokes are solid.
                if let Paint::Solid(_) = d.paint {
                    let col = paint.color(p[0]);
                    if !stroke.dash.is_empty() {
                        for dash in dashed_path_offset(&p, &stroke.dash, scale, stroke.dash_offset)
                        {
                            out.push(Shape::line(dash, Stroke::new(width, col)));
                        }
                        continue;
                    }
                    out.push(Shape::line(p.clone(), Stroke::new(width, col)));
                    if stroke.cap == "round" {
                        out.push(Shape::circle_filled(p[0], width * 0.5, col));
                        out.push(Shape::circle_filled(
                            *p.last().expect("nonempty"),
                            width * 0.5,
                            col,
                        ));
                    }
                    continue;
                }
                for pair in p.windows(2) {
                    let a = pair[0];
                    let b = pair[1];
                    let n = (b - a).normalized().rot90() * width * 0.5;
                    polygon(&mut mesh, &paint, vec![a + n, b + n, b - n, a - n]);
                }
            } else {
                for clipping in &d.clips {
                    for (boundary, _) in paths(&clipping.path, clipping.transform) {
                        p = clip(p, &boundary);
                    }
                }
                if !radial_circle(&mut mesh, &paint, &p) {
                    polygon(&mut mesh, &paint, p);
                }
            }
        }
        if !mesh.indices.is_empty() {
            out.push(Shape::mesh(mesh));
        }
    }
    out
}

#[cfg(test)]
fn dashed_path(points: &[Pos2], pattern: &[f64], scale: f32) -> Vec<Vec<Pos2>> {
    dashed_path_offset(points, pattern, scale, 0.)
}
fn dashed_path_offset(points: &[Pos2], pattern: &[f64], scale: f32, offset: f64) -> Vec<Vec<Pos2>> {
    let lengths: Vec<f32> = pattern
        .iter()
        .map(|n| (*n as f32 * scale).max(0.001))
        .collect();
    let mut out = Vec::new();
    let mut index = 0;
    let total: f32 = lengths.iter().sum();
    let mut phase = (offset as f32 * scale).rem_euclid(total);
    while phase >= lengths[index % lengths.len()] {
        phase -= lengths[index % lengths.len()];
        index += 1;
    }
    let mut left = lengths[index % lengths.len()] - phase;
    let mut line = Vec::new();
    for pair in points.windows(2) {
        let mut a = pair[0];
        let b = pair[1];
        while a.distance(b) > 0.0001 {
            let distance = a.distance(b);
            let step = distance.min(left);
            let q = a + (b - a) * (step / distance);
            if index % 2 == 0 {
                if line.is_empty() {
                    line.push(a);
                }
                line.push(q);
            }
            a = q;
            left -= step;
            if left <= 0.0001 {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                index += 1;
                left = lengths[index % lengths.len()];
            }
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn string<'a>(v: &'a serde_json::Value, key: &str, default: &'a str) -> &'a str {
    v[key].as_str().unwrap_or(default)
}
fn stage(xp: f64) -> usize {
    match dragon_game::level_of(xp) {
        12.. => 3,
        8.. => 2,
        4.. => 1,
        _ => 0,
    }
}
fn horn(d: &serde_json::Value) -> String {
    string(
        d,
        "horn",
        match string(d, "kind", "bond") {
            "flame" => "#ffd0a0",
            "spark" => "#fff27a",
            "boulder" => "#d8c6a8",
            "frost" => "#e6fbff",
            "star" => "#fff8d8",
            _ => "#ffe7a6",
        },
    )
    .into()
}
fn breath(kind: &str) -> Option<[String; 3]> {
    Some(
        match kind {
            "flame" => ["#fff6c2", "#ffb347", "#ff5a1f"],
            "spark" => ["#ffffff", "#fff27a", "#ffc800"],
            "frost" => ["#ffffff", "#bff0ff", "#5fc8ff"],
            "star" => ["#ffffff", "#fff1a8", "#ffc83d"],
            _ => return None,
        }
        .map(str::to_owned),
    )
}

/// At most one viewport/realm backdrop is retained. Dynamic commands reuse their outer
/// vector; inner paths/paints and dynamic GPU meshes still allocate and need profiling.
#[derive(Default)]
pub struct ArtCache {
    canvas: Canvas,
    weather: dragon_game::drawing::Weather,
    visual_rng: u32,
    background_key: Option<([f64; 6], usize, bool)>,
    sky: Vec<Shape>,
    far: [Vec<Shape>; 2],
    land: [Vec<Shape>; 2],
}
fn mirrored(source: &[Shape], width: f32) -> Vec<Shape> {
    source
        .iter()
        .cloned()
        .map(|mut shape| {
            match &mut shape {
                Shape::Mesh(mesh) => {
                    for v in &mut std::sync::Arc::make_mut(mesh).vertices {
                        v.pos.x = width - v.pos.x;
                    }
                }
                Shape::Path(path) => {
                    for p in &mut path.points {
                        p.x = width - p.x;
                    }
                }
                Shape::Circle(circle) => circle.center.x = width - circle.center.x,
                _ => {}
            }
            shape
        })
        .collect()
}
impl ArtCache {
    pub fn clear_weather(&mut self) {
        self.weather = Default::default();
    }
    /// Update once per visual frame, never from an egui paint pass.
    pub fn update_weather(&mut self, dt: f64, frame: dragon_game::drawing::WeatherFrame) {
        let mut rng = dragon_game::random::Mulberry32 {
            state: self.visual_rng,
        };
        self.weather.update(dt, frame, &mut || rng.next_unit());
        self.visual_rng = rng.state;
        if !frame.in_sky {
            self.weather.trails.clear();
        }
    }
    /// Emit once per active visual frame after update_weather, for each Daddio.
    pub fn emit_dad_trail(
        &mut self,
        position: [f64; 2],
        size: f64,
        facing: f64,
        scale: f64,
        enabled: bool,
    ) {
        let mut rng = dragon_game::random::Mulberry32 {
            state: self.visual_rng,
        };
        self.weather
            .dad_trail(position, size, facing, scale, enabled, &mut || {
                rng.next_unit()
            });
        self.visual_rng = rng.state;
    }
    #[allow(clippy::too_many_arguments)]
    fn background(
        &mut self,
        painter: &egui::Painter,
        view: dragon_game::flight::Viewport,
        realm: usize,
        song: bool,
        scroll: f64,
        zoom: f64,
        fancy: bool,
        time: f64,
    ) {
        let key = (
            [
                view.width,
                view.height,
                view.scale,
                view.ground_y,
                view.top_limit,
                zoom,
            ],
            realm,
            song,
        );
        if self.background_key != Some(key) {
            let b = dragon_game::drawing::draw_background(view, realm, song);
            self.sky = shapes(&b.sky);
            self.far[0] = shapes(&b.far);
            self.far[1] = mirrored(&self.far[0], view.width as f32);
            self.land[0] = shapes(&b.land);
            self.land[1] = mirrored(&self.land[0], view.width as f32);
            if zoom != 1. {
                let transform = egui::emath::TSTransform::from_scaling(zoom as f32);
                for shape in self
                    .sky
                    .iter_mut()
                    .chain(self.far.iter_mut().flatten())
                    .chain(self.land.iter_mut().flatten())
                {
                    shape.transform(transform);
                }
            }
            self.background_key = Some(key);
        }
        painter.extend(self.sky.iter().cloned());
        for (layer, speed) in [(&self.far, 0.22), (&self.land, 0.55)] {
            if speed == 0.55 && fancy {
                self.canvas.reset();
                self.canvas.scale(zoom, zoom);
                dragon_game::drawing::draw_moon_rays(&mut self.canvas, view, time);
                paint(painter, &self.canvas);
            }
            let off = (scroll * speed).rem_euclid(view.width * 2.);
            let mut x = -off;
            let mut k = 0;
            while x < view.width {
                let clip = painter.clip_rect().intersect(egui::Rect::from_min_size(
                    egui::pos2((x * zoom) as f32, 0.),
                    egui::vec2((view.width * zoom) as f32, (view.height * zoom) as f32),
                ));
                let p = painter.with_clip_rect(clip);
                p.extend(layer[k % 2].iter().cloned().map(|mut shape| {
                    shape.translate(egui::vec2((x * zoom) as f32, 0.));
                    shape
                }));
                x += view.width;
                k += 1;
            }
        }
    }
}
/// Original team/enemy layers with separate source global and flap clocks.
/// The source geometry itself is retained; this adapter does not claim pixel-identical rasterization.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub fn paint_game(
    painter: &egui::Painter,
    cache: &mut ArtCache,
    game: &dragon_game::game::Game,
    view: dragon_game::flight::Viewport,
    animation_time: f64,
    flap_time: f64,
    squash: f64,
    zoom: f64,
    fancy: bool,
) {
    paint_game_with_rider(
        painter,
        cache,
        game,
        view,
        animation_time,
        flap_time,
        squash,
        zoom,
        fancy,
        true,
    );
}
#[allow(clippy::too_many_arguments)]
pub fn paint_game_with_rider(
    painter: &egui::Painter,
    cache: &mut ArtCache,
    game: &dragon_game::game::Game,
    view: dragon_game::flight::Viewport,
    animation_time: f64,
    flap_time: f64,
    squash: f64,
    zoom: f64,
    fancy: bool,
    source_rider: bool,
) {
    use dragon_game::drawing::{
        DragonArt, EnemyArt, RiderArt, SceneArt, draw_cracks, draw_dragon, draw_enemy_fx,
        draw_hazard, draw_mark, draw_objective, draw_star, draw_strike, draw_warning,
    };
    let song = game
        .profile
        .as_ref()
        .and_then(|p| p["hatched"].as_u64())
        .unwrap_or(0)
        >= 3;
    cache.background(
        painter,
        view,
        game.definition.realm,
        song,
        game.scroll,
        zoom,
        fancy,
        animation_time,
    );
    let mut c = std::mem::take(&mut cache.canvas);
    c.reset();
    c.scale(zoom, zoom);
    let time = animation_time;
    let p = &game.player;
    let scene = SceneArt {
        player: [p.x, p.y],
        ground: view.ground_y,
        scale: view.scale,
        time,
    };
    dragon_game::drawing::draw_scenery(
        &mut c,
        view,
        game.definition.realm,
        game.scroll,
        game.scroll_v,
    );
    // Local visual randomness never advances the gameplay stream.
    let mut bolt_rng = dragon_game::random::Mulberry32 {
        state: (time * 1000.) as u32,
    };
    for strike in &game.strikes {
        draw_strike(&mut c, strike, &scene, view.top_limit, || {
            bolt_rng.next_unit()
        });
    }
    for e in &game.enemies {
        let target = &e.target;
        let kind = match e.kind {
            dragon_game::enemies::Kind::Cloud => "cloud",
            dragon_game::enemies::Kind::Bigcloud => "bigcloud",
            dragon_game::enemies::Kind::Bat => "bat",
            dragon_game::enemies::Kind::Sprite => "sprite",
            dragon_game::enemies::Kind::Urgal => "urgal",
            dragon_game::enemies::Kind::Wolf => "wolf",
            dragon_game::enemies::Kind::Troll => "troll",
            dragon_game::enemies::Kind::Thunderhead => "thunderhead",
            dragon_game::enemies::Kind::Chieftain => "chieftain",
            dragon_game::enemies::Kind::Cavetroll => "cavetroll",
            dragon_game::enemies::Kind::Jelly => "jelly",
            dragon_game::enemies::Kind::Leaper => "leaper",
            dragon_game::enemies::Kind::Serpent => "serpent",
            dragon_game::enemies::Kind::Slime => "slime",
            dragon_game::enemies::Kind::Slimelet => "slimelet",
            dragon_game::enemies::Kind::Imp => "imp",
            dragon_game::enemies::Kind::Bigslime => "bigslime",
            dragon_game::enemies::Kind::Gobbler => "gobbler",
            dragon_game::enemies::Kind::Moth => "moth",
            dragon_game::enemies::Kind::Swallower => "swallower",
        };
        let art = EnemyArt {
            kind: kind.into(),
            x: target.x,
            y: target.y,
            r: target.r,
            dir: e.dir,
            t: e.t,
            wob: e.wob,
            flash: target.flash,
            calm: target.calm,
            slow: target.slow,
            windup: e.windup,
            stun: target.stun,
            air: e.air,
            vy: e.velocity[1],
            hop: e.kind.definition().hop,
        };
        let hot = e.hurt
            || e.sting > 0.
            || (e.air
                && (matches!(
                    e.kind,
                    dragon_game::enemies::Kind::Wolf | dragon_game::enemies::Kind::Leaper
                ) || e.kind.definition().hop))
            || (e.kind == dragon_game::enemies::Kind::Swallower && e.windup > 0.);
        if !target.dead && !target.calm && (e.pre > 0. || e.windup > 0. || hot) {
            draw_warning(&mut c, &art, hot, &scene);
        }
        if target.marked > 0. && !target.dead && !target.calm {
            draw_mark(&mut c, &art);
        }
        draw_enemy_fx(
            &mut c, &art, &scene, e.squash, e.pre, e.lunge, e.hurt, e.sting,
        );
        if target.cracked > 0. && !target.dead && !target.calm {
            draw_cracks(&mut c, &art);
        }
    }
    if let Some(ob) = &game.objective {
        draw_objective(&mut c, ob, &scene);
    }
    let empty = serde_json::Value::Null;
    let profile = game.profile.as_ref().unwrap_or(&empty);
    let build = &profile["build"];
    let party = profile["party"].as_array();
    let bond = party
        .and_then(|a| a.iter().find(|d| d["bonded"] == true))
        .unwrap_or(&empty);
    let main = party
        .and_then(|a| a.iter().find(|d| d["id"] == profile["main"]))
        .unwrap_or(bond);
    let dad = string(build, "name", "")
        .trim()
        .eq_ignore_ascii_case("daddio");
    if fancy {
        dragon_game::drawing::draw_shot_lights(&mut c, view, &game.shots);
    }
    for gem in &game.gems {
        // Empty colors occur only in synthetic/older in-memory inputs.
        dragon_game::drawing::draw_gem(
            &mut c,
            gem,
            if !gem.color.is_empty() {
                &gem.color
            } else if gem.golden {
                "#ffd76e"
            } else {
                "#62f0d4"
            },
            &scene,
        );
    }
    for ally in &game.allies {
        c.alpha(0.9);
        draw_dragon(
            &mut c,
            ally.x,
            ally.y,
            view.scale * 0.75,
            &ally.color,
            if ally.vx == 0. { 1. } else { ally.vx.signum() },
            ally.t * 1.2,
            &DragonArt::default(),
        );
        c.alpha(1.);
    }
    if let Some(party) = party {
        for d in party {
            if d["bonded"] == true {
                continue;
            }
            let Some(w) = d["id"].as_str().and_then(|id| game.wings.get(id)) else {
                continue;
            };
            let st = stage(d["xp"].as_f64().unwrap_or(0.));
            draw_dragon(
                &mut c,
                w.x,
                w.y,
                view.scale * 0.7 * [0.62, 0.8, 1., 1.18][st],
                string(d, "color", "#7f88aa"),
                p.facing as f64,
                w.t + 1.3,
                &DragonArt {
                    horn: Some(horn(d)),
                    aura: st == 3 || d["kind"] == "star",
                    fire: d == main && p.fire_flash > 0.,
                    fire_cols: breath(string(d, "kind", "bond")),
                    ..Default::default()
                },
            );
        }
    }
    let st = stage(bond["xp"].as_f64().unwrap_or(game.xp));
    let s = view.scale * game.dragon_scale();
    let bob = (time * 2.2 + flap_time).sin() * view.scale * 0.05;
    c.alpha(if p.down > 0. {
        0.4
    } else if p.shadow > 0. {
        0.3
    } else if p.sila > 0. {
        0.6
    } else if p.safe > 0. && p.dizzy <= 0. && (time * 12.).floor() % 2. == 0. {
        0.45
    } else {
        1.
    });
    let bank = if p.out || p.down > 0. || p.dizzy > 0. {
        0.
    } else {
        (p.vy / (view.scale * 7.5)).clamp(-1., 1.) * 0.26 * p.facing as f64
    };
    c.save();
    c.translate(p.x, p.y + bob);
    c.rotate(bank);
    let psq = if squash > 0. {
        (squash / 0.3)
            .min(1.)
            .mul_add(std::f64::consts::PI, 0.)
            .sin()
            * 0.18
    } else {
        0.
    };
    c.scale(1. + psq, 1. - psq);
    c.translate(-p.x, -(p.y + bob));
    let rider = (source_rider
        && dragon_game::level_of(bond["xp"].as_f64().unwrap_or(game.xp)) >= 8
        && profile["craft"]["saddles"] == true)
        .then(|| RiderArt {
            hair: string(build, "hair", "#5a3418").into(),
            skin: string(build, "skin", "#f0c49c").into(),
            cloak: string(build, "cloak", "#3a3f4f").into(),
            style: string(build, "style", "short").into(),
            path: string(build, "path", "shadow").into(),
            crown: build["crown"] == true,
            daddio: dad,
        });
    let color = string(bond, "color", string(build, "dcolor", "#7f88aa"));
    let rgb = dragon_game::drawing::source_rgb(color).map(|n| (n as f64 * 0.3 + 0.5).floor() as u8);
    let eye = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let breaths = [
        ["#f2f8ff", "#8fc8ff", "#3f7dff"],
        ["#fffbe0", "#ffe07a", "#ff9a2e"],
        ["#fff6c2", "#ffb347", "#ff3f1f"],
        ["#f4fff0", "#9dffb0", "#2fd67a"],
        ["#fbf2ff", "#d6a8ff", "#9b5cff"],
        ["#fff0f8", "#ffb0d8", "#ff5fae"],
        ["#ffffff", "#ffe9a0", "#ffc400"],
    ];
    let fire_cols =
        Some(breaths[build["breath"].as_u64().unwrap_or(0).min(6) as usize].map(str::to_owned));
    draw_dragon(
        &mut c,
        p.x,
        p.y + bob,
        s,
        color,
        p.facing as f64,
        flap_time,
        &DragonArt {
            rider_time: Some(time),
            spin: p.spin,
            dizzy: p.dizzy > 0.,
            fire: main == bond && p.fire_flash > 0.,
            fire_cols: fire_cols.clone(),
            rider,
            aura: st == 3 && !dad,
            eye: Some(eye),
            horn: Some(horn(bond)),
            dad,
        },
    );
    c.restore();
    c.alpha(1.);
    if p.shield > 0. {
        c.stroke_style("rgba(168,228,255,0.85)");
        c.fill_style("rgba(168,228,255,0.15)");
        c.width(3.);
        c.begin();
        c.arc(p.x, p.y - s * 0.1, s * 1.15, 0., TAU);
        c.fill();
        c.stroke();
    }
    if p.dizzy > 0. {
        let hx = p.x + p.facing as f64 * s * 0.5;
        let hy = p.y - s * 0.7;
        for i in 0..3 {
            let a = time * 6. + i as f64 * 2.09;
            draw_star(
                &mut c,
                hx + a.cos() * s * 0.38,
                hy + a.sin() * s * 0.12,
                s * 0.1,
                "#ffd76e",
                a,
            );
        }
    }
    let realm = ["hollow", "wood", "frost", "sea", "ember", "stars"]
        .get(game.definition.realm)
        .copied()
        .unwrap_or("hollow");
    for hazard in &game.hazards {
        draw_hazard(&mut c, hazard, &scene, realm);
    }
    for shot in &game.shots {
        let owner = party.and_then(|a| {
            a.iter()
                .find(|d| d["id"].as_str() == Some(shot.owner.as_str()))
        });
        let cols = if owner.is_some_and(|d| d["bonded"] == true) || shot.owner == "bond" {
            fire_cols.clone()
        } else {
            breath(match shot.kind {
                dragon_game::combat::Kind::Spark => "spark",
                dragon_game::combat::Kind::Frost => "frost",
                dragon_game::combat::Kind::Star => "star",
                _ => "flame",
            })
        };
        dragon_game::drawing::draw_shot(&mut c, shot, cols.as_ref(), &scene);
    }
    if fancy {
        cache.weather.draw_ambient(&mut c, view.scale, time);
        dragon_game::drawing::draw_foreground(
            &mut c,
            view,
            game.definition.realm,
            game.scroll,
            time,
        );
    }
    cache.weather.draw_trails(&mut c);
    paint(painter, &c);
    cache.canvas = c;
    if let Some(ob) = &game.objective {
        if ob.max_hp > 0 {
            label(
                painter,
                &format!(
                    "{}{}",
                    "♥".repeat(ob.hp as usize),
                    "♡".repeat(ob.max_hp.saturating_sub(ob.hp) as usize)
                ),
                ob.x,
                ob.y - ob.r - view.scale * 0.55,
                Color32::from_rgb(255, 143, 160),
                (view.scale * 0.3).round().max(12.) as f32,
            );
        }
        if ob.what != "star" {
            label(
                painter,
                &ob.name,
                ob.x,
                ob.y + ob.r + view.scale * 0.35,
                Color32::from_rgb(255, 243, 214),
                (view.scale * 0.2).round().max(10.) as f32,
            );
        } else if ob.carrier >= 0 && !ob.done {
            label(
                painter,
                "Carrying the star",
                ob.x,
                ob.y - view.scale * 0.9,
                Color32::from_rgb(255, 215, 110),
                (view.scale * 0.24).round().max(11.) as f32,
            );
        }
    }
}

fn label(painter: &egui::Painter, text: &str, x: f64, y: f64, color: Color32, size: f32) {
    let galley = painter.layout_no_wrap(
        text.into(),
        egui::FontId::proportional(size),
        Color32::from_rgb(255, 243, 214),
    );
    let rect = egui::Rect::from_center_size(
        egui::pos2(x as f32, y as f32),
        egui::vec2(galley.size().x + size * 0.9, size * 1.35),
    );
    painter.rect_filled(
        rect,
        rect.height() * 0.5,
        Color32::from_rgba_unmultiplied(8, 14, 36, 191),
    );
    painter.rect_stroke(
        rect,
        rect.height() * 0.5,
        Stroke::new(2., color),
        egui::StrokeKind::Middle,
    );
    painter.galley(
        rect.center() - galley.size() * 0.5 + egui::vec2(0., size * 0.06),
        galley,
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use dragon_game::drawing::{DragonArt, draw_dragon};
    #[test]
    fn mirrored_cached_layer_keeps_vertical_geometry() {
        let mut c = Canvas::default();
        c.fill_rect(10., 20., 30., 40.);
        let source = shapes(&c);
        let mirrored = mirrored(&source, 100.);
        let (Shape::Mesh(a), Shape::Mesh(b)) = (&source[0], &mirrored[0]) else {
            panic!("filled rectangle mesh");
        };
        for (a, b) in a.vertices.iter().zip(&b.vertices) {
            assert_eq!(a.pos.y, b.pos.y);
            assert_eq!(a.pos.x + b.pos.x, 100.);
            assert_eq!(a.color, b.color);
        }
    }
    #[test]
    fn dash_phase_continues_across_polyline_corners() {
        let d = dashed_path(
            &[egui::pos2(0., 0.), egui::pos2(6., 0.), egui::pos2(6., 14.)],
            &[10., 5.],
            1.,
        );
        assert_eq!(
            d,
            vec![
                vec![egui::pos2(0., 0.), egui::pos2(6., 0.), egui::pos2(6., 4.)],
                vec![egui::pos2(6., 9.), egui::pos2(6., 14.)]
            ]
        );
    }
    #[test]
    fn concave_polygon_is_not_filled_as_a_fan() {
        let mut c = Canvas::default();
        c.begin();
        for (i, [x, y]) in [[0., 0.], [10., 0.], [10., 10.], [5., 4.], [0., 10.]]
            .into_iter()
            .enumerate()
        {
            if i == 0 {
                c.move_to(x, y)
            } else {
                c.line_to(x, y)
            }
        }
        c.fill();
        let d = &c.commands[0];
        let mut m = Mesh::default();
        polygon(
            &mut m,
            &PreparedPaint::new(d),
            paths(&d.path, d.transform).remove(0).0,
        );
        let area: f32 = m
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|i| {
                cross(
                    m.vertices[i[0] as usize].pos,
                    m.vertices[i[1] as usize].pos,
                    m.vertices[i[2] as usize].pos,
                )
                .abs()
                    * 0.5
            })
            .sum();
        assert!((area - 70.).abs() < 0.01);
    }
    #[test]
    fn radial_colors_preserve_transformed_gradient() {
        let mut c = Canvas::default();
        c.translate(20., 30.);
        c.scale(-2., 3.);
        let g = c.radial(
            [0., 0., 0., 0., 0., 10.],
            &[(0., "#ffffff".into()), (1., "#000000".into())],
        );
        c.fill_style(g);
        c.fill_rect(-10., -10., 20., 20.);
        let d = &c.commands[0];
        assert_eq!(color(d, Pos2::new(20., 30.)), Color32::WHITE);
        assert_eq!(color(d, Pos2::new(0., 30.)), Color32::BLACK);
    }
    #[test]
    fn animated_dash_offset_moves_phase_without_changing_pattern() {
        let line = [Pos2::new(0., 0.), Pos2::new(10., 0.)];
        assert_eq!(
            dashed_path_offset(&line, &[4., 2.], 1., 2.),
            vec![
                vec![Pos2::new(0., 0.), Pos2::new(2., 0.)],
                vec![Pos2::new(4., 0.), Pos2::new(8., 0.)]
            ]
        );
        assert_eq!(
            dashed_path_offset(&line, &[4., 2.], 1., -2.),
            vec![
                vec![Pos2::new(2., 0.), Pos2::new(6., 0.)],
                vec![Pos2::new(8., 0.), Pos2::new(10., 0.)]
            ]
        );
    }
    #[test]
    fn cubic_and_round_rect_tessellation_is_bounded_and_preserves_endpoints() {
        let t = Affine([2., 0., 0., 3., 10., 20.]);
        let curve = paths(
            &[
                PathCommand::Move(0., 0.),
                PathCommand::Cubic(0., 10., 10., 10., 10., 0.),
            ],
            t,
        );
        assert_eq!(curve.len(), 1);
        assert_eq!(curve[0].0.first(), Some(&Pos2::new(10., 20.)));
        assert_eq!(curve[0].0.last(), Some(&Pos2::new(30., 20.)));
        assert!(curve[0].0.len() <= 193);
        for radius in [0., 12., 100.] {
            let rounded = paths(
                &[PathCommand::RoundRect(0., 0., 100., 60., radius)],
                Affine::default(),
            );
            assert_eq!(rounded.len(), 1);
            assert!(rounded[0].1);
            assert!(rounded[0].0.len() <= 260);
            assert!(
                rounded[0]
                    .0
                    .iter()
                    .all(|p| p.x >= -0.001 && p.x <= 100.001 && p.y >= -0.001 && p.y <= 60.001)
            );
            let points = &rounded[0].0;
            let area: f32 = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .take(points.len())
                .map(|(a, b)| a.x * b.y - a.y * b.x)
                .sum::<f32>()
                .abs()
                * 0.5;
            let r = radius.min(30.) as f32;
            let expected = 6000. - (4. - std::f32::consts::PI) * r * r;
            assert!(
                (area - expected).abs() < 16.,
                "radius {radius}: {area} != {expected}"
            );
        }
    }
    #[test]
    fn source_dragon_tessellates_without_nonfinite_vertices() {
        let mut c = Canvas::default();
        draw_dragon(
            &mut c,
            150.,
            120.,
            48.,
            "#3f5ad0",
            -1.,
            0.7,
            &DragonArt {
                dad: true,
                ..Default::default()
            },
        );
        let mut count = 0;
        for d in &c.commands {
            if d.stroke.is_some() {
                continue;
            }
            let mut mesh = Mesh::default();
            for (p, _) in paths(&d.path, d.transform) {
                polygon(&mut mesh, &PreparedPaint::new(d), p);
            }
            assert!(
                mesh.vertices
                    .iter()
                    .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite())
            );
            count += mesh.vertices.len();
        }
        assert!(count > 100);
        assert!(count < 500_000, "bounded presentation geometry: {count}");
    }
    fn gradient_draw(paint: Paint) -> Draw {
        Draw {
            path: vec![],
            transform: Affine::default(),
            paint,
            alpha: 1.,
            additive: false,
            stroke: None,
            clips: vec![],
        }
    }
    fn original_triangle(mesh: &mut Mesh, d: &Draw, points: [Pos2; 3], depth: u8) {
        let [a, b, c] = points;
        if !matches!(d.paint, Paint::Solid(_))
            && depth < 7
            && a.distance(b).max(b.distance(c)).max(c.distance(a)) > 8.
        {
            let ab = a + (b - a) * 0.5;
            let bc = b + (c - b) * 0.5;
            let ca = c + (a - c) * 0.5;
            for points in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
                original_triangle(mesh, d, points, depth + 1);
            }
        } else {
            emit_triangle(mesh, points.map(|p| (p, color(d, p))));
        }
    }
    fn mesh_color(mesh: &Mesh, point: Pos2) -> Option<[f32; 4]> {
        for indices in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = std::array::from_fn(|i| mesh.vertices[indices[i] as usize]);
            let area = cross(a.pos, b.pos, c.pos);
            if area.abs() < f32::EPSILON {
                continue;
            }
            let weights = [
                cross(point, b.pos, c.pos) / area,
                cross(a.pos, point, c.pos) / area,
                cross(a.pos, b.pos, point) / area,
            ];
            if weights.iter().all(|w| *w >= -0.0001) {
                return Some(std::array::from_fn(|i| {
                    [a, b, c]
                        .iter()
                        .zip(weights)
                        .map(|(v, w)| f32::from(v.color[i]) * w)
                        .sum()
                }));
            }
        }
        None
    }
    #[test]
    fn prepared_colors_match_original_sampler_and_fallback_geometry() {
        let transform = Affine([-1.2, 0.2, 0.3, 0.7, 21., 9.]);
        let paints = [
            Paint::Solid("#8ad".into()),
            Paint::Linear {
                coords: [0., 0., 130., 20.],
                transform,
                stops: vec![
                    (0., "rgba(12,80,220,0.15)".into()),
                    (1., "rgba(240,30,10,0.9)".into()),
                ],
            },
            Paint::Linear {
                coords: [0., 0., 130., 20.],
                transform,
                stops: vec![
                    (0., "#fff".into()),
                    (0.5, "#000".into()),
                    (0.5, "#f00".into()),
                    (1., "#00f".into()),
                ],
            },
            Paint::Radial {
                coords: [2., 3., 5., 12., 8., 60.],
                transform,
                stops: vec![
                    (0., "rgba(255,80,16,0.8)".into()),
                    (1., "rgba(255,80,16,0)".into()),
                ],
            },
            Paint::Linear {
                coords: [2., 3., 2., 3.],
                transform,
                stops: vec![(0., "#fff".into()), (1., "#000".into())],
            },
            Paint::Linear {
                coords: [0., 0., 1., 1.],
                transform: Affine([0.; 6]),
                stops: vec![(0., "#fff".into()), (1., "#000".into())],
            },
        ];
        for paint in paints {
            for additive in [false, true] {
                let mut draw = gradient_draw(paint.clone());
                draw.alpha = 0.73;
                draw.additive = additive;
                let prepared = PreparedPaint::new(&draw);
                assert!(!prepared.linear_bands);
                for y in -5..25 {
                    for x in -5..25 {
                        let p = egui::pos2(x as f32 * 4.1, y as f32 * 3.7);
                        assert_eq!(prepared.color(p), color(&draw, p));
                    }
                }
                let points = [
                    egui::pos2(-20., -10.),
                    egui::pos2(130., 20.),
                    egui::pos2(30., 100.),
                ];
                let mut before = Mesh::default();
                original_triangle(&mut before, &draw, points, 0);
                let mut after = Mesh::default();
                triangle(&mut after, &prepared, points[0], points[1], points[2], 0);
                assert_eq!(after.indices, before.indices);
                assert_eq!(after.vertices, before.vertices);
            }
        }
    }
    #[test]
    fn linear_bands_preserve_transformed_colors_clamping_and_coverage() {
        for transform in [Affine::default(), Affine([-1.3, 0.2, 0.4, 0.8, 500., -20.])] {
            for alpha in [1., 0.4] {
                for additive in [false, true] {
                    let mut draw = gradient_draw(Paint::Linear {
                        coords: [20., 20., 240., 100.],
                        transform,
                        stops: vec![
                            (0.2, format!("rgba(20,160,240,{alpha})")),
                            (0.55, format!("rgba(240,60,20,{alpha})")),
                            (0.75, format!("rgba(40,220,140,{alpha})")),
                        ],
                    });
                    draw.alpha = 0.7;
                    draw.additive = additive;
                    let paint = PreparedPaint::new(&draw);
                    assert!(paint.linear_bands);
                    let points = [[0., 0.], [300., 0.], [300., 200.], [0., 200.]]
                        .map(|[x, y]| point(transform, x, y));
                    let mut mesh = Mesh::default();
                    polygon(&mut mesh, &paint, points.to_vec());
                    let area: f32 = mesh
                        .indices
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|i| {
                            cross(
                                mesh.vertices[i[0] as usize].pos,
                                mesh.vertices[i[1] as usize].pos,
                                mesh.vertices[i[2] as usize].pos,
                            )
                            .abs()
                                * 0.5
                        })
                        .sum();
                    let expected_area = cross(points[0], points[1], points[2]).abs();
                    assert!((area - expected_area).abs() < expected_area * 0.00001);
                    // Sample interiors and both clamped end bands. Egui interpolates
                    // sRGB vertex channels; allow two byte levels for endpoint
                    // quantization and alpha premultiplication, not a visual shift.
                    for y in 0..20 {
                        for x in 0..30 {
                            let p =
                                point(transform, (x as f64 + 0.37) * 10., (y as f64 + 0.61) * 10.);
                            let actual =
                                mesh_color(&mesh, p).expect("gradient covers original rectangle");
                            let expected = color(&draw, p);
                            for i in 0..4 {
                                assert!(
                                    (actual[i] - f32::from(expected[i])).abs() <= 2.,
                                    "color channel {i}: {actual:?} != {expected:?} at {p:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn phone_sky_linear_gradient_geometry_depends_on_stops_not_pixel_area() {
        for (width, height) in [(390., 844.), (844., 390.), (1920., 1080.)] {
            let view = dragon_game::flight::Viewport::from_screen(width, height, 1., 130., 0.62);
            let background = dragon_game::drawing::draw_background(view, 0, false);
            let sky = &background.sky.commands[0];
            let paint = PreparedPaint::new(sky);
            assert!(paint.linear_bands);
            let mut mesh = Mesh::default();
            let points = paths(&sky.path, sky.transform).remove(0).0;
            polygon(&mut mesh, &paint, points);
            assert!(
                mesh.vertices.len() <= 18,
                "{} sky vertices",
                mesh.vertices.len()
            );
            let original_points = [
                egui::pos2(0., 0.),
                egui::pos2(width as f32, 0.),
                egui::pos2(width as f32, height as f32),
                egui::pos2(0., height as f32),
            ];
            let mut before = Mesh::default();
            original_triangle(
                &mut before,
                sky,
                [original_points[3], original_points[0], original_points[1]],
                0,
            );
            original_triangle(
                &mut before,
                sky,
                [original_points[1], original_points[2], original_points[3]],
                0,
            );
            assert_eq!(before.vertices.len(), 98_304);
            println!(
                "sky {width}x{height}: {} -> {} fill vertices",
                before.vertices.len(),
                mesh.vertices.len()
            );
        }
    }
    #[test]
    #[ignore = "manual native CPU comparison; not a frame-rate or device benchmark"]
    fn gradient_tessellation_cpu() {
        use std::{hint::black_box, time::Instant};
        let view = dragon_game::flight::Viewport::from_screen(390., 844., 1., 130., 0.62);
        let background = dragon_game::drawing::draw_background(view, 0, false);
        let draw = &background.sky.commands[0];
        let points = [
            egui::pos2(0., 0.),
            egui::pos2(390., 0.),
            egui::pos2(390., 844.),
            egui::pos2(0., 844.),
        ];
        let mut before = Vec::new();
        let mut after = Vec::new();
        for sample in 0..7 {
            for optimized in [sample % 2 == 0, sample % 2 != 0] {
                let start = Instant::now();
                for _ in 0..20 {
                    let mut mesh = Mesh::default();
                    if optimized {
                        polygon(
                            &mut mesh,
                            &PreparedPaint::new(black_box(draw)),
                            points.to_vec(),
                        );
                    } else {
                        original_triangle(
                            &mut mesh,
                            black_box(draw),
                            [points[3], points[0], points[1]],
                            0,
                        );
                        original_triangle(
                            &mut mesh,
                            black_box(draw),
                            [points[1], points[2], points[3]],
                            0,
                        );
                    }
                    black_box(mesh);
                }
                if optimized { &mut after } else { &mut before }
                    .push(start.elapsed().as_secs_f64() * 1e6 / 20.);
            }
        }
        before.sort_by(f64::total_cmp);
        after.sort_by(f64::total_cmp);
        println!(
            "phone sky tessellation CPU us: before median {:.3} range {:.3}..{:.3}; after {:.3} range {:.3}..{:.3}",
            before[3], before[0], before[6], after[3], after[0], after[6]
        );
    }
    // Fill geometry only: line strokes are tessellated later by egui. Disabling
    // the new bands reproduces the old triangle counts, not its parsing time.
    fn fill_vertex_count(canvas: &Canvas, bands: bool) -> usize {
        let mut count = 0;
        for draw in &canvas.commands {
            if draw.stroke.is_some() {
                continue;
            }
            let mut paint = PreparedPaint::new(draw);
            paint.linear_bands &= bands;
            let mut mesh = Mesh::default();
            for (mut p, _) in paths(&draw.path, draw.transform) {
                for clipping in &draw.clips {
                    for (boundary, _) in paths(&clipping.path, clipping.transform) {
                        p = clip(p, &boundary);
                    }
                }
                if !bands || !radial_circle(&mut mesh, &paint, &p) {
                    polygon(&mut mesh, &paint, p);
                }
            }
            count += mesh.vertices.len();
        }
        count
    }
    #[test]
    #[ignore = "manual representative scene geometry counts, no rendered frame timing"]
    fn first_flight_fill_mesh_counts() {
        let view = dragon_game::flight::Viewport::from_screen(390., 844., 1., 130., 0.62);
        let mut game = dragon_game::game::Game::first_flight(view, 42);
        for _ in 0..120 {
            game.step(1. / 60., dragon_game::flight::Input::default(), view)
                .unwrap();
        }
        let ctx = egui::Context::default();
        let mut cache = ArtCache::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::background());
            paint_game(&painter, &mut cache, &game, view, 2., 2., 1., 1., true);
        });
        output.textures_delta.clear();
        let background = dragon_game::drawing::draw_background(view, 0, false);
        for (name, canvas) in [
            ("sky", &background.sky),
            ("far (one tile)", &background.far),
            ("land (one tile)", &background.land),
            ("dynamic First Flight t=2s", &cache.canvas),
        ] {
            println!(
                "{name} fill vertices: {} -> {}",
                fill_vertex_count(canvas, false),
                fill_vertex_count(canvas, true)
            );
        }
    }
    fn circle_glow(stops: Vec<(f64, String)>, transform: Affine) -> Draw {
        let mut draw = gradient_draw(Paint::Radial {
            coords: [0., 0., 20., 0., 0., 100.],
            transform,
            stops,
        });
        draw.transform = transform;
        draw.path = vec![PathCommand::Ellipse(0., 0., 100., 100., 0., 0., TAU)];
        draw
    }
    #[test]
    fn concentric_glow_rings_preserve_dense_colors_and_original_outline() {
        for transform in [Affine::default(), Affine([-1.3, 0.2, 0.4, 0.8, 310., 200.])] {
            for additive in [false, true] {
                for stops in [
                    vec![
                        (0., "rgba(234,242,255,0.3)".into()),
                        (1., "rgba(234,242,255,0)".into()),
                    ],
                    // An interior peak must not vanish just because the center
                    // and outside of the source circle are both transparent.
                    vec![
                        (0., "rgba(180,70,250,0)".into()),
                        (0.4, "rgba(180,70,250,0)".into()),
                        (0.55, "rgba(180,70,250,0.8)".into()),
                        (0.7, "rgba(180,70,250,0)".into()),
                        (1., "rgba(180,70,250,0)".into()),
                    ],
                ] {
                    let mut draw = circle_glow(stops, transform);
                    draw.additive = additive;
                    draw.alpha = 0.85;
                    let paint = PreparedPaint::new(&draw);
                    let boundary = paths(&draw.path, draw.transform).remove(0).0;
                    let mut mesh = Mesh::default();
                    assert!(radial_circle(&mut mesh, &paint, &boundary));
                    let count = boundary.len() - 1;
                    let outline = &mesh.vertices[mesh.vertices.len() - count..];
                    for (vertex, point) in outline.iter().zip(&boundary) {
                        assert_eq!(vertex.pos, *point);
                    }
                    for r in 0..64 {
                        let radius = (r as f64 + 0.37) * 100. / 64.;
                        for a in 0..64 {
                            let angle = (a as f64 + 0.43) * TAU / 64.;
                            let p = point(transform, radius * angle.cos(), radius * angle.sin());
                            let actual =
                                mesh_color(&mesh, p).expect("rings cover the original circle");
                            let expected = color(&draw, p);
                            for i in 0..4 {
                                assert!(
                                    (actual[i] - f32::from(expected[i])).abs() <= 2.,
                                    "radial channel {i}: {actual:?} != {expected:?} at radius {radius}"
                                );
                            }
                        }
                    }
                    // A sample at an interior ring's peak retains its alpha;
                    // this is additional to the dense grid above.
                    if !additive && paint.stops.len() > 2 {
                        let peak = point(transform, 20. + 0.55 * 80., 0.);
                        assert!(mesh_color(&mesh, peak).unwrap()[3] > 170.);
                    }
                }
            }
        }
    }
    #[test]
    fn unresolved_thin_rings_and_unsupported_radials_keep_fallback() {
        let mut draw = circle_glow(
            vec![
                (0., "rgba(255,255,255,0)".into()),
                (0.5, "rgba(255,255,255,0)".into()),
                (0.5001, "rgba(255,255,255,1)".into()),
                (0.5002, "rgba(255,255,255,0)".into()),
                (1., "rgba(255,255,255,0)".into()),
            ],
            Affine::default(),
        );
        for case in 0..5 {
            if case == 1 {
                draw = circle_glow(
                    vec![(0., "#f00".into()), (1., "#00f".into())],
                    Affine::default(),
                );
            } else if case == 2 {
                draw = circle_glow(
                    vec![
                        (0., "rgba(255,255,255,1)".into()),
                        (1., "rgba(255,255,255,0)".into()),
                    ],
                    Affine::default(),
                );
                if let Paint::Radial { coords, .. } = &mut draw.paint {
                    coords[3] = 1.;
                }
            } else if case == 3 {
                draw = circle_glow(
                    vec![(0., "#fff".into()), (1., "#fff".into())],
                    Affine([0.; 6]),
                );
            } else if case == 4 {
                draw = circle_glow(
                    vec![(0., "#fff".into()), (1., "#fff".into())],
                    Affine::default(),
                );
                draw.clips.push(dragon_game::drawing::Clip {
                    path: vec![],
                    transform: Affine::default(),
                });
            }
            let paint = PreparedPaint::new(&draw);
            let boundary = paths(&draw.path, draw.transform).remove(0).0;
            let mut mesh = Mesh::default();
            assert!(!radial_circle(&mut mesh, &paint, &boundary), "case {case}");
            assert!(mesh.vertices.is_empty() && mesh.indices.is_empty());
        }
    }
    #[test]
    fn complete_phone_sky_mesh_keeps_bounded_stop_geometry() {
        let view = dragon_game::flight::Viewport::from_screen(390., 844., 1., 130., 0.62);
        let sky = dragon_game::drawing::draw_background(view, 0, false).sky;
        let count = fill_vertex_count(&sky, true);
        // Current source fixture: the gradient rectangle plus a two-stop glow,
        // solid moon and stars. Bound geometry, not uncalibrated elapsed time.
        assert!(count < 2_000, "phone sky fill vertices: {count}");
        assert_eq!(fill_vertex_count(&sky, false), 660_807);
    }
}
