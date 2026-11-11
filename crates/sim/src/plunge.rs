//! The Plunge: a secret found after act three (`docs/design/world-vision-brief.md`,
//! "The Plunge", twelve beats). You cannot die in it: the terror is real, the
//! risk is not.
//!
//! Everything here is a **rule**, never terrain (no cell is edited, nothing
//! rebakes), and every place is found by the **name** of a level frame
//! ([`Places`]): "odd tunnel", "Plunge shaft", "Plunge crack", "angler stage".
//!
//! - **Tells** (beat 1): once a crab has act three, the odd tunnel tastes
//!   metallic, marine snow drifts down in it and the water is cold ([`tells`]).
//!   The game draws them; nothing marks the place.
//! - **Dig around** (beat 2): a crab holding dig in the odd tunnel adds to its
//!   own counter; after 30 s the way breaks. Silt trickles first, then a low
//!   roar (cues).
//! - **The lip and the grab** (beats 3, 4): once the way is broken, a weak pull
//!   (a twentieth of the walking speed) draws a crab at the dead end towards the
//!   hole. It can simply walk away. Over the hole itself, the current takes it.
//! - **The ride** (beats 5 to 7, 30 s): the crab is carried along a path down
//!   the shaft to the crack. The crab's own legs are not in it (it is carried,
//!   steering a little, never teleported, and the path ignores the bedrock
//!   between frames), nothing reaches it, and its strain goes to 1.0 (cracks
//!   stay until the moult).
//! - **The crack** (beats 8 to 11): a refuge. Once still for 3 s the scene
//!   beyond the slit plays (once per crab), a small blue-green light can be
//!   taken (once per crab) and, after it, a light far below blinks twice
//!   (once per crab).
//! - **The vent** (beat 12, 20 s): at the crack's far end a hot updraft carries
//!   the crab up the shaft to the odd tunnel, always the same landing.
//!
//! The ride is the same idea as [`crate::ride`] (carried, steering a little,
//! one way, harmless) but a path in time, not a lane in space: the fall goes
//! through bedrock between sealed frames and must end exactly at its landing
//! whatever the controls, whatever the world has become.
use crate::level::Level;
use crate::level::bands::Smell;
use crate::level::landmarks::Landmark;
use crate::physics::Carry;
use crate::tuning::Tuning;
use crate::world::{PlayerId, SimCrab};
use crate::STEP;
use crate::world::strain;
use bevy_math::{Rect, Vec2};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Ticks per second.
const HZ: u32 = 120;
/// How long holding dig at the dead end takes to break the way (design: 20
/// to 40 s).
pub const DIG_BREAK: u32 = 30 * HZ;
/// Silt starts to trickle this long into the dig, and the low roar follows.
pub const SILT_AT: u32 = 22 * HZ;
pub const ROAR_AT: u32 = 27 * HZ;
/// The fall, from the grab to the crack (design: 25 to 40 s).
pub const RIDE_TICKS: u32 = 30 * HZ;
/// The vent ride home (design: 15 to 25 s).
pub const VENT_TICKS: u32 = 20 * HZ;
/// How far from the hole the lip's weak pull reaches, and how close over the
/// hole the current takes a crab.
pub const LIP_REACH: f32 = 140.0;
pub const HOLE_HALF: f32 = 18.0;
/// The lip's pull, as a share of the walking speed: much weaker than walking.
pub const LIP_PULL: f32 = 0.05;
/// A friend digging this close beside the crab that is taken comes too.
pub const FRIEND_REACH: f32 = 150.0;
/// How far the controls can nudge the crab off the path (units).
pub const NUDGE: f32 = 60.0;
/// Still in the crack this long before the scene plays.
pub const STILL_TICKS: u32 = 3 * HZ;
/// The scene beyond the slit, in ticks (about 20 s), and its beats.
pub const SCENE_TICKS: u32 = 20 * HZ;
pub const SCENE_LIGHT_AT: u32 = 2 * HZ;
pub const SCENE_PREY_AT: u32 = 5 * HZ;
pub const SCENE_FLARE_AT: u32 = 12 * HZ;
pub const SCENE_GONE_AT: u32 = 13 * HZ;
/// The answer comes this long after the treasure is taken; its two blinks
/// last this long.
pub const ANSWER_DELAY: u32 = 4 * HZ;
pub const ANSWER_TICKS: u32 = 3 * HZ;
/// The odd tunnel's tells reach this far from the tunnel.
pub const TELL_REACH: f32 = 700.0;
/// How near the crack's far end a crab must be to step into the vent.
const VENT_REACH: f32 = 24.0;
/// How near the treasure a crab must be to take it.
const TREASURE_REACH: f32 = 50.0;
/// The ride's beats, as shares of the fall (beats 5, 6, 7 of the brief).
pub const SNOW_END: f32 = 0.5;
pub const CARCASS_END: f32 = 0.67;

