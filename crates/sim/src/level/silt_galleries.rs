//! The Silt Galleries (region 2 of `world-plan.md`): the first cavern, under
//! the tide-pool shelf and the trench, authored as layer shapes
//! (`level/layers.rs`) and nothing else.
//!
//! ```text
//!   tide-pool shelf ..plug (manhole cover).. [plug shaft]   [mouth shaft] <- trench floor, rest point
//!                             |              |
//!   upper gallery:  [entry hall]-[sponge 1]-[sponge 2]-[sponge 3]  floor -1,040
//!                                                         |  (drop, coral back up)
//!   lower gallery:   [R2] ..........  [drop cavern with its ledge]  floor -2,100
//!                      |  (drop, coral back up)
//!   moray hall:     [................ moray ..]                      floor -3,000
//! ```
//!
//! Every cavity is a rectangle ("room"); the rest follows. A room is a cave in
//! the old bedrock with a twenty-unit lining of bedrock (so soil never hangs
//! over open water or stands in a vertical face, which settling would slump)
//! and, in the upper gallery, the lower gallery and the hall, a flat bed of
//! clay on its floor. Without a cell world the silt still holds a
//! body up: [`walls`] are the same floors as collision boxes (the cell world
//! takes its own from the cells, so a dug floor stays dug).
//!
//! Sizes are the crab's: 65 units tall at its biggest, 97 wide. Doors are 120
//! wide; rooms are 220 tall.
//!
//! Nothing here touches the bone hall group (clear by 140 units of clay), the
//! trench's stepping stone or its moray.

use super::landmarks::{Kind, Landmark};
use super::layers::{Edges, Shape};
use super::{Climb, Crevice, CutFrom, Growth, Level, Net};
use crate::enemy;
use crate::underwater::Cell;
use bevy_math::{Rect, Vec2};
use std::sync::OnceLock;

/// The upper gallery's floor.
pub const UPPER_FLOOR: f32 = -1040.0;
/// The lower gallery's floor.
pub const LOWER_FLOOR: f32 = -2100.0;
/// The moray hall's floor.
pub const HALL_FLOOR: f32 = -3000.0;
/// The trench's floor (the mouth's top).
const TRENCH_FLOOR: f32 = -260.0;
const EDGE: f32 = 20.0;

/// The mouth on the trench floor: a shaft down to the entry hall. East of the
/// trench's stepping stone, west of its moray.
pub const MOUTH: Rect = Rect::new(1540.0, -820.0, 1700.0, TRENCH_FLOOR);
/// The plug: a shaft climbs from the upper gallery to the tide-pool shelf (x
/// 920 to 1040, east of the long chimney, west of the trench). The net is its
/// manhole cover, set 10 units under the shelf's bedrock top (the shelf's loose
/// sand lies on that and blows away in the bake, as all of it does; a test
/// world that flattens the shelf to y 0 still stands on the cover, not in it):
/// solid until a crab below digs it open, and never from above.
pub const PLUG: Rect = Rect::new(920.0, -20.0, 1040.0, 0.0);
/// The shelf's bedrock top (the plateau's sand lies on it): the shaft's lining
/// stops here.
const SHELF_ROCK: f32 = 10.0;

/// The glass sponges, one in each sponge gallery, lying on the silt.
pub const SPONGES: [Vec2; 3] = [
    Vec2::new(1980.0, UPPER_FLOOR + 10.0),
    Vec2::new(2540.0, UPPER_FLOOR + 10.0),
    Vec2::new(3020.0, UPPER_FLOOR + 10.0),
];
/// Clam beds on the moray hall's floor: the hall's prize, in sight of its door.
pub const HALL_CLAMS: [Vec2; 2] = [
    Vec2::new(1650.0, HALL_FLOOR + 4.0),
    Vec2::new(1730.0, HALL_FLOOR + 4.0),
];
/// The moray's burrow, in a crevice at the hall's far end.
pub const MORAY: Vec2 = Vec2::new(2400.0, HALL_FLOOR);

/// How a room is read (landmarks) and whether it has a roof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Calm: no hunter may reach it (a Tunnel).
    Calm,
    /// Hunting ground and the way to it (a Frame).
    Deep,
    /// The mouth's shaft (a Mouth landmark; open to the water at the top).
    Mouth,
}

pub struct Room {
    pub name: &'static str,
    pub rect: Rect,
    pub part: Part,
    /// A floor of silt (sand) over clay; otherwise bedrock.
    pub silt: bool,
}

const fn room(name: &'static str, rect: Rect, part: Part, silt: bool) -> Room {
    Room {
        name,
        rect,
        part,
        silt,
    }
}

