//! Rest points (`docs/design/world.md`, Death): the places a crab wakes after
//! dying. Home and every crevice are rest points already; so are a region's
//! entrance and the door of a big fight. Derived from the level here, with no
//! new level data beyond the authored [`MARKERS`].
use super::Level;
use crate::enemy;
use bevy_math::{Rect, Vec2};

/// A place a crab can wake.
#[derive(Clone, Debug, PartialEq)]
pub struct RestPoint {
    /// Its stable name (what a save remembers it by).
    pub name: String,
    /// Where the crab stands on waking (its feet).
    pub at: Vec2,
    pub kind: RestKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RestKind {
    /// Its home shell: always reached; the crab wakes tucked in.
    Home,
    /// One of [`Level::crevices`]: reached by resting in it.
    Crevice(usize),
    /// A region's entrance: reached by passing within reach of it.
    Entrance,
    /// The door of a big fight. Dying inside `arena` wakes the crab here
    /// (reached or not), with its load kept.
    BossDoor { arena: Rect },
}

/// What an authored marker is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkerKind {
    Entrance,
    /// An arena as `[min x, min y, max x, max y]`.
    BossDoor {
        arena: [f32; 4],
    },
}

/// An authored rest marker: a point in open water at `[x, y]` that snaps
/// down to the ground below it (`y: f32::INFINITY` means the top surface).
/// In a cavern, give a `y` inside the cavern so it lands on the cavern floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub name: &'static str,
    pub at: [f32; 2],
    pub kind: MarkerKind,
}

/// The authored markers. The two boss doors (the octopus's and the grouper's)
/// are derived from where those animals live, not listed here.
pub const MARKERS: &[Marker] = &[
    // Where the basin's cliff gives out into the far basin.
    Marker {
        name: "far basin",
        at: [4500.0, f32::INFINITY],
        kind: MarkerKind::Entrance,
    },
    // The top of the slump dam: the way into Plate D's lower canyon. (The
    // far rim needs none: the octopus's door is there.)
    Marker {
        name: "lower canyon",
        at: [17540.0, f32::INFINITY],
        kind: MarkerKind::Entrance,
    },
    // The Drop's top ledge, at the Deep Galleries' door (`deep_galleries`):
    // the way down into region 4.
    Marker {
        name: "drop top ledge",
        at: [1500.0, -1650.0],
        kind: MarkerKind::Entrance,
    },
    // The slope mouth (`slope_caves`): the neck down to region 6, in the leash
    // gap between the moray (to x 10,280) and the slope's mantis (from x
    // 10,530); the rest point lies on the floor beside it.
    Marker {
        name: "slope mouth",
        at: [10450.0, f32::INFINITY],
        kind: MarkerKind::Entrance,
    },
    // The octopus lair's door at the Drop's foot. (The far rim's octopus keeps
    // its own derived door; this is the lair's second octopus.)
    Marker {
        name: "octopus lair door",
        at: [1340.0, -11150.0],
        kind: MarkerKind::BossDoor {
            arena: [1280.0, -11200.0, 3600.0, -10900.0],
        },
    },
];

/// A crab reaches a rest point by passing within this many units of it.
pub const REACH: f32 = 120.0;

/// The ground below `p`: the y a crab stands at, the nearest solid top under
/// `p` (inside caverns too). Every rest point snaps through here; the floor
/// line only where the layer map has no ground under `p`.
fn ground_below(level: &Level, p: Vec2) -> f32 {
    level
        .ground_below(p)
        .unwrap_or_else(|| level.floor_height(p.x))
}

/// How far in front of the octopus's den its door lies (the fair bot starts
/// there).
const OCTOPUS_DOOR: f32 = 120.0;
/// How far the octopus's arena reaches out in front of its den.
const OCTOPUS_APRON: f32 = 700.0;
/// The grouper's door lies this far in front of its mouth (just past the
/// distance it notices a crab from, so a retry starts before the boom).
const GROUPER_DOOR: f32 = 760.0;

impl Level {
    /// Every rest point: home, then each crevice, then the entrances and
    /// boss doors (markers first, then the doors derived from the foes).
    pub fn rest_points(&self) -> Vec<RestPoint> {
        let mut points = vec![RestPoint {
            name: "home".into(),
            at: self.home_den,
            kind: RestKind::Home,
        }];
        for (i, c) in self.crevices.iter().enumerate() {
            points.push(RestPoint {
                name: format!("crevice {i}"),
                at: c.den,
                kind: RestKind::Crevice(i),
            });
        }
        for m in MARKERS {
            let y = ground_below(self, Vec2::from(m.at));
            points.push(RestPoint {
                name: m.name.into(),
                at: Vec2::new(m.at[0], y),
                kind: match m.kind {
                    MarkerKind::Entrance => RestKind::Entrance,
                    MarkerKind::BossDoor { arena } => RestKind::BossDoor {
                        arena: Rect::new(arena[0], arena[1], arena[2], arena[3]),
                    },
                },
            });
        }
        // The octopus: its door is in front of the den, the arena the den
        // and the apron before it.
        if self
            .foes
            .iter()
            .any(|(kind, _)| *kind == enemy::Kind::Octopus)
        {
            let x = self.den.min.x - OCTOPUS_DOOR;
            points.push(RestPoint {
                name: "octopus den".into(),
                at: Vec2::new(x, ground_below(self, Vec2::new(x, self.den.max.y))),
                kind: RestKind::BossDoor {
                    arena: Rect::new(
                        self.den.min.x - OCTOPUS_APRON,
                        self.den.min.y - 300.0,
                        self.den.max.x,
                        self.den.max.y + 300.0,
                    ),
                },
            });
        }
        // The grouper: its door is before its mouth, the arena a box round
        // its spot reaching back to the door.
        if let Some(&spot) = crate::creatures::grouper::spots(self).first() {
            let mouth = spot.x - crate::creatures::grouper::MOUTH_X;
            let x = mouth - GROUPER_DOOR;
            let ground = ground_below(self, Vec2::new(x, spot.y));
            points.push(RestPoint {
                name: "goliath grouper".into(),
                at: Vec2::new(x, ground),
                kind: RestKind::BossDoor {
                    arena: Rect::new(
                        x - 150.0,
                        ground.min(spot.y) - 200.0,
                        spot.x + crate::creatures::grouper::BODY.0 / 2.0,
                        ground.max(spot.y) + 700.0,
                    ),
                },
            });
        }
        points
    }
}