/// Where a crab is in the Plunge. Kept on its own progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Phase {
    /// Not in it (the odd tunnel, or anywhere else).
    #[default]
    Walking,
    /// Falling: ticks into the ride and the nudge off the path.
    Ride { t: u32, off: Vec2 },
    /// In the crack.
    Crack {
        /// Ticks it has been still.
        still: u32,
        /// Ticks into the scene beyond the slit (None: not begun).
        scene: Option<u32>,
        /// Ticks since it took the treasure (None: not taken here).
        found: Option<u32>,
        /// Ticks into the answering light (None: not begun).
        answer: Option<u32>,
    },
    /// Rising on the vent: ticks into the ride and the nudge.
    Vent { t: u32, off: Vec2 },
}

/// One crab's Plunge: counters and once-only flags.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Plunge {
    /// Ticks spent holding dig in the odd tunnel (at most [`DIG_BREAK`]).
    #[serde(default)]
    pub dig: u32,
    #[serde(default)]
    pub phase: Phase,
    /// The scene has played for this crab.
    #[serde(default)]
    pub flash_seen: bool,
    /// The treasure has been taken.
    #[serde(default)]
    pub treasure_taken: bool,
    /// The answering light has begun.
    #[serde(default)]
    pub answered: bool,
    /// The lantern is blue-green (cultured at home from the treasure).
    #[serde(default)]
    pub blue_lantern: bool,
}

impl Plunge {
    /// Is the crab being carried (the fall, or the vent)?
    pub fn carried(&self) -> bool {
        matches!(self.phase, Phase::Ride { .. } | Phase::Vent { .. })
    }

    /// Is the way broken for this crab?
    pub fn broken(&self) -> bool {
        self.dig >= DIG_BREAK
    }

    /// How far through the fall it is (0 to 1), if falling.
    pub fn fall(&self) -> Option<f32> {
        match self.phase {
            Phase::Ride { t, .. } => Some(t as f32 / RIDE_TICKS as f32),
            _ => None,
        }
    }

    /// How far up the vent it is (0 to 1), if rising.
    pub fn rise(&self) -> Option<f32> {
        match self.phase {
            Phase::Vent { t, .. } => Some(t as f32 / VENT_TICKS as f32),
            _ => None,
        }
    }

    /// Is the crab in the crack?
    pub fn in_crack(&self) -> bool {
        matches!(self.phase, Phase::Crack { .. })
    }

    /// Ticks into the scene beyond the slit, while it plays.
    pub fn scene(&self) -> Option<u32> {
        match self.phase {
            Phase::Crack {
                scene: Some(t), ..
            } if t < SCENE_TICKS => Some(t),
            _ => None,
        }
    }

    /// Ticks into the answering light, while it plays.
    pub fn answer(&self) -> Option<u32> {
        match self.phase {
            Phase::Crack {
                answer: Some(t), ..
            } if t < ANSWER_TICKS => Some(t),
            _ => None,
        }
    }

