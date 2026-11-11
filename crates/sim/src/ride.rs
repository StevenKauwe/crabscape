//! Forced-motion rides: a **lane** is an ordered chain of rectangles, each
//! with a flow direction and speed, that carries a crab along while its body
//! centre is inside one of them (the seep chimneys, the long chimney home,
//! and later the Plunge's fall).
//!
//! A ride is *being taken*, not teleported: the crab's own physics step still
//! runs (walls, floor and rocks stop it as ever), but its velocity is drawn
//! towards the lane's flow every tick, so the motion is continuous, and the
//! controls steer a little across the flow. It is one-way (the along-flow
//! speed never drops below a floor, whatever the crab pushes), free (no
//! strokes, no reserve), and harmless (nothing here touches health).
//!
//! The lane's geometry comes from the level's named places (see
//! [`crate::chimney`]); this module is the generic mechanism, so it works on
//! any level whose named frames are open water.
use crate::level::Level;
use crate::physics::{Controls, Mover, StepEvents};
use bevy_math::{Rect, Vec2};

/// How quickly the crab's velocity is drawn towards the flow (per second).
const CATCH: f32 = 14.0;
/// Sideways speed the controls can add across the flow (units per second).
const STEER: f32 = 140.0;
/// The pull towards the middle of the lane (per second), and its limit
/// (units per second): the water carries a crab along its middle.
const CENTRING: f32 = 2.0;
const CENTRING_MAX: f32 = 100.0;
/// How much the controls can trim the speed along the flow (a share of it).
/// Pushing against the flow only ever slows the ride to this floor.
const TRIM: f32 = 0.12;
/// The flow slows over this distance before the end of a lane.
const EASE: f32 = 220.0;
/// How far past its end a lane that has a landing keeps hold of the crab.
const LANDING: f32 = 400.0;

/// When a lane carries.
#[derive(Clone, Debug, PartialEq)]
pub enum Open {
    /// Always (a free ride).
    Free,
    /// Once this brittle rock has been cracked (it is shared by everyone
    /// like any brittle rock).
    Cracked(Rect),
}

/// One stretch of a lane.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub rect: Rect,
    /// Unit vector of the flow.
    pub dir: Vec2,
    /// Flow speed, in units per second.
    pub speed: f32,
}

/// An ordered chain of segments, entered at the first and left at the last.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub name: String,
    pub open: Open,
    pub segments: Vec<Segment>,
    /// Where the lane lets the crab out: past its last segment the flow
    /// bends towards this point (a refuge), then lets go.
    pub landing: Option<Vec2>,
    /// The flow's speed at the very end of the lane.
    pub exit_speed: f32,
}

/// What a lane does to a crab at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flow {
    pub lane: usize,
    pub segment: usize,
    /// The water's velocity here.
    pub velocity: Vec2,
    /// Towards the middle of the lane, across the flow (zero past its end).
    pub centring: Vec2,
}

impl Lane {
    /// Is the lane carrying right now?
    pub fn is_open(&self, level: &Level) -> bool {
        match self.open {
            Open::Free => true,
            Open::Cracked(plug) => level.brittle.iter().any(|b| b.rect == plug && b.cracked),
        }
    }

    /// The part of the world segment `i` holds: the last segment of a lane
    /// with a landing reaches on past its end.
    fn reach(&self, i: usize) -> Rect {
        let s = &self.segments[i];
        if i + 1 == self.segments.len() && self.landing.is_some() {
            Rect::new(
                s.rect.min.x - LANDING * (-s.dir.x).max(0.0),
                s.rect.min.y - LANDING * (-s.dir.y).max(0.0),
                s.rect.max.x + LANDING * s.dir.x.max(0.0),
                s.rect.max.y + LANDING * s.dir.y.max(0.0),
            )
        } else {
            s.rect
        }
    }

    /// The first segment holding `centre`. Past the lane's end (the landing)
    /// only a crab that is already riding to its last stretch is held: one
    /// that merely wanders there is not.
    pub fn locate(&self, centre: Vec2, riding_out: bool) -> Option<usize> {
        let last = self.segments.len().checked_sub(1)?;
        (0..=last).find(|&i| {
            let rect = if riding_out && i == last {
                self.reach(i)
            } else {
                self.segments[i].rect
            };
            rect.contains(centre)
        })
    }

