//! Original Dragon Canvas geometry, expressed as renderer-independent Rust draw operations.
//! Coordinates and animation arithmetic intentionally remain f64, like the source JavaScript.
//! This is not a rasterizer. The engine adapter tessellates these paths for the GPU.
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PathCommand {
    Move(f64, f64),
    Line(f64, f64),
    Quad(f64, f64, f64, f64),
    Ellipse(f64, f64, f64, f64, f64, f64, f64),
    Close,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine(pub [f64; 6]);
impl Default for Affine {
    fn default() -> Self {
        Self([1., 0., 0., 1., 0., 0.])
    }
}
impl Affine {
    pub fn point(self, x: f64, y: f64) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }
    pub fn then(&mut self, rhs: [f64; 6]) {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = rhs;
        self.0 = [
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ];
    }
    pub fn inverse_point(self, x: f64, y: f64) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det.abs() < 1e-15 {
            return [0., 0.];
        }
        [
            (d * (x - e) - c * (y - f)) / det,
            (-b * (x - e) + a * (y - f)) / det,
        ]
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Paint {
    Solid(String),
    Linear {
        coords: [f64; 4],
        transform: Affine,
        stops: Vec<(f64, String)>,
    },
    Radial {
        coords: [f64; 6],
        transform: Affine,
        stops: Vec<(f64, String)>,
    },
}
impl From<&str> for Paint {
    fn from(s: &str) -> Self {
        Self::Solid(s.into())
    }
}
impl From<String> for Paint {
    fn from(s: String) -> Self {
        Self::Solid(s)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub path: Vec<PathCommand>,
    pub transform: Affine,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f64,
    pub cap: String,
    pub join: String,
    pub dash: Vec<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Draw {
    pub path: Vec<PathCommand>,
    pub transform: Affine,
    pub paint: Paint,
    pub alpha: f64,
    pub additive: bool,
    pub stroke: Option<Stroke>,
    pub clips: Vec<Clip>,
}
pub type CanvasCommand = Draw;
#[derive(Clone)]
struct State {
    transform: Affine,
    fill: Paint,
    stroke: Paint,
    alpha: f64,
    width: f64,
    cap: String,
    join: String,
    dash: Vec<f64>,
    additive: bool,
    clips: Vec<Clip>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            transform: Affine::default(),
            fill: "#000000".into(),
            stroke: "#000000".into(),
            alpha: 1.,
            width: 1.,
            cap: "butt".into(),
            join: "miter".into(),
            dash: vec![],
            additive: false,
            clips: vec![],
        }
    }
}
#[derive(Default)]
pub struct Canvas {
    pub commands: Vec<Draw>,
    state: State,
    stack: Vec<State>,
    path: Vec<PathCommand>,
}
impl Canvas {
    /// Reset a frame while retaining the top-level command, path and state-stack capacity.
    pub fn reset(&mut self) {
        self.commands.clear();
        self.path.clear();
        self.stack.clear();
        self.state = State::default();
    }
    pub fn save(&mut self) {
        self.stack.push(self.state.clone());
    }
    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }
    pub fn translate(&mut self, x: f64, y: f64) {
        self.state.transform.then([1., 0., 0., 1., x, y]);
    }
    pub fn scale(&mut self, x: f64, y: f64) {
        self.state.transform.then([x, 0., 0., y, 0., 0.]);
    }
    pub fn rotate(&mut self, a: f64) {
        self.state
            .transform
            .then([a.cos(), a.sin(), -a.sin(), a.cos(), 0., 0.]);
    }
    pub fn fill_style(&mut self, p: impl Into<Paint>) {
        self.state.fill = p.into();
    }
    pub fn stroke_style(&mut self, p: impl Into<Paint>) {
        self.state.stroke = p.into();
    }
    pub fn alpha(&mut self, a: f64) {
        self.state.alpha = a;
    }
    pub fn multiply_alpha(&mut self, a: f64) {
        self.state.alpha *= a;
    }
    pub fn width(&mut self, w: f64) {
        self.state.width = w;
    }
    pub fn dash(&mut self, values: &[f64]) {
        self.state.dash = values.to_vec();
    }
    pub fn cap(&mut self, s: &str) {
        self.state.cap = s.into();
    }
    pub fn join(&mut self, s: &str) {
        self.state.join = s.into();
    }
    pub fn additive(&mut self, b: bool) {
        self.state.additive = b;
    }
    pub fn begin(&mut self) {
        self.path.clear();
    }
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.path.push(PathCommand::Move(x, y));
    }
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.path.push(PathCommand::Line(x, y));
    }
    pub fn quad(&mut self, a: f64, b: f64, c: f64, d: f64) {
        self.path.push(PathCommand::Quad(a, b, c, d));
    }
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64, rot: f64, start: f64, end: f64) {
        self.path
            .push(PathCommand::Ellipse(x, y, rx, ry, rot, start, end));
    }
    pub fn arc(&mut self, x: f64, y: f64, r: f64, start: f64, end: f64) {
        self.ellipse(x, y, r, r, 0., start, end);
    }
    pub fn close(&mut self) {
        self.path.push(PathCommand::Close);
    }
    pub fn clip(&mut self) {
        self.state.clips.push(Clip {
            path: self.path.clone(),
            transform: self.state.transform,
        });
    }
    fn emit(&mut self, stroke: bool) {
        let s = &self.state;
        self.commands.push(Draw {
            path: self.path.clone(),
            transform: s.transform,
            paint: if stroke {
                s.stroke.clone()
            } else {
                s.fill.clone()
            },
            alpha: s.alpha,
            additive: s.additive,
            stroke: stroke.then(|| Stroke {
                width: s.width,
                cap: s.cap.clone(),
                join: s.join.clone(),
                dash: s.dash.clone(),
            }),
            clips: s.clips.clone(),
        });
    }
    pub fn fill(&mut self) {
        self.emit(false);
    }
    pub fn stroke(&mut self) {
        self.emit(true);
    }
    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let old = std::mem::take(&mut self.path);
        self.move_to(x, y);
        self.line_to(x + w, y);
        self.line_to(x + w, y + h);
        self.line_to(x, y + h);
        self.close();
        self.fill();
        self.path = old;
    }
    pub fn linear(&self, coords: [f64; 4], stops: &[(f64, String)]) -> Paint {
        Paint::Linear {
            coords,
            transform: self.state.transform,
            stops: stops.to_vec(),
        }
    }
    pub fn radial(&self, coords: [f64; 6], stops: &[(f64, String)]) -> Paint {
        Paint::Radial {
            coords,
            transform: self.state.transform,
            stops: stops.to_vec(),
        }
    }
}
// rgb() in the original only parses hexadecimal. In particular shade("rgb(...)"...) parses NaN,
// whose JS bitwise conversion is zero. Do not replace this with general CSS color parsing.
pub fn source_rgb(hex: &str) -> [i32; 3] {
    let text = hex.get(1..).unwrap_or("");
    let prefix: String = text.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    let n = u64::from_str_radix(&prefix, 16).unwrap_or(0) as u32 as i32;
    [n >> 16, (n >> 8) & 255, n & 255]
}
pub fn shade(hex: &str, amt: f64) -> String {
    let t = if amt < 0. { 0. } else { 255. };
    let p = amt.abs();
    let rgb = source_rgb(hex).map(|v| ((t - v as f64) * p + v as f64 + 0.5).floor() as i32);
    format!("rgb({},{},{})", rgb[0], rgb[1], rgb[2])
}
pub fn hex_a(hex: &str, a: f64) -> String {
    let [r, g, b] = source_rgb(hex);
    format!("rgba({r},{g},{b},{a})")
}
pub fn ell(c: &mut Canvas, x: f64, y: f64, rx: f64, ry: f64) {
    c.begin();
    c.ellipse(x, y, rx.max(0.1), ry.max(0.1), 0., 0., TAU);
    c.fill();
}
pub fn tri(c: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
    c.begin();
    c.move_to(x1, y1);
    c.line_to(x2, y2);
    c.line_to(x3, y3);
    c.close();
    c.fill();
}
pub fn circle(c: &mut Canvas, x: f64, y: f64, r: f64) {
    c.begin();
    c.arc(x, y, r.max(0.1), 0., TAU);
    c.fill();
}
pub fn draw_star(c: &mut Canvas, x: f64, y: f64, r: f64, color: &str, rot: f64) {
    c.save();
    c.translate(x, y);
    c.rotate(rot);
    c.fill_style(color);
    c.begin();
    for i in 0..10 {
        let rr = if i % 2 != 0 { r * 0.45 } else { r };
        let a = -PI / 2. + i as f64 * PI / 5.;
        c.line_to(a.cos() * rr, a.sin() * rr);
    }
    c.close();
    c.fill();
    c.restore();
}
pub fn wing(
    c: &mut Canvas,
    s: f64,
    fill: &str,
    flap: f64,
    dx: f64,
    vein: Option<&str>,
    line: Option<&str>,
) {
    c.save();
    c.translate(-s * 0.02 + dx, -s * 0.2);
    c.rotate(-0.15 + (1. - flap) * 0.35);
    c.scale(1., 0.3 + 0.7 * flap);
    let tips = [
        [s * 0.06, -s * 0.75],
        [-s * 0.3, -s * 0.56],
        [-s * 0.55, -s * 0.3],
    ];
    let g = c.linear(
        [0., -s * 0.7, -s * 0.4, 0.],
        &[
            (
                0.,
                if vein.is_some() {
                    shade(fill, 0.25)
                } else {
                    fill.into()
                },
            ),
            (1., shade(fill, -0.18)),
        ],
    );
    c.fill_style(g);
    c.begin();
    c.move_to(0., 0.);
    c.line_to(tips[0][0], tips[0][1]);
    c.quad(-s * 0.08, -s * 0.48, tips[1][0], tips[1][1]);
    c.quad(-s * 0.32, -s * 0.3, tips[2][0], tips[2][1]);
    c.quad(-s * 0.35, -s * 0.1, -s * 0.32, s * 0.04);
    c.close();
    c.fill();
    if let Some(line) = line {
        c.stroke_style(line);
        c.width((s * 0.022).max(1.));
        c.join("round");
        c.stroke();
    }
    c.stroke_style(vein.map(str::to_owned).unwrap_or_else(|| shade(fill, 0.3)));
    c.multiply_alpha(0.7);
    c.width((s * 0.03).max(1.));
    c.cap("round");
    c.begin();
    for [x, y] in tips {
        c.move_to(0., 0.);
        c.line_to(x * 0.96, y * 0.96);
    }
    c.stroke();
    c.fill_style("#f4ecd8");
    for [x, y] in tips {
        c.begin();
        c.arc(x, y, (s * 0.022).max(0.8), 0., TAU);
        c.fill();
    }
    c.restore();
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiderArt {
    pub hair: String,
    pub skin: String,
    pub cloak: String,
    pub style: String,
    pub path: String,
    pub crown: bool,
    pub daddio: bool,
}
impl Default for RiderArt {
    fn default() -> Self {
        Self {
            hair: "#6b4423".into(),
            skin: "#e8b98a".into(),
            cloak: "#3a3f4f".into(),
            style: "short".into(),
            path: "shadow".into(),
            crown: false,
            daddio: false,
        }
    }
}
pub fn hair_back(c: &mut Canvas, cx: f64, cy: f64, r: f64, rd: &RiderArt) {
    c.fill_style(rd.hair.clone());
    match rd.style.as_str() {
        "long" => ell(c, cx - r * 0.55, cy + r * 0.55, r * 0.7, r * 1.3),
        "braid" => {
            for k in 0..4 {
                let k = k as f64;
                ell(
                    c,
                    cx - r * 0.85 - k * r * 0.1,
                    cy + r * 0.35 + k * r * 0.5,
                    r * 0.32,
                    r * 0.3,
                )
            }
        }
        "curly" => ell(c, cx - r * 0.25, cy - r * 0.05, r * 1.3, r * 1.2),
        _ => {}
    }
}
pub fn hair_top(c: &mut Canvas, cx: f64, cy: f64, r: f64, rd: &RiderArt) {
    if rd.style == "hood" {
        c.fill_style(rd.hair.clone());
        ell(c, cx - r * 0.72, cy + r * 0.56, r * 0.44, r * 0.8);
        ell(c, cx + r * 0.76, cy + r * 0.48, r * 0.32, r * 0.64);
        c.fill_style(rd.cloak.clone());
        c.begin();
        c.arc(cx, cy - r * 0.16, r * 1.2, PI * 0.95, PI * 2.05);
        c.close();
        c.fill();
        return;
    }
    c.fill_style(rd.hair.clone());
    c.begin();
    c.arc(cx, cy - r * 0.24, r * 1.04, PI * 1.02, PI * 1.98);
    c.close();
    c.fill();
    c.stroke_style("rgba(255,255,255,0.28)");
    c.width((r * 0.12).max(1.));
    c.cap("round");
    c.begin();
    c.arc(cx, cy - r * 0.24, r * 0.78, PI * 1.25, PI * 1.55);
    c.stroke();
    if rd.style == "curly" {
        for k in -2..=2 {
            ell(
                c,
                cx + k as f64 * r * 0.42,
                cy - r * 0.8 - if k != 0 { 0. } else { r * 0.12 },
                r * 0.36,
                r * 0.36,
            );
        }
    }
}
pub fn draw_gear(c: &mut Canvas, path: &str, x: f64, y: f64, u: f64, in_hand: bool) {
    c.cap("round");
    match path {
        "shadow" => {
            let rr = u * if in_hand { 1.9 } else { 1.5 };
            let cx = x - u * if in_hand { 0.2 } else { 0. };
            c.stroke_style("#8a5a2b");
            c.width(u * if in_hand { 0.35 } else { 0.28 });
            c.begin();
            c.arc(cx, y, rr, -1.25, 1.25);
            c.stroke();
            c.stroke_style("#e8dcc0");
            c.width(u * 0.12);
            c.begin();
            c.move_to(cx + (-1.25_f64).cos() * rr, y + (-1.25_f64).sin() * rr);
            c.line_to(cx + 1.25_f64.cos() * rr, y + 1.25_f64.sin() * rr);
            c.stroke();
        }
        "forge" => {
            c.stroke_style("#6b4423");
            c.width(u * 0.36);
            c.begin();
            c.move_to(x, y - u * 1.2);
            c.line_to(x - u * 0.3, y + u * 1.2);
            c.stroke();
            c.fill_style("#9aa3b3");
            c.fill_rect(x - u * 0.8, y - u * 1.6, u * 1.4, u * 0.8);
        }
        "storm" => {
            c.stroke_style("#c9b27a");
            c.width(u * 0.24);
            c.begin();
            c.move_to(x - u * 0.6, y + u * 1.6);
            c.line_to(x + u * 0.4, y - u * 2.2);
            c.stroke();
            c.fill_style("#fff27a");
            tri(
                c,
                x + u * 0.55,
                y - u * 2.9,
                x + u * 0.15,
                y - u * 2.1,
                x + u * 0.7,
                y - u * 2.0,
            );
        }
        _ => {
            c.stroke_style("#8a6a3b");
            c.width(u * 0.26);
            c.begin();
            c.move_to(x - u * 0.2, y + u * 1.8);
            c.line_to(x + u * 0.2, y - u * 2.0);
            c.stroke();
            c.fill_style("rgba(157,255,176,0.35)");
            ell(c, x + u * 0.25, y - u * 2.4, u * 0.8, u * 0.8);
            c.fill_style("#d6ffe0");
            ell(c, x + u * 0.25, y - u * 2.4, u * 0.4, u * 0.4);
        }
    }
}
pub fn draw_crown(c: &mut Canvas, x: f64, y: f64, w: f64) {
    c.fill_style("#ffcf3f");
    c.begin();
    c.move_to(x - w, y);
    c.line_to(x - w, y - w * 0.9);
    c.line_to(x - w * 0.5, y - w * 0.45);
    c.line_to(x, y - w * 1.1);
    c.line_to(x + w * 0.5, y - w * 0.45);
    c.line_to(x + w, y - w * 0.9);
    c.line_to(x + w, y);
    c.close();
    c.fill();
    c.fill_style("#ff5a8a");
    ell(c, x, y - w * 0.35, w * 0.17, w * 0.17);
}
pub fn draw_rider(c: &mut Canvas, s: f64, rd: &RiderArt, time: f64) {
    let u = s * 0.11;
    c.save();
    c.translate(-s * 0.06, -s * 0.34);
    c.fill_style(shade(&rd.cloak, -0.25));
    c.begin();
    c.move_to(-u * 0.5, -u * 2.4);
    c.quad(
        -u * 3.2,
        -u * 1.8 + (time * 6.).sin() * u * 0.3,
        -u * 3.4,
        u * 0.1,
    );
    c.line_to(-u * 0.4, u * 0.2);
    c.close();
    c.fill();
    draw_gear(c, &rd.path, -u * 0.9, -u * 1.5, u * 0.8, false);
    let body = c.linear(
        [0., -u * 2.6, 0., u * 0.2],
        &[(0., shade(&rd.cloak, 0.25)), (1., shade(&rd.cloak, -0.2))],
    );
    c.fill_style(body);
    c.begin();
    c.ellipse(0., -u * 1.2, u * 0.95, u * 1.45, -0.15, 0., TAU);
    c.fill();
    c.stroke_style("#1a1424");
    c.width((u * 0.12).max(1.));
    c.stroke();
    c.stroke_style(rd.skin.clone());
    c.width(u * 0.42);
    c.cap("round");
    c.begin();
    c.move_to(u * 0.3, -u * 1.7);
    c.line_to(u * 1.7, -u * 1.0);
    c.stroke();
    hair_back(c, u * 0.25, -u * 3.2, u * 0.86, rd);
    c.fill_style(rd.skin.clone());
    c.begin();
    c.ellipse(u * 0.25, -u * 3.2, u * 0.85, u * 0.9, 0., 0., TAU);
    c.fill();
    c.stroke_style("#1a1424");
    c.width((u * 0.12).max(1.));
    c.stroke();
    hair_top(c, u * 0.25, -u * 3.2, u * 0.86, rd);
    c.fill_style("rgba(255,120,130,0.35)");
    ell(c, u * 0.55, -u * 2.85, u * 0.2, u * 0.12);
    c.fill_style("#fff");
    ell(c, u * 0.62, -u * 3.15, u * 0.19, u * 0.21);
    c.fill_style("#3a2a1c");
    ell(c, u * 0.66, -u * 3.13, u * 0.12, u * 0.15);
    c.fill_style("#fff");
    ell(c, u * 0.7, -u * 3.2, u * 0.05, u * 0.05);
    if rd.daddio {
        c.fill_style("#14141c");
        ell(c, u * 0.62, -u * 3.15, u * 0.28, u * 0.2);
        c.fill_rect(u * 0.2, -u * 3.22, u * 0.3, u * 0.08);
    }
    if rd.crown {
        draw_crown(c, u * 0.25, -u * 3.9, u * 0.6);
    }
    c.restore();
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DragonArt {
    pub rider_time: Option<f64>,
    pub aura: bool,
    pub dad: bool,
    pub spin: f64,
    pub horn: Option<String>,
    pub eye: Option<String>,
    pub dizzy: bool,
    pub fire: bool,
    pub fire_cols: Option<[String; 3]>,
    pub rider: Option<RiderArt>,
}
fn lit(c: &Canvas, col: &str, x: f64, y: f64, r: f64) -> Paint {
    c.radial(
        [x - r * 0.35, y - r * 0.45, r * 0.08, x, y, r * 1.1],
        &[
            (0., shade(col, 0.3)),
            (0.55, col.into()),
            (1., shade(col, -0.22)),
        ],
    )
}
#[allow(clippy::too_many_arguments)]
fn blob(
    c: &mut Canvas,
    x: f64,
    y: f64,
    rx: f64,
    ry: f64,
    fill: impl Into<Paint>,
    line: &str,
    lw: f64,
) {
    c.fill_style(fill);
    ell(c, x, y, rx, ry);
    c.stroke_style(line);
    c.width(lw);
    c.stroke();
}
fn spike(c: &mut Canvas, points: [f64; 6], fill: &str, line: &str, lw: f64) {
    c.fill_style(fill);
    tri(
        c, points[0], points[1], points[2], points[3], points[4], points[5],
    );
    c.stroke_style(line);
    c.width(lw * 0.8);
    c.stroke();
}
/// Literal geometry and layer order of the original drawDragon, including its CSS shade quirk.
#[allow(clippy::too_many_arguments)]
pub fn draw_dragon(
    c: &mut Canvas,
    x: f64,
    y: f64,
    s: f64,
    col: &str,
    facing: f64,
    t: f64,
    o: &DragonArt,
) {
    let dark = shade(col, -0.32);
    let light = shade(col, 0.45);
    let line = shade(col, -0.6);
    let flap = 0.5 + 0.5 * (t * 9.).sin();
    let lw = (s * 0.024).max(1.);
    c.save();
    c.translate(x, y);
    if o.aura {
        let g = c.radial(
            [0., -s * 0.1, s * 0.2, 0., -s * 0.1, s * 1.4],
            &[(0., hex_a(col, 0.35)), (1., hex_a(col, 0.))],
        );
        c.fill_style(g);
        circle(c, 0., -s * 0.1, s * 1.4);
    }
    if o.dad {
        c.save();
        c.rotate(t * 0.15);
        for i in 0..12 {
            c.rotate(TAU / 12.);
            c.fill_style(format!(
                "rgba(255,215,110,{})",
                0.08 + 0.06 * (t * 2. + i as f64).sin()
            ));
            tri(c, -s * 0.09, -s * 0.1, 0., -s * 1.9, s * 0.09, -s * 0.1);
        }
        c.restore();
        let g = c.radial(
            [0., -s * 0.1, s * 0.15, 0., -s * 0.1, s * 1.5],
            &[
                (0., "rgba(255,225,140,0.45)".into()),
                (1., "rgba(255,200,90,0)".into()),
            ],
        );
        c.fill_style(g);
        circle(c, 0., -s * 0.1, s * 1.5);
    }
    if o.spin != 0. {
        c.rotate(o.spin.sin() * 0.45);
    }
    c.scale(facing, 1.);
    let gold = if o.dad { Some("#ffcf3f") } else { None };
    let horn = gold.or(o.horn.as_deref()).unwrap_or("#ffe7a6");
    let tw = (t * 3.).sin() * s * 0.1;
    let tx = -s;
    let ty = -s * 0.08 + tw * 1.4;
    c.cap("round");
    for (color, width) in [(&line as &str, s * 0.2 + lw * 2.), (col, s * 0.2)] {
        c.stroke_style(color);
        c.width(width);
        c.begin();
        c.move_to(-s * 0.3, s * 0.05);
        c.quad(-s * 0.75, s * 0.18 + tw, tx, ty);
        c.stroke();
    }
    c.stroke_style(shade(col, 0.22));
    c.width(s * 0.05);
    c.begin();
    c.move_to(-s * 0.34, 0.);
    c.quad(-s * 0.72, s * 0.11 + tw, tx + s * 0.04, ty - s * 0.05);
    c.stroke();
    spike(
        c,
        [
            tx + s * 0.05,
            ty - s * 0.04,
            tx - s * 0.24,
            ty - s * 0.17,
            tx - s * 0.1,
            ty + s * 0.15,
        ],
        gold.unwrap_or(&dark),
        &line,
        lw,
    );
    wing(c, s, &dark, flap * 0.9 + 0.05, s * 0.14, None, Some(&line));
    for lx in [-0.16, 0.17] {
        blob(
            c,
            lx * s,
            s * 0.27,
            s * 0.09,
            s * 0.12,
            dark.clone(),
            &line,
            lw,
        );
        c.fill_style("#f4ecd8");
        for k in [-1., 0., 1.] {
            tri(
                c,
                (lx + k * 0.04) * s - s * 0.012,
                s * 0.37,
                (lx + k * 0.04) * s,
                s * 0.42,
                (lx + k * 0.04) * s + s * 0.012,
                s * 0.37,
            );
        }
    }
    for sx in [-0.32, -0.12, 0.08] {
        spike(
            c,
            [
                (sx - 0.08) * s,
                -s * 0.22,
                sx * s,
                -s * 0.45,
                (sx + 0.08) * s,
                -s * 0.25,
            ],
            gold.unwrap_or(&shade(col, -0.18)),
            &line,
            lw,
        );
    }
    let g = lit(c, col, 0., 0., s * 0.48);
    blob(c, 0., 0., s * 0.48, s * 0.31, g, &line, lw);
    c.fill_style(if gold.is_some() { "#ffe9a0" } else { &light });
    ell(c, s * 0.06, s * 0.1, s * 0.3, s * 0.16);
    c.stroke_style("rgba(40,20,60,0.13)");
    c.width((s * 0.016).max(1.));
    for bx in [-0.08, 0.06, 0.2] {
        c.begin();
        c.move_to(bx * s, s * 0.03);
        c.quad((bx + 0.025) * s, s * 0.11, bx * s, s * 0.2);
        c.stroke();
    }
    c.stroke_style("rgba(255,255,255,0.35)");
    c.width((s * 0.035).max(1.));
    c.begin();
    c.ellipse(0., 0., s * 0.4, s * 0.23, 0., PI * 1.15, PI * 1.6);
    c.stroke();
    if o.dad {
        c.save();
        c.begin();
        c.ellipse(0., 0., s * 0.48, s * 0.31, 0., 0., TAU);
        c.clip();
        let gx = ((t * 0.35) % 2. - 1.) * s * 0.9;
        c.fill_style("rgba(255,255,255,0.32)");
        c.begin();
        c.move_to(gx - s * 0.05, -s * 0.35);
        c.line_to(gx + s * 0.07, -s * 0.35);
        c.line_to(gx - s * 0.05, s * 0.35);
        c.line_to(gx - s * 0.17, s * 0.35);
        c.close();
        c.fill();
        c.restore();
    }
    c.fill_style(col);
    c.begin();
    c.ellipse(s * 0.33, -s * 0.17, s * 0.17, s * 0.15, -0.6, 0., TAU);
    c.fill();
    for [hx, hy, len] in [[0.36, -0.44, 0.24], [0.5, -0.5, 0.24]] {
        let g = c.linear(
            [hx * s, hy * s, (hx - 0.06) * s, (hy - len) * s],
            &[(0., shade(horn, -0.2)), (1., shade(horn, 0.25))],
        );
        c.fill_style(g);
        c.begin();
        c.move_to((hx - 0.04) * s, hy * s);
        c.quad(
            (hx - 0.1) * s,
            (hy - len * 0.6) * s,
            (hx - 0.07) * s,
            (hy - len) * s,
        );
        c.quad(
            (hx + 0.03) * s,
            (hy - len * 0.5) * s,
            (hx + 0.08) * s,
            (hy + 0.02) * s,
        );
        c.close();
        c.fill();
        c.stroke_style(line.clone());
        c.width(lw * 0.8);
        c.stroke();
    }
    let g = lit(c, col, s * 0.5, -s * 0.3, s * 0.25);
    blob(c, s * 0.5, -s * 0.3, s * 0.25, s * 0.22, g, &line, lw);
    let g = lit(c, col, s * 0.72, -s * 0.22, s * 0.17);
    blob(c, s * 0.72, -s * 0.22, s * 0.17, s * 0.12, g, &line, lw);
    c.fill_style(col);
    ell(c, s * 0.6, -s * 0.25, s * 0.13, s * 0.1);
    c.fill_style("rgba(255,140,170,0.55)");
    ell(c, s * 0.52, -s * 0.18, s * 0.065, s * 0.045);
    c.fill_style(line.clone());
    ell(c, s * 0.83, -s * 0.27, s * 0.025, s * 0.02);
    c.stroke_style(line.clone());
    c.width((s * 0.03).max(1.2));
    c.cap("round");
    c.begin();
    c.arc(s * 0.72, -s * 0.2, s * 0.08, PI * 0.15, PI * 0.75);
    c.stroke();
    let ex = s * 0.55;
    let ey = -s * 0.34;
    c.fill_style("#fff");
    ell(c, ex, ey, s * 0.1, s * 0.105);
    c.stroke_style(line.clone());
    c.width(lw * 0.8);
    c.stroke();
    if o.dizzy {
        c.stroke_style("#1b1530");
        c.width((s * 0.025).max(1.2));
        c.begin();
        let mut a: f64 = 0.;
        while a < 12. {
            let rr = a * s * 0.0075;
            let px = ex + (a + t * 10.).cos() * rr;
            let py = ey + (a + t * 10.).sin() * rr;
            if a == 0. {
                c.move_to(px, py)
            } else {
                c.line_to(px, py)
            }
            a += 0.4;
        }
        c.stroke();
    } else if o.dad {
        c.fill_style("#14141c");
        c.begin();
        c.move_to(ex - s * 0.13, ey - s * 0.07);
        c.line_to(ex + s * 0.15, ey - s * 0.07);
        c.quad(ex + s * 0.15, ey + s * 0.09, ex + s * 0.01, ey + s * 0.09);
        c.quad(ex - s * 0.13, ey + s * 0.08, ex - s * 0.13, ey - s * 0.07);
        c.fill();
        c.stroke_style("#14141c");
        c.width((s * 0.035).max(1.2));
        c.begin();
        c.move_to(ex - s * 0.12, ey - s * 0.05);
        c.line_to(ex - s * 0.33, ey - s * 0.11);
        c.stroke();
        c.stroke_style("rgba(255,255,255,0.85)");
        c.width((s * 0.022).max(1.));
        c.begin();
        c.move_to(ex + s * 0.02, ey - s * 0.04);
        c.line_to(ex + s * 0.09, ey - s * 0.04 + s * 0.06);
        c.stroke();
    } else {
        let iris = o.eye.clone().unwrap_or_else(|| shade(col, -0.45));
        let g = c.radial(
            [
                ex + s * 0.03,
                ey + s * 0.02,
                s * 0.01,
                ex + s * 0.03,
                ey + s * 0.01,
                s * 0.07,
            ],
            &[(0., shade(&iris, 0.45)), (1., iris)],
        );
        c.fill_style(g);
        ell(c, ex + s * 0.03, ey + s * 0.01, s * 0.065, s * 0.075);
        c.fill_style("#120c20");
        ell(c, ex + s * 0.035, ey + s * 0.01, s * 0.022, s * 0.055);
        c.fill_style("#fff");
        ell(c, ex + s * 0.055, ey - s * 0.028, s * 0.024, s * 0.024);
        ell(c, ex + s * 0.01, ey + s * 0.045, s * 0.011, s * 0.011);
        c.stroke_style(line.clone());
        c.width((s * 0.022).max(1.));
        c.begin();
        c.arc(ex, ey + s * 0.02, s * 0.115, PI * 1.15, PI * 1.75);
        c.stroke();
    }
    if o.fire {
        let defaults = ["#fff6c2".into(), "#ffc247".into(), "#ff5a1f".into()];
        let fc = o.fire_cols.as_ref().unwrap_or(&defaults);
        let safe = if fc[2].len() == 7
            && fc[2].starts_with('#')
            && fc[2][1..].chars().all(|c| c.is_ascii_hexdigit())
        {
            &fc[2]
        } else {
            "#ff5a1f"
        };
        let g = c.linear(
            [s * 0.85, 0., s * 1.45, 0.],
            &[
                (0., fc[0].clone()),
                (0.4, fc[1].clone()),
                (1., hex_a(safe, 0.)),
            ],
        );
        c.fill_style(g);
        c.begin();
        c.move_to(s * 0.84, -s * 0.27);
        c.quad(s * 1.2, -s * 0.44, s * 1.48, -s * 0.22);
        c.quad(s * 1.2, -s * 0.01, s * 0.84, -s * 0.16);
        c.close();
        c.fill();
    }
    wing(c, s, col, flap, 0., Some(&light), Some(&line));
    if let Some(rd) = &o.rider {
        draw_rider(c, s, rd, o.rider_time.unwrap_or(t));
    }
    c.restore();
}

/// Presentation-only enemy inputs. No simulation state is changed by drawing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnemyArt {
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub dir: f64,
    pub t: f64,
    pub wob: f64,
    pub flash: f64,
    pub calm: bool,
    pub slow: f64,
    pub windup: f64,
    pub stun: f64,
    pub air: bool,
    pub vy: f64,
    pub hop: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SceneArt {
    pub player: [f64; 2],
    pub ground: f64,
    pub scale: f64,
    pub time: f64,
}
fn body_color<'a>(e: &EnemyArt, base: &'a str, calm: &'a str) -> &'a str {
    if e.flash > 0. {
        "#ffffff"
    } else if e.calm {
        calm
    } else if e.slow > 0. {
        "#bfe9ff"
    } else {
        base
    }
}
#[allow(clippy::too_many_arguments)]
fn blob_face(
    c: &mut Canvas,
    e: &EnemyArt,
    scene: &SceneArt,
    r: f64,
    ex: f64,
    ey: f64,
    er: f64,
    my: f64,
) {
    let dx = scene.player[0] - e.x;
    let dy = scene.player[1] - e.y;
    let d = dx.hypot(dy);
    let d = if d == 0. { 1. } else { d };
    let lx = dx / d * e.dir;
    let ly = dy / d;
    for sx in [-ex, ex] {
        c.fill_style("#fff");
        ell(c, sx * r, ey * r, er * r, er * r * 1.1);
        c.fill_style("#1b1530");
        ell(
            c,
            sx * r + lx * er * r * 0.4,
            ey * r + ly * er * r * 0.4,
            er * r * 0.55,
            er * r * 0.6,
        );
    }
    c.stroke_style("#1b1530");
    c.width((r * 0.07).max(1.5));
    c.cap("round");
    c.begin();
    if e.calm {
        c.arc(0., my * r - r * 0.12, r * 0.16, PI * 0.15, PI * 0.85);
    } else {
        c.arc(0., my * r + r * 0.06, r * 0.14, PI * 1.15, PI * 1.85);
    }
    c.stroke();
}
fn draw_cloud(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    let r = e.r;
    let big = e.kind == "bigcloud" || e.kind == "thunderhead";
    let boss = e.kind == "thunderhead";
    let dx = scene.player[0] - e.x;
    let dy = scene.player[1] - e.y;
    let d = dx.hypot(dy);
    let d = if d == 0. { 1. } else { d };
    let lx = dx / d;
    let ly = dy / d;
    c.save();
    c.translate(e.x, e.y);
    if e.calm {
        c.alpha(0.75);
    }
    if boss {
        let g = c.radial(
            [0., 0., r * 0.5, 0., 0., r * 1.6],
            &[
                (0., "rgba(255,215,110,0.18)".into()),
                (1., "rgba(255,215,110,0)".into()),
            ],
        );
        c.fill_style(g);
        circle(c, 0., 0., r * 1.6);
    }
    let p5 = [
        [-0.55, 0.12, 0.46],
        [-0.18, -0.22, 0.56],
        [0.3, -0.14, 0.52],
        [0.6, 0.16, 0.4],
        [0.02, 0.22, 0.52],
    ];
    c.fill_style(if boss {
        "#2e2f4e"
    } else if big {
        "#45476a"
    } else {
        "#6c7298"
    });
    for [x, y, rr] in p5 {
        circle(c, x * r, y * r + r * 0.09, rr * r);
    }
    let top = body_color(
        e,
        if boss {
            "#6d6f98"
        } else if big {
            "#9a9cc0"
        } else {
            "#e3e6f7"
        },
        "#f4f6ff",
    );
    let bot = if e.flash > 0. {
        "#ffffff"
    } else if e.calm {
        "#c9d0ee"
    } else if e.slow > 0. {
        "#8fcbe8"
    } else if boss {
        "#3c3e64"
    } else if big {
        "#5c5f84"
    } else {
        "#a5accf"
    };
    let g = c.linear(
        [0., -r * 0.8, 0., r * 0.6],
        &[(0., top.into()), (1., bot.into())],
    );
    c.fill_style(g);
    for [x, y, rr] in p5 {
        circle(c, x * r, y * r, rr * r);
    }
    c.stroke_style("#2a2b4a");
    c.width(r * 0.07);
    c.cap("round");
    if e.calm {
        for ex in [-0.2, 0.18] {
            c.begin();
            c.arc(ex * r, 0., r * 0.09, PI * 1.1, PI * 1.9);
            c.stroke();
        }
        c.begin();
        c.arc(-0.01 * r, 0.16 * r, r * 0.13, PI * 0.15, PI * 0.85);
        c.stroke();
    } else {
        for ex in [-0.2, 0.18] {
            c.fill_style(if boss { "#ffd76e" } else { "#fff" });
            circle(c, ex * r, 0., r * 0.12);
            c.fill_style("#2a2b4a");
            circle(c, ex * r + lx * r * 0.05, ly * r * 0.05, r * 0.065);
        }
        c.begin();
        c.move_to(-0.36 * r, -0.24 * r);
        c.line_to(-0.08 * r, -0.13 * r);
        c.move_to(0.34 * r, -0.24 * r);
        c.line_to(0.06 * r, -0.13 * r);
        c.stroke();
        c.begin();
        c.arc(-0.01 * r, 0.38 * r, r * 0.13, PI * 1.15, PI * 1.85);
        c.stroke();
        if big && (e.t * 5. + e.wob).sin() > 0.75 {
            c.stroke_style("#ffd76e");
            c.width(r * 0.05);
            c.begin();
            c.move_to(0.1 * r, 0.62 * r);
            c.line_to(-0.06 * r, 0.86 * r);
            c.line_to(0.12 * r, 0.9 * r);
            c.line_to(-0.03 * r, 1.18 * r);
            c.stroke();
        }
    }
    c.restore();
}
fn draw_bat(c: &mut Canvas, e: &EnemyArt) {
    let r = e.r;
    let f = (e.t * 18.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    c.fill_style(body_color(e, "#3b2e5e", "#9b8fc8"));
    for side in [-1., 1.] {
        c.begin();
        c.move_to(0., -r * 0.1);
        c.line_to(side * r * 1.7, -r * (0.3 + f * 0.8));
        c.line_to(side * r * 1.2, r * 0.25);
        c.line_to(side * r * 0.7, -r * 0.05);
        c.close();
        c.fill();
    }
    ell(c, 0., 0., r * 0.55, r * 0.6);
    tri(
        c,
        -r * 0.35,
        -r * 0.4,
        -r * 0.2,
        -r * 0.95,
        -r * 0.05,
        -r * 0.45,
    );
    tri(
        c,
        r * 0.35,
        -r * 0.4,
        r * 0.2,
        -r * 0.95,
        r * 0.05,
        -r * 0.45,
    );
    c.fill_style(if e.calm { "#fff3d6" } else { "#ffd76e" });
    ell(c, -r * 0.18, -r * 0.1, r * 0.11, r * 0.11);
    ell(c, r * 0.18, -r * 0.1, r * 0.11, r * 0.11);
    c.restore();
}
fn draw_sprite(c: &mut Canvas, e: &EnemyArt) {
    let r = e.r;
    c.save();
    c.translate(e.x, e.y);
    c.additive(true);
    let g = c.radial(
        [0., 0., 0., 0., 0., r * 2.2],
        &[
            (
                0.,
                if e.calm {
                    "rgba(200,255,240,0.6)"
                } else {
                    "rgba(255,240,120,0.7)"
                }
                .into(),
            ),
            (1., "rgba(255,240,120,0)".into()),
        ],
    );
    c.fill_style(g);
    circle(c, 0., 0., r * 2.2);
    c.additive(false);
    draw_star(
        c,
        0.,
        0.,
        r * 1.05,
        body_color(e, "#fff27a", "#c9fff0"),
        e.t * 3.,
    );
    c.fill_style("#ffffff");
    ell(c, 0., 0., r * 0.55, r * 0.55);
    c.fill_style("#2a2b4a");
    ell(c, -r * 0.18, -r * 0.05, r * 0.08, r * 0.1);
    ell(c, r * 0.18, -r * 0.05, r * 0.08, r * 0.1);
    c.restore();
}
fn draw_urgal(c: &mut Canvas, e: &EnemyArt) {
    let chief = e.kind == "chieftain";
    let r = e.r;
    let step = (e.t * 8.).sin();
    c.save();
    c.translate(e.x, e.y - step.abs() * r * 0.06);
    c.scale(e.dir, 1.);
    let skin = body_color(e, if chief { "#7d6a5a" } else { "#6f8a5a" }, "#a9bfa0");
    let dark = if chief { "#4e4036" } else { "#465a38" };
    if chief {
        c.fill_style("#9b2f3a");
        c.begin();
        c.move_to(-r * 0.4, -r * 0.6);
        c.quad(-r * 1.3, r * 0.2, -r * 0.9, r * 0.9);
        c.line_to(-r * 0.1, r * 0.4);
        c.close();
        c.fill();
    }
    c.fill_style(dark);
    c.fill_rect(-r * 0.4 + step * r * 0.1, r * 0.45, r * 0.28, r * 0.5);
    c.fill_rect(r * 0.12 - step * r * 0.1, r * 0.45, r * 0.28, r * 0.5);
    c.fill_style(skin);
    ell(c, 0., 0., r * 0.72, r * 0.8);
    c.fill_style("#7a5a3a");
    c.fill_rect(-r * 0.5, -r * 0.2, r, r * 0.45);
    c.fill_style("#9a7a52");
    c.fill_rect(-r * 0.5, -r * 0.2, r, r * 0.08);
    c.fill_style(skin);
    ell(c, r * 0.18, -r * 0.92, r * 0.46, r * 0.4);
    c.stroke_style(if chief { "#ffe7a6" } else { "#e8dcc0" });
    c.width(r * 0.14);
    c.cap("round");
    c.begin();
    c.move_to(-r * 0.12, -r * 1.12);
    c.quad(
        -r * 0.55,
        -r * 1.35,
        -r * 0.35,
        -r * if chief { 1.85 } else { 1.6 },
    );
    c.stroke();
    c.begin();
    c.move_to(r * 0.42, -r * 1.15);
    c.quad(
        r * 0.75,
        -r * 1.4,
        r * 0.6,
        -r * if chief { 1.85 } else { 1.6 },
    );
    c.stroke();
    if e.calm {
        c.stroke_style("#2a2b2a");
        c.width(r * 0.06);
        c.begin();
        c.arc(r * 0.32, -r * 0.95, r * 0.07, PI, TAU);
        c.stroke();
    } else {
        c.fill_style("#ffd76e");
        ell(c, r * 0.32, -r * 0.95, r * 0.08, r * 0.08);
        c.stroke_style("#2a2b2a");
        c.width(r * 0.06);
        c.begin();
        c.move_to(r * 0.18, -r * 1.12);
        c.line_to(r * 0.45, -r * 1.02);
        c.stroke();
    }
    c.fill_style("#fff8e6");
    tri(
        c,
        r * 0.42,
        -r * 0.72,
        r * 0.48,
        -r * 0.88,
        r * 0.54,
        -r * 0.72,
    );
    if e.windup > 0. {
        c.fill_style("#8f8f9c");
        ell(c, r * 0.1, -r * 1.7, r * 0.3, r * 0.26);
    }
    c.restore();
}
fn draw_wolf(c: &mut Canvas, e: &EnemyArt) {
    let r = e.r;
    let run = (e.t * 16.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    let fur = body_color(e, "#3e5a5a", "#9fc4c4");
    c.stroke_style("#24393a");
    c.width(r * 0.16);
    c.cap("round");
    c.begin();
    c.move_to(-r * 0.5, r * 0.2);
    c.line_to(-r * 0.6 - run * r * 0.2, r * 0.75);
    c.move_to(r * 0.45, r * 0.2);
    c.line_to(r * 0.55 + run * r * 0.2, r * 0.75);
    c.stroke();
    c.fill_style(fur);
    ell(c, 0., 0., r * 0.85, r * 0.42);
    c.stroke_style(fur);
    c.width(r * 0.2);
    c.begin();
    c.move_to(-r * 0.75, -r * 0.05);
    c.quad(-r * 1.3, -r * 0.4, -r * 1.25, -r * 0.7);
    c.stroke();
    c.fill_style("#5f8a3a");
    for sx in [-0.45, -0.15, 0.15] {
        tri(
            c,
            (sx - 0.1) * r,
            -r * 0.32,
            sx * r,
            -r * 0.68,
            (sx + 0.1) * r,
            -r * 0.35,
        );
    }
    c.fill_style(fur);
    ell(c, r * 0.78, -r * 0.25, r * 0.38, r * 0.3);
    ell(c, r * 1.08, -r * 0.18, r * 0.2, r * 0.14);
    tri(
        c,
        r * 0.6,
        -r * 0.45,
        r * 0.68,
        -r * 0.85,
        r * 0.8,
        -r * 0.48,
    );
    c.fill_style(if e.calm { "#fff3d6" } else { "#ffd76e" });
    ell(c, r * 0.86, -r * 0.3, r * 0.07, r * 0.07);
    c.restore();
}
fn draw_troll(c: &mut Canvas, e: &EnemyArt) {
    let cave = e.kind == "cavetroll";
    let r = e.r;
    let step = (e.t * 4.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    let rock = body_color(e, if cave { "#6a6274" } else { "#7a6e60" }, "#b9b3c6");
    let dark = if cave { "#463f50" } else { "#54493e" };
    c.fill_style(dark);
    ell(c, -r * 0.35 + step * r * 0.08, r * 0.72, r * 0.28, r * 0.22);
    ell(c, r * 0.35 - step * r * 0.08, r * 0.72, r * 0.28, r * 0.22);
    c.fill_style(rock);
    for [x, y, rr] in [
        [0., 0., 0.8],
        [-0.45, -0.3, 0.45],
        [0.45, -0.25, 0.5],
        [0., -0.6, 0.5],
    ] {
        ell(c, x * r, y * r, rr * r, rr * r * 0.9);
    }
    c.fill_style("#5f8a3a");
    ell(c, -r * 0.3, -r * 0.75, r * 0.22, r * 0.1);
    ell(c, r * 0.4, -r * 0.5, r * 0.15, r * 0.08);
    if cave {
        c.fill_style("#cfd6e6");
        for sx in [-0.5, -0.2, 0.1, 0.4] {
            tri(
                c,
                (sx - 0.1) * r,
                -r * 0.85,
                sx * r,
                -r * 1.35,
                (sx + 0.1) * r,
                -r * 0.85,
            );
        }
    }
    c.fill_style(rock);
    ell(c, r * 0.5, -r * 0.82, r * 0.3, r * 0.26);
    c.fill_style(if e.calm {
        "#fff3d6"
    } else if cave {
        "#9fe0ff"
    } else {
        "#ffd76e"
    });
    ell(c, r * 0.6, -r * 0.86, r * 0.07, r * 0.07);
    c.stroke_style(dark);
    c.width(r * 0.22);
    c.cap("round");
    c.begin();
    c.move_to(r * 0.55, -r * 0.2);
    c.line_to(r * 0.95, if e.windup > 0. { -r } else { r * 0.25 });
    c.stroke();
    if e.windup > 0. {
        c.fill_style("#8a7a6a");
        ell(c, r, -r * 1.25, r * 0.32, r * 0.3);
    }
    c.restore();
}
fn draw_jelly(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    let r = e.r;
    let p = (e.t * 5. + e.wob).sin() * 0.08;
    c.save();
    c.translate(e.x, e.y);
    c.additive(true);
    let g = c.radial(
        [0., 0., 0., 0., 0., r * 2.],
        &[
            (
                0.,
                if e.calm {
                    "rgba(210,255,245,0.3)"
                } else {
                    "rgba(110,255,230,0.4)"
                }
                .into(),
            ),
            (1., "rgba(110,255,230,0)".into()),
        ],
    );
    c.fill_style(g);
    circle(c, 0., 0., r * 2.);
    c.additive(false);
    c.stroke_style(if e.calm {
        "rgba(225,255,248,0.75)"
    } else {
        "rgba(150,240,255,0.8)"
    });
    c.width(r * 0.1);
    c.cap("round");
    for i in -2..=2 {
        let i = i as f64;
        c.begin();
        c.move_to(i * r * 0.28, r * 0.15);
        for k in 1..=4 {
            let k = k as f64;
            c.line_to(
                i * r * 0.28 + (e.t * 4. + i + k).sin() * r * 0.13,
                r * 0.15 + k * r * 0.27,
            );
        }
        c.stroke();
    }
    c.fill_style(body_color(e, "#5fe0d0", "#d2fff5"));
    c.alpha(0.9);
    c.begin();
    c.ellipse(0., r * 0.15, r * (1. + p), r * (0.95 - p), 0., PI, TAU);
    c.quad(0., r * 0.5, -r * (1. + p), r * 0.15);
    c.close();
    c.fill();
    c.alpha(1.);
    c.fill_style("rgba(255,255,255,0.35)");
    ell(c, -r * 0.4, -r * 0.35, r * 0.18, r * 0.12);
    if !e.calm {
        draw_star(c, r * 0.42, -r * 0.42, r * 0.17, "#fff6c2", e.t * 2.);
    }
    blob_face(c, e, scene, r, 0.28, -0.12, 0.13, 0.12);
    c.restore();
}
fn draw_slime(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    let r = e.r;
    let big = e.kind == "bigslime";
    let sq = if e.air {
        0.85
    } else {
        1. + (e.t * 8. + e.wob).sin() * 0.07
    };
    let w = r * sq;
    let h = r * (2. - sq) * 0.9;
    c.save();
    c.translate(e.x, e.y + r * 0.95);
    c.additive(true);
    let g = c.radial(
        [0., -h, 0., 0., -h, r * 1.8],
        &[
            (0., "rgba(255,140,60,0.35)".into()),
            (1., "rgba(255,140,60,0)".into()),
        ],
    );
    c.fill_style(g);
    circle(c, 0., -h, r * 1.8);
    c.additive(false);
    let g = c.linear(
        [0., -h * 2., 0., 0.],
        &[
            (0., body_color(e, "#ffc24a", "#ffe6b8").into()),
            (
                1.,
                if e.flash > 0. {
                    "#ffffff"
                } else if e.calm {
                    "#ffb88a"
                } else if e.slow > 0. {
                    "#9fd8f0"
                } else {
                    "#e0461c"
                }
                .into(),
            ),
        ],
    );
    c.fill_style(g);
    c.begin();
    c.move_to(-w, 0.);
    c.quad(-w * 1.05, -h * 1.7, 0., -h * 1.95);
    c.quad(w * 1.05, -h * 1.7, w, 0.);
    c.close();
    c.fill();
    c.fill_style("rgba(90,30,20,0.45)");
    ell(c, -w * 0.45, -h * 0.45, w * 0.16, h * 0.1);
    ell(c, w * 0.5, -h * 0.7, w * 0.12, h * 0.08);
    c.fill_style("rgba(255,240,180,0.6)");
    for i in 0..3 {
        let i = i as f64;
        circle(
            c,
            (i * 2.1 + e.wob).sin() * w * 0.5,
            -((e.t * 0.6 + i / 3.) % 1.) * h * 1.6,
            r * 0.07,
        );
    }
    if big {
        draw_star(c, w * 0.05, -h * 0.75, r * 0.28, "#fff6c2", e.t);
    }
    c.translate(0., -h * 1.15);
    blob_face(
        c,
        e,
        scene,
        r,
        0.3,
        -0.05,
        if big { 0.13 } else { 0.16 },
        0.22,
    );
    c.restore();
}
fn draw_leaper(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    let r = e.r;
    if !e.air && !e.calm {
        let k = 0.5 + 0.5 * (e.t * 6.).sin();
        c.stroke_style("rgba(200,255,250,0.65)");
        c.width(2.);
        c.begin();
        c.ellipse(
            e.x,
            scene.ground + 3.,
            r * (1.2 + k * 0.4),
            r * 0.22,
            0.,
            0.,
            TAU,
        );
        c.stroke();
        c.fill_style(body_color(e, "#2f8fb8", "#a8e8f0"));
        tri(
            c,
            e.x - r * 0.35,
            scene.ground + 2.,
            e.x + r * 0.15 * e.dir,
            scene.ground - r * 0.65,
            e.x + r * 0.4,
            scene.ground + 2.,
        );
        if e.windup > 0. {
            c.fill_style("rgba(235,255,255,0.85)");
            for i in 0..4 {
                let i = i as f64;
                circle(
                    c,
                    e.x + (scene.time * 20. + i * 2.).sin() * r * 0.7,
                    scene.ground - (scene.time * 14. + i).sin().abs() * r * 0.6,
                    r * 0.1,
                );
            }
        }
        return;
    }
    let flap = (e.t * 22.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    c.rotate((e.vy / (scene.scale * 10.)).clamp(-0.7, 0.7));
    c.fill_style(body_color(e, "#7fe0ff", "#d8fbff"));
    for side in [-1., 1.] {
        tri(
            c,
            -r * 0.1,
            0.,
            -r * 0.75,
            side * r * (0.9 + flap * 0.25),
            r * 0.25,
            side * r * 0.12,
        );
    }
    c.fill_style(body_color(e, "#2f8fb8", "#a8e8f0"));
    tri(c, -r * 0.85, 0., -r * 1.35, -r * 0.45, -r * 1.35, r * 0.45);
    ell(c, 0., 0., r * 0.95, r * 0.5);
    c.fill_style(if e.calm { "#f0ffff" } else { "#bdf3ff" });
    ell(c, r * 0.1, r * 0.18, r * 0.6, r * 0.22);
    c.fill_style("#fff");
    ell(c, r * 0.5, -r * 0.12, r * 0.2, r * 0.2);
    c.fill_style("#1b1530");
    ell(c, r * 0.56, -r * 0.1, r * 0.11, r * 0.12);
    c.stroke_style("#1b1530");
    c.width((r * 0.07).max(1.2));
    c.begin();
    if e.calm {
        c.arc(r * 0.72, r * 0.08, r * 0.12, PI * 0.1, PI * 0.8);
    } else {
        c.move_to(r * 0.35, -r * 0.36);
        c.line_to(r * 0.68, -r * 0.28);
    }
    c.stroke();
    c.restore();
}
fn draw_serpent(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    let r = e.r;
    let dir = e.dir;
    let body = body_color(e, "#3a8f6a", "#a9dcc4");
    let dark = "#25624a";
    let fin = "#5fe0d0";
    for i in 1..=3 {
        let i = i as f64;
        let hx = e.x - dir * r * (0.6 + i * 1.05);
        let lift = r * (0.42 - i * 0.06) * (0.8 + 0.2 * (e.t * 2. + i).sin());
        c.fill_style(body);
        c.begin();
        c.ellipse(hx, scene.ground + 2., r * 0.48, lift, 0., PI, TAU);
        c.fill();
        c.fill_style(fin);
        tri(
            c,
            hx - r * 0.12,
            scene.ground - lift * 0.8,
            hx,
            scene.ground - lift - r * 0.22,
            hx + r * 0.12,
            scene.ground - lift * 0.8,
        );
    }
    c.stroke_style(body);
    c.width(r * 0.55);
    c.cap("round");
    c.begin();
    c.move_to(e.x - dir * r * 0.45, scene.ground + r * 0.3);
    c.quad(
        e.x - dir * r * 0.75,
        e.y + r * 0.5,
        e.x - dir * r * 0.1,
        e.y + r * 0.1,
    );
    c.stroke();
    c.save();
    c.translate(e.x, e.y);
    c.scale(dir, 1.);
    c.fill_style(fin);
    for [a, l] in [[-2.2_f64, 0.7], [-1.8, 0.85], [-1.4, 0.7]] {
        tri(
            c,
            a.cos() * r * 0.45,
            a.sin() * r * 0.4,
            (a - 0.25).cos() * r * l * 1.3,
            (a - 0.25).sin() * r * l * 1.1,
            (a + 0.2).cos() * r * 0.5,
            (a + 0.2).sin() * r * 0.45,
        );
    }
    c.fill_style(body);
    ell(c, 0., 0., r * 0.72, r * 0.52);
    ell(c, r * 0.5, r * 0.12, r * 0.42, r * 0.3);
    c.fill_style("#bfeec8");
    ell(c, r * 0.42, r * 0.3, r * 0.4, r * 0.13);
    let open = if e.windup > 0. {
        0.6
    } else {
        (e.t * 2.4).sin().max(0.) * 0.25
    };
    c.fill_style("#5a1f3a");
    ell(c, r * 0.72, r * 0.25, r * 0.2, r * (0.05 + open * 0.3));
    c.stroke_style(dark);
    c.width(r * 0.035);
    for [y, k] in [[0.05, 1.], [0.15, -1.]] {
        c.begin();
        c.move_to(r * 0.85, r * y);
        c.quad(r * 1.15, r * (y + 0.05 * k), r * 1.25, r * (y + 0.28));
        c.stroke();
    }
    c.fill_style(dark);
    ell(c, r * 0.86, -r * 0.02, r * 0.035, r * 0.025);
    c.fill_style("#ffffff");
    ell(c, r * 0.25, -r * 0.18, r * 0.17, r * 0.17);
    c.fill_style("#1b1530");
    ell(c, r * 0.3, -r * 0.16, r * 0.09, r * 0.1);
    c.stroke_style(dark);
    c.width(r * 0.07);
    c.begin();
    c.move_to(r * 0.06, -r * 0.42);
    c.line_to(r * 0.44, -r * 0.3);
    c.stroke();
    c.restore();
}
fn draw_imp(c: &mut Canvas, e: &EnemyArt) {
    let r = e.r;
    let flap = (e.t * 20.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    c.fill_style(body_color(e, "#8a2a1a", "#d8a898"));
    for side in [-1., 1.] {
        tri(
            c,
            -r * 0.2,
            -r * 0.1,
            -r * 0.4 + side * r * 0.2,
            -r * (0.6 + flap * 0.5) * 1.4,
            -r * 1.1,
            -r * 0.15 + side * r * 0.05,
        );
    }
    c.additive(true);
    c.fill_style(if e.calm {
        "rgba(255,220,180,0.5)"
    } else {
        "rgba(255,170,60,0.75)"
    });
    for i in 0..3 {
        let i = i as f64;
        tri(
            c,
            (-0.3 + i * 0.3) * r,
            -r * 0.6,
            (-0.15 + i * 0.3) * r,
            -r * (1.25 + 0.2 * (e.t * 12. + i).sin()),
            i * 0.3 * r,
            -r * 0.6,
        );
    }
    c.additive(false);
    c.fill_style(body_color(e, "#e8572a", "#ffc8a8"));
    ell(c, 0., 0., r * 0.72, r * 0.75);
    c.fill_style("#5a1f12");
    tri(
        c,
        -r * 0.45,
        -r * 0.5,
        -r * 0.55,
        -r * 0.95,
        -r * 0.2,
        -r * 0.62,
    );
    tri(
        c,
        r * 0.2,
        -r * 0.62,
        r * 0.5,
        -r * 0.95,
        r * 0.45,
        -r * 0.5,
    );
    c.fill_style(if e.calm { "#fff3d6" } else { "#ffe27a" });
    ell(c, -r * 0.05, -r * 0.12, r * 0.13, r * 0.15);
    ell(c, r * 0.35, -r * 0.12, r * 0.13, r * 0.15);
    c.fill_style("#1b1530");
    ell(c, 0., -r * 0.1, r * 0.06, r * 0.08);
    ell(c, r * 0.4, -r * 0.1, r * 0.06, r * 0.08);
    c.stroke_style("#3a120a");
    c.width((r * 0.08).max(1.2));
    c.begin();
    c.arc(r * 0.15, r * 0.15, r * 0.25, PI * 0.15, PI * 0.85);
    c.stroke();
    c.fill_style("#fff8e6");
    tri(c, r * 0.1, r * 0.36, r * 0.18, r * 0.5, r * 0.26, r * 0.35);
    if e.windup > 0. {
        c.fill_style("#ffb347");
        circle(c, r * 0.75, -r * 0.55, r * 0.22);
        c.fill_style("#ff5a1f");
        circle(c, r * 0.75, -r * 0.55, r * 0.12);
    }
    c.restore();
}
fn draw_gobbler(c: &mut Canvas, e: &EnemyArt) {
    let r = e.r;
    let chomp = 0.5 + 0.5 * (e.t * 9.).sin();
    let flap = (e.t * 16.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    c.fill_style(body_color(e, "#9b7cff", "#d8ccff"));
    for side in [-1., 1.] {
        ell(
            c,
            -r * 0.55,
            side * r * 0.1 - r * 0.45 * (0.6 + flap * 0.4 * side),
            r * 0.35,
            r * 0.18,
        );
    }
    c.fill_style(body_color(e, "#5a3fc0", "#b8a8f0"));
    ell(c, 0., 0., r, r * 0.92);
    c.fill_style("rgba(255,240,180,0.8)");
    for [x, y] in [[-0.5, -0.4], [-0.15, -0.65], [-0.65, 0.2]] {
        draw_star(c, x * r, y * r, r * 0.09, "#fff1a8", e.t + x);
    }
    c.fill_style("#2a0f3a");
    ell(c, r * 0.38, r * 0.2, r * 0.42, r * (0.08 + chomp * 0.3));
    c.fill_style("#fff8e6");
    for x in [0.16, 0.38, 0.6] {
        tri(
            c,
            (x - 0.07) * r,
            r * (0.2 - chomp * 0.28),
            x * r,
            r * (0.2 - chomp * 0.28) + r * 0.14,
            (x + 0.07) * r,
            r * (0.2 - chomp * 0.28),
        );
    }
    c.fill_style("#fff");
    ell(c, r * 0.1, -r * 0.42, r * 0.2, r * 0.22);
    ell(c, r * 0.5, -r * 0.38, r * 0.17, r * 0.19);
    c.fill_style("#1b1530");
    ell(c, r * 0.16, -r * 0.4, r * 0.1, r * 0.11);
    ell(c, r * 0.55, -r * 0.36, r * 0.09, r * 0.1);
    c.restore();
}
fn draw_moth(c: &mut Canvas, e: &EnemyArt) {
    let r = e.r;
    let flap = 0.55 + 0.45 * (e.t * 13.).sin();
    c.save();
    c.translate(e.x, e.y);
    c.scale(e.dir, 1.);
    for (col, dy, sy) in [("#a8b8ff", -1., 1.1), ("#ffb8e8", 1., 0.75)] {
        c.fill_style(body_color(e, col, "#f0eaff"));
        for side in [-1., 1.] {
            c.save();
            c.scale(1., flap * 0.6 + 0.4);
            ell(
                c,
                side * r * 0.75,
                dy * r * 0.35 * sy,
                r * 0.75 * sy,
                r * 0.6 * sy,
            );
            c.restore();
        }
    }
    c.fill_style("rgba(80,60,160,0.55)");
    for side in [-1., 1.] {
        ell(
            c,
            side * r * 0.8,
            -r * 0.3 * (flap * 0.6 + 0.4),
            r * 0.18,
            r * 0.14,
        );
    }
    c.fill_style(body_color(e, "#efe4ff", "#ffffff"));
    ell(c, 0., 0., r * 0.28, r * 0.6);
    c.stroke_style("#efe4ff");
    c.width((r * 0.06).max(1.2));
    c.begin();
    c.move_to(-r * 0.08, -r * 0.5);
    c.quad(-r * 0.3, -r, -r * 0.45, -r * 1.05);
    c.move_to(r * 0.08, -r * 0.5);
    c.quad(r * 0.3, -r, r * 0.45, -r * 1.05);
    c.stroke();
    c.stroke_style("#3a2a6a");
    c.width((r * 0.06).max(1.2));
    for sx in [-0.1, 0.12] {
        c.begin();
        c.arc(sx * r, -r * 0.3, r * 0.07, 0., PI);
        c.stroke();
    }
    c.restore();
}
fn draw_swallower(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    let r = e.r;
    let wob = (e.t * 2.).sin() * 0.03;
    c.save();
    c.translate(e.x, e.y);
    let g = c.radial(
        [0., 0., r * 0.6, 0., 0., r * 1.7],
        &[
            (0., "rgba(160,120,255,0.25)".into()),
            (1., "rgba(160,120,255,0)".into()),
        ],
    );
    c.fill_style(g);
    circle(c, 0., 0., r * 1.7);
    c.scale(e.dir, 1.);
    let flap = (e.t * 4.).sin();
    c.fill_style(body_color(e, "#3a2a8a", "#9a8ad0"));
    for side in [-1., 1.] {
        c.begin();
        c.move_to(-r * 0.3, side * r * 0.5);
        c.quad(
            -r * 1.25,
            side * r * (0.9 + flap * 0.2),
            -r * 1.1,
            side * r * 0.2,
        );
        c.close();
        c.fill();
    }
    let g = c.radial(
        [-r * 0.3, -r * 0.35, r * 0.1, 0., 0., r],
        &[
            (0., if e.flash > 0. { "#ffffff" } else { "#5a46b8" }.into()),
            (1., if e.flash > 0. { "#e8e0ff" } else { "#1f1650" }.into()),
        ],
    );
    c.fill_style(g);
    ell(c, 0., 0., r * (1. + wob), r * (0.95 - wob));
    for i in 0..9 {
        let a = i as f64 * 2.4;
        let rr = r * (0.3 + (i % 3) as f64 * 0.2);
        draw_star(
            c,
            a.cos() * rr * 0.9 - r * 0.15,
            a.sin() * rr * 0.8,
            r * 0.05 * (1. + 0.5 * (scene.time * 3. + i as f64).sin()),
            "#fff1a8",
            scene.time + i as f64,
        );
    }
    if e.windup > 0. {
        c.fill_style("#0a0620");
        ell(c, r * 0.45, r * 0.25, r * 0.38, r * 0.42);
        c.stroke_style("rgba(200,180,255,0.6)");
        c.width(r * 0.03);
        for i in 0..6 {
            let i = i as f64;
            let a = i / 6. * TAU + scene.time * 3.;
            let k = (scene.time * 1.6 + i / 6.) % 1.;
            c.begin();
            c.arc(
                r * 0.45 + a.cos() * r * (1.4 - k),
                r * 0.25 + a.sin() * r * (1.4 - k),
                r * 0.06,
                0.,
                TAU,
            );
            c.stroke();
        }
    } else {
        c.fill_style("#0a0620");
        c.begin();
        c.ellipse(
            r * 0.4,
            r * 0.32,
            r * 0.42,
            r * (0.1 + 0.06 * (e.t * 3.).sin()),
            0.12,
            0.,
            TAU,
        );
        c.fill();
        c.fill_style("#fff8e6");
        for x in [0.15, 0.4, 0.65] {
            tri(
                c,
                (x - 0.06) * r,
                r * 0.24,
                x * r,
                r * 0.36,
                (x + 0.06) * r,
                r * 0.24,
            );
        }
    }
    for [ex, ey, er] in [[0.05, -0.32, 0.2], [0.55, -0.28, 0.17]] {
        c.fill_style("#ffd76e");
        ell(c, ex * r, ey * r, er * r, er * r * 1.05);
        c.fill_style("#1b1530");
        ell(
            c,
            ex * r + er * r * 0.25,
            ey * r + er * r * 0.1,
            er * r * 0.5,
            er * r * 0.6,
        );
    }
    c.restore();
}
/// All original enemy silhouettes, dispatched in the same order as drawEnemy.
pub fn draw_enemy(c: &mut Canvas, e: &EnemyArt, scene: &SceneArt) {
    if e.stun > 0. {
        for i in 0..3 {
            let a = scene.time * 5. + i as f64 * TAU / 3.;
            draw_star(
                c,
                e.x + a.cos() * e.r * 0.8,
                e.y - e.r * 1.05 + a.sin() * e.r * 0.2,
                scene.scale * 0.13,
                "#ffd76e",
                a,
            );
        }
    }
    match e.kind.as_str() {
        "cloud" | "bigcloud" | "thunderhead" => draw_cloud(c, e, scene),
        "bat" => draw_bat(c, e),
        "sprite" => draw_sprite(c, e),
        "urgal" | "chieftain" => draw_urgal(c, e),
        "wolf" => draw_wolf(c, e),
        "jelly" => draw_jelly(c, e, scene),
        "leaper" => draw_leaper(c, e, scene),
        "serpent" => draw_serpent(c, e, scene),
        _ if e.hop => draw_slime(c, e, scene),
        "imp" => draw_imp(c, e),
        "gobbler" => draw_gobbler(c, e),
        "moth" => draw_moth(c, e),
        "swallower" => draw_swallower(c, e, scene),
        _ => draw_troll(c, e),
    }
}

/// Source attack ring. Text is painted by the engine adapter using the same position.
pub fn draw_warning(c: &mut Canvas, e: &EnemyArt, hot: bool, scene: &SceneArt) {
    let r = e.r
        * (1.25
            + if hot {
                0.1
            } else {
                0.12 * (scene.time * 18.).sin()
            });
    c.alpha(if hot {
        0.75
    } else {
        0.45 + 0.3 * (scene.time * 18.).sin().max(0.)
    });
    c.stroke_style(if hot { "#ff5a4e" } else { "#ffb347" });
    c.width((scene.scale * 0.07).max(2.));
    let dash = [scene.scale * 0.25, scene.scale * 0.18];
    c.dash(if hot { &[] } else { &dash });
    c.begin();
    c.arc(e.x, e.y, r, 0., TAU);
    c.stroke();
    c.dash(&[]);
    c.alpha(1.);
}
#[allow(clippy::too_many_arguments)]
pub fn draw_enemy_fx(
    c: &mut Canvas,
    e: &EnemyArt,
    scene: &SceneArt,
    squash: f64,
    pre: f64,
    lunge: f64,
    hurt: bool,
    sting: f64,
) {
    let sq = if squash > 0. {
        ((squash / 0.3).min(1.) * PI).sin() * 0.22
    } else {
        0.
    };
    let mut sx = 1. + sq;
    let mut sy = 1. - sq;
    if pre > 0. && !e.calm {
        sx *= 1.08;
        sy *= 0.9;
    }
    if (lunge > 0. || (hurt && sting <= 0.)) && !e.calm {
        sx *= 1.14;
        sy *= 0.9;
    }
    if sx != 1. || sy != 1. {
        let py = e.y + e.r * 0.6;
        c.save();
        c.translate(e.x, py);
        c.scale(sx, sy);
        c.translate(-e.x, -py);
        draw_enemy(c, e, scene);
        c.restore();
    } else {
        draw_enemy(c, e, scene);
    }
    if e.flash > 0. && !e.calm {
        c.alpha(0.5 * (e.flash / 0.12).min(1.));
        c.fill_style("#ffffff");
        circle(c, e.x, e.y, e.r * 0.9);
        c.alpha(1.);
    }
}
pub fn draw_mark(c: &mut Canvas, e: &EnemyArt) {
    let g = c.radial(
        [e.x, e.y, e.r * 0.4, e.x, e.y, e.r * 1.5],
        &[
            (0., "rgba(181,155,255,0.45)".into()),
            (1., "rgba(181,155,255,0)".into()),
        ],
    );
    c.fill_style(g);
    circle(c, e.x, e.y, e.r * 1.5);
}
pub fn draw_cracks(c: &mut Canvas, e: &EnemyArt) {
    c.stroke_style("rgba(20,10,30,0.85)");
    c.width((e.r * 0.08).max(1.5));
    c.cap("round");
    for k in 0..3 {
        let a = k as f64 * 2.1 + 0.4;
        let mut x = e.x + a.cos() * e.r * 0.2;
        let mut y = e.y + a.sin() * e.r * 0.2;
        c.begin();
        c.move_to(x, y);
        for j in 0..3 {
            let turn = if j % 2 != 0 { 0.5 } else { -0.5 };
            x += (a + turn).cos() * e.r * 0.28;
            y += (a + turn).sin() * e.r * 0.28;
            c.line_to(x, y);
        }
        c.stroke();
    }
    c.fill_style("rgba(255,179,71,0.35)");
    circle(c, e.x, e.y, e.r * 0.35);
}
pub fn draw_hazard(c: &mut Canvas, h: &crate::damage::Hazard, scene: &SceneArt, realm: &str) {
    let (x, y, r) = (h.x, h.y, h.r);
    if h.kind == "spark" {
        c.additive(true);
        let g = c.radial(
            [x, y, 0., x, y, r * 2.],
            &[
                (0., "#ffffff".into()),
                (0.4, "#fff27a".into()),
                (1., "rgba(255,200,0,0)".into()),
            ],
        );
        c.fill_style(g);
        circle(c, x, y, r * 2.);
        c.additive(false);
        return;
    }
    if h.kind == "fall" && h.warn > 0. {
        c.alpha(0.35 + 0.35 * (scene.time * 20.).sin());
        c.fill_style("#ff5a4e");
        ell(
            c,
            x,
            scene.ground - 4.,
            scene.scale * 0.5,
            scene.scale * 0.15,
        );
        c.alpha(1.);
        return;
    }
    if h.kind == "bubble" {
        c.fill_style("rgba(180,255,250,0.22)");
        circle(c, x, y, r);
        c.stroke_style("rgba(220,255,255,0.85)");
        c.width((r * 0.12).max(1.5));
        c.begin();
        c.arc(x, y, r, 0., TAU);
        c.stroke();
        c.fill_style("rgba(255,255,255,0.8)");
        ell(c, x - r * 0.35, y - r * 0.35, r * 0.22, r * 0.14);
        return;
    }
    if h.kind == "dust" {
        c.additive(true);
        let g = c.radial(
            [x, y, 0., x, y, r * 1.6],
            &[
                (0., "rgba(230,210,255,0.7)".into()),
                (1., "rgba(180,150,255,0)".into()),
            ],
        );
        c.fill_style(g);
        circle(c, x, y, r * 1.6);
        c.additive(false);
        for i in 0..3 {
            let i = i as f64;
            draw_star(
                c,
                x + (scene.time * 3. + i * 2.1).cos() * r * 0.6,
                y + (scene.time * 3. + i * 2.1).sin() * r * 0.6,
                r * 0.22,
                "#fff6ff",
                scene.time * 2. + i,
            );
        }
        return;
    }
    if h.kind == "ember" || (h.kind == "fall" && realm == "ember") {
        c.additive(true);
        let g = c.radial(
            [x, y, 0., x, y, r * 2.2],
            &[
                (0., "rgba(255,200,90,0.8)".into()),
                (1., "rgba(255,90,30,0)".into()),
            ],
        );
        c.fill_style(g);
        circle(c, x, y, r * 2.2);
        c.additive(false);
        c.fill_style("#5a2414");
        circle(c, x, y, r);
        c.fill_style("#ff8a3d");
        circle(c, x + r * 0.2, y - r * 0.2, r * 0.45);
        return;
    }
    if h.kind == "fall" && realm == "sea" {
        c.fill_style("rgba(160,240,255,0.85)");
        c.begin();
        c.move_to(x, y - r * 1.6);
        c.quad(x + r, y, x, y + r);
        c.quad(x - r, y, x, y - r * 1.6);
        c.fill();
        return;
    }
    c.save();
    c.translate(x, y);
    c.rotate(h.spin);
    c.fill_style(if h.kind == "boulder" {
        "#7a6e60"
    } else {
        "#8f8f9c"
    });
    c.begin();
    for i in 0..7 {
        let a = i as f64 / 7. * TAU;
        let rr = r * (0.85 + (i % 2) as f64 * 0.15);
        c.line_to(a.cos() * rr, a.sin() * rr);
    }
    c.close();
    c.fill();
    c.fill_style("rgba(0,0,0,0.25)");
    circle(c, r * 0.2, -r * 0.15, r * 0.25);
    c.restore();
}
/// Lightning shape randomness is supplied by the presentation caller, never the gameplay RNG.
pub fn draw_strike(
    c: &mut Canvas,
    st: &crate::damage::Strike,
    scene: &SceneArt,
    top: f64,
    mut random: impl FnMut() -> f64,
) {
    if st.warn > 0. {
        c.alpha(
            0.3 + if (scene.time * 24.).sin() > 0. {
                0.4
            } else {
                0.
            },
        );
        c.stroke_style("#ffd76e");
        c.width(3.);
        c.dash(&[10., 10.]);
        c.begin();
        c.move_to(st.x, st.y0.unwrap_or(top));
        c.line_to(st.x, scene.ground);
        c.stroke();
        c.dash(&[]);
        c.alpha(1.);
        return;
    }
    c.additive(true);
    c.stroke_style("#fffbe0");
    c.width(scene.scale * 0.18);
    c.join("round");
    c.begin();
    let mut y = st.y0.unwrap_or(top);
    c.move_to(st.x, y);
    while y < scene.ground {
        y += scene.scale * 0.6;
        let x = st.x + (random() * 2. - 1.) * scene.scale * 0.35;
        c.line_to(x, y);
    }
    c.stroke();
    c.stroke_style("rgba(255,215,110,0.5)");
    c.width(scene.scale * 0.5);
    c.stroke();
    c.additive(false);
}
/// Geometry of the mission's protected/escorted/carried object. Labels are adapter text.
pub fn draw_objective(c: &mut Canvas, ob: &crate::objectives::Objective, scene: &SceneArt) {
    let (x, y, s) = (ob.x, ob.y, scene.scale);
    c.save();
    if ob.hurt_t > 0. && (scene.time * 16.).floor() % 2. == 0. {
        c.alpha(0.55);
    }
    match ob.what.as_str() {
        "barn" => {
            let w = s * 1.9;
            let h = s * 1.3;
            c.fill_style("#b5443a");
            c.fill_rect(x - w / 2., y - h * 0.3, w, h);
            c.fill_style("#7a2f2a");
            tri(
                c,
                x - w * 0.6,
                y - h * 0.3,
                x,
                y - h,
                x + w * 0.6,
                y - h * 0.3,
            );
            c.fill_style("#5a3a24");
            c.fill_rect(x - w * 0.16, y + h * 0.2, w * 0.32, h * 0.5);
            c.fill_style("#e8c86a");
            c.begin();
            c.arc(x + w * 0.62, y + h * 0.65, s * 0.35, PI, TAU);
            c.fill();
        }
        "fawn" | "goat" => {
            let k = s * 0.36;
            let g = ob.what == "goat";
            let bob = (ob.t * 14.).sin() * k * 0.15;
            c.fill_style(if g { "#e8e4dc" } else { "#b07a4a" });
            ell(c, x, y + bob, k * 1.6, k * 0.95);
            ell(c, x + k * 1.7, y - k + bob, k * 0.8, k * 0.7);
            c.fill_style(if g { "#c8c4bc" } else { "#7a5434" });
            for lx in [-1.2, -0.5, 0.5, 1.2] {
                c.fill_rect(
                    x + lx * k,
                    y + k * 0.5 + bob,
                    k * 0.32,
                    k * 1.4
                        + if lx * (ob.t * 14.).sin() > 0. {
                            -k * 0.3
                        } else {
                            0.
                        },
                );
            }
            if g {
                c.stroke_style("#8a7a6a");
                c.width(k * 0.2);
                c.begin();
                c.move_to(x + k * 1.6, y - k * 1.5 + bob);
                c.quad(
                    x + k * 1.2,
                    y - k * 2.6 + bob,
                    x + k * 0.9,
                    y - k * 2.3 + bob,
                );
                c.stroke();
            } else {
                c.fill_style("#f4e4cc");
                for i in 0..4 {
                    circle(
                        c,
                        x - k * 0.9 + i as f64 * k * 0.5,
                        y - k * 0.3 + bob + (i % 2) as f64 * k * 0.4,
                        k * 0.14,
                    );
                }
            }
            c.fill_style("#20150c");
            circle(c, x + k * 2., y - k * 1.1 + bob, k * 0.12);
        }
        "lamp" => {
            let g = c.radial(
                [x, y, 0., x, y, s * 2.2],
                &[
                    (0., "rgba(255,215,110,0.55)".into()),
                    (1., "rgba(255,215,110,0)".into()),
                ],
            );
            c.fill_style(g);
            circle(c, x, y, s * 2.2);
            c.fill_style("#ffe9a0");
            c.fill_rect(x - s * 0.3, y - s * 0.35, s * 0.6, s * 0.7);
            c.stroke_style("#2a2a3a");
            c.width(2.);
            let old = std::mem::take(&mut c.path);
            c.move_to(x - s * 0.3, y - s * 0.35);
            c.line_to(x + s * 0.3, y - s * 0.35);
            c.line_to(x + s * 0.3, y + s * 0.35);
            c.line_to(x - s * 0.3, y + s * 0.35);
            c.close();
            c.stroke();
            c.path = old;
        }
        "boats" => {
            for (dx, col) in [(-1.6, "#c0553f"), (0., "#3f7ac0"), (1.6, "#e8a83f")] {
                let bx = x + dx * s;
                let by = y + (ob.t * 2. + dx).sin() * s * 0.08;
                c.fill_style(col);
                c.begin();
                c.move_to(bx - s * 0.6, by);
                c.line_to(bx + s * 0.6, by);
                c.line_to(bx + s * 0.4, by + s * 0.35);
                c.line_to(bx - s * 0.4, by + s * 0.35);
                c.close();
                c.fill();
                c.stroke_style("#4a3a2a");
                c.width(2.);
                c.begin();
                c.move_to(bx, by);
                c.line_to(bx, by - s * 1.1);
                c.stroke();
                c.fill_style("#fff6e0");
                tri(
                    c,
                    bx + 2.,
                    by - s * 1.05,
                    bx + 2.,
                    by - s * 0.15,
                    bx + s * 0.55,
                    by - s * 0.15,
                );
            }
        }
        "garden" => {
            c.fill_style("#4f8f6a");
            ell(c, x, y + s * 0.3, s * 1.4, s * 0.3);
            for i in 0..6 {
                let fx = x - s * 1.1 + i as f64 * s * 0.44;
                let fy = y - s * 0.1 - (i % 2) as f64 * s * 0.2;
                c.fill_style("#3f8a48");
                c.fill_rect(fx - 1.5, fy, 3., s * 0.5);
                c.fill_style(["#ffb0d8", "#9fd8ff", "#ffe27a"][i % 3]);
                circle(c, fx, fy, s * 0.14);
            }
            draw_star(c, x, y - s * 0.7, s * 0.42, "#fff1a8", scene.time * 0.5);
        }
        "star" => {
            let gy = scene.ground - s * 2.2;
            let pulse = 1. + 0.12 * (scene.time * 5.).sin();
            c.stroke_style(if ob.done { "#62f0d4" } else { "#ffd76e" });
            c.width((s * 0.1).max(3.));
            let dash = [s * 0.3, s * 0.2];
            c.dash(if ob.done { &[] } else { &dash });
            c.begin();
            c.arc(ob.to, gy, s * 1.1 * pulse, 0., TAU);
            c.stroke();
            c.dash(&[]);
            let g = c.radial(
                [x, y, 0., x, y, s * 1.4],
                &[
                    (0., "rgba(255,241,168,0.6)".into()),
                    (1., "rgba(255,241,168,0)".into()),
                ],
            );
            c.fill_style(g);
            circle(c, x, y, s * 1.4);
            draw_star(c, x, y, s * 0.5, "#fff1a8", scene.time * 1.5);
            draw_star(c, x, y, s * 0.24, "#ffffff", -scene.time);
        }
        _ => {}
    }
    c.restore();
}

pub fn draw_gem(c: &mut Canvas, g: &crate::gems::Gem, color: &str, scene: &SceneArt) {
    let y = g.y + (scene.time * 3. + g.t).sin() * scene.scale * 0.08;
    let r = g.r;
    c.alpha(if g.life < 2. && (scene.time * 8.).floor() % 2. == 0. {
        0.35
    } else {
        1.
    });
    c.additive(true);
    let gl = c.radial(
        [g.x, y, 0., g.x, y, r * 2.6],
        &[(0., hex_a(color, 0.5)), (1., hex_a(color, 0.))],
    );
    c.fill_style(gl);
    circle(c, g.x, y, r * 2.6);
    c.additive(false);
    if g.golden {
        draw_star(c, g.x, y, r * 1.2, color, scene.time * 1.5);
        draw_star(c, g.x, y, r * 0.55, "#fff8d8", scene.time * 1.5);
    } else {
        c.save();
        c.translate(g.x, y);
        c.fill_style(color);
        c.begin();
        c.move_to(-0.8 * r, -0.25 * r);
        c.line_to(-0.45 * r, -0.7 * r);
        c.line_to(0.45 * r, -0.7 * r);
        c.line_to(0.8 * r, -0.25 * r);
        c.line_to(0., r);
        c.close();
        c.fill();
        c.fill_style("rgba(255,255,255,0.55)");
        tri(
            c,
            -0.45 * r,
            -0.7 * r,
            0.05 * r,
            -0.7 * r,
            -0.3 * r,
            -0.25 * r,
        );
        c.fill_style("rgba(0,0,0,0.2)");
        tri(c, 0.8 * r, -0.25 * r, 0., r, 0.25 * r, -0.25 * r);
        c.restore();
    }
    c.alpha(1.);
}
pub fn draw_shot(
    c: &mut Canvas,
    shot: &crate::combat::Shot,
    colors: Option<&[String; 3]>,
    scene: &SceneArt,
) {
    use crate::combat::Kind;
    let (x, y, r, s) = (shot.x, shot.y, shot.r, scene.scale);
    if shot.dad && shot.kind != Kind::Boulder {
        let a = shot.vy.atan2(shot.vx);
        let tx = x - a.cos() * r * 6.;
        let ty = y - a.sin() * r * 6.;
        c.additive(true);
        let g = c.linear(
            [tx, ty, x, y],
            &[
                (0., "rgba(255,180,70,0)".into()),
                (1., "rgba(255,235,160,0.9)".into()),
            ],
        );
        c.stroke_style(g);
        c.width(r * 1.6);
        c.cap("round");
        c.begin();
        c.move_to(tx, ty);
        c.line_to(x, y);
        c.stroke();
        let g = c.radial(
            [x, y, 0., x, y, r * 2.6],
            &[
                (0., "rgba(255,250,220,0.95)".into()),
                (0.4, "rgba(255,207,63,0.7)".into()),
                (1., "rgba(255,150,40,0)".into()),
            ],
        );
        c.fill_style(g);
        circle(c, x, y, r * 2.6);
        c.additive(false);
        draw_star(c, x, y, r * 1.2, "#fff6c2", scene.time * 8.);
        return;
    }
    if shot.kind == Kind::Boulder {
        c.save();
        c.translate(x, y);
        c.rotate(shot.spin);
        c.fill_style("#ff8a3d");
        c.alpha(0.35);
        circle(c, 0., 0., r * 1.3);
        c.alpha(1.);
        c.fill_style("#8a6a4a");
        c.begin();
        for i in 0..7 {
            let a = i as f64 / 7. * TAU;
            let rr = r * (0.85 + (i % 2) as f64 * 0.15);
            c.line_to(a.cos() * rr, a.sin() * rr);
        }
        c.close();
        c.fill();
        c.fill_style("#6b4f35");
        circle(c, r * 0.2, -r * 0.2, r * 0.25);
        circle(c, -r * 0.3, r * 0.25, r * 0.18);
        c.restore();
        return;
    }
    if shot.kind == Kind::Arrow {
        c.save();
        c.translate(x, y);
        c.rotate(shot.vy.atan2(shot.vx));
        if shot.spear {
            c.stroke_style("#c9b27a");
            c.width((s * 0.06).max(2.));
            c.begin();
            c.move_to(-s * 0.55, 0.);
            c.line_to(s * 0.2, 0.);
            c.stroke();
            c.additive(true);
            c.fill_style("rgba(255,242,122,0.45)");
            circle(c, s * 0.28, 0., s * 0.16);
            c.additive(false);
            c.fill_style("#fff27a");
            tri(c, s * 0.42, 0., s * 0.18, -s * 0.09, s * 0.18, s * 0.09);
        } else {
            c.stroke_style("#d9c38a");
            c.width((s * 0.05).max(2.));
            c.begin();
            c.move_to(-s * 0.4, 0.);
            c.line_to(s * 0.2, 0.);
            c.stroke();
            c.fill_style("#c6cedd");
            tri(c, s * 0.3, 0., s * 0.16, -s * 0.07, s * 0.16, s * 0.07);
            c.fill_style("#56c08a");
            tri(c, -s * 0.4, 0., -s * 0.5, -s * 0.08, -s * 0.3, 0.);
            tri(c, -s * 0.4, 0., -s * 0.5, s * 0.08, -s * 0.3, 0.);
        }
        c.restore();
        return;
    }
    if shot.kind == Kind::Hammer {
        c.save();
        c.translate(x, y);
        c.rotate(shot.spin);
        c.fill_style("rgba(255,180,80,0.35)");
        circle(c, 0., 0., s * 0.4);
        c.stroke_style("#8a5a2b");
        c.width(s * 0.08);
        c.begin();
        c.move_to(0., s * 0.3);
        c.line_to(0., -s * 0.15);
        c.stroke();
        c.fill_style("#9aa3b3");
        c.fill_rect(-s * 0.2, -s * 0.32, s * 0.4, s * 0.2);
        c.restore();
        return;
    }
    let defaults = ["#fff6c2".into(), "#ffb347".into(), "#ff5a1f".into()];
    let cols = colors.unwrap_or(&defaults);
    c.additive(true);
    if shot.kind == Kind::Spark {
        c.stroke_style(cols[1].clone());
        c.width(r.max(2.));
        c.cap("round");
        c.begin();
        c.move_to(x - shot.vx * 0.025, y - shot.vy * 0.025);
        c.line_to(x, y);
        c.stroke();
    }
    let safe = if cols[2].len() == 7
        && cols[2].starts_with('#')
        && cols[2][1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        &cols[2]
    } else {
        "#ff5a1f"
    };
    let g = c.radial(
        [x, y, 0., x, y, r * 1.8],
        &[
            (0., cols[0].clone()),
            (0.4, cols[1].clone()),
            (1., hex_a(safe, 0.)),
        ],
    );
    c.fill_style(g);
    circle(c, x, y, r * 1.8);
    c.additive(false);
}

#[derive(Default)]
pub struct BackgroundArt {
    pub sky: Canvas,
    pub far: Canvas,
    pub land: Canvas,
}
/// Original cached background layers. World coordinates are logical pixels (DPR is the GPU's job).
pub fn draw_background(
    view: crate::flight::Viewport,
    realm_index: usize,
    song: bool,
) -> BackgroundArt {
    let mut out = BackgroundArt::default();
    let (w, h, s, gy, top) = (
        view.width,
        view.height,
        view.scale,
        view.ground_y,
        view.top_limit,
    );
    let realm = realm_index.min(5);
    let skies = [
        ["#050c24", "#15285a", "#2c3a6e"],
        ["#04120f", "#0f2f2a", "#24483a"],
        ["#061230", "#1b3a6e", "#5b7fb3"],
        ["#04122a", "#0d3050", "#1d5a7a"],
        ["#14050a", "#3e1210", "#8a3418"],
        ["#02030f", "#150f48", "#3a2a7a"],
    ][realm];
    let mut rng = crate::random::Mulberry32 {
        state: 11 + realm as u32,
    };
    let g = &mut out.sky;
    let sky = g.linear(
        [0., 0., 0., h],
        &[
            (0., skies[0].into()),
            (0.55, skies[1].into()),
            (1., skies[2].into()),
        ],
    );
    g.fill_style(sky);
    g.fill_rect(0., 0., w, h);
    let mx = w * 0.84;
    let my = (h * 0.24).max(top + s * 1.4);
    let mr = s * 1.05;
    let glow = g.radial(
        [mx, my, mr * 0.6, mx, my, mr * 3.4],
        &[
            (0., "rgba(234,242,255,0.3)".into()),
            (1., "rgba(234,242,255,0)".into()),
        ],
    );
    g.fill_style(glow);
    circle(g, mx, my, mr * 3.4);
    g.fill_style(if realm == 4 { "#ffcfa8" } else { "#eaf2ff" });
    circle(g, mx, my, mr);
    g.fill_style("rgba(159,182,255,0.45)");
    for [dx, dy, r] in [[-0.32, -0.2, 0.2], [0.28, 0.12, 0.25], [-0.08, 0.42, 0.13]] {
        circle(g, mx + dx * mr, my + dy * mr, r * mr);
    }
    if song {
        let pts = [
            [0.08, 0.2],
            [0.13, 0.16],
            [0.18, 0.18],
            [0.23, 0.14],
            [0.27, 0.17],
            [0.31, 0.22],
            [0.2, 0.25],
            [0.15, 0.27],
        ];
        g.stroke_style("rgba(255,243,214,0.25)");
        g.width(1.5);
        g.begin();
        for (i, [x, y]) in pts.into_iter().enumerate() {
            let (px, py) = (w * 0.04 + x * w * 0.9, top + y * h * 0.8);
            if i == 0 {
                g.move_to(px, py)
            } else {
                g.line_to(px, py)
            }
        }
        g.stroke();
        g.fill_style("#fff6dd");
        for [x, y] in pts {
            circle(g, w * 0.04 + x * w * 0.9, top + y * h * 0.8, 2.6);
        }
    }
    if realm >= 3 {
        draw_far_realm(&mut out.far, &mut out.land, view, realm, &mut rng);
        background_fog(&mut out.far, view, skies[2], 0.4);
        return out;
    }
    let g = &mut out.far;
    let tall = if realm == 2 {
        0.3
    } else if realm == 1 {
        0.12
    } else {
        0.17
    };
    let mut peaks = Vec::new();
    g.fill_style(["#1c2b5e", "#12302a", "#2a4a7e"][realm]);
    g.begin();
    g.move_to(0., h);
    g.line_to(0., gy - h * 0.1);
    let mut x = 0.;
    while x <= w + 80. {
        let step = 70. + rng.next_unit() * 80.;
        let py = gy - h * (0.12 + rng.next_unit() * tall);
        g.line_to(x + step / 2., py);
        peaks.push([x + step / 2., py, step]);
        x += step;
        g.line_to(x, gy - h * (0.05 + rng.next_unit() * 0.05));
    }
    g.line_to(w + 160., h);
    g.close();
    g.fill();
    if realm != 1 {
        g.fill_style(if realm == 2 {
            "rgba(240,248,255,0.85)"
        } else {
            "rgba(234,242,255,0.5)"
        });
        for [px, py, step] in peaks {
            let k = if realm == 2 { 0.22 } else { 0.13 };
            tri(
                g,
                px,
                py,
                px - step * k,
                py + step * k * 1.2,
                px + step * k,
                py + step * k * 1.2,
            );
        }
    }
    background_fog(g, view, skies[2], 0.55);
    let g = &mut out.land;
    if realm == 1 {
        for (col, hs) in [("#0b2420", 1.6), ("#071a17", 1.1)] {
            g.fill_style(col);
            let mut x = -10.;
            while x < w + 20. {
                let hh = s * (0.9 + rng.next_unit() * hs);
                tri(
                    g,
                    x,
                    gy + 4.,
                    x + hh * 0.26,
                    gy - hh,
                    x + hh * 0.52,
                    gy + 4.,
                );
                x += 12. + rng.next_unit() * 12.;
            }
        }
        g.fill_style("#0a1d12");
    } else if realm == 2 {
        g.fill_style("#dfeeff");
    } else {
        g.fill_style("#0d1a38");
        let mut x = w * 0.05;
        while x < w * 0.6 {
            let ww = s * (0.9 + rng.next_unit() * 0.6);
            let hh = s * (0.7 + rng.next_unit() * 0.6);
            g.fill_style("#0d1a38");
            g.fill_rect(x, gy - hh, ww, hh);
            tri(
                g,
                x - s * 0.1,
                gy - hh,
                x + ww / 2.,
                gy - hh - s * 0.5,
                x + ww + s * 0.1,
                gy - hh,
            );
            let gx = x + ww * 0.3 + s * 0.09;
            let yy = gy - hh * 0.6 + s * 0.1;
            let gl = g.radial(
                [gx, yy, 0., gx, yy, s * 0.9],
                &[
                    (0., "rgba(255,215,110,0.35)".into()),
                    (1., "rgba(255,215,110,0)".into()),
                ],
            );
            g.fill_style(gl);
            g.fill_rect(x - s, gy - hh - s, ww + s * 2., hh + s * 2.);
            g.fill_style("#ffd76e");
            g.fill_rect(x + ww * 0.3, gy - hh * 0.6, s * 0.18, s * 0.2);
            x += s * (1.4 + rng.next_unit());
        }
        g.fill_style("#0a1430");
    }
    g.fill_rect(0., gy, w, h - gy);
    if realm == 2 {
        g.fill_style("#b9d4f0");
        g.fill_rect(0., gy, w, 4.);
    }
    out
}
fn background_fog(c: &mut Canvas, v: crate::flight::Viewport, col: &str, a: f64) {
    let g = c.linear(
        [0., v.ground_y - v.height * 0.32, 0., v.ground_y + 2.],
        &[(0., hex_a(col, 0.)), (1., hex_a(col, a))],
    );
    c.fill_style(g);
    c.fill_rect(
        0.,
        v.ground_y - v.height * 0.32,
        v.width,
        v.height * 0.32 + 2.,
    );
}
fn draw_far_realm(
    far: &mut Canvas,
    g: &mut Canvas,
    v: crate::flight::Viewport,
    realm: usize,
    rng: &mut crate::random::Mulberry32,
) {
    let (w, h, s, gy, top) = (v.width, v.height, v.scale, v.ground_y, v.top_limit);
    match realm {
        3 => {
            far.fill_style("#0a2a40");
            for [x, ww, hh] in [[0.55, 0.18, 0.05], [0.78, 0.12, 0.035], [0.32, 0.1, 0.025]] {
                far.begin();
                far.ellipse(w * x, gy, w * ww, h * hh, 0., PI, TAU);
                far.fill();
            }
            g.fill_style("#0b2233");
            g.begin();
            g.move_to(0., gy);
            g.line_to(0., gy - h * 0.3);
            g.line_to(w * 0.07, gy - h * 0.32);
            g.line_to(w * 0.11, gy - h * 0.2);
            g.line_to(w * 0.15, gy);
            g.close();
            g.fill();
            let (lx, ly, lw, lh) = (w * 0.05, gy - h * 0.31, s * 0.5, s * 1.8);
            g.fill_style("#e8e0d0");
            g.begin();
            g.move_to(lx - lw * 0.5, ly);
            g.line_to(lx - lw * 0.35, ly - lh);
            g.line_to(lx + lw * 0.35, ly - lh);
            g.line_to(lx + lw * 0.5, ly);
            g.close();
            g.fill();
            g.fill_style("#c0453f");
            g.fill_rect(lx - lw * 0.45, ly - lh * 0.4, lw * 0.9, lh * 0.15);
            g.fill_rect(lx - lw * 0.4, ly - lh * 0.8, lw * 0.8, lh * 0.12);
            g.fill_style("#ffd76e");
            g.fill_rect(lx - lw * 0.3, ly - lh - s * 0.35, lw * 0.6, s * 0.35);
            g.fill_style("#2a2a3a");
            tri(
                g,
                lx - lw * 0.45,
                ly - lh - s * 0.35,
                lx,
                ly - lh - s * 0.7,
                lx + lw * 0.45,
                ly - lh - s * 0.35,
            );
            let bm = g.linear(
                [lx, 0., lx + w * 0.45, 0.],
                &[
                    (0., "rgba(255,230,150,0.28)".into()),
                    (1., "rgba(255,230,150,0)".into()),
                ],
            );
            g.fill_style(bm);
            tri(
                g,
                lx,
                ly - lh - s * 0.18,
                lx + w * 0.45,
                ly - lh - s * 1.4,
                lx + w * 0.45,
                ly - lh + s * 0.9,
            );
            let sea = g.linear(
                [0., gy, 0., h],
                &[(0., "#13506a".into()), (1., "#062030".into())],
            );
            g.fill_style(sea);
            g.fill_rect(0., gy, w, h - gy);
            g.fill_style("rgba(234,242,255,0.35)");
            for k in 0..7 {
                let k = k as f64;
                let ww = s * (1.6 - k * 0.15);
                g.fill_rect(
                    w * 0.84 - ww / 2. + rng.next_unit() * s * 0.4,
                    gy + 6. + k * (h - gy) / 8.,
                    ww,
                    2.,
                );
            }
            g.stroke_style("rgba(190,250,255,0.35)");
            g.width(2.);
            for _ in 0..18 {
                g.begin();
                g.arc(
                    rng.next_unit() * w,
                    gy + 8. + rng.next_unit() * (h - gy - 12.),
                    s * 0.25,
                    PI * 1.1,
                    PI * 1.9,
                );
                g.stroke();
            }
            g.fill_style("rgba(200,255,250,0.6)");
            g.fill_rect(0., gy, w, 2.);
        }
        4 => {
            for (x0, w0, h0, col) in [
                (0.25, 0.3, 0.36, "#2a0f0c"),
                (0.68, 0.36, 0.46, "#33130e"),
                (0.97, 0.22, 0.28, "#2a0f0c"),
            ] {
                let (cx, yy, half) = (w * x0, gy - h * h0, w * w0 / 2.);
                let cw = half * 0.18;
                far.fill_style(col);
                far.begin();
                far.move_to(cx - half, gy);
                far.line_to(cx - cw, yy);
                far.line_to(cx + cw, yy);
                far.line_to(cx + half, gy);
                far.close();
                far.fill();
                let gl = far.radial(
                    [cx, yy, 0., cx, yy, half * 0.7],
                    &[
                        (0., "rgba(255,140,50,0.55)".into()),
                        (1., "rgba(255,90,30,0)".into()),
                    ],
                );
                far.fill_style(gl);
                circle(far, cx, yy, half * 0.7);
                far.fill_style("#ff8a3d");
                ell(far, cx, yy + 2., cw, cw * 0.3);
                far.stroke_style("rgba(255,120,40,0.75)");
                far.width((s * 0.08).max(2.));
                far.begin();
                let mut x = cx + cw * 0.3;
                let mut y = yy;
                far.move_to(x, y);
                while y < gy {
                    y += s * 0.6;
                    x += (rng.next_unit() - 0.3) * s * 0.5;
                    far.line_to(x, y);
                }
                far.stroke();
                far.fill_style("rgba(90,55,55,0.35)");
                for k in 0..4 {
                    let k = k as f64;
                    circle(
                        far,
                        cx + (rng.next_unit() - 0.5) * s + k * s * 0.4,
                        yy - s * (0.6 + k * 0.7),
                        s * (0.5 + k * 0.25),
                    );
                }
            }
            g.fill_style("#1a0d0b");
            g.fill_rect(0., gy, w, h - gy);
            g.stroke_style("rgba(255,110,40,0.8)");
            g.width(2.);
            for _ in 0..10 {
                let mut x = rng.next_unit() * w;
                let mut y = gy + 6. + rng.next_unit() * (h - gy) * 0.6;
                g.begin();
                g.move_to(x, y);
                for _ in 0..4 {
                    x += s * (0.3 + rng.next_unit() * 0.5);
                    y += (rng.next_unit() - 0.5) * s * 0.3;
                    g.line_to(x, y);
                }
                g.stroke();
            }
            g.fill_style("#ff7b3a");
            g.fill_rect(0., gy, w, 3.);
        }
        _ => {
            for (y0, col) in [
                (0.22, "rgba(98,240,212,0.13)"),
                (0.32, "rgba(255,111,181,0.1)"),
            ] {
                far.fill_style(col);
                far.begin();
                far.move_to(0., h * y0);
                let mut x = 0.;
                while x <= w + s {
                    far.line_to(x, h * y0 + (x / (s * 3.) + y0 * 9.).sin() * s * 0.8);
                    x += s;
                }
                x = w + s;
                while x >= 0. {
                    far.line_to(
                        x,
                        h * y0 + s * 1.6 + (x / (s * 2.4) + y0 * 5.).sin() * s * 0.6,
                    );
                    x -= s;
                }
                far.close();
                far.fill();
            }
            for [x0, y0, w0] in [[0.16, 0.56, 0.11], [0.6, 0.44, 0.08], [0.88, 0.62, 0.13]] {
                let (cx, cy, ww) = (w * x0, top + (gy - top) * y0, w * w0);
                far.fill_style("#3a2f6a");
                far.begin();
                far.move_to(cx - ww, cy);
                far.quad(cx, cy + ww * 0.9, cx + ww, cy);
                far.close();
                far.fill();
                far.fill_style("rgba(190,230,255,0.4)");
                far.fill_rect(cx + ww * 0.35, cy, s * 0.12, (gy - cy) * 0.8);
                far.fill_style("#4f8f6a");
                ell(far, cx, cy, ww, ww * 0.15);
                far.fill_style("#2f6a4a");
                for k in 0..3 {
                    let tx = cx - ww * 0.6 + k as f64 * ww * 0.5;
                    tri(g, tx - s * 0.2, cy, tx, cy - s * 0.7, tx + s * 0.2, cy);
                }
            }
            g.fill_style("#d8d0f0");
            g.fill_rect(0., gy, w, h - gy);
            g.fill_style("#eee8ff");
            let mut x = -s;
            while x < w + s {
                g.begin();
                g.arc(
                    x + rng.next_unit() * s * 0.4,
                    gy + s * 0.1,
                    s * (0.5 + rng.next_unit() * 0.4),
                    PI,
                    TAU,
                );
                g.fill();
                x += s * 1.1;
            }
        }
    }
}

fn hash01(i: f64, k: f64) -> f64 {
    let v = (i * 127.1 + k * 311.7).sin() * 43758.5453;
    v - v.floor()
}
/// Original ground scenery and wind streaks, keyed to travelled distance rather than frame RNG.
pub fn draw_scenery(
    c: &mut Canvas,
    v: crate::flight::Viewport,
    realm: usize,
    scroll: f64,
    scroll_v: f64,
) {
    let (w, s, ground, top) = (v.width, v.scale, v.ground_y, v.top_limit);
    let sp = s * 2.3;
    let i0 = (scroll / sp).floor() as i64 - 1;
    let i1 = i0 + (w / sp).ceil() as i64 + 3;
    let gy = ground + 1.;
    for i in i0..i1 {
        let i = i as f64;
        if hash01(i, 1.) < 0.35 {
            continue;
        }
        let x = i * sp - scroll + hash01(i, 2.) * sp * 0.6;
        let k = s * (0.3 + hash01(i, 3.) * 0.3);
        let kind = hash01(i, 4.);
        match realm {
            0 => {
                if kind < 0.55 {
                    c.fill_style("#12301e");
                    circle(c, x, gy - k * 0.3, k * 0.55);
                    circle(c, x + k * 0.5, gy - k * 0.2, k * 0.42);
                } else if kind < 0.8 {
                    c.fill_style("#3a2a1e");
                    c.fill_rect(x, gy - k, k * 0.14, k);
                    c.fill_rect(x + k * 0.8, gy - k, k * 0.14, k);
                    c.fill_rect(x - k * 0.1, gy - k * 0.75, k * 1.1, k * 0.1);
                    c.fill_rect(x - k * 0.1, gy - k * 0.4, k * 1.1, k * 0.1);
                } else {
                    c.fill_style("#8a6a2a");
                    c.begin();
                    c.arc(x, gy, k * 0.7, PI, TAU);
                    c.fill();
                    c.fill_style("rgba(255,220,120,0.25)");
                    c.begin();
                    c.arc(x - k * 0.15, gy - k * 0.2, k * 0.3, PI, TAU);
                    c.fill();
                }
            }
            1 => {
                c.fill_style(if kind < 0.6 { "#0b2a1f" } else { "#123a28" });
                if kind < 0.6 {
                    tri(c, x - k * 0.5, gy, x, gy - k * 1.8, x + k * 0.5, gy);
                    tri(
                        c,
                        x - k * 0.4,
                        gy - k * 0.7,
                        x,
                        gy - k * 2.3,
                        x + k * 0.4,
                        gy - k * 0.7,
                    );
                } else {
                    circle(c, x, gy - k * 0.25, k * 0.5);
                    circle(c, x + k * 0.45, gy - k * 0.15, k * 0.38);
                }
            }
            2 => {
                if kind < 0.5 {
                    c.fill_style("#5a6a86");
                    c.begin();
                    c.ellipse(x, gy, k * 0.7, k * 0.5, 0., PI, TAU);
                    c.fill();
                    c.fill_style("#f0f6ff");
                    ell(c, x, gy - k * 0.36, k * 0.5, k * 0.16);
                } else {
                    c.fill_style("#1e3a5a");
                    tri(c, x - k * 0.5, gy, x, gy - k * 1.9, x + k * 0.5, gy);
                    c.fill_style("#f0f6ff");
                    tri(
                        c,
                        x - k * 0.2,
                        gy - k * 1.2,
                        x,
                        gy - k * 1.9,
                        x + k * 0.2,
                        gy - k * 1.2,
                    );
                }
            }
            3 => {
                if kind < 0.75 {
                    c.fill_style("#0c2a3a");
                    c.begin();
                    c.ellipse(x, gy + k * 0.1, k * 0.7, k * 0.45, 0., PI, TAU);
                    c.fill();
                    c.stroke_style("rgba(220,250,255,0.6)");
                    c.width(2.);
                    c.begin();
                    c.move_to(x - k * 0.9, gy + k * 0.12);
                    c.quad(x, gy - k * 0.05, x + k * 0.9, gy + k * 0.12);
                    c.stroke();
                } else {
                    c.fill_style("#e8e0d0");
                    c.fill_rect(x - k * 0.15, gy - k * 0.7, k * 0.3, k * 0.7);
                    c.fill_style("#c0453f");
                    c.fill_rect(x - k * 0.15, gy - k * 0.45, k * 0.3, k * 0.18);
                    c.fill_style("#ffd76e");
                    circle(c, x, gy - k * 0.8, k * 0.12);
                }
            }
            4 => {
                c.fill_style("#241210");
                c.begin();
                c.move_to(x - k * 0.7, gy);
                c.line_to(x - k * 0.3, gy - k * 0.8);
                c.line_to(x + k * 0.2, gy - k * 0.6);
                c.line_to(x + k * 0.7, gy);
                c.close();
                c.fill();
                c.stroke_style("rgba(255,120,40,0.8)");
                c.width(2.);
                c.begin();
                c.move_to(x - k * 0.25, gy - k * 0.6);
                c.line_to(x - k * 0.05, gy - k * 0.3);
                c.line_to(x - k * 0.2, gy);
                c.stroke();
            }
            _ => {
                c.fill_style("rgba(240,236,255,0.85)");
                circle(c, x, gy - k * 0.15, k * 0.5);
                circle(c, x + k * 0.45, gy - k * 0.05, k * 0.38);
                circle(c, x - k * 0.45, gy - k * 0.02, k * 0.34);
            }
        }
    }
    let a = scroll_v / (s * 4.2);
    if a > 0.05 {
        c.stroke_style(format!("rgba(234,242,255,{})", 0.16 * a));
        c.width(2.);
        for k in 0..7 {
            let y = top + (0.08 + ((k as f64 * 0.37) % 1.) * 0.8) * (ground - top);
            let x = w + s * 2. - ((scroll * 1.8 + k as f64 * w * 0.41) % (w + s * 4.));
            let len = s * (1. + (k % 3) as f64 * 0.6) * a;
            c.begin();
            c.move_to(x, y);
            c.line_to(x + len, y);
            c.stroke();
        }
    }
}