    /// Is something still to happen in the crack that the vent waits for?
    fn busy(&self) -> bool {
        match self.phase {
            Phase::Crack {
                scene,
                found,
                answer,
                ..
            } => {
                scene.is_some_and(|t| t < SCENE_TICKS)
                    || answer.is_some_and(|t| t < ANSWER_TICKS)
                    || (found.is_some() && answer.is_none())
            }
            _ => false,
        }
    }
}

/// Is the answering light lit, `t` ticks into it? Two blinks, then dark.
pub fn answer_lit(t: u32) -> bool {
    let s = t as f32 / HZ as f32;
    (0.4..0.8).contains(&s) || (1.4..1.8).contains(&s)
}

/// The Plunge's places, found by the names of the level's frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Places {
    pub tunnel: Rect,
    pub shaft: Rect,
    pub crack: Rect,
    pub stage: Rect,
}

impl Places {
    /// The places the landmarks name (None when any is missing).
    pub fn from_landmarks(landmarks: &[Landmark]) -> Option<Places> {
        let find = |name: &str| landmarks.iter().find(|l| l.name == name).map(|l| l.rect);
        Some(Places {
            tunnel: find("odd tunnel")?,
            shaft: find("Plunge shaft")?,
            crack: find("Plunge crack")?,
            stage: find("angler stage")?,
        })
    }

    /// The hole: the middle of the odd tunnel's floor.
    pub fn hole(&self) -> Vec2 {
        Vec2::new(self.tunnel.center().x, self.tunnel.min.y)
    }

    /// Where the vent sets a crab down: in the odd tunnel it started in.
    pub fn home_landing(&self) -> Vec2 {
        let w = self.tunnel.width();
        Vec2::new(self.tunnel.min.x + w * 0.2, self.tunnel.min.y)
    }

    /// Which way the slit (and the stage) lies from the crack: +1 east.
    pub fn slit_side(&self) -> f32 {
        if self.stage.center().x >= self.crack.center().x {
            1.0
        } else {
            -1.0
        }
    }

    /// Where the fall sets a crab down in the crack.
    pub fn crack_landing(&self) -> Vec2 {
        let c = self.crack;
        Vec2::new(c.center().x - self.slit_side() * c.width() * 0.2, c.min.y)
    }

    /// The treasure, on the crack's floor near the slit.
    pub fn treasure(&self) -> Vec2 {
        let c = self.crack;
        Vec2::new(c.center().x + self.slit_side() * c.width() * 0.3, c.min.y)
    }

    /// The vent, on the crack's floor at its far end (away from the slit).
    pub fn vent(&self) -> Vec2 {
        let c = self.crack;
        let x = if self.slit_side() > 0.0 {
            c.min.x
        } else {
            c.max.x
        };
        Vec2::new(x, c.min.y)
    }

    /// The path of the fall, from the hole to the crack.
    pub fn fall_path(&self) -> Vec<Vec2> {
        let x = self.shaft.center().x;
        vec![
            self.hole(),
            Vec2::new(x, self.shaft.max.y),
            Vec2::new(x, self.shaft.min.y),
            self.crack_landing(),
        ]
    }

    /// The path of the vent, from the crack to the odd tunnel.
    pub fn vent_path(&self) -> Vec<Vec2> {
        let x = self.shaft.center().x;
        let v = self.vent();
        vec![
            v,
            Vec2::new(x, self.shaft.min.y),
            Vec2::new(x, self.shaft.max.y),
            self.home_landing(),
        ]
    }
}

