//! Region 5 of `world-plan.md`: the open canyon above the stepped slope
//! (x about 8,800 to 14,900). Open water, daylight fading to blue, a wide
//! silence. Four bedrock shelves float out of the canyon's walls and two rock
//! arches span the water over the slope; a kelp column climbs from the slope
//! to the surface.
//!
//! ```text
//!  surface  ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//!                                  |  kelp column (x 10,560)
//!                                  |  [kelp ledge]
//!            [tower shelf]         |                [first arch]     [second arch]
//!  tower ====                      |              |_____________|    |_______|
//!          \  refuges every ~600 under them, rooted on the slope  ......
//! ```
//!
//! The shelves and arches are not rooted on the slope floor (a rooted pier
//! would wall the walking route across the slope: the third dimension holds
//! them up). Their undersides stay more than a full-kit rise (about 1,020
//! units) plus a coral clump above the floor, so a crab on the slope sees the
//! sponge gardens on their tops long before it can reach them. They are reached
//! by the kelp column (up to the kelp ledge) and by gliding down from a higher
//! shelf. Rock is not climbable without glass grips, and nothing here is.
//!
//! The kelp column is a costed way home over the top: about 9,300 units of
//! climbing is more than a rested crab's reserve holds (`climb_cost`), so it
//! rests on the kelp ledge halfway, or climbs the last stretch tired.
//! Giant kelp grows to 30 m in light (Monterey Bay Aquarium species guide, not
//! opened this session: **not verified**); here the water is 15 m deep at the
//! column (9,300 units at 625 units to the metre), well inside its range.
use super::landmarks::{Kind, Landmark};
use super::{Climb, Growth, Level, WATER_SURFACE_Y};
use bevy_math::{Rect, Vec2};

/// The kelp column's x (the plan said about 10,500; 10,480 holds the first
/// clam and the lower-route walking tests start there).
pub const COLUMN_X: f32 = 10560.0;
/// The column's top, as the shallow kelp near home: just under the surface.
pub const COLUMN_TOP: f32 = WATER_SURFACE_Y - 20.0;
/// The rest point at the column's foot, a niche in its stand.
pub const FOOT_HEIGHT: f32 = 200.0;

/// A coral clump is this tall (as in `refuges.rs`).
const CLUMP: f32 = 200.0;
/// The clumps under the overhangs, between the far rim's tower and the
/// canyon's existing coral (x of each; the line has a refuge every ~600).
pub const OVERHANG_REFUGES: [f32; 2] = [9080.0, 9650.0];
/// The line the refuge gap is measured along: from the tower's foot to the
/// last of the slope's coral under the arches.
pub const LINE: (f32, f32) = (8800.0, 15100.0);
/// The most a crab on the line may be from cover (edge to edge).
pub const MAX_GAP: f32 = 600.0;

/// One feature of the region: a named shelf or arch, its rocks and the
/// sponge garden on top.
pub struct Feature {
    pub name: &'static str,
    pub garden: &'static str,
    pub rocks: &'static [Rect],
    /// The top of the shelf (or the arch's lintel): the garden's floor.
    pub top: f32,
    /// x extent of the standable top.
    pub span: (f32, f32),
    /// Where a glass sponge lies on the top.
    pub sponge: f32,
}

const fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect {
        min: Vec2::new(x0, y0),
        max: Vec2::new(x1, y1),
    }
}

/// The shelves, highest first. The tower shelf stands out of the far rim
/// tower's east face (the rock over the octopus's den); the kelp ledge beside
/// the column; the arches' lintels are the other two.
pub const FEATURES: [Feature; 4] = [
    Feature {
        name: "kelp ledge",
        garden: "sponge garden kelp ledge",
        rocks: &[r(10620.0, -860.0, 11300.0, -700.0)],
        top: -700.0,
        span: (10620.0, 11300.0),
        sponge: 11000.0,
    },
    Feature {
        name: "tower shelf",
        garden: "sponge garden tower shelf",
        rocks: &[r(8780.0, -1960.0, 10000.0, -1800.0)],
        top: -1800.0,
        span: (8800.0, 10000.0),
        sponge: 9350.0,
    },
    Feature {
        name: "first arch",
        garden: "sponge garden first arch",
        rocks: &[
            r(11420.0, -2200.0, 12380.0, -2040.0),
            r(11420.0, -3100.0, 11540.0, -2200.0),
            r(12260.0, -3100.0, 12380.0, -2200.0),
        ],
        top: -2040.0,
        span: (11420.0, 12380.0),
        sponge: 11980.0,
    },
    Feature {
        name: "second arch",
        garden: "sponge garden second arch",
        rocks: &[
            r(12700.0, -2660.0, 13760.0, -2500.0),
            r(12700.0, -3400.0, 12820.0, -2660.0),
            r(13640.0, -3400.0, 13760.0, -2660.0),
        ],
        top: -2500.0,
        span: (12700.0, 13760.0),
        sponge: 13230.0,
    },
];