const U0: f32 = UPPER_FLOOR;
const L0: f32 = LOWER_FLOOR;
const H0: f32 = HALL_FLOOR;

/// Every cavity, by name.
pub fn rooms() -> &'static [Room] {
    static ROOMS: OnceLock<Vec<Room>> = OnceLock::new();
    ROOMS.get_or_init(|| {
        use Part::{Calm, Deep, Mouth};
        vec![
            // The upper gallery: a calm walk at floor -1,040, rooms 220 tall,
            // doors 160.
            room(
                "entry hall",
                Rect::new(920.0, U0, 1720.0, U0 + 220.0),
                Calm,
                true,
            ),
            room("silt mouth shaft", MOUTH, Mouth, false),
            room(
                "plug shaft",
                Rect::new(920.0, U0 + 220.0, 1040.0, 112.0),
                Mouth,
                false,
            ),
            room(
                "sponge door 1",
                Rect::new(1720.0, U0, 1840.0, U0 + 160.0),
                Calm,
                true,
            ),
            room(
                "sponge gallery 1",
                Rect::new(1840.0, U0, 2200.0, U0 + 220.0),
                Calm,
                true,
            ),
            room(
                "sponge door 2",
                Rect::new(2200.0, U0, 2320.0, U0 + 160.0),
                Calm,
                true,
            ),
            room(
                "sponge gallery 2",
                Rect::new(2320.0, U0, 2760.0, U0 + 220.0),
                Calm,
                true,
            ),
            room(
                "sponge door 3",
                Rect::new(2760.0, U0, 2880.0, U0 + 160.0),
                Calm,
                true,
            ),
            room(
                "sponge gallery 3",
                Rect::new(2880.0, U0, 3200.0, U0 + 220.0),
                Calm,
                true,
            ),
            // The way down: a drop at the far end, past the last sponge.
            room(
                "lower drop",
                Rect::new(3080.0, L0 + 220.0, 3200.0, U0),
                Deep,
                false,
            ),
            // The lower gallery, and the Drop's cavern with its ledge.
            room(
                "lower gallery",
                Rect::new(1600.0, L0, 3200.0, L0 + 220.0),
                Deep,
                true,
            ),
            room(
                "drop cavern",
                Rect::new(2300.0, L0 + 220.0, 2580.0, -1420.0),
                Deep,
                false,
            ),
            room(
                "drop stub",
                Rect::new(2580.0, -1700.0, 2760.0, -1560.0),
                Deep,
                false,
            ),
            // The moray hall, down a shaft from the lower gallery.
            room(
                "moray shaft",
                Rect::new(1780.0, H0 + 300.0, 1900.0, L0),
                Deep,
                false,
            ),
            room(
                "moray hall",
                Rect::new(1500.0, H0, 2500.0, H0 + 300.0),
                Deep,
                true,
            ),
        ]
    })
}

impl Level {
    /// Does this level carry the Silt Galleries (a custom level may not)?
    ///
    /// It does when the plug is in its nets and its first lining is among its
    /// rocks: a test world that clears the rocks, or swaps the floor, is not
    /// the galleries' world any more.
    pub fn has_silt_galleries(&self) -> bool {
        self.nets.iter().any(|n| n.rect == PLUG) && self.rocks.contains(&lining()[0])
    }

    /// The bottom of the mouth shaft at `x`, if the sea floor has a hole there
    /// (the first solid from above is the entry hall's floor).
    pub(super) fn hole_bottom(&self, x: f32) -> Option<f32> {
        ((MOUTH.min.x..MOUTH.max.x).contains(&x) && self.has_silt_galleries())
            .then_some(UPPER_FLOOR)
    }
}

fn subtract(piece: Rect, cut: Rect) -> Vec<Rect> {
    let hit = piece.intersect(cut);
    if hit.is_empty() {
        return vec![piece];
    }
    let mut out = Vec::new();
    let mut add = |r: Rect| {
        if !r.is_empty() {
            out.push(r);
        }
    };
    add(Rect::new(piece.min.x, piece.min.y, hit.min.x, piece.max.y));
    add(Rect::new(hit.max.x, piece.min.y, piece.max.x, piece.max.y));
    add(Rect::new(hit.min.x, piece.min.y, hit.max.x, hit.min.y));
    add(Rect::new(hit.min.x, hit.max.y, hit.max.x, piece.max.y));
    out
}

fn subtract_all(pieces: Vec<Rect>, cuts: impl Iterator<Item = Rect> + Clone) -> Vec<Rect> {
    let mut pieces = pieces;
    for cut in cuts {
        pieces = pieces.into_iter().flat_map(|p| subtract(p, cut)).collect();
    }
    pieces
}