/// The game's places, found once from the authored level's named frames.
pub fn places() -> Option<&'static Places> {
    static PLACES: OnceLock<Option<Places>> = OnceLock::new();
    PLACES
        .get_or_init(|| Places::from_landmarks(&Level::test_level().landmarks()))
        .as_ref()
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The point `s` units along a path, and the path's length.
fn along(path: &[Vec2], s: f32) -> Vec2 {
    let mut left = s.max(0.0);
    for pair in path.windows(2) {
        let len = pair[0].distance(pair[1]);
        if left <= len {
            return pair[0].lerp(pair[1], left / len.max(1e-3));
        }
        left -= len;
    }
    path.last().copied().unwrap_or(Vec2::ZERO)
}

fn length(path: &[Vec2]) -> f32 {
    path.windows(2).map(|p| p[0].distance(p[1])).sum()
}

/// What the controls ask for across the path: a little nudge, never more
/// than [`NUDGE`] from it, fading out towards both ends so the ride starts
/// at the hole and always ends exactly at its landing.
fn nudge(off: Vec2, c: &crate::Controls, u: f32) -> Vec2 {
    let want = Vec2::new(
        c.walk.clamp(-1.0, 1.0),
        f32::from(c.jump_held) - f32::from(c.down),
    ) * NUDGE;
    let off = (off + (want - off) * (4.0 * STEP)).clamp_length_max(NUDGE);
    off * (smooth(u / 0.1) * smooth((1.0 - u) / 0.12))
}

/// The cues the Plunge asks for, with the crab they are for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moment {
    /// Silt trickles into the gap.
    Silt,
    /// A low roar behind the wall.
    Roar,
    /// The one low tone under the scene.
    Tone,
}

/// A crab carried along the fall or the vent: one tick. Replaces the physics
/// step (the crab's legs are not in it). Costs nothing, harms nothing.
pub(crate) fn carry(crab: &mut SimCrab, places: &Places) {
    let m = &mut crab.mover;
    let before = m.pos;
    let controls = crab.controls;
    let (path, ticks, t, off, fall) = match crab.plunge.phase {
        Phase::Ride { t, off } => (places.fall_path(), RIDE_TICKS, t, off, true),
        Phase::Vent { t, off } => (places.vent_path(), VENT_TICKS, t, off, false),
        _ => return,
    };
    let t = t + 1;
    let u = (t as f32 / ticks as f32).min(1.0);
    let off = nudge(off, &controls, u);
    let at = if t >= ticks {
        *path.last().unwrap_or(&before)
    } else {
        along(&path, length(&path) * smooth(u)) + off
    };
    m.pos = at;
    m.vel = if t >= ticks {
        Vec2::ZERO
    } else {
        (at - before) / STEP
    };
    m.grounded = false;
    m.climbing = false;
    m.dodge_left = 0.0;
    m.dash_left = 0.0;
    m.plunge_safe = true;
    crab.fighter.venom.clear();
    crab.fighter.knocked = 0;
    if fall {
        strain::raise_to(crab, if t >= ticks { 1.0 } else { u });
        crab.plunge.phase = if t >= ticks {
            Phase::Crack {
                still: 0,
                scene: None,
                found: None,
                answer: None,
            }
        } else {
            Phase::Ride { t, off }
        };
    } else {
        crab.plunge.phase = if t >= ticks {
            // Home again: the way is closed until it is dug open once more.
            crab.plunge.dig = 0;
            Phase::Walking
        } else {
            Phase::Vent { t, off }
        };
    }
}

/// Is a crab at `centre` digging in the odd tunnel?
fn digging(crab: &SimCrab, places: &Places, t: &Tuning) -> bool {
    let m = &crab.mover;
    let centre = m.pos + Vec2::Y * m.height(t) * 0.5;
    crab.controls.dig_held && places.tunnel.contains(centre)
}