/// A clam on each shelf top (the open-water finds), and grit under the arches.
pub fn shelf_clams() -> Vec<Vec2> {
    FEATURES
        .iter()
        .map(|f| Vec2::new(f.sponge + 90.0, f.top + 4.0))
        .collect()
}

/// Every gather spot on a shelf top: the sponge and the clam.
pub fn gather_spots() -> Vec<Vec2> {
    FEATURES
        .iter()
        .flat_map(|f| {
            [
                Vec2::new(f.sponge, f.top + 10.0),
                Vec2::new(f.sponge + 90.0, f.top + 4.0),
            ]
        })
        .collect()
}

/// Shell grit lying under the arches (the floor below each lintel).
pub const ARCH_GRIT: [f32; 2] = [11800.0, 13200.0];

/// Every rock the region adds.
pub fn rocks() -> Vec<Rect> {
    FEATURES
        .iter()
        .flat_map(|f| f.rocks.iter().copied())
        .collect()
}

/// The kelp column, rooted on the slope.
pub fn column(level: &Level) -> Climb {
    Climb {
        x: COLUMN_X,
        bottom: level.floor_height(COLUMN_X),
        top: COLUMN_TOP,
        kind: Growth::Kelp,
    }
}

/// The coral clumps under the overhangs.
pub fn overhang_refuges(level: &Level) -> Vec<Climb> {
    OVERHANG_REFUGES
        .iter()
        .map(|&x| {
            let bottom = level.floor_height(x);
            Climb {
                x,
                bottom,
                top: bottom + CLUMP,
                kind: Growth::Coral,
            }
        })
        .collect()
}

pub(super) fn shape(level: &mut Level) {
    level.rocks.extend(rocks());
    let column = column(level);
    let clumps = overhang_refuges(level);
    level.climbs.push(column);
    level.climbs.extend(clumps);
    for f in &FEATURES {
        level.sponges.push(Vec2::new(f.sponge, f.top + 10.0));
    }
    level.clams.extend(shelf_clams());
    for x in ARCH_GRIT {
        level.grit.push(Vec2::new(x, level.floor_height(x) + 4.0));
    }
    // The column's foot is the sixgill's boss door, a rest point: the slope's
    // mantis (400 units off, with an 800-unit leash) moves east, to between
    // the coral stands, so no hunter stands at it.
    let x = 11330.0;
    let y = level.floor_height(x);
    if let Some((_, at)) = level
        .foes
        .iter_mut()
        .find(|(kind, at)| *kind == crate::enemy::Kind::Mantis && at.x == 10720.0)
    {
        *at = Vec2::new(x, y);
    }
}

/// The name of one of this region's climbs, if `climb` is.
pub(super) fn climb_name(level: &Level, climb: &Climb) -> Option<String> {
    if *climb == column(level) {
        return Some("kelp column".into());
    }
    overhang_refuges(level)
        .iter()
        .position(|c| c == climb)
        .map(|n| format!("overhang refuge {n}"))
}

pub(super) fn landmarks(level: &Level) -> Vec<Landmark> {
    let mut out = Vec::new();
    for f in &FEATURES {
        let (lo, hi) = f.rocks.iter().fold(
            (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
            |(lo, hi), r| (lo.min(r.min), hi.max(r.max)),
        );
        out.push(Landmark {
            name: f.name.into(),
            rect: Rect::from_corners(lo, hi),
            kind: Kind::Frame,
        });
        out.push(Landmark {
            name: f.garden.into(),
            rect: Rect::new(f.span.0, f.top, f.span.1, f.top + 200.0),
            kind: Kind::Frame,
        });
    }
    // The rest point at the column's foot (the sixgill's boss door).
    let foot = level.floor_height(COLUMN_X);
    out.push(Landmark {
        name: "kelp column foot".into(),
        rect: Rect::new(COLUMN_X - 60.0, foot, COLUMN_X + 60.0, foot + FOOT_HEIGHT),
        kind: Kind::Refuge,
    });
    out
}
