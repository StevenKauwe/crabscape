//! Actual mission objective rules. Geometry is deterministic and all randomness is
//! supplied by the caller. UI/audio/particles are explicit events; no saves or network.
use crate::flight::Viewport;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Protect,
    Escort,
    Carry,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub kind: Kind,
    pub what: &'static str,
    pub name: &'static str,
    pub x: f64,
    pub to: Option<f64>,
    pub dur: f64,
    pub hp: u8,
    pub say: &'static str,
}
pub fn specification(mission: &str, stop: usize) -> Option<Spec> {
    let (kind, what, name, x, to, dur, hp, say) =
        match (mission.strip_suffix('c').unwrap_or(mission), stop) {
            ("m3", 3) => (
                Kind::Protect,
                "barn",
                "the hay barn",
                0.5,
                None,
                40.,
                7,
                "Protect the hay barn! Keep the sprites off it!",
            ),
            ("m6", 3) => (
                Kind::Escort,
                "fawn",
                "the fawn",
                0.06,
                Some(0.64),
                45.,
                5,
                "The fawn is running for the stream. Keep the wolves off it!",
            ),
            ("m11", 3) => (
                Kind::Escort,
                "goat",
                "the mountain goats",
                0.05,
                Some(0.64),
                50.,
                5,
                "The goat family is running for the ledge. Guard them!",
            ),
            ("m14", 3) => (
                Kind::Protect,
                "lamp",
                "the lighthouse lamp",
                0.05,
                None,
                40.,
                6,
                "Guard the lamp! If the Splashfins smash it, the boats are lost!",
            ),
            ("m15", 2) => (
                Kind::Protect,
                "boats",
                "the fishing boats",
                0.45,
                None,
                40.,
                7,
                "The boats are right below you. Don't let anything touch them!",
            ),
            ("m17", 3) => (
                Kind::Carry,
                "star",
                "the star piece",
                0.1,
                Some(0.6),
                40.,
                0,
                "Grab the star piece and carry it to the glowing ring!",
            ),
            ("m18", 3) => (
                Kind::Carry,
                "star",
                "the star piece",
                0.12,
                Some(0.62),
                40.,
                0,
                "The imps dropped the star piece! Carry it to the ring before they grab it back!",
            ),
            ("m21", 3) => (
                Kind::Protect,
                "garden",
                "the star garden",
                0.52,
                None,
                40.,
                7,
                "Protect the star garden! The gobblers want every sparkle in it!",
            ),
            _ => return None,
        };
    Some(Spec {
        kind,
        what,
        name,
        x,
        to,
        dur,
        hp,
        say,
    })
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Objective {
    pub kind: Kind,
    pub what: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub x0: f64,
    pub to: f64,
    pub dur: f64,
    pub t: f64,
    pub hp: u8,
    pub max_hp: u8,
    pub carrier: i32,
    pub done: bool,
    pub vx: f64,
    pub vy: f64,
    pub steal_cd: f64,
    pub hurt_t: f64,
    pub r: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Dropped,
    FirstHurt,
    Hurt { hp: u8, by: String },
    Failed(String),
    Arrived,
    Delivered(i32),
    PickedUp(i32),
    Stolen(u64),
}
#[derive(Clone, Copy, Debug)]
pub struct Rider {
    pub id: i32,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Default)]
