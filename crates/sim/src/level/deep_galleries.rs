//! The Drop and the Deep Galleries (region 4 of `world-plan.md`): the great
//! dead-rock mass on the left, authored as layer shapes (`level/layers.rs`)
//! like the Silt Galleries above it.
//!
//! ```text
//!   drop ledge (silt galleries) .. jump west across the cavern
//!        |
//!   [drop approach] ------------------+   y -1,700, x 1,400 to 2,300
//!                                     |
//!   x 1,000 to 1,400:  the Drop       |   a stairwell of ledges, one every 500 units,
//!        slab 1  (rock, gap west)     |   a coral stalk through each gap
//!        slab 2  (rock, gap east)     |
//!        ...                          |
//!        slab 5  -> lanternfish hall  |   y -4,200  (water-filled hall, a shoal)
//!        slab 9  -> nodule hall       |   y -6,200  (iron-red nodules)
//!        slab 13 -> whale-fall hall   |   y -8,200  (a drowned whale-fall)
//!        slab 18                      |
//!        foot  -> lair door -> [octopus lair] -> [lair passage east] (closed)
//!                                                      |
//!                                           [breach shaft] under a brittle cover,
//!                                           down to the long chimney's return duct
//! ```
//!
//! **The Drop** is a stairwell, not a ladder: slabs of bedrock stand in the
//! shaft, each anchored to the opposite wall from the one above it, so a crab
//! walks to a slab's free end and drops one level onto the next. The way back
//! up is a coral stalk through each slab's gap, from the slab below to the lip
//! of the slab above: a climb of 500 units (half a stroke of reserve), then a
//! standing place. Nothing hunts in the shaft or the halls.
//!
//! **Why here.** The Silt Galleries' ledge over the drop cavern is seen from
//! the lower gallery's floor and reached only with the paddle strokes; a door
//! in the cavern's west wall at the ledge's height leads west along the
//! approach to the Drop. (The Drop cannot start at the ledge's stub: the lower
//! gallery lies directly under it, so the shaft descends west of the galleries
//! instead.)
//!
//! Sizes are the crab's: doors are 160 tall, ledges 280 wide, the lair 300.
//! The bake covers the world down to y -10,512 (the lowest floor less 512); the
//! lair lies below it and keeps the authored rock and water, which the bake
//! never moves.

use super::landmarks::{Kind, Landmark};
use super::layers::{Edges, Shape};
use super::silt_galleries::{all_voids, subtract_all};
use super::{Brittle, Climb, Growth, Level};
use crate::enemy;
use crate::underwater::Cell;
use bevy_math::{Rect, Vec2};
use std::sync::OnceLock;

const EDGE: f32 = 20.0;

/// The approach's floor, and the top of the Drop: a crab arriving from the
/// drop cavern stands here.
pub const TOP_FLOOR: f32 = -1700.0;
/// The distance between one standing place and the next.
pub const STEP: f32 = 500.0;
/// How many slabs stand in the shaft (the floor under them is the 19th).
pub const SLABS: i32 = 18;
/// The shaft's floor, where the lair door opens.
pub const FOOT: f32 = TOP_FLOOR - STEP * (SLABS + 1) as f32;
/// The shaft: 400 wide, from the foot to the approach's roof.
pub const SHAFT: Rect = Rect::new(1000.0, FOOT, 1400.0, TOP_FLOOR + 140.0);
/// The tallest a ledge may leave a climber (units).
pub const LEDGE_MAX_GAP: f32 = 600.0;
const SLAB_THICK: f32 = 40.0;
/// How far a slab reaches across the shaft: the rest is its gap (120 wide, the
/// door width), where its coral stalk stands.
const GAP: f32 = 120.0;

/// The top of slab `k` (`k` = 0 is the approach's floor, `SLABS + 1` the
/// shaft's floor): where a crab stands.
pub const fn level_of(k: i32) -> f32 {
    TOP_FLOOR - STEP * k as f32
}

