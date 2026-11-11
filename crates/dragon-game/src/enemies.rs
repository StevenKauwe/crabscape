//! Enemy definitions/spawning and all twenty source movement/attack brains.
//! Caller resolves `retarget`/`aimFor`, safety, mission scaling and viewport geometry;
//! effects, hazard simulation, damage, multiplayer and presentation remain outside this core.
//! Inject gameplay draws in source order (presentation/retarget draws are not consumed).
//! Spawn/removal effects must be applied before advancing the next enemy. Deferred
//! events do not reproduce the original shared presentation/spawn RNG stream.
//! Touch/sting events are requests: consumers must recheck current safety because
//! an earlier bonk can grant invulnerability before the next request is applied.
use crate::projectiles::Target;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Cloud,
    Bigcloud,
    Bat,
    Sprite,
    Urgal,
    Wolf,
    Troll,
    Thunderhead,
    Chieftain,
    Cavetroll,
    Jelly,
    Leaper,
    Serpent,
    Slime,
    Slimelet,
    Imp,
    Bigslime,
    Gobbler,
    Moth,
    Swallower,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Definition {
    pub radius: f64,
    pub hp: f64,
    pub speed: f64,
    pub gems: u32,
    pub ground: bool,
    pub armor: f64,
    pub boss: bool,
    pub from_sea: bool,
    pub hop: bool,
}
impl Kind {
    pub fn definition(self) -> Definition {
        use Kind::*;
        let (radius, hp, speed, gems) = match self {
            Cloud => (0.95, 1., 1.8, 2),
            Bigcloud => (1.3, 2.6, 1.4, 3),
            Bat => (0.42, 0.6, 4., 1),
            Sprite => (0.45, 1.3, 3., 2),
            Urgal => (0.72, 3., 1.3, 3),
            Wolf => (0.55, 1.6, 4.4, 2),
            Troll => (1.1, 8., 0.7, 5),
            Thunderhead => (2.2, 40., 0.5, 25),
            Chieftain => (1.3, 46., 1.1, 25),
            Cavetroll => (1.75, 64., 0.55, 25),
            Jelly => (0.6, 1.6, 1.1, 2),
            Leaper => (0.45, 1.1, 2.2, 2),
            Serpent => (1.9, 56., 0.5, 25),
            Slime => (0.62, 2.2, 2.2, 2),
            Slimelet => (0.36, 0.8, 3., 1),
            Imp => (0.42, 1.3, 3.2, 2),
            Bigslime => (1.8, 32., 2.4, 25),
            Gobbler => (0.5, 1.5, 2.8, 2),
            Moth => (0.6, 2., 1.6, 2),
            Swallower => (2., 64., 0.9, 25),
        };
        Definition {
            radius,
            hp,
            speed,
            gems,
            ground: matches!(
                self,
                Urgal | Wolf | Troll | Chieftain | Cavetroll | Slime | Slimelet | Bigslime
            ),
            armor: match self {
                Urgal | Troll => 0.6,
                Chieftain | Cavetroll => 0.8,
                _ => 0.,
            },
            boss: matches!(
                self,
                Thunderhead | Chieftain | Cavetroll | Serpent | Bigslime | Swallower
            ),
            from_sea: matches!(self, Jelly | Leaper),
            hop: matches!(self, Slime | Slimelet | Bigslime),
        }
    }
    pub fn supported(self) -> bool {
        true
    }
}
#[derive(Clone, Copy, Debug)]
pub struct World {
    pub width: f64,
    pub ground_y: f64,
    pub top: f64,
    pub sky_bottom: f64,
    pub scale: f64,
    pub player: [f64; 2],
    pub aim: [f64; 2],
    pub quiet: bool,
    pub safe: bool,
    pub dragon_scale: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Gem {
    pub id: u64,
    pub position: [f64; 2],
    pub radius: f64,
}
/// Snapshot refreshed between enemies, after applying preceding events.
#[derive(Clone, Copy, Debug, Default)]
pub struct Context<'a> {
    pub gems: &'a [Gem],
    pub sibling_boss_damage: f64,
    pub active_extras: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct MissionScaling {
    pub difficulty: f64,
    pub toughness: f64,
    pub sky_toughness: f64,
    pub speed: f64,
}
impl Default for MissionScaling {
    fn default() -> Self {
        Self {
            difficulty: 1.,
            toughness: 1.,
            sky_toughness: 1.,
            speed: 1.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Enemy {
    pub kind: Kind,
    pub target: Target,
    pub extra: bool,
    pub entered: bool,
    pub dir: f64,
    pub t: f64,
    pub wob: f64,
    pub speed: f64,
    pub cool: f64,
    pub cool2: f64,
    pub strike_cd: Option<f64>,
    pub pre: f64,
    pub lunge: f64,
    pub lunge_velocity: [f64; 2],
    pub base_y: Option<f64>,
    pub retarget: f64,
    pub destination: Option<[f64; 2]>,
    pub hurt: bool,
    pub air: bool,
    pub velocity: [f64; 2],
    pub windup: f64,
    pub sting: f64,
    pub risen: bool,
    pub center: Option<[f64; 2]>,
    pub split: Option<f64>,
    pub slam: bool,
    pub squash: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HazardKind {
    Rock,
    Boulder,
    Ember,
    Bubble,
    Spark,
    Dust,
    Fall,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hazard {
    pub kind: HazardKind,
    pub position: [f64; 2],
    pub velocity: [f64; 2],
    pub gravity: f64,
    pub radius: f64,
    pub life: f64,
    pub warn: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Strike {
        x: f64,
        y0: Option<f64>,
        warn: f64,
        bolt: f64,
    },
    Spark {
        position: [f64; 2],
        velocity: [f64; 2],
        radius: f64,
        life: f64,
    },
    Spawn {
        kind: Kind,
        extra: bool,
        at: [f64; 2],
    },
    Touch {
        id: u64,
        hurts: bool,
    },
    Hazard(Hazard),
    /// Source `spawnEnemy('leaper', true)` chooses its own sea position.
    SpawnSea {
        kind: Kind,
        extra: bool,
    },
    /// Source `spawnBaby`: spawn a slimelet, then apply these initial conditions.
    SpawnBaby {
        at: [f64; 2],
        velocity: [f64; 2],
        dir: f64,
        extra: bool,
    },
    Gobble {
        gem_id: u64,
    },
    Sting {
        id: u64,
    },
    Ring {
        position: [f64; 2],
        radius: f64,
        duration: f64,
        kind: Kind,
    },
    Splash {
        x: f64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedBehavior(pub Kind);
fn rand(draw: &mut impl FnMut() -> f64, a: f64, b: f64) -> f64 {
    a + draw() * (b - a)
}
fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    lo.max(hi.min(v))
}
impl Enemy {
    /// `at` is a source spawn position. Ground spawns ignore its y component.
    pub fn spawn(
        kind: Kind,
        id: u64,
        extra: bool,
        at: Option<[f64; 2]>,
        w: &World,
        m: MissionScaling,
        draw: &mut impl FnMut() -> f64,
    ) -> Self {
        let d = kind.definition();
        let r = w.scale * d.radius;
        let hp = d.hp
            * m.difficulty
            * (if d.boss { 1.35 } else { 1. })
            * (if m.toughness == 0. { 1. } else { m.toughness })
            * m.sky_toughness;
        let left = at.map_or_else(|| draw() < 0.5, |p| p[0] < w.width / 2.);
        let t = rand(draw, 0., 6.);
        let wob = rand(draw, 0., 6.);
        let mut cool = rand(draw, 1.6, 3.);
        let cool2 = rand(draw, 5., 7.);
        let speed = d.speed * w.scale * m.speed * rand(draw, 0.88, 1.12);
        if d.hop {
            cool = rand(draw, 0.2, 0.6);
        }
        let mut entered = false;
        let (x, y) = match kind {
            Kind::Thunderhead | Kind::Serpent | Kind::Swallower => {
                entered = true;
                (
                    w.width / 2.,
                    if kind == Kind::Serpent {
                        w.ground_y - r * 0.8
                    } else {
                        w.top
                            + r * (if kind == Kind::Thunderhead {
                                0.85
                            } else {
                                0.95
                            })
                    },
                )
            }
            _ if d.from_sea && at.is_none() => {
                entered = true;
                let mut x = rand(draw, w.scale * 1.5, w.width - w.scale * 1.5);
                for _ in 0..6 {
                    if (x - w.player[0]).abs() >= w.scale * 3. {
                        break;
                    }
                    x = rand(draw, w.scale * 1.5, w.width - w.scale * 1.5);
                }
                (x, w.ground_y + r * 1.2)
            }
            _ => {
                let x = at.map_or(if left { -r } else { w.width + r }, |p| p[0]);
                let y = if d.ground {
                    w.ground_y - r * 0.95
                } else {
                    at.map_or_else(
                        || {
                            rand(
                                draw,
                                w.top + r,
                                (w.top + r + 1.).max(w.sky_bottom - r * 0.5),
                            )
                        },
                        |p| p[1],
                    )
                };
                (x, y)
            }
        };
        Self {
            kind,
            target: Target {
                id,
                x,
                y,
                r,
                hp,
                max_hp: hp,
                boss: d.boss,
                armor: d.armor,
                ..Target::default()
            },
            extra,
            entered,
            dir: if left { 1. } else { -1. },
            t,
            wob,
            speed,
            cool,
            cool2,
            strike_cd: None,
            pre: 0.,
            lunge: 0.,
            lunge_velocity: [0.; 2],
            base_y: None,
            retarget: 0.,
            destination: None,
            hurt: false,
            air: false,
            velocity: [0.; 2],
            windup: 0.,
            sting: 0.,
            risen: false,
            center: None,
            split: None,
            slam: false,
            squash: 0.,
        }
    }
    pub fn step(
        &mut self,
        dt: f64,
        w: &World,
        draw: &mut impl FnMut() -> f64,
        events: &mut Vec<Event>,
    ) -> Result<(), UnsupportedBehavior> {
        self.step_with_context(dt, w, Context::default(), draw, events)
    }
    pub fn step_with_context(
        &mut self,
        dt: f64,
        w: &World,
        context: Context<'_>,
        draw: &mut impl FnMut() -> f64,
        events: &mut Vec<Event>,
    ) -> Result<(), UnsupportedBehavior> {
        if !self.kind.supported() {
            return Err(UnsupportedBehavior(self.kind));
        }
        let e = &mut self.target;
        if e.dead {
            return Ok(());
        }
        self.t += dt;
        e.flash -= dt;
        e.slow -= dt;
        let sm = if e.slow > 0. { 0.35 } else { 1. };
        let s = w.scale;
        if e.calm {
            if self.kind.definition().ground {
                e.x += self.dir * s * 3. * dt;
                e.y = w.ground_y - e.r * 0.95;
            } else {
                e.y -= s * 1.2 * dt;
                e.x += self.dir * s * 0.8 * dt;
            }
            if e.x < -e.r * 3. || e.x > w.width + e.r * 3. || e.y < -e.r * 3. {
                e.dead = true;
            }
            return Ok(());
        }
        if e.stun > 0. {
            e.stun -= dt;
            self.hurt = false;
            return Ok(());
        }
        if e.x > e.r && e.x < w.width - e.r {
            self.entered = true;
        }
        self.hurt = false;
        if self.sting > 0. {
            self.sting -= dt;
        }
        if e.cracked > 0. {
            e.cracked -= dt;
        }
        if e.marked > 0. {
            e.marked -= dt;
        }
        if matches!(
            self.kind,
            Kind::Cloud | Kind::Bigcloud | Kind::Bat | Kind::Jelly | Kind::Gobbler | Kind::Moth
        ) {
            let cd = self.strike_cd.get_or_insert_with(|| rand(draw, 1.2, 2.6));
            *cd -= dt;
            let dx = w.aim[0] - e.x;
            let dy = w.aim[1] - e.y;
            let close = if matches!(self.kind, Kind::Cloud | Kind::Bigcloud) {
                dx.abs() <= s * 1.6 && dy >= -s
            } else {
                dx.abs() <= s * 3.4 && dy.abs() <= s * 3.
            };
            if !w.quiet && *cd <= 0. && self.entered && self.pre <= 0. && self.lunge <= 0. && close
            {
                *cd = if self.kind == Kind::Bat {
                    rand(draw, 3.2, 4.6)
                } else if self.kind == Kind::Moth {
                    rand(draw, 3.5, 5.)
                } else {
                    rand(draw, 2.6, 3.8)
                };
                self.pre = match self.kind {
                    Kind::Bat => 0.5,
                    Kind::Jelly => 0.7,
                    _ => 0.6,
                };
            }
        }
        if self.lunge > 0. {
            self.lunge -= dt;
            self.hurt = true;
            e.x += self.lunge_velocity[0] * dt;
            e.y += self.lunge_velocity[1] * dt;
            e.x = clamp(e.x, e.r, w.width - e.r);
            e.y = clamp(e.y, w.top + e.r * 0.5, (w.top + e.r).max(w.sky_bottom));
            if self.kind == Kind::Bat {
                self.base_y = Some(e.y);
            }
        } else if self.pre > 0. {
            self.pre -= dt;
            e.x += (self.t * 40.).sin() * s * 0.5 * dt;
            if self.pre <= 0. {
                if matches!(self.kind, Kind::Bat | Kind::Gobbler) {
                    let dx = w.aim[0] - e.x;
                    let dy = w.aim[1] - e.y;
                    let d = dx.hypot(dy);
                    let d = if d == 0. { 1. } else { d };
                    self.lunge = 0.42;
                    let sp = s * if self.kind == Kind::Bat { 9. } else { 8. };
                    self.lunge_velocity = [dx / d * sp, dy / d * sp];
                    self.dir = if dx >= 0. { 1. } else { -1. };
                } else if matches!(self.kind, Kind::Jelly | Kind::Moth) {
                    self.sting = 0.4;
                    events.push(Event::Ring {
                        position: [e.x, e.y],
                        radius: e.r * 1.9,
                        duration: 0.4,
                        kind: self.kind,
                    });
                } else {
                    events.push(Event::Strike {
                        x: e.x,
                        y0: Some(e.y + e.r * 0.5),
                        warn: 0.45,
                        bolt: 0.25,
                    });
                }
            }
        } else {
            match self.kind {
                Kind::Cloud | Kind::Bigcloud | Kind::Bat => {
                    if self.kind == Kind::Bat {
                        let base = self.base_y.get_or_insert(e.y);
                        *base += (w.aim[1] - *base) * (dt * 0.35).min(1.);
                    }
                    e.x += self.dir * self.speed * sm * dt;
                    if self.entered {
                        if e.x < e.r {
                            self.dir = 1.;
                        } else if e.x > w.width - e.r {
                            self.dir = -1.;
                        }
                    }
                    e.y = if self.kind == Kind::Bat {
                        clamp(
                            self.base_y.unwrap_or(e.y) + (self.t * 3. + self.wob).sin() * s * 0.8,
                            w.top + e.r,
                            (w.top + e.r).max(w.sky_bottom),
                        )
                    } else {
                        clamp(
                            e.y + (self.t * 1.4 + self.wob).sin() * s * 0.35 * dt,
                            w.top + e.r * 0.6,
                            (w.top + e.r).max(w.sky_bottom - e.r * 0.4),
                        )
                    };
                }
                Kind::Sprite => {
                    self.retarget -= dt;
                    if self.retarget <= 0. || self.destination.is_none() {
                        self.destination = Some([
                            clamp(w.aim[0] + rand(draw, -2.5, 2.5) * s, e.r, w.width - e.r),
                            clamp(
                                w.aim[1] + rand(draw, -2., 2.) * s,
                                w.top + e.r,
                                (w.top + e.r).max(w.sky_bottom),
                            ),
                        ]);
                        self.retarget = 0.9;
                    }
                    let p = self.destination.unwrap_or(w.aim);
                    let dx = p[0] - e.x;
                    let dy = p[1] - e.y;
                    let d = dx.hypot(dy);
                    let d = if d == 0. { 1. } else { d };
                    let step = d.min(self.speed * sm * dt);
                    e.x += dx / d * step;
                    e.y += dy / d * step;
                    self.dir = if dx >= 0. { 1. } else { -1. };
                    self.entered = true;
                    self.cool -= dt;
                    if self.cool <= 0. {
                        if !w.quiet {
                            let dx = w.aim[0] - e.x;
                            let dy = w.aim[1] - e.y;
                            let d = dx.hypot(dy);
                            let d = if d == 0. { 1. } else { d };
                            events.push(Event::Spark {
                                position: [e.x, e.y],
                                velocity: [dx / d * s * 6., dy / d * s * 6.],
                                radius: s * 0.14,
                                life: 2.5,
                            });
                        }
                        self.cool = rand(draw, 2.4, 3.2);
                    }
                }
                Kind::Thunderhead => {
                    e.x = w.width / 2. + (self.t * 0.45).sin() * (w.width / 2. - e.r - s).max(0.);
                    e.y = w.top + e.r * 0.85 + (self.t * 1.3).sin() * s * 0.2;
                    self.cool -= dt;
                    if self.cool <= 0. {
                        if !w.quiet {
                            events.push(Event::Strike {
                                x: w.aim[0],
                                y0: None,
                                warn: 0.9,
                                bolt: 0.25,
                            });
                        }
                        self.cool = 2.6;
                    }
                    self.cool2 -= dt;
                    if self.cool2 <= 0. {
                        for x in [e.x - e.r, e.x + e.r] {
                            events.push(Event::Spawn {
                                kind: Kind::Cloud,
                                extra: true,
                                at: [x, e.y + e.r * 0.8],
                            });
                        }
                        self.cool2 = 7.;
                    }
                }
                Kind::Wolf => {
                    if self.air {
                        self.velocity[1] += s * 18. * dt;
                        e.x += self.velocity[0] * dt;
                        e.y += self.velocity[1] * dt;
                        if e.y >= w.ground_y - e.r * 0.95 {
                            e.y = w.ground_y - e.r * 0.95;
                            self.air = false;
                            self.squash = 0.3;
                        }
                    } else {
                        let dx = w.aim[0] - e.x;
                        if dx.abs() < s * 5. && self.entered {
                            self.dir = direction(dx);
                        }
                        e.x += self.dir * self.speed * sm * dt;
                        edge(e, &mut self.dir, self.entered, w);
                        e.y = w.ground_y - e.r * 0.95;
                        self.cool -= dt;
                        if !w.quiet
                            && self.cool <= 0.
                            && dx.abs() < s * 2.8
                            && w.aim[1] > w.ground_y - s * 5.
                        {
                            self.air = true;
                            self.velocity = [self.dir * s * 5. * sm, -s * 10.];
                            self.cool = 2.2;
                        }
                    }
                }
                Kind::Jelly => {
                    if !self.risen {
                        e.y -= s * 2.4 * dt;
                        if e.y < w.ground_y - e.r * 1.5 {
                            self.risen = true;
                        }
                    } else {
                        let phase = (self.t * 0.7 + self.wob) % 1.;
                        let up = phase < 0.3 && e.y > w.aim[1] - s;
                        e.y += (if up { -s * 2.6 } else { s * 0.7 }) * sm * dt;
                        if (w.aim[0] - e.x).abs() > s * 0.5 {
                            self.dir = if w.aim[0] > e.x { 1. } else { -1. };
                        }
                        e.x += self.dir * self.speed * sm * dt;
                        e.y = clamp(e.y, w.top + e.r, (w.top + e.r).max(w.sky_bottom));
                    }
                }
                Kind::Leaper => {
                    if self.air {
                        self.velocity[1] += s * 9. * dt;
                        e.x += self.velocity[0] * sm * dt;
                        e.y += self.velocity[1] * sm * dt;
                        if self.velocity[1] > 0. && e.y > w.ground_y + e.r {
                            self.air = false;
                            e.y = w.ground_y + e.r * 1.1;
                            self.cool = rand(draw, 1.4, 2.4);
                            events.push(Event::Splash { x: e.x });
                        }
                    } else {
                        e.y = w.ground_y + e.r * 1.1;
                        if (w.aim[0] - e.x).abs() > s * 0.4 {
                            self.dir = if w.aim[0] > e.x { 1. } else { -1. };
                            e.x += self.dir * self.speed * sm * dt;
                        }
                        e.x = clamp(e.x, e.r, w.width - e.r);
                        self.cool -= dt;
                        self.windup = if self.cool < 0.6 && !w.quiet { 0.6 } else { 0. };
                        if self.cool <= 0. && !w.quiet {
                            let g = s * 9.;
                            let h = (s * 2.).max(w.ground_y - w.aim[1] + s * 0.6);
                            let vy = -(2. * g * h).sqrt();
                            let up = -vy / g;
                            self.air = true;
                            self.velocity = [clamp((w.aim[0] - e.x) / up, -s * 7., s * 7.), vy];
                            self.dir = direction(self.velocity[0]);
                            self.windup = 0.;
                            events.push(Event::Splash { x: e.x });
                        } else if self.cool <= 0. {
                            self.cool = 1.;
                        }
                    }
                }
                Kind::Serpent => {
                    e.x = w.width / 2.
                        + (self.t * 0.35).sin() * (w.width / 2. - e.r - s * 1.5).max(0.);
                    self.dir = direction(w.aim[0] - e.x);
                    let dip = if self.windup > 0. {
                        ((1.2 - self.windup) / 1.2).min(1.)
                    } else {
                        0.
                    };
                    e.y = w.ground_y - e.r * (0.8 - dip * 0.9) + (self.t * 1.2).sin() * s * 0.25;
                    self.cool -= dt;
                    if self.cool <= 0. {
                        if !w.quiet {
                            fan(e, self.dir, w, HazardKind::Bubble, events);
                        }
                        self.cool = rand(draw, 2.2, 2.8);
                    }
                    if self.windup > 0. {
                        self.windup -= dt;
                        if self.windup <= 0. {
                            boss_special(self.kind, e, w, context.active_extras, draw, events);
                        }
                    } else {
                        self.cool2 -= dt;
                        if self.cool2 <= 0. {
                            self.windup = 1.2;
                            self.cool2 = 8.;
                        }
                    }
                }
                Kind::Slime | Kind::Slimelet | Kind::Bigslime => {
                    let base = w.ground_y - e.r * 0.95;
                    if self.air {
                        self.velocity[1] += s * 16. * dt;
                        e.x += self.velocity[0] * sm * dt;
                        e.y += self.velocity[1] * sm * dt;
                        if self.entered {
                            if e.x < e.r {
                                e.x = e.r;
                                self.velocity[0] = self.velocity[0].abs();
                                self.dir = 1.;
                            } else if e.x > w.width - e.r {
                                e.x = w.width - e.r;
                                self.velocity[0] = -self.velocity[0].abs();
                                self.dir = -1.;
                            }
                        }
                        if self.velocity[1] > 0. && e.y >= base {
                            e.y = base;
                            self.air = false;
                            self.squash = 0.3;
                            self.cool = if e.boss {
                                rand(draw, 0.8, 1.2)
                            } else {
                                rand(draw, 0.35, 0.9)
                            };
                            if self.slam {
                                self.slam = false;
                                for _ in 0..6 {
                                    events.push(Event::Hazard(Hazard {
                                        kind: HazardKind::Ember,
                                        position: [
                                            e.x + rand(draw, -0.5, 0.5) * e.r,
                                            e.y - e.r * 0.6,
                                        ],
                                        velocity: [
                                            rand(draw, -1., 1.) * s * 5.,
                                            -s * rand(draw, 5., 8.),
                                        ],
                                        gravity: s * 9.,
                                        radius: s * 0.2,
                                        life: 3.,
                                        warn: 0.,
                                    }));
                                }
                            }
                        }
                    } else {
                        e.y = base;
                        self.cool -= dt;
                        if !self.entered {
                            e.x += self.dir * self.speed * dt;
                        } else if self.cool <= 0. {
                            let dx = w.aim[0] - e.x;
                            if dx.abs() < s * 9. {
                                self.dir = direction(dx);
                            }
                            let high =
                                !w.quiet && dx.abs() < s * 3. && w.aim[1] > w.ground_y - s * 5.5;
                            self.air = true;
                            self.velocity = [
                                self.dir * self.speed * if high { 1.3 } else { 1. },
                                -s * if high { 9. } else { 5.2 },
                            ];
                        }
                    }
                    if e.boss {
                        let split = self.split.get_or_insert(0.8);
                        if (e.hp - context.sibling_boss_damage) / e.max_hp < *split {
                            *split -= 0.2;
                            if context.active_extras < 5 {
                                for k in [-1., 1.] {
                                    events.push(Event::SpawnBaby {
                                        at: [
                                            clamp(e.x + k * e.r * 0.7, s, w.width - s),
                                            w.ground_y - s * 0.36 * 0.95,
                                        ],
                                        velocity: [k * s * 3., -s * 6.],
                                        dir: k,
                                        extra: true,
                                    });
                                }
                            }
                        }
                        self.cool2 -= dt;
                        if self.cool2 <= 0. && !self.air && self.entered {
                            self.cool2 = 8.;
                            if !w.quiet {
                                self.slam = true;
                                self.air = true;
                                self.velocity =
                                    [clamp((w.aim[0] - e.x) / 1.5, -s * 6., s * 6.), -s * 12.];
                            }
                        }
                    }
                }
                Kind::Imp => {
                    self.retarget -= dt;
                    if self.retarget <= 0. || self.destination.is_none() {
                        let side = if draw() < 0.5 { -1. } else { 1. };
                        self.destination = Some([
                            clamp(
                                w.aim[0] + side * rand(draw, 3., 4.5) * s,
                                e.r,
                                w.width - e.r,
                            ),
                            clamp(
                                w.aim[1] - rand(draw, 0.5, 2.5) * s,
                                w.top + e.r,
                                (w.top + e.r).max(w.sky_bottom),
                            ),
                        ]);
                        self.retarget = rand(draw, 1.4, 2.2);
                    }
                    let p = self.destination.unwrap_or(w.aim);
                    let dx = p[0] - e.x;
                    let dy = p[1] - e.y;
                    let d = distance(dx, dy);
                    let step = d.min(self.speed * sm * dt);
                    e.x += dx / d * step;
                    e.y += dy / d * step + (self.t * 6. + self.wob).sin() * s * 0.4 * dt;
                    self.dir = direction(w.aim[0] - e.x);
                    if self.windup > 0. {
                        self.windup -= dt;
                        if self.windup <= 0. {
                            throw_at(e, self.dir, w, HazardKind::Ember, 0., events);
                        }
                    } else {
                        self.cool -= dt;
                        if self.cool <= 0. {
                            if !w.quiet {
                                self.windup = 0.45;
                            }
                            self.cool = rand(draw, 2.4, 3.2);
                        }
                    }
                }
                Kind::Gobbler => {
                    let mut gem = None;
                    let mut best = f64::INFINITY;
                    for g in context.gems {
                        let d = (g.position[0] - e.x).hypot(g.position[1] - e.y);
                        if d < best {
                            best = d;
                            gem = Some(g);
                        }
                    }
                    let p = gem.map_or(
                        [w.aim[0], w.aim[1] + (self.t * 2. + self.wob).sin() * s],
                        |g| g.position,
                    );
                    let dx = p[0] - e.x;
                    let dy = p[1] - e.y;
                    let d = distance(dx, dy);
                    let step = d.min(self.speed * sm * dt * if gem.is_some() { 1. } else { 0.8 });
                    e.x += dx / d * step;
                    e.y += dy / d * step;
                    if dx.abs() > 2. {
                        self.dir = direction(dx);
                    }
                    e.y = clamp(e.y, w.top + e.r * 0.5, (w.top + e.r).max(w.ground_y - e.r));
                    if let Some(g) = gem
                        && best < e.r + g.radius
                    {
                        events.push(Event::Gobble { gem_id: g.id });
                    }
                }
                Kind::Moth => {
                    let center = self.center.get_or_insert([e.x, e.y]);
                    center[0] += clamp((w.aim[0] - center[0]) / s, -1., 1.) * self.speed * sm * dt;
                    center[1] += clamp((w.aim[1] - s * 2.5 - center[1]) / s, -1., 1.)
                        * self.speed
                        * 0.6
                        * sm
                        * dt;
                    center[1] = clamp(
                        center[1],
                        w.top + e.r * 2.,
                        (w.top + e.r * 2.).max(w.sky_bottom - s * 2.),
                    );
                    e.x = center[0] + (self.t * 1.6 + self.wob).cos() * s * 1.4;
                    e.y = center[1] + (self.t * 3.2 + self.wob).sin() * s * 0.6;
                    self.dir = if (self.t * 1.6 + self.wob).sin() < 0. {
                        1.
                    } else {
                        -1.
                    };
                    self.cool -= dt;
                    if self.cool <= 0. {
                        if !w.quiet && self.entered {
                            events.push(Event::Hazard(Hazard {
                                kind: HazardKind::Dust,
                                position: [e.x, e.y + e.r * 0.5],
                                velocity: [rand(draw, -0.3, 0.3) * s, s * 0.5],
                                gravity: s * 1.2,
                                radius: s * 0.26,
                                life: 4.5,
                                warn: 0.,
                            }));
                        }
                        self.cool = rand(draw, 1.8, 2.6);
                    }
                }
                Kind::Swallower => {
                    e.x = w.width / 2. + (self.t * 0.3).sin() * (w.width / 2. - e.r - s).max(0.);
                    e.y = w.top + e.r * 0.95 + (self.t * 0.9).sin() * s * 0.35;
                    self.dir = direction(w.aim[0] - e.x);
                    if self.windup > 0. {
                        self.windup -= dt;
                        if self.windup <= 0. {
                            boss_special(self.kind, e, w, context.active_extras, draw, events);
                        }
                    } else {
                        self.cool -= dt;
                        if self.cool <= 0. {
                            if !w.quiet {
                                fan(e, self.dir, w, HazardKind::Spark, events);
                            }
                            self.cool = rand(draw, 2.6, 3.2);
                        }
                        self.cool2 -= dt;
                        if self.cool2 <= 0. {
                            self.cool2 = 9.;
                            if !w.quiet {
                                self.windup = 2.4;
                            }
                        }
                    }
                }
                Kind::Urgal | Kind::Troll | Kind::Chieftain | Kind::Cavetroll => {
                    if self.windup > 0. {
                        self.windup -= dt;
                        if self.windup <= 0. {
                            match self.kind {
                                Kind::Urgal => {
                                    throw_at(e, self.dir, w, HazardKind::Rock, 0., events)
                                }
                                Kind::Chieftain => {
                                    for off in [-1.4, 0., 1.4] {
                                        throw_at(e, self.dir, w, HazardKind::Rock, off * s, events);
                                    }
                                }
                                _ => throw_at(e, self.dir, w, HazardKind::Boulder, 0., events),
                            }
                        }
                    } else {
                        e.x += self.dir * self.speed * sm * dt;
                        edge(e, &mut self.dir, self.entered, w);
                        if self.entered && !e.boss && draw() < dt * 0.2 {
                            self.dir *= -1.;
                        }
                    }
                    e.y = w.ground_y - e.r * 0.95;
                    self.cool -= dt;
                    if self.cool <= 0. && self.entered {
                        self.cool = if e.boss {
                            rand(draw, 2.8, 3.4)
                        } else {
                            rand(draw, 2.6, 3.6) * if self.kind == Kind::Troll { 1.3 } else { 1. }
                        };
                        if !w.quiet {
                            self.windup = if self.kind == Kind::Urgal { 0.45 } else { 0.7 };
                            self.dir = direction(w.aim[0] - e.x);
                        }
                    }
                    if e.boss {
                        self.cool2 -= dt;
                        if self.cool2 <= 0. {
                            boss_special(self.kind, e, w, context.active_extras, draw, events);
                            self.cool2 = if self.kind == Kind::Chieftain {
                                11.
                            } else {
                                9.
                            };
                        }
                    }
                }
            }
        }
        if !w.safe
            && !(self.kind == Kind::Leaper && !self.air && !e.calm)
            && (w.player[0] - e.x).hypot(w.player[1] - e.y) < e.r * 0.85 + s * w.dragon_scale * 0.35
        {
            events.push(Event::Touch {
                id: e.id,
                hurts: self.hurt
                    || self.sting > 0.
                    || (self.air
                        && (matches!(self.kind, Kind::Wolf | Kind::Leaper)
                            || self.kind.definition().hop))
                    || (self.kind == Kind::Swallower && self.windup > 0.),
            });
        }
        if self.sting > 0.
            && !w.safe
            && !e.calm
            && (w.player[0] - e.x).hypot(w.player[1] - e.y) < e.r * 1.9 + s * w.dragon_scale * 0.3
        {
            events.push(Event::Sting { id: e.id });
        }
        Ok(())
    }
}

fn direction(dx: f64) -> f64 {
    if dx >= 0. { 1. } else { -1. }
}
fn distance(dx: f64, dy: f64) -> f64 {
    let d = dx.hypot(dy);
    if d == 0. { 1. } else { d }
}
fn edge(e: &Target, dir: &mut f64, entered: bool, w: &World) {
    if entered {
        if e.x < e.r {
            *dir = 1.;
        } else if e.x > w.width - e.r {
            *dir = -1.;
        }
    }
}
fn throw_at(e: &Target, dir: f64, w: &World, kind: HazardKind, off: f64, events: &mut Vec<Event>) {
    let t = if kind == HazardKind::Boulder {
        1.25
    } else {
        1.
    };
    let g = w.scale * 9.;
    let x = e.x + dir * e.r * 0.3;
    let y = e.y - e.r;
    events.push(Event::Hazard(Hazard {
        kind,
        position: [x, y],
        velocity: [
            (w.aim[0] + off - x) / t,
            (w.aim[1] - y - 0.5 * g * t * t) / t,
        ],
        gravity: g,
        radius: w.scale
            * if kind == HazardKind::Boulder {
                0.34
            } else {
                0.2
            },
        life: 3.,
        warn: 0.,
    }));
}
fn fan(e: &Target, dir: f64, w: &World, kind: HazardKind, events: &mut Vec<Event>) {
    let bubble = kind == HazardKind::Bubble;
    let x = e.x + dir * e.r * if bubble { 0.7 } else { 0.5 };
    let y = e.y + e.r * if bubble { 0.2 } else { 0.3 };
    let a = (w.aim[1] - y).atan2(w.aim[0] - x);
    let offsets: &[f64] = if bubble {
        &[-0.28, 0., 0.28]
    } else {
        &[-0.5, -0.25, 0., 0.25, 0.5]
    };
    for off in offsets {
        let speed = w.scale * if bubble { 3.6 } else { 5. };
        events.push(Event::Hazard(Hazard {
            kind,
            position: [x, y],
            velocity: [(a + off).cos() * speed, (a + off).sin() * speed],
            gravity: if bubble { -w.scale * 0.5 } else { 0. },
            radius: w.scale * if bubble { 0.24 } else { 0.14 },
            life: if bubble { 4.5 } else { 3. },
            warn: 0.,
        }));
    }
}
fn boss_special(
    kind: Kind,
    e: &Target,
    w: &World,
    extras: usize,
    draw: &mut impl FnMut() -> f64,
    events: &mut Vec<Event>,
) {
    let s = w.scale;
    match kind {
        Kind::Chieftain if extras < 4 => {
            for x in [-s, w.width + s] {
                events.push(Event::Spawn {
                    kind: Kind::Urgal,
                    extra: true,
                    at: [x, w.ground_y - s * 0.72 * 0.95],
                });
            }
        }
        Kind::Serpent | Kind::Cavetroll => {
            // Source builds all five x coordinates before emitting hazards.
            let xs = [
                w.aim[0],
                rand(draw, s, w.width - s),
                rand(draw, s, w.width - s),
                rand(draw, s, w.width - s),
                rand(draw, s, w.width - s),
            ];
            for x in xs {
                events.push(Event::Hazard(Hazard {
                    kind: HazardKind::Fall,
                    position: [x, w.top],
                    velocity: [0., 0.],
                    gravity: s * 8.,
                    radius: s * 0.28,
                    life: 4.,
                    warn: 0.9,
                }));
            }
            if kind == Kind::Serpent && extras < 3 {
                for _ in 0..2 {
                    events.push(Event::SpawnSea {
                        kind: Kind::Leaper,
                        extra: true,
                    });
                }
            }
        }
        Kind::Swallower if extras < 4 => {
            for x in [-s, w.width + s] {
                events.push(Event::Spawn {
                    kind: Kind::Gobbler,
                    extra: true,
                    at: [x, e.y],
                });
            }
        }
        _ => {}
    }
}

/// Source `inhalePull` displacement, before the caller's `clampPlayer`.
/// Apply each active Swallower in list order, using the player's updated position.
/// Downed players are exempt; normal safety/shield states do not prevent pulling.
pub fn inhale_displacement(
    e: &Enemy,
    player: [f64; 2],
    down: bool,
    scale: f64,
    dt: f64,
) -> [f64; 2] {
    if down || e.kind != Kind::Swallower || e.target.calm || e.windup <= 0. {
        return [0.; 2];
    }
    let dx = e.target.x + e.dir * e.target.r * 0.45 - player[0];
    let dy = e.target.y + e.target.r * 0.25 - player[1];
    let d = distance(dx, dy);
    let k = scale * 3.2 * (scale * 7. / d).min(1.3) * dt;
    [dx / d * k.min(d), dy / d * k.min(d)]
}