/// All crabs' Plunge for one tick, after they have moved. Returns the cues to
/// sound. Nothing here touches a remote crab.
pub(crate) fn step_all(
    crabs: &mut [SimCrab],
    level: &Level,
    t: &Tuning,
    places: Option<&Places>,
) -> Vec<(PlayerId, Moment)> {
    let mut moments = Vec::new();
    let Some(p) = places else {
        for c in crabs.iter_mut() {
            c.plunge.phase = Phase::Walking;
            c.mover.plunge_safe = false;
        }
        return moments;
    };
    let _ = level;
    // Who is taken this tick: those over the hole, then friends beside them.
    let mut taken: Vec<usize> = Vec::new();
    for (i, crab) in crabs.iter_mut().enumerate() {
        if crab.remote || crab.fighter.dead.is_some() {
            continue;
        }
        crab.mover.plunge_safe = !matches!(crab.plunge.phase, Phase::Walking);
        match crab.plunge.phase {
            Phase::Walking => {
                if !crab.mover.act_three {
                    continue;
                }
                if digging(crab, p, t) && crab.plunge.dig < DIG_BREAK {
                    crab.plunge.dig += 1;
                    let d = crab.plunge.dig;
                    if d == ROAR_AT || d == DIG_BREAK {
                        moments.push((crab.id, Moment::Roar));
                    } else if d >= SILT_AT && d.is_multiple_of(HZ) {
                        moments.push((crab.id, Moment::Silt));
                    }
                }
                if !crab.plunge.broken() {
                    continue;
                }
                let m = &mut crab.mover;
                let h = p.hole();
                let near_floor = m.pos.y <= h.y + 40.0 && m.pos.y >= h.y - 40.0;
                let dx = h.x - m.pos.x;
                if !near_floor || dx.abs() > LIP_REACH.min(p.tunnel.width() * 0.5) || !p.tunnel.contains(m.pos + Vec2::Y) {
                    continue;
                }
                if dx.abs() <= HOLE_HALF {
                    taken.push(i);
                } else {
                    // The lip: a pull weaker than walking.
                    m.pos.x += dx.signum() * t.walk_speed * LIP_PULL * STEP;
                }
            }
            Phase::Crack { .. } => crack(crab, p, t, &mut moments),
            Phase::Ride { .. } | Phase::Vent { .. } => {}
        }
    }
    // A friend digging beside whoever is taken comes too, here or (a remote
    // crab already in the shaft) on another device.
    let mut leaders: Vec<Vec2> = taken.iter().map(|&i| crabs[i].mover.pos).collect();
    leaders.extend(
        crabs
            .iter()
            .filter(|c| c.remote && p.shaft.contains(c.mover.pos + Vec2::Y))
            .map(|c| c.mover.pos),
    );
    for (i, crab) in crabs.iter().enumerate() {
        if crab.remote
            || crab.fighter.dead.is_some()
            || taken.contains(&i)
            || !matches!(crab.plunge.phase, Phase::Walking)
            || !crab.mover.act_three
            || !digging(crab, p, t)
        {
            continue;
        }
        if leaders
            .iter()
            .any(|l| l.distance(crab.mover.pos) <= FRIEND_REACH)
        {
            taken.push(i);
        }
    }
    for i in taken {
        let crab = &mut crabs[i];
        crab.plunge.dig = DIG_BREAK;
        crab.plunge.phase = Phase::Ride {
            t: 0,
            off: Vec2::ZERO,
        };
        crab.mover.plunge_safe = true;
        crab.mover.vel = Vec2::ZERO;
        crab.mover.pos = p.hole();
    }
    moments
}

