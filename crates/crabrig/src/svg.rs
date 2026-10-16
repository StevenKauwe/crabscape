//! SVG backend (port of `svg.mjs`): per-shape wobble on the ink line, per-shape light gradient, heavier line weight
//! on the shadow side.

use crate::geom::*;
use crate::item::{Item, Kind};
use crate::palette::{Mat, Palette, js_round, mix_hex};

/// JS `(Math.round(v * 10) / 10).toString()`.
pub fn f2(v: f64) -> String {
    num(js_round(v * 10.0) / 10.0)
}
/// A JS number as JS prints it (for the plain values the art uses).
pub fn num(v: f64) -> String {
    if v == 0.0 {
        "0".to_string()
    } else {
        format!("{v}")
    }
}
/// JS `v.toFixed(1)`.
pub fn fixed1(v: f64) -> String {
    let t = v * 10.0;
    let n = if (t - t.floor() - 0.5).abs() == 0.0 {
        t.floor() + 1.0
    } else {
        t.round()
    };
    let s = format!("{:.1}", n / 10.0);
    if s == "-0.0" { "0.0".to_string() } else { s }
}
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

/// Resolve a material for a shape: [lit, base, shade].
pub fn mat_cols(p: &Palette, it: &Item) -> [String; 3] {
    let mut m = match p.get(it.mat.as_str()).or_else(|| p.get("leg")) {
        Some(Mat::Tri(c)) => c.clone(),
        Some(Mat::One(c)) => [c.clone(), c.clone(), c.clone()],
        None => unreachable!(),
    };
    let shadow = crate::palette::one(p, "shadow");
    if it.far {
        m = m.map(|c| mix_hex(&c, &shadow, 0.22));
    }
    if it.mid {
        m = m.map(|c| mix_hex(&c, &shadow, 0.1)); // v0.2: middle walking leg, slight recession
    }
    if it.shade {
        m = [m[2].clone(), mix_hex(&m[1], &m[2], 0.6), m[2].clone()];
    }
    if it.under {
        m = [
            mix_hex(&m[1], &m[2], 0.2),
            mix_hex(&m[1], &m[2], 0.55),
            mix_hex(&m[2], "#3A1A12", 0.3),
        ];
    }
    m
}

/// Small, low-frequency wobble along the normal, seeded by the shape id (so it stays with the shape). The JS used
/// 6.28 / 6.283 rather than TAU: kept, so the wobble is the same.
#[allow(clippy::approx_constant)]
pub fn wobble(pts: &[P], id: &str, amp: f64) -> Vec<P> {
    let mut r = rng(hash_str(id));
    let n = pts.len();
    let (ph, ph2) = (r() * 6.28, r() * 6.28);
    let f1 = 2.0 + (r() * 3.0).floor();
    let f2_ = 5.0 + (r() * 4.0).floor();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[(i + n - 1) % n], pts[(i + 1) % n]);
            let nn = perp(norm(sub(b, a)));
            let t = i as f64 / n as f64 * 6.283;
            let w = amp * ((t * f1 + ph).sin() * 0.7 + (t * f2_ + ph2).sin() * 0.3);
            [pts[i][0] + nn[0] * w, pts[i][1] + nn[1] * w]
        })
        .collect()
}

pub fn signed_area(p: &[P]) -> f64 {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (q0, q1) = (p[i], p[(i + 1) % p.len()]);
        a += q0[0] * q1[1] - q1[0] * q0[1];
    }
    a / 2.0
}

fn path_d(pts: &[P]) -> String {
    pts.iter()
        .map(|p| format!("{} {}", f2(p[0]), f2(p[1])))
        .collect::<Vec<_>>()
        .join("L")
}