    /// Is a crab at `centre` in segment `i` in the last stretch of the lane?
    pub fn finishing(&self, i: usize, centre: Vec2) -> bool {
        let Some(s) = self.segments.get(i) else {
            return false;
        };
        let end = if s.dir.x + s.dir.y > 0.0 {
            s.rect.max
        } else {
            s.rect.min
        };
        i + 1 == self.segments.len() && (end - centre).dot(s.dir) < EASE
    }

    /// The water's velocity at `centre` inside segment `i`.
    pub fn velocity(&self, i: usize, centre: Vec2) -> Vec2 {
        let s = &self.segments[i];
        if i + 1 < self.segments.len() {
            return s.dir * s.speed;
        }
        // The last segment: slow over its final stretch, and past its end
        // bend towards the landing.
        let end = if s.dir.x + s.dir.y > 0.0 {
            s.rect.max
        } else {
            s.rect.min
        };
        let left = (end - centre).dot(s.dir);
        let k = (left / EASE).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let speed = self.exit_speed + (s.speed - self.exit_speed) * k;
        match self.landing {
            Some(target) if left < 0.0 => {
                let to = (target - centre).normalize_or_zero();
                // Turn from the lane's own direction towards the landing as
                // the crab passes further out.
                let turn = (-left / LANDING).clamp(0.0, 1.0);
                (s.dir * (1.0 - turn) + to * turn).normalize_or_zero() * self.exit_speed
            }
            _ => s.dir * speed,
        }
    }
}

/// The lane flow at `centre` (a crab's body centre), if any open lane holds it.
///
/// `riding` is the crab's unbroken ride so far (see [`Mover::ride`]).
pub fn flow_at(
    lanes: &[Lane],
    level: &Level,
    centre: Vec2,
    riding: Option<(u8, u8)>,
) -> Option<Flow> {
    lanes.iter().enumerate().find_map(|(lane, l)| {
        let riding_out = riding
            .is_some_and(|(r, s)| usize::from(r) == lane && usize::from(s) + 1 == l.segments.len());
        let segment = l.locate(centre, riding_out).filter(|_| l.is_open(level))?;
        let s = &l.segments[segment];
        let inside = s.rect.contains(centre);
        let off = s.rect.center() - centre;
        let centring = if inside {
            off - s.dir * off.dot(s.dir)
        } else {
            Vec2::ZERO
        };
        Some(Flow {
            lane,
            segment,
            velocity: l.velocity(segment, centre),
            centring,
        })
    })
}

/// One tick of a crab carried by `flow`: the usual physics step, with the
/// crab's velocity drawn into the flow first and never left against it after.
/// Costs no stroke and no reserve.
pub fn carry(
    m: &mut Mover,
    c: &Controls,
    flow: &Flow,
    step: impl FnOnce(&mut Mover, &Controls) -> StepEvents,
) -> StepEvents {
    let speed = flow.velocity.length();
    let dir = flow.velocity / speed.max(1e-3);
    // Steering: sideways across the flow with the walk keys (up and down
    // for a flow that runs sideways); the other axis trims the speed.
    let up = f32::from(c.jump_held) - f32::from(c.down);
    let walk = c.walk.clamp(-1.0, 1.0);
    let (across, along, lateral) = if dir.y.abs() >= dir.x.abs() {
        (walk, up * dir.y.signum(), Vec2::X)
    } else {
        (up, walk * dir.x.signum(), Vec2::Y)
    };
    let drift = (flow.centring * CENTRING).clamp_length_max(CENTRING_MAX);
    let target = dir * speed * (1.0 + TRIM * along) + lateral * (across * STEER) + drift;
    m.grounded = false;
    m.climbing = false;
    m.dodge_left = 0.0;
    m.dash_left = 0.0;
    m.vel += (target - m.vel) * (CATCH * crate::STEP).min(1.0);
    let (stamina, reserve) = (m.stamina, m.reserve);
    // The steering keys are ours; the physics gets none of them, so nothing
    // here spends a stroke.
    let plain = Controls {
        walk: 0.0,
        jump_held: false,
        jump_pressed: false,
        dodge_pressed: false,
        dodge_held: false,
        dig_held: false,
        dig_up: false,
        down: false,
        ..*c
    };
    let events = step(m, &plain);
    m.stamina = stamina;
    m.reserve = reserve;
    // One way: whatever it did, it is not left going against the flow.
    let floor = speed * (1.0 - TRIM);
    let along = m.vel.dot(dir);
    if along < floor {
        m.vel += dir * (floor - along);
    }
    events
}