/// Slab `k`'s gap is on the east side when `k` is even, the west when odd.
fn gap_east(k: i32) -> bool {
    k % 2 == 0
}

/// The stalk through slab `k`'s gap: its x.
pub fn stalk_x(k: i32) -> f32 {
    if gap_east(k) {
        SHAFT.max.x - GAP / 2.0
    } else {
        SHAFT.min.x + GAP / 2.0
    }
}

/// Slab `k` (1 to `SLABS`): a ledge of bedrock across all but a gap of the
/// shaft.
pub fn slab(k: i32) -> Rect {
    let top = level_of(k);
    let (x0, x1) = if gap_east(k) {
        (SHAFT.min.x, SHAFT.max.x - GAP)
    } else {
        (SHAFT.min.x + GAP, SHAFT.max.x)
    };
    Rect::new(x0, top - SLAB_THICK, x1, top)
}

/// The slabs, by level (1 to `SLABS`).
pub fn slabs() -> Vec<Rect> {
    (1..=SLABS).map(slab).collect()
}

/// The coral stalks that climb back up the Drop, bottom to top: stalk `k`
/// rises from the standing place under slab `k` to the lip of slab `k`.
pub fn corals() -> &'static [Climb] {
    static CORALS: OnceLock<Vec<Climb>> = OnceLock::new();
    CORALS.get_or_init(|| {
        (0..=SLABS)
            .rev()
            .map(|k| Climb {
                x: stalk_x(k),
                bottom: level_of(k + 1),
                top: level_of(k) + 10.0,
                kind: Growth::Coral,
            })
            .collect()
    })
}

/// The levels whose east wall opens on a hall (their slabs reach the wall).
const HALL_LEVELS: [i32; 3] = [5, 9, 13];

/// The octopus's den: the lair's far end, facing west, the way in.
pub const OCTOPUS_DEN: Vec2 = Vec2::new(3400.0, FOOT + 4.0);
/// The breach: a shaft from the passage's floor down to the long chimney's
/// return duct, under a cover of brittle rock (a plain brittle cover: the
/// act-three flag is a crab's own, not the world's).
pub const BREACH: Rect = Rect::new(4200.0, -12600.0, 4320.0, FOOT);
/// The cover, flush with the passage's floor.
pub const BREACH_COVER: Rect = Rect::new(4200.0, FOOT - 40.0, 4320.0, FOOT);

/// The bone hall's far door: a gap in its frame's west wall (the hall is
/// `lower_canyon`'s; this is one of its doors, filled with clay like the
/// others, to be dug through) as far from the sulfide haze (the deep east
/// corner) as the hall allows. A crab digs 80 units, the gap and the hall's
/// clay wall, between the far door's room and the hall's water.
pub const FAR_GAP: Rect = Rect::new(3360.0, -2520.0, 3380.0, -2400.0);
/// The far door's coral: the climb from the passage up to the door's floor.
pub const FAR_CORAL: Climb = Climb {
    x: 3180.0,
    bottom: -3300.0,
    top: -2510.0,
    kind: Growth::Coral,
};

/// What a room is, for its lining.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The way in from the drop cavern.
    Approach,
    /// The Drop.
    Shaft,
    /// A door from the shaft's east wall.
    Door,
    /// A water-filled hall.
    Hall,
    /// The octopus's lair.
    Lair,
    /// The passage east, a closed stub.
    Passage,
    /// The breach shaft: open at the bottom, into the return duct.
    Breach,
}

pub struct Room {
    pub name: &'static str,
    pub rect: Rect,
    pub part: Part,
}

const fn room(name: &'static str, rect: Rect, part: Part) -> Room {
    Room { name, rect, part }
}

