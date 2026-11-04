//! The far basin as a hub (`docs/design/world.md`, "The crossroads"): each
//! branch out of it ends in a find that makes the way home cheaper.
//!
//! - **The ridge.** The pit's right wall stands on up as a ridge, as tall as
//!   the end wall, so the hub is walled on both sides: nothing glides into it
//!   from the cliff's crest or the shallows (that used to skip the ghost
//!   net). A window through the ridge is netted, and its knots are on the hub
//!   side: cut from there, the glide off the cliff's crest carries a crab
//!   over the pit and through it, the way back in, for everyone.
//! - **Glass grips (the long glide home).** The ridge's hub face is bare rock
//!   above the shaft's coral: with glass grips a crab climbs to its crest and
//!   glides off it west, over the pit, the basin and the cliff, to the
//!   shallows. Every descent is a debt; height is the credit.
//! - **The tide (the crack).** The flood runs up the crack too, at the
//!   canyon's head where tidal currents are strongest (`world.md`, [KPC08]):
//!   it lifts a crab from the doorstep to the hub, and the ebb runs it down.
//! - **Digging (the buried lattice).** A dead glass sponge's skeleton lies
//!   buried in the hub's sand: glass grips without the canyon's floor.
//! - **The octopus** guards the last branch, the far rim and the reef past it.
//!
//! The frame holds every gate: the ridge and its window's sill are rock, so
//! no dig reaches round the net (`crates/sim/tests/dig_gates.rs`). Appended
//! after the other shapes so earlier indices stay put.
use super::{Climb, CutFrom, Growth, Level, Net};
use bevy_math::{Rect, Vec2};

/// The ridge: the pit's right wall, from the passage's roof up.
const RIDGE: (f32, f32) = (4300.0, 4370.0);
const RIDGE_FOOT: f32 = -4230.0;
/// As tall as the end wall: higher than any glide from the west arrives.
const RIDGE_CREST: f32 = 3000.0;
/// The window through the ridge, where the glide off the cliff's crest
/// passes: its sill and roof.
const WINDOW: (f32, f32) = (-760.0, -280.0);
/// Where the skeleton lies buried in the hub's sand, 70 under the floor.
const BURIED_SPONGE: Vec2 = Vec2::new(4700.0, -770.0);

pub(super) fn shape(level: &mut Level) {
    let (west, east) = RIDGE;
    let wall = Rect::new(west, RIDGE_FOOT, east, -700.0);
    if let Some(i) = level.rocks.iter().position(|r| *r == wall) {
        level.rocks[i] = Rect::new(west, RIDGE_FOOT, east, WINDOW.0);
        level
            .rocks
            .insert(i + 1, Rect::new(west, WINDOW.1, east, RIDGE_CREST));
    }
    level.nets.push(Net {
        rect: Rect::new(east - 16.0, WINDOW.0, east, WINDOW.1),
        cut: false,
        from: CutFrom::East,
    });
    // Bare rock up the ridge's hub face, from where the shaft's coral stops.
    level.climbs.push(Climb {
        x: east + 36.0,
        bottom: -650.0,
        top: RIDGE_CREST - 10.0,
        kind: Growth::Rock,
    });
    // The flood up the crack, from the chimney's floor to the hub's.
    level
        .currents
        .push(Rect::new(5380.0, -1500.0, 5495.0, -540.0));
    level.sponges.push(BURIED_SPONGE);
}
