//! Rest points: where a crab wakes after dying (`docs/design/world.md`, Death).
//!
//! A crab wakes at the REACHED rest point nearest where it fell: home (always
//! reached), a crevice it has rested in, or an entrance it has passed within
//! [`REACH`] of. Dying inside a boss's arena is different: it wakes at that
//! door, reached or not, and keeps its load (no cast-off shell): a big fight
//! is tried again straight away. Everything here is per crab and
//! deterministic; remote crabs are never touched.
use bevy_math::Vec2;

pub use crate::level::rest::{REACH, RestKind, RestPoint};
use crate::world::SimCrab;

/// Where a dead crab wakes.
#[derive(Clone, Copy, Debug)]
pub struct Wake<'a> {
    pub point: &'a RestPoint,
    /// It fell in a boss's arena: it keeps its load.
    pub boss: bool,
}

/// Remember `name` as reached. True the first time.
pub(crate) fn mark(reached: &mut Vec<String>, name: &str) -> bool {
    if reached.iter().any(|r| r == name) {
        return false;
    }
    reached.push(name.to_owned());
    true
}

/// The name of crevice `i`, if the level has it.
pub(crate) fn crevice_name(points: &[RestPoint], i: usize) -> Option<&str> {
    points
        .iter()
        .find(|p| p.kind == RestKind::Crevice(i))
        .map(|p| p.name.as_str())
}

/// Passing within [`REACH`] of an entrance or a boss's door reaches it.
pub(crate) fn reach(crab: &mut SimCrab, points: &[RestPoint]) {
    if crab.remote || crab.fighter.dead.is_some() {
        return;
    }
    for p in points {
        if matches!(p.kind, RestKind::Entrance | RestKind::BossDoor { .. })
            && crab.mover.pos.distance(p.at) <= REACH
            && mark(&mut crab.reached, &p.name)
        {
            crab.events.wakes_here = true;
        }
    }
}

/// Where a crab that fell at `fell` wakes. `points` always lead with home.
pub fn wake<'a>(reached: &[String], points: &'a [RestPoint], fell: Vec2) -> Option<Wake<'a>> {
    if let Some(point) = points
        .iter()
        .find(|p| matches!(&p.kind, RestKind::BossDoor { arena } if arena.contains(fell)))
    {
        return Some(Wake { point, boss: true });
    }
    points
        .iter()
        .filter(|p| p.kind == RestKind::Home || reached.contains(&p.name))
        .min_by(|a, b| a.at.distance(fell).total_cmp(&b.at.distance(fell)))
        .map(|point| Wake { point, boss: false })
}

/// A saved set of reached names, kept to the ones the level knows (each once,
/// home never stored), plus the crevice an older save last rested in.
pub fn restore(saved: &[String], wakes_at: Option<usize>, points: &[RestPoint]) -> Vec<String> {
    let mut reached = Vec::new();
    let known = |name: &str| {
        points
            .iter()
            .any(|p| p.name == name && p.kind != RestKind::Home)
    };
    for name in saved {
        if known(name) {
            mark(&mut reached, name);
        }
    }
    if let Some(name) = wakes_at.and_then(|i| crevice_name(points, i)) {
        mark(&mut reached, name);
    }
    reached
}

/// Is a saved set valid for this level (no unknown, repeated or home names)?
pub fn valid(saved: &[String], points: &[RestPoint]) -> bool {
    saved.len() <= points.len()
        && saved.iter().enumerate().all(|(i, name)| {
            points
                .iter()
                .any(|p| p.name == *name && p.kind != RestKind::Home)
                && !saved[..i].contains(name)
        })
}
