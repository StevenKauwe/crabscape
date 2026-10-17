//! The watcher: the doctrine's one finder. A patrol point that sweeps a
//! fixed waypoint path at a fixed speed, and sees the crab when it is close
//! and not hidden.
//!
//! Deliberately the deterministic minimum (core authorization): no
//! chasing, no scent, no memory — the patrol sweeps its path, that is all.
//! The soft-window lethality doctrine needs exactly one question answered
//! per tick: *am I being found?* — [`Sim::watcher_finding`].
//!
//! Hidden means gripping: a crab hanging on kelp or coral is part of the
/// weed (the climb is walking legs, and the watcher reads shapes, not
/// heat). Detection is a distance threshold against the crab's position,
/// modified by that flag — nothing else.
use bevy_math::Vec2;

use crate::physics::Mover;
use crate::tuning::Tuning;

/// A sweeping patrol: walks its waypoint loop, sees what comes close.
#[derive(Clone, Debug, PartialEq)]
pub struct Watcher {
    /// The waypoint loop (world positions). The patrol walks it in order
    /// and wraps around.
    pub path: Vec<Vec2>,
    /// Which waypoint it is heading toward.
    pub leg: usize,
    /// Where it is now.
    pub pos: Vec2,
}

impl Watcher {
    /// Start at the first waypoint, heading for the second (a single
    /// waypoint path stands still — a sentry).
    pub fn new(path: Vec<Vec2>) -> Self {
        let leg = if path.len() > 1 { 1 } else { 0 };
        let pos = path.first().copied().unwrap_or(Vec2::ZERO);
        Self { path, leg, pos }
    }

    /// Advance one tick along the current leg at the watcher's speed.
    /// Ping-pong at the ends: the sweep reverses, it never teleports.
    pub fn tick(&mut self, t: &Tuning) {
        if self.path.len() < 2 {
            return;
        }
        let target = self.path[self.leg];
        let to_target = target - self.pos;
        let dist = to_target.length();
        let step = t.watcher_speed * crate::physics::STEP;
        if dist <= step {
            self.pos = target;
            self.leg = (self.leg + 1) % self.path.len();
            return;
        }
        self.pos += to_target / dist * step;
    }

    /// Is the crab found? Distance threshold against the crab's position,
    /// unless it is hidden (gripping kelp or coral reads as weed).
    pub fn finding(&self, crab: &Mover, t: &Tuning) -> bool {
        if crab.climbing {
            return false;
        }
        self.pos.distance(crab.pos) <= t.watcher_detect_radius
    }
}
