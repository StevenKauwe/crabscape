//! The Plate D geometry batch's rigid moves (`docs/design/decisions.md`,
//! 2026-11-10): the world steps down to the right.
//!
//! - The basin pit is filled and the canyon floor raised in the base level
//!   itself (`level.rs`), where those pieces are authored.
//! - Everything east of the canyon crossing drops [`DROP`] units: the far
//!   rim, the octopus den, the eastern reef and Coral Rise keep their shape,
//!   so each step lies below the last and the lower canyon (narrows lock,
//!   vent field, floor y -10,000) stays the deepest.
//! - The clams, culture and sponge that live in the new burrows are placed
//!   here, after every other shape has run, by appending (indices of the
//!   older pickups stay put).
use super::lower_canyon::{
    CATHEDRAL_PRIZE, FIRST_DIG_CLAM, GALLERY_CLAMS, SEEP_GARDEN_PRIZE, WARM_SEAM_PRIZE,
};
use super::{Level, UNITS_PER_METRE};
use bevy_math::{Rect, Vec2};

/// How far the east steps down (3.2 m at creature scale).
pub(crate) const DROP: f32 = 2000.0;
/// The far wall's foot: everything from here east moves.
pub(crate) const EAST: f32 = 7901.0;

const _: () = assert!(DROP == 3.2 * UNITS_PER_METRE);

fn down(p: &mut Vec2) {
    p.y -= DROP;
}

fn down_rect(r: &mut Rect) {
    r.min.y -= DROP;
    r.max.y -= DROP;
}

/// Move every authored thing from the far wall east (to the slump dam, which
/// `lower_canyon` adds afterwards at its own height) down by [`DROP`].
pub(super) fn drop_east(level: &mut Level) {
    // Things standing exactly on the floor stay exactly on it (a rigid move
    // would round differently from the re-interpolated floor).
    let on_floor: Vec<bool> = level
        .climbs
        .iter()
        .map(|c| c.bottom == level.floor_height(c.x))
        .collect();
    let foes_on_floor: Vec<bool> = level
        .foes
        .iter()
        .map(|(_, at)| at.y == level.floor_height(at.x))
        .collect();
    for p in level.floor.iter_mut().filter(|p| p.x >= EAST) {
        down(p);
    }
    for r in level.rocks.iter_mut().filter(|r| r.min.x >= EAST) {
        down_rect(r);
    }
    let floor_now: Vec<f32> = level
        .climbs
        .iter()
        .map(|c| level.floor_height(c.x))
        .collect();
    for ((c, on_floor), floor) in level.climbs.iter_mut().zip(on_floor).zip(floor_now) {
        if c.x >= EAST {
            c.bottom = if on_floor { floor } else { c.bottom - DROP };
            c.top -= DROP;
        }
    }
    for c in level.crevices.iter_mut().filter(|c| c.door.min.x >= EAST) {
        down_rect(&mut c.door);
        down(&mut c.den);
    }
    for list in [
        &mut level.eggs,
        &mut level.cultures,
        &mut level.clues,
        &mut level.teeth,
        &mut level.grit,
        &mut level.sponges,
        &mut level.clams,
        &mut level.drifting,
    ] {
        list.iter_mut().filter(|p| p.x >= EAST).for_each(down);
    }
    for n in level.nets.iter_mut().filter(|n| n.rect.min.x >= EAST) {
        down_rect(&mut n.rect);
    }
    for b in level.brittle.iter_mut().filter(|b| b.rect.min.x >= EAST) {
        down_rect(&mut b.rect);
    }
    for r in level.secrets.iter_mut().filter(|r| r.min.x >= EAST) {
        down_rect(r);
    }
    let foe_floor: Vec<f32> = level
        .foes
        .iter()
        .map(|(_, at)| level.floor_height(at.x))
        .collect();
    for (((_, at), on_floor), floor) in level.foes.iter_mut().zip(foes_on_floor).zip(foe_floor) {
        if at.x >= EAST {
            at.y = if on_floor { floor } else { at.y - DROP };
        }
    }
    for r in level.currents.iter_mut().filter(|r| r.min.x >= EAST) {
        down_rect(r);
    }
    for r in level.upwelling.iter_mut().filter(|r| r.min.x >= EAST) {
        down_rect(r);
    }
    down_rect(&mut level.den);
}

/// The finds that live in the new burrows. Appended after every other shape.
pub(super) fn place_finds(level: &mut Level) {
    // Home burrow: the first-dig clam at the top of the shaft by the shell's
    // doorway, and two in the gallery floor under their siphon holes.
    level.clams.push(FIRST_DIG_CLAM);
    level.clams.extend(GALLERY_CLAMS);
    // Seep garden: glowing seep life (wild bacteria, a culture).
    level.cultures.push(SEEP_GARDEN_PRIZE);
    // Warm seam: a bed of warm-water clams (the prize behind the crust).
    level.clams.push(WARM_SEAM_PRIZE);
    // The mantis guarding Coral Rise's find stands further in (x 17250), so
    // no walking hunter's leash reaches the pebble warren's door.
    let rise = level.floor_height(17250.0);
    if let Some((_, at)) = level
        .foes
        .iter_mut()
        .find(|(kind, at)| *kind == crate::enemy::Kind::Mantis && at.x == 16800.0)
    {
        *at = Vec2::new(17250.0, rise);
    }
    // Clay cathedral: the eastern gallery's second glass sponge (the last of
    // the redundant lattices) is the cathedral's prize.
    if let Some(sponge) = level.sponges.iter_mut().find(|p| p.x == 14400.0) {
        *sponge = CATHEDRAL_PRIZE;
    }
}