/// Every cavity, by name.
pub fn rooms() -> &'static [Room] {
    static ROOMS: OnceLock<Vec<Room>> = OnceLock::new();
    ROOMS.get_or_init(|| {
        use Part::{Approach, Breach, Door, Hall, Lair, Passage, Shaft};
        let door = |k: i32, name: &'static str| {
            room(
                name,
                Rect::new(
                    SHAFT.max.x,
                    level_of(k),
                    SHAFT.max.x + 120.0,
                    level_of(k) + 160.0,
                ),
                Door,
            )
        };
        let hall = |k: i32, name: &'static str, x1: f32, height: f32| {
            room(
                name,
                Rect::new(SHAFT.max.x + 120.0, level_of(k), x1, level_of(k) + height),
                Hall,
            )
        };
        vec![
            room(
                "drop approach",
                Rect::new(SHAFT.max.x, TOP_FLOOR, 2300.0, TOP_FLOOR + 140.0),
                Approach,
            ),
            room("drop shaft", SHAFT, Shaft),
            door(HALL_LEVELS[0], "lanternfish door"),
            hall(HALL_LEVELS[0], "lanternfish hall", 3320.0, 300.0),
            door(HALL_LEVELS[1], "nodule door"),
            hall(HALL_LEVELS[1], "nodule hall", 3520.0, 300.0),
            door(HALL_LEVELS[2], "whale door"),
            hall(HALL_LEVELS[2], "whale-fall hall", 5800.0, 600.0),
            door(SLABS + 1, "lair door"),
            room(
                "octopus lair",
                Rect::new(SHAFT.max.x + 120.0, FOOT, 3600.0, FOOT + 300.0),
                Lair,
            ),
            room(
                "lair passage",
                Rect::new(3600.0, FOOT, 4800.0, FOOT + 160.0),
                Passage,
            ),
            room("breach shaft", BREACH, Breach),
            // The far door: out of the bone hall's west end, down a shaft
            // (with a niche to rest in), west under the moray hall to the
            // Drop's third ledge.
            room(
                "bone far door",
                Rect::new(3120.0, -2520.0, 3360.0, -2360.0),
                Door,
            ),
            room(
                "bone far shaft",
                Rect::new(3120.0, -3300.0, 3240.0, -2520.0),
                Shaft,
            ),
            room(
                "bone far ledge",
                Rect::new(3240.0, -2920.0, 3400.0, -2760.0),
                Door,
            ),
            room(
                "bone far passage",
                Rect::new(1520.0, -3300.0, 3240.0, -3140.0),
                Passage,
            ),
            room(
                "bone far step",
                Rect::new(
                    SHAFT.max.x,
                    level_of(3),
                    SHAFT.max.x + 120.0,
                    level_of(3) + 160.0,
                ),
                Door,
            ),
        ]
    })
}

fn room_named(name: &str) -> Rect {
    rooms()
        .iter()
        .find(|r| r.name == name)
        .map_or(Rect::new(0.0, 0.0, 0.0, 0.0), |r| r.rect)
}

/// The hall named `name`.
pub fn hall_rect(name: &str) -> Rect {
    room_named(name)
}

/// The lair's door (its west end): a crab on the shaft's foot stands outside.
pub fn lair_door() -> Rect {
    room_named("lair door")
}

/// The bedrock lining of every cavity: its sides, its roof and (but the
/// breach's) its floor, twenty units thick.
pub fn lining() -> &'static [Rect] {
    static LINING: OnceLock<Vec<Rect>> = OnceLock::new();
    LINING.get_or_init(|| {
        let mut out = Vec::new();
        for r in rooms() {
            let a = r.rect;
            // The breach's walls start where the duct's roof does. (No floors:
            // a body in a cavern stands on the layer map's own floor,
            // `Level::footing`, and the cells under it are bedrock.)
            let low = if r.part == Part::Breach {
                a.min.y + 20.0
            } else {
                a.min.y
            };
            let mut bands = vec![
                Rect::new(a.min.x - EDGE, low, a.min.x, a.max.y + EDGE),
                Rect::new(a.max.x, low, a.max.x + EDGE, a.max.y + EDGE),
                Rect::new(a.min.x, a.max.y, a.max.x, a.max.y + EDGE),
            ];
            out.extend(subtract_all(
                bands,
                all_voids().chain(std::iter::once(FAR_GAP)),
            ));
        }
        merge(out)
    })
}

