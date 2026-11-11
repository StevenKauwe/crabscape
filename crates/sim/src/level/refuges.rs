//! Small, local shelter in the canyon's open water (`world-vision-brief.md`,
//! register 1): short coral clumps rooted on the walls, never a ladder.
//!
//! Spacing rule: a full-kit rise is about 1,020 units (a jump with all six
//! strokes, `full_kit_single_rise_for_refuge_spacing`). The clump a crab on the
//! sea-pen floor can reach has nothing within a rise above it, and every clump
//! stays a full rise below the ledge above it (the shelf, the far rim, the
//! far-wall cave), so no clump climbs a crab anywhere it could not already
//! reach: the tide, the upwelling and glass grips stay the ways up
//! (`refuges_make_no_new_climb`). Clumps higher up may be within a rise of
//! each other: reaching them already takes the tide.
//! Deep-sea coral grows on rock in the dark (`world.md`); the clumps' places
//! are a design choice.
use super::{Climb, Growth, Level};

/// The near wall's face below the shelf.
const NEAR: f32 = 6160.0;

/// (x, top) of each clump; each is 200 tall. The canyon's floor rose to
/// y -6,000, so the three clumps that stood below it are gone and the floor
/// has none of its own: the one a floor crab can reach would be a ladder up
/// to the far-wall cave.
const CLUMPS: [(f32, f32); 2] = [
    // Up the near wall: a full rise below the shelf, so it is never a step
    // up to it.
    (NEAR, -4550.0),
    // Immediately below the dead-end egg cave: its retreat. The root meets
    // the far wall; the tip catches a crab backing out of the west door.
    (7670.0, -5150.0),
];

pub(super) fn shape(level: &mut Level) {
    level.climbs.extend(clumps());
}

/// The clumps this module adds (for tests that compare with and without).
pub fn clumps() -> Vec<Climb> {
    CLUMPS
        .iter()
        .map(|&(x, top)| Climb {
            x,
            bottom: top - 200.0,
            top,
            kind: Growth::Coral,
        })
        .collect()
}