pub struct Enemy {
    pub id: u64,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub dir: f64,
    pub dead: bool,
    pub calm: bool,
    pub boss: bool,
    pub stun: f64,
    pub ground: bool,
    pub ob_cd: f64,
}
#[derive(Clone, Debug, Default)]
pub struct Hazard {
    pub id: u64,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub warn: f64,
}
#[derive(Clone, Debug, Default)]
pub struct Strike {
    pub id: u64,
    pub x: f64,
    pub y0: Option<f64>,
    pub warn: f64,
    pub ob_hit: bool,
}
pub struct World<'a> {
    pub view: Viewport,
    pub mission_done: bool,
    pub mission_failing: bool,
    pub riders: &'a [Rider],
    pub enemies: &'a mut [Enemy],
    pub hazards: &'a mut Vec<Hazard>,
    pub strikes: &'a mut [Strike],
}
fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    lo.max(hi.min(v))
}
fn y_for(what: &str, v: Viewport) -> f64 {
    match what {
        "lamp" => v.ground_y - v.height * 0.31 - v.scale * 1.95,
        "boats" => v.ground_y + v.scale * 0.15,
        "star" => v.ground_y - v.scale * 2.2,
        _ => {
            v.ground_y
                - v.scale
                    * match what {
                        "garden" => 0.4,
                        "barn" => 0.8,
                        _ => 0.55,
                    }
        }
    }
}
impl Objective {
    pub fn new(spec: &Spec, v: Viewport) -> Self {
        Self {
            kind: spec.kind,
            what: spec.what.into(),
            name: spec.name.into(),
            x: spec.x * v.width,
            y: y_for(spec.what, v),
            x0: spec.x * v.width,
            to: spec.to.unwrap_or(spec.x) * v.width,
            dur: if spec.dur == 0. { 40. } else { spec.dur },
            t: 0.,
            hp: spec.hp,
            max_hp: spec.hp,
            carrier: -1,
            done: false,
            vx: 0.,
            vy: 0.,
            steal_cd: 0.,
            hurt_t: 0.,
            r: v.scale
                * match spec.what {
                    "boats" => 1.6,
                    "star" => 0.45,
                    _ => 0.9,
                },
        }
    }
    pub fn drop_piece(
        &mut self,
        v: Viewport,
        draw: &mut impl FnMut() -> f64,
        events: &mut Vec<Event>,
    ) {
        if self.kind != Kind::Carry || self.carrier < 0 || self.done {
            return;
        }
        self.carrier = -1;
        self.vx = (-3. + draw() * 6.) * v.scale;
        self.vy = -v.scale * 4.;
        self.steal_cd = 1.;
        events.push(Event::Dropped);
    }
    pub fn hurt(&mut self, why: &str, events: &mut Vec<Event>) {
        if self.done || self.hurt_t > 0. {
            return;
        }
        self.hp = self.hp.saturating_sub(1);
        self.hurt_t = 3.5;
        if self.max_hp > 0 && self.hp == self.max_hp - 1 {
            events.push(Event::FirstHurt);
        }
        events.push(Event::Hurt {
            hp: self.hp,
            by: why.into(),
        });
        if self.hp == 0 {
            let name = self
                .name
                .strip_prefix("the ")
                .map_or_else(|| self.name.clone(), |s| format!("The {s}"));
            events.push(Event::Failed(format!(
                "{name} {} lost!",
                if matches!(self.what.as_str(), "boats" | "goat") {
                    "were"
                } else {
                    "was"
                }
            )));
        }
    }
    pub fn update(
        &mut self,
        dt: f64,
        w: &mut World<'_>,
        draw: &mut impl FnMut() -> f64,
        events: &mut Vec<Event>,
    ) {
        if w.mission_done || w.mission_failing {
            return;
        }
        let v = w.view;
        let s = v.scale;
        self.t += dt;
        self.hurt_t -= dt;
        self.steal_cd -= dt;
        if self.kind == Kind::Escort {
            self.x = self.x0 + (self.to - self.x0) * (self.t / self.dur).min(1.);
            self.y = y_for(&self.what, v)
                - if self.t < self.dur {
                    (self.t * 7.).sin().abs() * s * 0.25
                } else {
                    0.
                };
            if self.t >= self.dur && !self.done {
                self.done = true;
                events.push(Event::Arrived);
            }
        }
        if self.kind == Kind::Carry {
            if self.done {
                return;
            }
            if self.carrier >= 0 {
                if let Some(r) = w.riders.iter().find(|r| r.id == self.carrier) {
                    self.x = r.x;
                    self.y = r.y - s * 1.1;
                    if (r.x - self.to).abs() < s * 1.4 {
                        self.done = true;
                        self.x = self.to;
                        self.y = v.ground_y - s * 2.2;
                        events.push(Event::Delivered(r.id));
                    }
                } else {
                    self.drop_piece(v, draw, events);
                }
            } else {
                self.vy += s * 9. * dt;
                self.vx *= 1. - (dt * 1.5).min(1.);
                self.x += self.vx * dt;
                self.y += self.vy * dt;
                let floor = v.ground_y - s * 1.2;
                if self.y > floor {
                    self.y = floor;
                    self.vy = 0.;
                }
                self.x = clamp(self.x, s, v.width - s);
                for r in w.riders {
                    if self.steal_cd <= 0. && (r.x - self.x).hypot(r.y - self.y) < s * 1.3 {
                        self.carrier = r.id;
                        events.push(Event::PickedUp(r.id));
                        break;
                    }
                }
                if self.carrier < 0 && self.steal_cd <= 0. {
                    for e in w.enemies.iter() {
                        if !e.dead
                            && !e.calm
                            && !e.boss
                            && (e.x - self.x).hypot(e.y - self.y) < e.r + s * 0.6
                        {
                            self.x = clamp(
                                e.x + e.dir * s * 5. + (-s + draw() * s * 2.),
                                s * 1.5,
                                v.width - s * 1.5,
                            );
                            self.y = clamp(e.y - s, v.top_limit + s, v.ground_y - s * 1.2);
                            self.vx = 0.;
                            self.vy = 0.;
                            self.steal_cd = 2.5;
                            events.push(Event::Stolen(e.id));
                            break;
                        }
                    }
                }
            }
            return;
        }
        for e in w.enemies.iter_mut() {
            if e.dead || e.calm || e.stun > 0. {
                continue;
            }
            e.ob_cd -= dt;
            if e.ob_cd <= 0. && (e.x - self.x).hypot(e.y - self.y) < e.r + self.r {
                e.ob_cd = 3.;
                self.hurt(&e.kind, events);
                if !e.ground {
                    e.x -= e.dir * s * 0.5;
                }
            }
        }
        for i in (0..w.hazards.len()).rev() {
            let h = &w.hazards[i];
            if h.warn <= 0. && (h.x - self.x).hypot(h.y - self.y) < h.r + self.r {
                let h = w.hazards.remove(i);
                self.hurt(&h.kind, events);
            }
        }
        for st in w.strikes.iter_mut() {
            if !st.ob_hit
                && st.warn <= 0.
                && (st.x - self.x).abs() < self.r
                && st.y0.is_none_or(|y| self.y > y - s)
            {
                st.ob_hit = true;
                self.hurt("zap", events);
            }
        }
    }
}
