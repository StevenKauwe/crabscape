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

/// The near wall's face below the shelf, and the far wall's face.
const NEAR: f32 = 6160.0;
const FAR: f32 = 7690.0;

/// (x, top) of each clump; each is 200 tall.
const CLUMPS: [(f32, f32); 7] = [
    // Just above the sea-pen floor, by the far wall: the only one a crab on
    // the floor can reach, and nothing is within a rise of its top.
    (FAR, -9150.0),
    // Up the near wall.
    (NEAR, -7800.0),
    (NEAR, -6150.0),
    // A full rise below the shelf, so it is never a step up to it.
    (NEAR, -4550.0),
    // Far wall, below the cave by more than a rise.
    (FAR, -6450.0),
    // Immediately below the dead-end egg cave: its retreat. The root meets
    // the far wall; the tip catches a crab backing out of the west door.
    (7670.0, -5150.0),
    // High on the far wall, a full rise below the far rim.
    (FAR, -3000.0),
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