/// Options for `SvgOut::items`.
#[derive(Clone)]
pub struct ItemsOpt {
    pub light: P,
    pub ink: f64,
    pub scale: f64,
    pub id_prefix: String,
}
impl Default for ItemsOpt {
    fn default() -> Self {
        ItemsOpt {
            light: [-0.25, -1.0],
            ink: 1.0,
            scale: 1.0,
            id_prefix: String::new(),
        }
    }
}

pub struct SvgOut {
    pub w: f64,
    pub h: f64,
    pub defs: Vec<String>,
    pub body: Vec<String>,
    pub n: usize,
}

pub struct Text<'a> {
    pub size: f64,
    pub fill: &'a str,
    pub weight: &'a str,
    pub anchor: &'a str,
    pub family: &'a str,
    pub style: &'a str,
}
impl Default for Text<'_> {
    fn default() -> Self {
        Text {
            size: 20.0,
            fill: "#2A140F",
            weight: "normal",
            anchor: "start",
            family: "C059, serif",
            style: "",
        }
    }
}

impl SvgOut {
    pub fn new(w: f64, h: f64, bg: Option<&str>) -> SvgOut {
        let mut s = SvgOut {
            w,
            h,
            defs: vec![],
            body: vec![],
            n: 0,
        };
        if let Some(bg) = bg {
            s.body.push(format!(
                r#"<rect width="{}" height="{}" fill="{bg}"/>"#,
                num(w),
                num(h)
            ));
        }
        s
    }
    pub fn raw(&mut self, s: impl Into<String>) {
        self.body.push(s.into());
    }
    /// Items already in output coordinates (after transform).
    pub fn items(&mut self, items: &[Item], p: &Palette, o: &ItemsOpt) {
        let l = norm(o.light);
        let mut open: Option<String> = None;
        for it in items {
            let g = it.g.clone().unwrap_or_else(|| "misc".into());
            if open.as_ref() != Some(&g) {
                if open.is_some() {
                    self.body.push("</g>".into());
                }
                self.body.push(format!(
                    r#"<g id="{}" data-part="{}">"#,
                    esc(&format!("{}{g}", o.id_prefix)),
                    esc(&g)
                ));
                open = Some(g);
            }
            match it.k {
                Kind::Shape => self.shape(it, p, l, o.ink * o.scale, &o.id_prefix),
                Kind::Line => self.line(it, p, o.ink * o.scale),
                Kind::Glow => self.glow(it),
                _ => {}
            }
        }
        if open.is_some() {
            self.body.push("</g>".into());
        }
    }
    fn shape(&mut self, it: &Item, p: &Palette, l: P, ink_k: f64, pre: &str) {
        let id = format!("{pre}{}", it.id)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            + &format!("_{}", self.n);
        self.n += 1;
        let cols = mat_cols(p, it);
        let b = bbox(&it.pts);
        // gradient along the light direction across this shape's own extent
        let (c, ext) = (
            [b.cx, b.cy],
            l[0].abs() * b.w / 2.0 + l[1].abs() * b.h / 2.0,
        );
        let (g1, g2) = (
            [c[0] + l[0] * ext, c[1] + l[1] * ext],
            [c[0] - l[0] * ext, c[1] - l[1] * ext],
        );
        self.defs.push(format!(
            r#"<linearGradient id="g{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}"><stop offset="0" stop-color="{}"/><stop offset=".48" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient>"#,
            f2(g1[0]),
            f2(g1[1]),
            f2(g2[0]),
            f2(g2[1]),
            cols[0],
            cols[1],
            cols[2]
        ));
        let wob = wobble(
            &it.pts,
            &it.id,
            (0.25 + b.w * 0.004).min(1.2) * ink_k.max(0.4),
        );
        let d = format!("M{}Z", path_d(&wob));
        let op = it
            .op
            .map(|v| format!(r#" fill-opacity="{}""#, num(v)))
            .unwrap_or_default();
        self.body.push(format!(
            r#"<path id="{id}" d="{d}" fill="url(#g{id})"{op}/>"#
        ));
        if !it.ink {
            return;
        }
        let sw = it.sw.unwrap_or(1.0) * 1.6 * ink_k;
        let ink = crate::palette::one(p, "ink");
        let ink_col = if it.far {
            mix_hex(&ink, &crate::palette::one(p, "shadow"), 0.3)
        } else {
            ink
        };
        self.body.push(format!(r#"<path d="{d}" fill="none" stroke="{ink_col}" stroke-width="{}" stroke-linejoin="round"/>"#, f2(sw)));
        // heavier weight on the side facing away from the light
        let n = wob.len();
        let mut runs: Vec<Vec<usize>> = vec![];
        let mut run = false;
        let area = signed_area(&wob);
        for i in 0..n {
            let (a, bb) = (wob[(i + n - 1) % n], wob[(i + 1) % n]);
            let t = norm(sub(bb, a));
            let nn = [t[1], -t[0]]; // outward normal for CW-in-screen
            let out = if area > 0.0 { nn } else { [-nn[0], -nn[1]] };
            let away = out[0] * l[0] + out[1] * l[1] < -0.35;
            if away {
                if !run {
                    runs.push(vec![]);
                    run = true;
                }
                runs.last_mut().unwrap().push(i);
            } else {
                run = false;
            }
        }
        if runs.len() > 1 && runs[0][0] == 0 && *runs.last().unwrap().last().unwrap() == n - 1 {
            let mut last = runs.pop().unwrap();
            last.extend(runs[0].iter());
            runs[0] = last;
        }
        for r in runs.iter().filter(|r| r.len() > 2) {
            let pts: Vec<P> = r.iter().map(|&i| wob[i]).collect();
            self.body.push(format!(r#"<path d="M{}" fill="none" stroke="{ink_col}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#, path_d(&pts), f2(sw * 1.9)));
        }
    }
    fn line(&mut self, it: &Item, p: &Palette, ink_k: f64) {
        let col = if it.mat == "ink" {
            crate::palette::one(p, "ink")
        } else if p.contains_key(it.mat.as_str()) {
            crate::palette::one(p, &it.mat)
        } else {
            it.mat.clone()
        };
        let pts = if it.pts.len() > 2 {
            through(&it.pts, 5, false)
        } else {
            it.pts.clone()
        };
        self.body.push(format!(
            r#"<path d="M{}" fill="none" stroke="{col}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
            path_d(&pts),
            f2(it.sw.unwrap_or(1.0) * 1.6 * ink_k)
        ));
    }
    fn glow(&mut self, it: &Item) {
        let id = format!("glow{}", self.n);
        self.n += 1;
        let c = &it.col;
        self.defs.push(format!(
            r#"<radialGradient id="{id}"><stop offset="0" stop-color="{c}" stop-opacity=".55"/><stop offset=".35" stop-color="{c}" stop-opacity=".2"/><stop offset="1" stop-color="{c}" stop-opacity="0"/></radialGradient>"#
        ));
        self.body.push(format!(
            r#"<circle cx="{}" cy="{}" r="{}" fill="url(#{id})" style="mix-blend-mode:screen"/>"#,
            f2(it.x),
            f2(it.y),
            f2(it.r)
        ));
    }
    pub fn text(&mut self, x: f64, y: f64, s: &str, t: &Text) {
        self.body.push(format!(
            r#"<text x="{}" y="{}" font-family="{}" font-size="{}" font-weight="{}" fill="{}" text-anchor="{}" {}>{}</text>"#,
            f2(x),
            f2(y),
            t.family,
            num(t.size),
            t.weight,
            t.fill,
            t.anchor,
            t.style,
            esc(s)
        ));
    }
}
impl std::fmt::Display for SvgOut {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<defs>\n{}\n</defs>\n{}\n</svg>\n",
            self.defs.join("\n"),
            self.body.join("\n"),
            w = num(self.w),
            h = num(self.h)
        )
    }
}