fn all_voids() -> impl Iterator<Item = Rect> + Clone {
    rooms()
        .iter()
        .map(|r| r.rect)
        .collect::<Vec<_>>()
        .into_iter()
}

/// Floors are forty units of clay. (A skin of loose silt on top was tried: the
/// bake's uniform current swept it into the doors in piles 56 units high.)
const FLOOR_SOIL: f32 = 40.0;

/// The bedrock lining of every cavity: its two sides, its roof (a mouth has
/// none: it opens to the water) and, where there is no soil, its floor.
/// Soil never hangs over open water and never stands in a vertical face (the
/// settling would slump it), so every cavity is a bedrock cave with a
/// flat bed of silt on its floor. Twenty units thick.
pub fn lining() -> &'static [Rect] {
    static LINING: OnceLock<Vec<Rect>> = OnceLock::new();
    LINING.get_or_init(|| {
        let mut out = Vec::new();
        for r in rooms() {
            let a = r.rect;
            let below = if r.silt { FLOOR_SOIL } else { EDGE };
            let top = match r.part {
                Part::Mouth if r.name == "plug shaft" => SHELF_ROCK,
                Part::Mouth => a.max.y,
                _ => a.max.y + EDGE,
            };
            let mut bands = vec![
                Rect::new(a.min.x - EDGE, a.min.y - below, a.min.x, top),
                Rect::new(a.max.x, a.min.y - below, a.max.x + EDGE, top),
            ];
            if r.part != Part::Mouth {
                bands.push(Rect::new(a.min.x, a.max.y, a.max.x, a.max.y + EDGE));
            }
            if !r.silt {
                bands.push(Rect::new(a.min.x, a.min.y - EDGE, a.max.x, a.min.y));
            }
            out.extend(subtract_all(bands, all_voids()));
        }
        out
    })
}

/// The silt floors as collision boxes: used only without a cell world, whose
/// cells carry the soil themselves (so a dug floor stays dug).
pub fn walls() -> &'static [Rect] {
    static FLOORS: OnceLock<Vec<Rect>> = OnceLock::new();
    FLOORS.get_or_init(|| {
        let mut out = Vec::new();
        for r in rooms().iter().filter(|r| r.silt) {
            let a = r.rect;
            let floor = Rect::new(a.min.x, a.min.y - FLOOR_SOIL, a.max.x, a.min.y);
            out.extend(subtract_all(vec![floor], all_voids()));
        }
        out
    })
}

/// The cavities, and the clay on the upper gallery's, lower gallery's and
/// hall's floors, painted after the strata and before the rocks
/// (the lining).
pub(super) fn shapes(level: &Level) -> Vec<Shape> {
    if !level.has_silt_galleries() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in rooms() {
        out.push(Shape::rect(
            "gallery cavity",
            Cell::Water,
            r.rect,
            Edges::HalfOpen,
        ));
    }
    for r in rooms().iter().filter(|r| r.silt) {
        let a = r.rect;
        let layer = Rect::new(a.min.x, a.min.y - FLOOR_SOIL, a.max.x, a.min.y);
        for piece in subtract_all(vec![layer], all_voids()) {
            out.push(Shape::rect(
                "gallery floor",
                Cell::Mud,
                piece,
                Edges::HalfOpen,
            ));
        }
    }
    out
}

pub(super) fn after_rocks() -> Vec<Shape> {
    Vec::new()
}

/// Add the galleries to a level: roofs, the plug, the climbs back up, the
/// sponges, the hall's clams and moray, the two rest points.
pub(super) fn shape(level: &mut Level) {
    let existing: Vec<Rect> = level.rocks.clone();
    for roof in lining() {
        for piece in subtract_all(vec![*roof], existing.iter().copied()) {
            level.rocks.push(piece);
        }
    }
    // The ledge on the Drop's cavern wall: visible from the floor, a stroke or
    // two out of reach.
    level
        .rocks
        .push(Rect::new(2480.0, -1720.0, 2580.0, -1700.0));
    level.nets.push(Net {
        rect: PLUG,
        cut: false,
        from: CutFrom::Below,
    });
    // The way back up out of each drop is a coral stalk rooted in the soil,
    // as the crack's is: a debt repaid in reserve.
    level.climbs.extend(CORAL);
    level.sponges.extend(SPONGES);
    level.clams.extend(HALL_CLAMS);
    for (door, den) in REST_POINTS {
        level.crevices.push(Crevice { door, den });
    }
    level.foes.push((enemy::Kind::Moray, MORAY));
}