/// Fewer, larger boxes: drop a box inside another and join two that share a
/// whole edge. (Every rock is looked at by every creature on every tick, so
/// the corner crumbs where two linings meet cost more than they hold.)
fn merge(mut rects: Vec<Rect>) -> Vec<Rect> {
    'again: loop {
        for i in 0..rects.len() {
            for j in 0..rects.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (rects[i], rects[j]);
                let inside = a.min.x >= b.min.x
                    && a.min.y >= b.min.y
                    && a.max.x <= b.max.x
                    && a.max.y <= b.max.y;
                let same_rows = a.min.y == b.min.y && a.max.y == b.max.y;
                let same_columns = a.min.x == b.min.x && a.max.x == b.max.x;
                let joined = if inside {
                    Some(b)
                } else if same_rows && a.min.x <= b.max.x && b.min.x <= a.max.x {
                    Some(Rect::new(
                        a.min.x.min(b.min.x),
                        a.min.y,
                        a.max.x.max(b.max.x),
                        a.max.y,
                    ))
                } else if same_columns && a.min.y <= b.max.y && b.min.y <= a.max.y {
                    Some(Rect::new(
                        a.min.x,
                        a.min.y.min(b.min.y),
                        a.max.x,
                        a.max.y.max(b.max.y),
                    ))
                } else {
                    None
                };
                if let Some(r) = joined {
                    rects[i] = r;
                    rects.swap_remove(j);
                    continue 'again;
                }
            }
        }
        return rects;
    }
}

impl Level {
    /// Does this level carry the deep galleries?
    pub fn has_deep_galleries(&self) -> bool {
        self.has_silt_galleries() && self.rocks.contains(&slab(1))
    }
}

/// The cavities, painted after the strata and before the rocks.
pub(super) fn shapes(level: &Level) -> Vec<Shape> {
    if !level.has_deep_galleries() {
        return Vec::new();
    }
    rooms()
        .iter()
        .map(|r| Shape::rect("deep cavity", Cell::Water, r.rect, Edges::HalfOpen))
        .collect()
}

pub(super) fn after_rocks() -> Vec<Shape> {
    Vec::new()
}

/// Add the deep galleries to a level: linings, slabs, the stalks, the octopus
/// and the breach's cover.
pub(super) fn shape(level: &mut Level) {
    // Carve the breach through the return duct's roof (the wall is a rock
    // box; the cells under it are bedrock anyway).
    let hole = Rect::new(
        BREACH.min.x,
        BREACH.min.y,
        BREACH.max.x,
        BREACH.min.y + 20.0,
    );
    level.rocks = std::mem::take(&mut level.rocks)
        .into_iter()
        .flat_map(|r| subtract_all(vec![r], std::iter::once(hole)))
        .collect();
    let existing: Vec<Rect> = level.rocks.clone();
    for wall in lining() {
        for piece in subtract_all(vec![*wall], existing.iter().copied()) {
            level.rocks.push(piece);
        }
    }
    level.rocks.extend(slabs());
    level.climbs.extend(corals().iter().copied());
    level.climbs.push(FAR_CORAL);
    level.brittle.push(Brittle {
        rect: BREACH_COVER,
        cracked: false,
    });
    level.foes.push((enemy::Kind::Octopus, OCTOPUS_DEN));
}

