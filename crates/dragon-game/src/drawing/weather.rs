//! Source ambient weather and Daddio particles; never uses the gameplay RNG.
use super::{Canvas, circle, draw_star, ell};
use crate::flight::Viewport;
use serde::Serialize;
use std::f64::consts::{PI, TAU};

#[derive(Clone, Debug, Serialize)]
pub struct Mote {
    pub kind: &'static str,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub life: f64,
    pub max: f64,
    pub ph: f64,
    pub r: f64,
    pub rot: f64,
    pub vr: f64,
    pub g: f64,
    pub color: &'static str,
}
impl Default for Mote {
    fn default() -> Self {
        Self {
            kind: "",
            x: 0.,
            y: 0.,
            vx: 0.,
            vy: 0.,
            life: 0.,
            max: 0.,
            ph: 0.,
            r: 0.,
            rot: 0.,
            vr: 0.,
            g: 0.,
            color: "",
        }
    }
}
#[derive(Default)]
pub struct Weather {
    pub ambient: Vec<Mote>,
    pub trails: Vec<Mote>,
}
#[derive(Clone, Copy)]
pub struct WeatherFrame {
    pub view: Viewport,
    pub realm: usize,
    pub time: f64,
    pub scroll_v: f64,
    pub fancy: bool,
    pub in_sky: bool,
}
fn rand(rng: &mut impl FnMut() -> f64, a: f64, b: f64) -> f64 {
    a + (b - a) * rng()
}
impl Weather {
    /// Call exactly once per visual frame, including source pause frames.
    pub fn update(&mut self, dt: f64, f: WeatherFrame, rng: &mut impl FnMut() -> f64) {
        self.trails.retain_mut(|p| {
            p.life -= dt;
            if p.life <= 0. {
                return false;
            }
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            true
        });
        if !f.fancy || !f.in_sky {
            self.ambient.clear();
            return;
        }
        let v = f.view;
        let s = v.scale;
        let top = v.top_limit - s;
        let bottom = v.ground_y
            - s * if matches!(f.realm, 1 | 2 | 4) {
                2.6
            } else {
                0.5
            };
        let mut add = |p: Mote| {
            if self.ambient.len() < 90 {
                self.ambient.push(p);
            }
        };
        match f.realm {
            0 => {
                if rng() < dt * 22. {
                    add(Mote {
                        kind: "rain",
                        x: rand(rng, -s, v.width + s * 3.),
                        y: top,
                        vx: -s * 2.,
                        vy: s * rand(rng, 9., 12.),
                        life: 2.,
                        max: 2.,
                        ..Default::default()
                    });
                }
                let _sky_flash = rng() < dt / 9.;
            }
            1 => {
                // Source short-circuits the probability draw at fourteen fireflies.
                if self.ambient.iter().filter(|a| a.kind == "fly").count() < 14 && rng() < dt * 2. {
                    let p = Mote {
                        kind: "fly",
                        x: rand(rng, 0., v.width),
                        y: rand(rng, bottom - s * 3., v.ground_y - s * 0.4),
                        vx: rand(rng, -0.4, 0.4) * s,
                        vy: rand(rng, -0.3, 0.3) * s,
                        ph: rand(rng, 0., 6.),
                        life: rand(rng, 5., 9.),
                        max: 9.,
                        ..Default::default()
                    };
                    if self.ambient.len() < 90 {
                        self.ambient.push(p);
                    }
                }
                if rng() < dt * 1.3 {
                    let p = Mote {
                        kind: "leaf",
                        x: rand(rng, 0., v.width + s * 2.),
                        y: top,
                        vx: -s * rand(rng, 0.8, 1.6),
                        vy: s * rand(rng, 0.7, 1.3),
                        rot: rand(rng, 0., TAU),
                        vr: rand(rng, -4., 4.),
                        color: ["#8a5a2b", "#c8702e", "#6a8a3a", "#b8442a"]
                            [(rng() * 4.).floor() as usize],
                        life: 7.,
                        max: 7.,
                        ..Default::default()
                    };
                    if self.ambient.len() < 90 {
                        self.ambient.push(p);
                    }
                }
            }
            2 => {
                if rng() < dt * 16. {
                    add(Mote {
                        kind: "snow",
                        x: rand(rng, -s, v.width + s * 2.),
                        y: top,
                        vx: -s * 0.5,
                        vy: s * rand(rng, 1., 2.2),
                        r: rand(rng, 1.2, 3.),
                        ph: rand(rng, 0., 6.),
                        life: 9.,
                        max: 9.,
                        ..Default::default()
                    });
                }
            }
            3 => {
                if rng() < dt * 7. {
                    add(Mote {
                        kind: "spray",
                        x: rand(rng, 0., v.width),
                        y: v.ground_y + 2.,
                        vx: rand(rng, -1., 1.) * s,
                        vy: -s * rand(rng, 1.5, 3.2),
                        g: s * 7.,
                        life: 0.9,
                        max: 0.9,
                        ..Default::default()
                    });
                }
            }
            4 => {
                if rng() < dt * 9. {
                    add(Mote {
                        kind: "ember",
                        x: rand(rng, 0., v.width),
                        y: v.ground_y + rand(rng, 0., v.height - v.ground_y),
                        vx: rand(rng, -0.5, 0.5) * s,
                        vy: -s * rand(rng, 1., 2.4),
                        ph: rand(rng, 0., 6.),
                        life: rand(rng, 1.6, 3.),
                        max: 3.,
                        ..Default::default()
                    });
                }
            }
            _ => {
                if rng() < dt * 3. {
                    add(Mote {
                        kind: "twinkle",
                        x: rand(rng, 0., v.width),
                        y: rand(rng, v.top_limit, bottom),
                        vx: f64::NAN,
                        vy: f64::NAN,
                        life: 1.3,
                        max: 1.3,
                        ph: rand(rng, 0., 6.),
                        ..Default::default()
                    });
                }
                if rng() < dt / 6. {
                    add(Mote {
                        kind: "shoot",
                        x: rand(rng, v.width * 0.3, v.width + s),
                        y: v.top_limit + rand(rng, 0., v.height * 0.25),
                        vx: -s * 18.,
                        vy: s * 6.,
                        life: 0.7,
                        max: 0.7,
                        ..Default::default()
                    });
                }
            }
        }
        // Reverse integration preserves source random draws for the fireflies.
        for i in (0..self.ambient.len()).rev() {
            let a = &mut self.ambient[i];
            a.life -= dt;
            if a.life <= 0. {
                self.ambient.remove(i);
                continue;
            }
            a.vy += a.g * dt;
            a.x += (a.vx - f.scroll_v * 0.5) * dt
                + if matches!(a.kind, "snow" | "ember") {
                    (f.time * 1.5 + a.ph).sin() * s * 0.4 * dt
                } else {
                    0.
                };
            a.y += a.vy * dt;
            a.rot += a.vr * dt;
            if a.kind == "fly" {
                a.vx = (a.vx + rand(rng, -1., 1.) * s * dt).clamp(-s * 0.6, s * 0.6);
                a.vy = (a.vy + rand(rng, -1., 1.) * s * dt).clamp(-s * 0.4, s * 0.4);
                if a.y > v.ground_y - s * 0.3 {
                    a.vy = -a.vy.abs();
                }
            }
            if a.y > v.height + s
                || a.x < -s * 4.
                || (!matches!(a.kind, "fly" | "twinkle" | "ember" | "spray")
                    && a.y > v.ground_y + s * 0.5)
            {
                self.ambient.remove(i);
            }
        }
    }
    /// Source dadTrail emission, moved out of multipass painting. Call after update.
    /// Retains at most 512 trail particles as a presentation safety bound.
    pub fn dad_trail(
        &mut self,
        position: [f64; 2],
        size: f64,
        facing: f64,
        scale: f64,
        enabled: bool,
        rng: &mut impl FnMut() -> f64,
    ) {
        if !enabled || rng() > 0.7 {
            return;
        }
        let p = Mote {
            kind: "glow",
            x: position[0] - facing * size * (0.8 + rng() * 0.3),
            y: position[1] + rand(rng, -0.2, 0.2) * size,
            vx: -facing * scale * rand(rng, 0.5, 1.5),
            vy: rand(rng, -0.4, 0.4) * scale,
            life: 0.6,
            max: 0.6,
            r: size * rand(rng, 0.05, 0.11),
            color: ["#ffd76e", "#fff1a8", "#ffb347"][(rng() * 3.).floor() as usize],
            ..Default::default()
        };
        if self.trails.len() < 512 {
            self.trails.push(p);
        }
    }
    pub fn draw_ambient(&self, c: &mut Canvas, s: f64, time: f64) {
        c.save();
        for a in &self.ambient {
            let f = (a.life / a.max).min(1.);
            match a.kind {
                "rain" => {
                    c.alpha(0.35);
                    c.stroke_style("#9fc8ff");
                    c.width(1.5);
                    c.begin();
                    c.move_to(a.x, a.y);
                    c.line_to(a.x - s * 0.08, a.y + s * 0.45);
                    c.stroke();
                }
                "snow" => {
                    c.alpha(0.8 * (f * 3.).min(1.));
                    c.fill_style("#f4f8ff");
                    circle(c, a.x, a.y, a.r);
                }
                "fly" => {
                    c.additive(true);
                    c.alpha(0.5 + 0.5 * (time * 4. + a.ph).sin());
                    c.fill_style("#fff27a");
                    circle(c, a.x, a.y, s * 0.05);
                    c.multiply_alpha(0.35);
                    circle(c, a.x, a.y, s * 0.16);
                    c.additive(false);
                }
                "leaf" => {
                    c.alpha(0.9);
                    c.save();
                    c.translate(a.x, a.y);
                    c.rotate(a.rot);
                    c.fill_style(a.color);
                    ell(c, 0., 0., s * 0.14, s * 0.07);
                    c.restore();
                }
                "spray" => {
                    c.alpha(0.6 * f);
                    c.fill_style("#d8f4ff");
                    circle(c, a.x, a.y, s * 0.05);
                }
                "ember" => {
                    c.additive(true);
                    c.alpha(f * (0.6 + 0.4 * (time * 9. + a.ph).sin()));
                    c.fill_style("#ffb347");
                    circle(c, a.x, a.y, s * 0.05);
                    c.multiply_alpha(0.4);
                    c.fill_style("#ff6a3d");
                    circle(c, a.x, a.y, s * 0.14);
                    c.additive(false);
                }
                "twinkle" => {
                    c.alpha((f * PI).sin());
                    // Canvas translate(nonfinite,nonfinite) is a no-op, not an invisible star.
                    let (x, y) = if a.x.is_finite() && a.y.is_finite() {
                        (a.x, a.y)
                    } else {
                        (0., 0.)
                    };
                    draw_star(c, x, y, s * 0.12, "#fff6dd", a.ph);
                }
                "shoot" => {
                    c.additive(true);
                    let g = c.linear(
                        [a.x, a.y, a.x - a.vx * 0.12, a.y - a.vy * 0.12],
                        &[
                            (0., format!("rgba(255,246,221,{})", 0.9 * f)),
                            (1., "rgba(255,246,221,0)".into()),
                        ],
                    );
                    c.stroke_style(g);
                    c.width(2.5);
                    c.begin();
                    c.move_to(a.x, a.y);
                    c.line_to(a.x - a.vx * 0.12, a.y - a.vy * 0.12);
                    c.stroke();
                    c.additive(false);
                }
                _ => {}
            }
        }
        c.restore();
    }
    pub fn draw_trails(&self, c: &mut Canvas) {
        c.save();
        c.additive(true);
        for p in &self.trails {
            let a = (p.life / p.max).max(0.);
            c.alpha(a);
            c.fill_style(p.color);
            circle(c, p.x, p.y, p.r * a + 0.5);
        }
        c.restore();
    }
}