/// The coral stalks that climb back out of each drop: the mouth's, the plug
/// shaft's, the lower drop's and the moray shaft's. (Not landmarks of the
/// refuge kind: they are the way out of a cavern, nowhere to be set down.)
pub const CORAL: [Climb; 4] = [
    Climb {
        x: 1620.0,
        bottom: UPPER_FLOOR,
        top: TRENCH_FLOOR + 20.0,
        kind: Growth::Coral,
    },
    Climb {
        x: 980.0,
        bottom: UPPER_FLOOR,
        top: -25.0,
        kind: Growth::Coral,
    },
    Climb {
        x: 3140.0,
        bottom: LOWER_FLOOR,
        top: UPPER_FLOOR + 10.0,
        kind: Growth::Coral,
    },
    Climb {
        x: 1840.0,
        bottom: HALL_FLOOR,
        top: LOWER_FLOOR + 10.0,
        kind: Growth::Coral,
    },
];

/// The two rest points: at the mouth on the trench floor, and at the moray
/// hall's door (the lower gallery's west dead end, beside the hall's shaft).
pub const REST_POINTS: [(Rect, Vec2); 2] = [
    (
        Rect::new(1432.0, TRENCH_FLOOR - 1.0, 1498.0, TRENCH_FLOOR + 40.0),
        Vec2::new(1446.0, TRENCH_FLOOR),
    ),
    (
        Rect::new(1604.0, LOWER_FLOOR - 1.0, 1690.0, LOWER_FLOOR + 40.0),
        Vec2::new(1624.0, LOWER_FLOOR),
    ),
];
pub const REST_NAMES: [&str; 2] = ["silt mouth rest", "moray hall door rest"];

/// The galleries' places, for `Level::landmarks`.
pub(super) fn landmarks(level: &Level) -> Vec<Landmark> {
    if !level.has_silt_galleries() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut add = |name: &str, rect: Rect, kind| {
        out.push(Landmark {
            name: name.into(),
            rect,
            kind,
        });
    };
    let calm = rooms()
        .iter()
        .filter(|r| r.part == Part::Calm && r.name != "plug shaft");
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for r in calm {
        lo = lo.min(r.rect.min);
        hi = hi.max(r.rect.max);
    }
    add("upper gallery", Rect::from_corners(lo, hi), Kind::Tunnel);
    for r in rooms() {
        match r.name {
            "sponge gallery 1" | "sponge gallery 2" | "sponge gallery 3" | "lower gallery"
            | "moray hall" | "drop cavern" => add(r.name, r.rect, Kind::Frame),
            "plug shaft" => add(
                r.name,
                Rect::new(r.rect.min.x, r.rect.min.y, r.rect.max.x, PLUG.min.y - 40.0),
                Kind::Tunnel,
            ),
            "silt mouth shaft" => add(
                "silt mouth",
                Rect::new(
                    r.rect.min.x,
                    r.rect.max.y - 40.0,
                    r.rect.max.x,
                    r.rect.max.y + 40.0,
                ),
                Kind::Mouth,
            ),
            _ => {}
        }
    }
    // The doors of the calm places: the two shafts meet the gallery's ceiling,
    // and the plug shaft ends under the plug (two exits each).
    let shaft_foot = |x0: f32, x1: f32| Rect::new(x0, U0 + 220.0 - 20.0, x1, U0 + 220.0 + 20.0);
    add(
        "upper gallery mouth 0",
        shaft_foot(MOUTH.min.x, MOUTH.max.x),
        Kind::Mouth,
    );
    add("plug shaft mouth 0", shaft_foot(920.0, 1040.0), Kind::Mouth);
    add(
        "plug shaft mouth 1",
        Rect::new(920.0, PLUG.min.y - 60.0, 1040.0, PLUG.min.y - 20.0),
        Kind::Mouth,
    );
    add("silt gallery plug", PLUG, Kind::Plug);
    add(
        "drop ledge",
        Rect::new(2480.0, -1700.0, 2580.0, -1640.0),
        Kind::Refuge,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_are_on_the_grid_and_clear_of_the_bone_hall() {
        for r in rooms() {
            for v in [r.rect.min.x, r.rect.min.y, r.rect.max.x, r.rect.max.y] {
                assert_eq!(v % 4.0, 0.0, "{} is off the 4-unit grid: {v}", r.name);
            }
            // The bone hall's wall stands at x 3,360; keep a 120-unit clay
            // wall (past this room's own twenty-unit wall).
            assert!(r.rect.max.x + EDGE + 120.0 <= 3360.0, "{}", r.name);
            // Big enough for the biggest crab (97 wide, 65 tall).
            assert!(r.rect.width().min(r.rect.height()) >= 72.0, "{}", r.name);
        }
    }
}
