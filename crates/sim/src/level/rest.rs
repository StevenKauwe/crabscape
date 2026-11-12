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

/// An authored rest marker: `x`, and `dy` over the ground below it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub name: &'static str,
    pub x: f32,
    pub dy: f32,
    pub kind: MarkerKind,
}

/// The authored markers. The two boss doors (the octopus's and the grouper's)
/// are derived from where those animals live, not listed here.
pub const MARKERS: &[Marker] = &[
    // Where the basin's cliff gives out into the far basin.
    Marker {
        name: "far basin",
        x: 4500.0,
        dy: 0.0,
        kind: MarkerKind::Entrance,
    },
    // The top of the slump dam: the way into Plate D's lower canyon. (The
    // far rim needs none: the octopus's door is there.)
    Marker {
        name: "lower canyon",
        x: 17540.0,
        dy: 0.0,
        kind: MarkerKind::Entrance,
    },
];

/// A crab reaches a rest point by passing within this many units of it.
pub const REACH: f32 = 120.0;

/// The ground below `(x, y)`: the y a crab stands at. Today the floor line;
/// the world lead's layer map will replace this body with a
/// ground-below-a-point query (a one-line change), so every marker follows.
fn ground_below(level: &Level, x: f32, _y: f32) -> f32 {
    level.floor_height(x)
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
            let y = ground_below(self, m.x, f32::INFINITY) + m.dy;
            points.push(RestPoint {
                name: m.name.into(),
                at: Vec2::new(m.x, y),
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
                at: Vec2::new(x, ground_below(self, x, self.den.min.y)),
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
            let ground = ground_below(self, x, spot.y);
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