/// Where the nodes lie in the halls, for the economy's table (the systems
/// lead places them; `place_of` there only knows strata pockets and open
/// water): `(hall, node kind, position)`. Iron-red nodules and pebbles are the
/// second crafting tier; the lanternfish shoal is a fish fall.
pub const NODE_SITES: [(&str, &str, Vec2); 9] = [
    (
        "lanternfish hall",
        "FishFall",
        Vec2::new(2100.0, level_of(5) + 6.0),
    ),
    (
        "lanternfish hall",
        "FishFall",
        Vec2::new(2400.0, level_of(5) + 6.0),
    ),
    (
        "lanternfish hall",
        "FishFall",
        Vec2::new(2700.0, level_of(5) + 6.0),
    ),
    (
        "nodule hall",
        "NoduleField",
        Vec2::new(2200.0, level_of(9) + 6.0),
    ),
    (
        "nodule hall",
        "NoduleField",
        Vec2::new(2500.0, level_of(9) + 6.0),
    ),
    (
        "nodule hall",
        "NoduleField",
        Vec2::new(2800.0, level_of(9) + 6.0),
    ),
    (
        "whale-fall hall",
        "WhaleFallBones",
        Vec2::new(3400.0, level_of(13) + 6.0),
    ),
    (
        "whale-fall hall",
        "WhaleFallBones",
        Vec2::new(3700.0, level_of(13) + 6.0),
    ),
    (
        "whale-fall hall",
        "WhaleFallBones",
        Vec2::new(4000.0, level_of(13) + 6.0),
    ),
];

/// The names of the deep corals (landmark names are `drop coral {n}`).
pub(super) fn coral_index(climb: &Climb) -> Option<usize> {
    if *climb == FAR_CORAL {
        return Some(corals().len());
    }
    corals().iter().position(|c| c == climb)
}

/// The places of the deep galleries, for `Level::landmarks`.
pub(super) fn landmarks(level: &Level) -> Vec<Landmark> {
    if !level.has_deep_galleries() {
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
    add("drop approach", room_named("drop approach"), Kind::Frame);
    add("drop shaft", SHAFT, Kind::Frame);
    for name in ["lanternfish hall", "nodule hall", "whale-fall hall"] {
        add(name, room_named(name), Kind::Frame);
    }
    // The skeleton lies along the whale-fall hall's floor.
    let whale = room_named("whale-fall hall");
    add(
        "whale fall",
        Rect::new(
            whale.min.x + 600.0,
            whale.min.y,
            whale.max.x - 600.0,
            whale.max.y,
        ),
        Kind::Frame,
    );
    add("octopus lair", room_named("octopus lair"), Kind::Frame);
    add("lair passage", room_named("lair passage"), Kind::Frame);
    add("breach shaft", BREACH, Kind::Frame);
    // The far door out of the bone hall (its gap is a mouth of the hall, so
    // the haze keeps clear of it), its shaft, the niche to rest in and the
    // passage west.
    add("bone far door", room_named("bone far door"), Kind::Frame);
    add("bone far shaft", room_named("bone far shaft"), Kind::Frame);
    add(
        "bone far passage",
        room_named("bone far passage"),
        Kind::Frame,
    );
    add("bone far step", room_named("bone far step"), Kind::Frame);
    let niche = room_named("bone far ledge");
    add(
        "bone far ledge",
        Rect::new(niche.min.x, niche.min.y, niche.max.x, niche.min.y + 60.0),
        Kind::Refuge,
    );
    // The resting places: the Drop's top ledge, at the approach's west end,
    // and the foot, outside the lair's door.
    add(
        "drop top ledge",
        Rect::new(
            SHAFT.max.x + 40.0,
            TOP_FLOOR,
            SHAFT.max.x + 160.0,
            TOP_FLOOR + 60.0,
        ),
        Kind::Refuge,
    );
    add(
        "octopus lair door",
        Rect::new(SHAFT.max.x - 120.0, FOOT, SHAFT.max.x, FOOT + 60.0),
        Kind::Refuge,
    );
    // Every standing place on the way: the slabs' free ends are ledges.
    for (k, slab) in slabs().iter().enumerate() {
        let name = format!("drop ledge {}", k + 1);
        add(
            &name,
            Rect::new(slab.min.x, slab.max.y, slab.max.x, slab.max.y + 60.0),
            Kind::Refuge,
        );
    }
    out
}
