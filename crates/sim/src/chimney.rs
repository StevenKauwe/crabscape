//! The seep chimney and the long chimney home: ride lanes ([`crate::ride`])
//! found by the **names** of the level's places, never by coordinates, so
//! they follow the frames wherever the world's shape puts them.
//!
//! - **Seep chimney** (`seep shaft`): free from the start, one way up, from
//!   the bone hall side to the shaft's upper exit.
//! - **The long chimney home** (`chimney hall`, then `chimney return`
//!   westwards, then `long chimney` up): closed until the crust plug at the
//!   chimney's top is cracked by a fully charged heavy crusher smash, from
//!   below (a brittle rock, so it is shared like a cut net). A crab that
//!   rides it home has reached **act three**, kept on its own progress.
//!
//! Both lanes only carry through open water. While the named frames are
//! sealed bedrock they simply never meet a crab.
use crate::level::Level;
use crate::level::landmarks::{Kind, Landmark};
use crate::physics::{Controls, Mover, StepEvents};
use crate::ride::{self, Lane, Open, Segment};
use crate::tuning::Tuning;
use bevy_math::{Rect, Vec2};
use std::sync::OnceLock;

/// The seep chimney's flow: slow enough to be a ride (about 5 s up).
pub const SEEP_SPEED: f32 = 280.0;
/// The long chimney's flow: about 33 s from the hall to the shallows.
pub const LONG_SPEED: f32 = 1000.0;
/// How fast a lane lets go of the crab at its exit.
pub const EXIT_SPEED: f32 = 240.0;
pub const SEEP: &str = "seep chimney";
pub const CATHEDRAL_SEEP: &str = "cathedral seep";
pub const LONG: &str = "long chimney home";
/// A refuge farther than this from a lane's exit is not its landing.
const LANDING_REACH: f32 = 3000.0;

fn find<'a>(landmarks: &'a [Landmark], name: &str) -> Option<&'a Landmark> {
    landmarks.iter().find(|l| l.name == name)
}

/// The refuge nearest to `exit` that is not below it: where the lane sets
/// the crab down.
fn landing(landmarks: &[Landmark], exit: Vec2) -> Option<Vec2> {
    landmarks
        .iter()
        .filter(|l| matches!(l.kind, Kind::Refuge))
        .map(|l| Vec2::new(l.rect.center().x, l.rect.max.y))
        .filter(|p| p.y >= exit.y - 150.0 && p.distance(exit) <= LANDING_REACH)
        .min_by(|a, b| a.distance(exit).total_cmp(&b.distance(exit)))
}

/// Every chimney lane the landmarks name. Lanes whose places are missing
/// are left out.
pub fn lanes_from(landmarks: &[Landmark]) -> Vec<Lane> {
    let mut lanes = Vec::new();
    if let Some(shaft) = find(landmarks, "seep shaft") {
        let r = shaft.rect;
        lanes.push(Lane {
            name: SEEP.into(),
            open: Open::Free,
            segments: vec![Segment {
                rect: r,
                dir: Vec2::Y,
                speed: SEEP_SPEED,
            }],
            landing: landing(landmarks, Vec2::new(r.center().x, r.max.y)),
            exit_speed: EXIT_SPEED,
        });
    }
    if let Some(seep) = find(landmarks, "cathedral seep") {
        let r = seep.rect;
        lanes.push(Lane {
            name: CATHEDRAL_SEEP.into(),
            open: Open::Free,
            segments: vec![Segment {
                rect: r,
                dir: Vec2::X,
                speed: SEEP_SPEED,
            }],
            landing: find(landmarks, "hanging garden")
                .map(|g| Vec2::new(g.rect.center().x, g.rect.max.y)),
            exit_speed: EXIT_SPEED,
        });
    }
    if let (Some(hall), Some(back), Some(up), Some(plug)) = (
        find(landmarks, "chimney hall"),
        find(landmarks, "chimney return"),
        find(landmarks, "long chimney"),
        find(landmarks, "long chimney crust plug"),
    ) {
        let (hall, back, up) = (hall.rect, back.rect, up.rect);
        // The bedrock between two frames is a few tens of units: the lane
        // spans it so the crab is never between two flows.
        let segments = vec![
            Segment {
                rect: hall,
                dir: -Vec2::X,
                speed: LONG_SPEED,
            },
            Segment {
                rect: Rect::new(up.max.x, back.min.y, hall.min.x.max(back.max.x), back.max.y),
                dir: -Vec2::X,
                speed: LONG_SPEED,
            },
            Segment {
                rect: up,
                dir: Vec2::Y,
                speed: LONG_SPEED,
            },
        ];
        lanes.push(Lane {
            name: LONG.into(),
            open: Open::Cracked(plug.rect),
            segments,
            landing: landing(landmarks, Vec2::new(up.center().x, up.max.y)),
            exit_speed: EXIT_SPEED,
        });
    }
    lanes
}

/// The game's lanes, found once from the authored level's named places.
pub fn lanes() -> &'static [Lane] {
    static LANES: OnceLock<Vec<Lane>> = OnceLock::new();
    LANES.get_or_init(|| lanes_from(&Level::test_level().landmarks()))
}

/// One physics tick of a crab, riding a chimney if it is in an open one, else
/// as ever (the vent field's tether included).
pub fn step(level: &mut Level, m: &mut Mover, c: &Controls, t: &Tuning) -> StepEvents {
    step_with(lanes(), level, m, c, t)
}

/// [`step`] over the given lanes (tests build their own).
pub fn step_with(
    lanes: &[Lane],
    level: &mut Level,
    m: &mut Mover,
    c: &Controls,
    t: &Tuning,
) -> StepEvents {
    let centre = m.pos + Vec2::Y * m.height(t) * 0.5;
    let Some(flow) = ride::flow_at(lanes, level, centre, m.ride) else {
        m.ride = None;
        return level.step_with_tether(m, c, t);
    };
    // Only an unbroken ride from a lane's first stretch counts as a ride.
    let here = (flow.lane as u8, flow.segment as u8);
    m.ride = match m.ride {
        Some((l, s)) if l == here.0 && (s == here.1 || s + 1 == here.1) => Some(here),
        None if here.1 == 0 => Some(here),
        _ => None,
    };
    let lane = &lanes[flow.lane];
    if m.ride.is_some() && lane.name == LONG && lane.finishing(flow.segment, centre) {
        m.act_three = true;
    }
    let lvl = &*level;
    ride::carry(m, c, &flow, |m, c| {
        crate::physics::step(m, c, t, lvl, crate::STEP)
    })
}

/// May a crab at `pos` crack this brittle rock? The long chimney's crust
/// plug breaks only from below (from the chimney), never from the shelf.
pub fn may_crack(rock: Rect, pos: Vec2) -> bool {
    let guarded = lanes()
        .iter()
        .any(|l| matches!(l.open, Open::Cracked(plug) if plug == rock));
    !guarded || pos.y < rock.min.y
}

impl crate::Sim {
    /// Has this crab ridden the long chimney home once? (Act three: the
    /// Plunge checks it. Kept on the crab's own progress.)
    pub fn act_three(&self, player: crate::PlayerId) -> bool {
        self.crab(player).is_some_and(|c| c.mover.act_three)
    }
}