/// One tick of a crab in the crack: the scene, the treasure, the answer, the
/// vent.
fn crack(crab: &mut SimCrab, p: &Places, t: &Tuning, moments: &mut Vec<(PlayerId, Moment)>) {
    let Phase::Crack {
        mut still,
        mut scene,
        mut found,
        mut answer,
    } = crab.plunge.phase
    else {
        return;
    };
    let m = &mut crab.mover;
    // The crack is a refuge; the crab walks about it and goes no further.
    let half = m.half_width(t);
    m.pos.x = m.pos.x.clamp(p.crack.min.x + half, p.crack.max.x - half);
    m.pos.y = m.pos.y.max(p.crack.min.y);
    let c = crab.controls;
    let idle = m.vel.length() < 8.0
        && c.walk == 0.0
        && !c.jump_held
        && !c.dodge_held
        && !c.dig_held
        && !c.smash_held
        && !c.block_held;
    still = if idle { still + 1 } else { 0 };
    // The scene beyond the slit, once per crab.
    if scene.is_none() && still >= STILL_TICKS && !crab.plunge.flash_seen {
        crab.plunge.flash_seen = true;
        scene = Some(0);
        moments.push((crab.id, Moment::Tone));
    } else if let Some(s) = scene {
        scene = Some((s + 1).min(SCENE_TICKS));
    }
    // The treasure, small, taken by touch.
    if found.is_none() && !crab.plunge.treasure_taken {
        let at = p.treasure();
        let touching = (m.pos.x - at.x).abs() <= m.half_width(t) + TREASURE_REACH
            && (m.pos.y - at.y).abs() <= TREASURE_REACH + m.height(t);
        if touching && m.take_up(Carry::WildLight, t) {
            crab.plunge.treasure_taken = true;
            crab.events.picked_up = Some(Carry::WildLight);
            found = Some(0);
        } else if touching {
            crab.events.shell_full = true;
        }
    } else if let Some(f) = found {
        found = Some(f.saturating_add(1));
    }
    // Something answers: once, after the treasure.
    if let Some(f) = found
        && answer.is_none()
        && !crab.plunge.answered
        && f >= ANSWER_DELAY
    {
        crab.plunge.answered = true;
        answer = Some(0);
    } else if let Some(a) = answer {
        answer = Some((a + 1).min(ANSWER_TICKS));
    }
    crab.plunge.phase = Phase::Crack {
        still,
        scene,
        found,
        answer,
    };
    // The vent opens when the show is over: step into it.
    let v = p.vent();
    if !crab.plunge.busy() && (crab.mover.pos.x - v.x).abs() <= crab.mover.half_width(t) + VENT_REACH
    {
        crab.plunge.phase = Phase::Vent {
            t: 0,
            off: Vec2::ZERO,
        };
        crab.mover.vel = Vec2::ZERO;
    }
}

/// What the odd tunnel tells a crab that has act three.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tells {
    /// The water tastes of metal (the smell wisp turns).
    pub smell: Smell,
    /// Marine snow drifts down in the tunnel (0 to 1).
    pub snow: f32,
    /// A cold tint on the lantern's own light (0 to 1).
    pub cold: f32,
}

/// The tells at crab `id`'s place: none before it has act three, and none far
/// from the odd tunnel.
pub fn tells(sim: &crate::Sim, id: PlayerId) -> Option<Tells> {
    let crab = sim.crab(id)?;
    let p = places()?;
    if !crab.mover.act_three {
        return None;
    }
    let m = &crab.mover;
    let centre = m.pos + Vec2::Y * m.height(&sim.tuning) * 0.5;
    let near = 1.0 - (rect_distance(p.tunnel, centre) / TELL_REACH).clamp(0.0, 1.0);
    (near > 0.0).then_some(Tells {
        smell: Smell::Metallic,
        snow: near,
        cold: near,
    })
}

fn rect_distance(r: Rect, p: Vec2) -> f32 {
    let d = (r.min - p).max(p - r.max).max(Vec2::ZERO);
    d.length()
}

impl crate::Sim {
    /// Crab `id`'s Plunge.
    pub fn plunge(&self, id: PlayerId) -> Option<&Plunge> {
        self.crab(id).map(|c| &c.plunge)
    }

    /// The odd tunnel's tells at crab `id` (see [`tells`]).
    pub fn odd_tunnel_tells(&self, id: PlayerId) -> Option<Tells> {
        tells(self, id)
    }
}
