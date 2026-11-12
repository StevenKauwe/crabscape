//! Region 6 of `world-plan.md`: the slope caves and the sunken reef, cut into
//! the dead rock under the eastern slope (x 8,700 to 16,000, y -4,400 to
//! -9,000), authored as layer shapes (`level/layers.rs`) like the Deep
//! Galleries west of them.
//!
//! ```text
//!   slope  ~~~~\_______                                   /~~~~~  (unchanged)
//!                  [neck] slope mouth (x 10,660..10,780)    [pebble warren]
//!                    |  slope mouth shaft: 4 slabs, a stalk through each   |
//!   y -6,700 [midden cave]-[gravel]-+-[rubble]-[rubble hall]-[cobble]-[cobble cave]-[pillar]-
//!            x 8,700            foot |            |  reef shaft: a lid + 2 slabs, stalks
//!                                    |            |                      [warren shaft]
//!                         ... [pillar cave]-[east passage]-[east cave]-[warren passage]
//!                                                                  niches, stalks up to
//!   y -8,200  whale-fall hall ==[reef passage]=========+--[door]
//!             (region 4, x 5,800)                       [reef west bay][reef great hall][reef east bay]
//!                                                    y -9,000: mounds, three shelf heights
//! ```
//!
//! **The upper caves** are a line of five chambers (floor y -6,700, hunters
//! never reach them: the deepest hunting box over the slope ends at y -6,060)
//! joined by 160-tall passages. A refuge stands at most 600 units apart along
//! it. They are shown to sit at least 250 under the slope above.
//! **The slope mouth** is a 120-wide neck in the slope floor at x 10,660 to
//! 10,780, east of the kelp column's foot and the rubble haven (x 10,200 to
//! 10,430, which stay as they were) and 570 units from the slope's mantis, over
//! a stairwell like the Drop's. **The warren
//! shaft** is the line's other mouth: it rises through the pebble warren's
//! floor, with two niches to rest in. **The sunken reef** is one cavern of
//! three bays with a rubble mound at each of three shelf heights, reached by a
//! second stairwell (a lid slab and two more, 500 apart); **the reef passage**
//! runs west from its stairwell's foot to the whale-fall hall's east end,
//! 2,000 units below the canyon deep water's floor: region 6's way to region 4.
//!
//! Sizes are the crab's: tunnels 160 tall, slabs 280 wide, gaps 120. The slope
//! surface, the coral stands and the grouper's spot are untouched; a cut
//! through the sea floor leaves `floor_height` as it was (like the pebble
//! warren's door). The bake covers the world down to y -10,512; nothing here
//! lies below y -9,000.
use super::deep_galleries::merge;
use super::landmarks::{Kind, Landmark};
use super::layers::{Edges, Shape};
use super::silt_galleries::{all_voids, subtract_all};
use super::{Climb, Growth, Level};
use crate::underwater::Cell;
use bevy_math::{Rect, Vec2};
use std::sync::OnceLock;

const EDGE: f32 = 20.0;
const SLAB_THICK: f32 = 40.0;
/// How far a slab stops short of a shaft wall: its gap, the door width.
const GAP: f32 = 120.0;
/// The most one standing place may be above the next on a stairwell.
pub const MAX_STEP: f32 = 500.0;
/// The cavern chain's floor.
pub const LINE_FLOOR: f32 = -6700.0;
/// The reef's floor, under its mounds.
pub const REEF_FLOOR: f32 = -9000.0;
/// The most a crab on the line may be from cover (edge to edge).
pub const MAX_GAP: f32 = 600.0;
/// The line the refuge gap is measured along: from the midden cave's west end
/// to the warren shaft.
pub const LINE: (f32, f32) = (8700.0, 16000.0);

/// A stairwell of slabs in a shaft, as the Drop's: each slab is anchored to the
/// wall opposite the one above it, so a crab walks to a slab's free end and
/// drops one level; a coral stalk through each gap climbs back, from the
/// standing place below to the lip of the slab above.
#[derive(Clone, Copy, Debug)]
pub struct Stair {
    pub name: &'static str,
    pub shaft: Rect,
    /// Where a crab stands at the top (level 0).
    pub top: f32,
    /// The distance between one standing place and the next (at most
    /// [`MAX_STEP`]).
    pub step: f32,
    /// How many slabs stand in the shaft (the floor under them is the next
    /// level).
    pub slabs: i32,
    /// Does level 0 have a slab of its own (a bridge over the shaft), not just
    /// the open air over the sea floor?
    pub lid: bool,
    /// The x of the stalk through level 0, if it is not a slab's gap.
    pub top_stalk: Option<f32>,
}

impl Stair {
    /// The top of level `k` (0 is the top, `slabs + 1` the shaft's floor).
    pub const fn level_of(&self, k: i32) -> f32 {
        self.top - self.step * k as f32
    }

    /// The shaft's floor.
    pub const fn foot(&self) -> f32 {
        self.level_of(self.slabs + 1)
    }

    /// Slab `k`'s gap is on the east side when `k` is even.
    fn gap_east(k: i32) -> bool {
        k % 2 == 0
    }

    /// The stalk through level `k`'s gap: its x.
    pub fn stalk_x(&self, k: i32) -> f32 {
        if k == 0
            && let Some(x) = self.top_stalk
        {
            return x;
        }
        if Self::gap_east(k) {
            self.shaft.max.x - GAP / 2.0
        } else {
            self.shaft.min.x + GAP / 2.0
        }
    }

    /// Slab `k`: bedrock across all but a gap of the shaft.
    pub fn slab(&self, k: i32) -> Rect {
        let top = self.level_of(k);
        let (x0, x1) = if Self::gap_east(k) {
            (self.shaft.min.x, self.shaft.max.x - GAP)
        } else {
            (self.shaft.min.x + GAP, self.shaft.max.x)
        };
        Rect::new(x0, top - SLAB_THICK, x1, top)
    }

    /// The slabs that exist, top to bottom.
    pub fn slab_list(&self) -> Vec<(i32, Rect)> {
        let first = if self.lid { 0 } else { 1 };
        (first..=self.slabs).map(|k| (k, self.slab(k))).collect()
    }

    /// The stalks, bottom to top.
    pub fn corals(&self) -> Vec<Climb> {
        (0..=self.slabs)
            .rev()
            .map(|k| Climb {
                x: self.stalk_x(k),
                bottom: self.level_of(k + 1),
                top: self.level_of(k) + 10.0,
                kind: Growth::Coral,
            })
            .collect()
    }
}

/// The slope mouth's neck: a 120-wide hole through the slope floor, on the
/// slope east of the kelp column's foot (floor y -4,362 at its west edge, -4,437
/// at its east). Its walls end flush with the floor on each side.
pub const NECK: Rect = Rect::new(10660.0, -4620.0, 10780.0, -4360.0);
/// Where the neck's west and east walls end: the slope floor at each wall's lower edge (never above it, so the walking route is not stepped).
const NECK_WALLS: (f32, f32) = (-4362.5, -4450.0);
/// The slope mouth's stairwell: from the slope floor (y -4,400 at the neck's
/// middle) down five levels of 460 to the line's floor.
pub const MOUTH_STAIR: Stair = Stair {
    name: "slope stair",
    shaft: Rect::new(10520.0, LINE_FLOOR, 10920.0, -4620.0),
    top: -4400.0,
    step: 460.0,
    slabs: 4,
    lid: false,
    top_stalk: Some(10720.0),
};
/// The reef stairwell: from the rubble hall's floor down three levels to the
/// reef door. Its lid is a bridge, so the line crosses it dry-shod.
pub const REEF_STAIR: Stair = Stair {
    name: "reef stair",
    shaft: Rect::new(11300.0, -8200.0, 11700.0, LINE_FLOOR),
    top: LINE_FLOOR,
    step: 500.0,
    slabs: 2,
    lid: true,
    top_stalk: None,
};
/// Both stairwells.
pub const STAIRS: [Stair; 2] = [MOUTH_STAIR, REEF_STAIR];

/// The warren shaft's stalk x, and the heights of its two niches' floors.
const WARREN_X: f32 = 15940.0;
const NICHES: [f32; 2] = [-6200.0, -5700.0];
/// The pebble warren's floor (its rock lid is cut for the shaft).
const WARREN_FLOOR: f32 = -5500.0;
/// The warren's soil lies on its floor: the shaft reaches up through it.
const WARREN_TOP: f32 = -5460.0;

/// What a room is, for its lining and for the hunters' reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// One of the line's chambers.
    Cave,
    /// A 160-tall passage.
    Passage,
    /// A stairwell shaft.
    Shaft,
    /// A way to the slope: open to the water above (no roof).
    Mouth,
    /// A niche in a shaft's wall to rest in.
    Niche,
    /// The door from the reef stairwell into the reef.
    Door,
    /// A bay of the sunken reef.
    Reef,
}

impl Part {
    /// Is this a way up to the slope? A mouth, a shaft or a niche reaches up
    /// to ground where hunters live, so only each hunter's notice range (and
    /// body) is kept off it, where the rest keep the whole of a hunter's
    /// reach (its chase, lunge and a margin) off.
    pub fn is_way_up(self) -> bool {
        matches!(self, Part::Mouth | Part::Shaft | Part::Niche)
    }
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
        use Part::{Cave, Door, Mouth, Niche, Passage, Reef, Shaft};
        let r = Rect::new;
        let f = LINE_FLOOR;
        vec![
            room("midden cave", r(8700.0, f, 9900.0, f + 300.0), Cave),
            room("gravel passage", r(9900.0, f, 10520.0, f + 160.0), Passage),
            room("slope mouth shaft", MOUTH_STAIR.shaft, Shaft),
            room("slope mouth", NECK, Mouth),
            room("rubble passage", r(10920.0, f, 11100.0, f + 160.0), Passage),
            room("rubble hall", r(11100.0, f, 12200.0, f + 340.0), Cave),
            room("reef shaft", REEF_STAIR.shaft, Shaft),
            room("cobble passage", r(12200.0, f, 12600.0, f + 160.0), Passage),
            room("cobble cave", r(12600.0, f, 13600.0, f + 320.0), Cave),
            room("pillar passage", r(13600.0, f, 14000.0, f + 160.0), Passage),
            room("pillar cave", r(14000.0, f, 14900.0, f + 360.0), Cave),
            room("east passage", r(14900.0, f, 15200.0, f + 160.0), Passage),
            room("east cave", r(15200.0, f, 15700.0, f + 300.0), Cave),
            room("warren passage", r(15700.0, f, 15880.0, f + 160.0), Passage),
            room(
                "warren shaft",
                r(WARREN_X - 60.0, f, WARREN_X + 60.0, WARREN_TOP),
                Mouth,
            ),
            room(
                "warren shaft ledge 1",
                r(15720.0, NICHES[0], 15880.0, NICHES[0] + 160.0),
                Niche,
            ),
            room(
                "warren shaft ledge 2",
                r(15720.0, NICHES[1], 15880.0, NICHES[1] + 160.0),
                Niche,
            ),
            room("reef door", r(11700.0, -8200.0, 11820.0, -8040.0), Door),
            room(
                "reef west bay",
                r(11820.0, REEF_FLOOR, 12800.0, -7700.0),
                Reef,
            ),
            room(
                "reef great hall",
                r(12800.0, REEF_FLOOR, 14000.0, -7200.0),
                Reef,
            ),
            room(
                "reef east bay",
                r(14000.0, REEF_FLOOR, 15200.0, -7500.0),
                Reef,
            ),
            room(
                "reef passage",
                r(5800.0, -8200.0, REEF_STAIR.shaft.min.x, -8040.0),
                Passage,
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

/// The room named `name`.
pub fn room_rect(name: &str) -> Rect {
    room_named(name)
}

/// The sunken reef as a whole: the three bays.
pub fn reef() -> Rect {
    room_named("reef west bay")
        .union(room_named("reef great hall"))
        .union(room_named("reef east bay"))
}

/// The reef's mounds: rubble of coral and rock on its floor, shelves at three
/// heights (y -8,200, -8,600 and -8,850). Each is `(rect, name of the bay)`.
pub const MOUNDS: [Rect; 6] = [
    // The west terrace, flush with the reef door's floor.
    Rect::new(11820.0, REEF_FLOOR, 12300.0, -8200.0),
    Rect::new(12300.0, REEF_FLOOR, 12700.0, -8600.0),
    Rect::new(13050.0, REEF_FLOOR, 13550.0, -8850.0),
    Rect::new(13750.0, REEF_FLOOR, 14350.0, -8600.0),
    Rect::new(14350.0, REEF_FLOOR, 14750.0, -8200.0),
    Rect::new(14850.0, REEF_FLOOR, 15150.0, -8850.0),
];

/// The coral stalks between the mounds, on the lower side of each step:
/// `(x, bottom, top of the shelf it climbs to)`.
const REEF_CORAL: [(f32, f32, f32); 6] = [
    (12740.0, REEF_FLOOR, -8600.0),
    (12340.0, -8600.0, -8200.0),
    (13010.0, REEF_FLOOR, -8850.0),
    (13710.0, REEF_FLOOR, -8600.0),
    (14310.0, -8600.0, -8200.0),
    (14810.0, REEF_FLOOR, -8850.0),
];

/// Small rubble on the chambers' floors: `(x0, x1, height)`. Each is a step a
/// crab walks over.
const RUBBLE: [(f32, f32, f32); 8] = [
    (9060.0, 9150.0, 60.0),
    (9420.0, 9520.0, 40.0),
    (12060.0, 12140.0, 40.0),
    (12880.0, 12960.0, 40.0),
    (13300.0, 13400.0, 60.0),
    (14240.0, 14320.0, 40.0),
    (14720.0, 14800.0, 60.0),
    (15300.0, 15370.0, 40.0),
];

/// Where a refuge stands (x of its west end) along the line: one niche in the
/// floor, 120 wide.
pub const REFUGES: [f32; 16] = [
    8740.0, 9200.0, 9680.0, 10160.0, 10640.0, 11120.0, 11740.0, 12200.0, 12680.0, 13160.0, 13640.0,
    14120.0, 14600.0, 15040.0, 15480.0, 15720.0,
];
/// Refuges along the reef passage, which is a long walk home.
const PASSAGE_REFUGES: [f32; 4] = [6400.0, 7400.0, 8400.0, 9400.0];

/// The refuge rects along the line, west to east.
pub fn line_refuges() -> Vec<Rect> {
    REFUGES
        .iter()
        .map(|&x| Rect::new(x, LINE_FLOOR, x + 120.0, LINE_FLOOR + 60.0))
        .collect()
}

/// The wall crumbs of every cavity: sides, roof (not for a mouth) and (but the
/// reef's mounds) no floor: a body in a cavern stands on the layer map's own
/// floor.
pub fn lining() -> &'static [Rect] {
    static LINING: OnceLock<Vec<Rect>> = OnceLock::new();
    LINING.get_or_init(|| {
        let mut out = Vec::new();
        let voids: Vec<Rect> = rooms().iter().map(|r| r.rect).collect();
        for r in rooms() {
            let a = r.rect;
            let mut bands = vec![
                Rect::new(a.min.x - EDGE, a.min.y, a.min.x, a.max.y + EDGE),
                Rect::new(a.max.x, a.min.y, a.max.x + EDGE, a.max.y + EDGE),
            ];
            if r.part == Part::Mouth {
                // Open at the top: its walls stop where the ground does, each
                // flush with the floor beside it.
                let (west, east) = match r.name {
                    "warren shaft" => (WARREN_FLOOR, WARREN_FLOOR),
                    _ => NECK_WALLS,
                };
                bands[0].max.y = west;
                bands[1].max.y = east;
            } else {
                bands.push(Rect::new(a.min.x, a.max.y, a.max.x, a.max.y + EDGE));
            }
            out.extend(subtract_all(
                bands,
                all_voids().chain(voids.iter().copied()),
            ));
        }
        merge(out)
    })
}

/// All the corals this module plants, in the order of their names
/// (`cave coral {n}`).
pub fn corals() -> &'static [Climb] {
    static CORALS: OnceLock<Vec<Climb>> = OnceLock::new();
    CORALS.get_or_init(|| {
        let mut out = Vec::new();
        for s in &STAIRS {
            out.extend(s.corals());
        }
        // The warren shaft: a stalk from each standing place to the next.
        let levels = [LINE_FLOOR, NICHES[0], NICHES[1], WARREN_FLOOR];
        for w in levels.windows(2) {
            let top = if w[1] == WARREN_FLOOR {
                WARREN_TOP - 10.0
            } else {
                w[1] - 10.0
            };
            out.push(Climb {
                x: WARREN_X,
                bottom: w[0],
                top,
                kind: Growth::Coral,
            });
        }
        for &(x, bottom, top) in &REEF_CORAL {
            out.push(Climb {
                x,
                bottom,
                top: top - 10.0,
                kind: Growth::Coral,
            });
        }
        out
    })
}

/// The name index of one of this module's corals.
pub(super) fn coral_index(climb: &Climb) -> Option<usize> {
    corals().iter().position(|c| c == climb)
}

impl Level {
    /// Does this level carry the slope caves?
    pub fn has_slope_caves(&self) -> bool {
        self.has_deep_galleries() && self.rocks.contains(&MOUNDS[0])
    }
}

/// The cavities, painted after the strata and before the rocks.
pub(super) fn shapes(level: &Level) -> Vec<Shape> {
    if !level.has_slope_caves() {
        return Vec::new();
    }
    rooms()
        .iter()
        .map(|r| Shape::rect("slope cavity", Cell::Water, r.rect, Edges::HalfOpen))
        .collect()
}

/// Add the slope caves to a level: the linings, the stairwells' slabs and
/// stalks, the reef's mounds and the rubble. Two doors are cut in rock that
/// was there: the whale-fall hall's east wall (the reef passage's door) and
/// the pebble warren's floor (the warren shaft's).
pub(super) fn shape(level: &mut Level) {
    let cuts: Vec<Rect> = [
        Rect::new(5800.0, -8200.0, 5800.0 + EDGE, -8040.0),
        Rect::new(
            WARREN_X - 60.0,
            WARREN_FLOOR - EDGE,
            WARREN_X + 60.0,
            WARREN_FLOOR,
        ),
    ]
    .to_vec();
    level.rocks = std::mem::take(&mut level.rocks)
        .into_iter()
        .flat_map(|r| subtract_all(vec![r], cuts.iter().copied()))
        .collect();
    let existing: Vec<Rect> = level.rocks.clone();
    for wall in lining() {
        for piece in subtract_all(vec![*wall], existing.iter().copied()) {
            level.rocks.push(piece);
        }
    }
    for s in &STAIRS {
        level
            .rocks
            .extend(s.slab_list().into_iter().map(|(_, r)| r));
    }
    for (x0, x1, h) in RUBBLE {
        level
            .rocks
            .push(Rect::new(x0, LINE_FLOOR, x1, LINE_FLOOR + h));
    }
    level.rocks.extend(MOUNDS);
    level.climbs.extend(corals().iter().copied());
}

/// Where the nodes lie, for the systems lead (who places them):
/// `(room, node kind, position)`. Three coral fragments on the reef's mounds,
/// three grit heaps on the upper caves' floors and one midden (a tooth bed) in
/// the cave nearest the far rim's octopus den.
pub const NODE_SITES: [(&str, &str, Vec2); 7] = [
    (
        "midden cave",
        "ToothBed",
        Vec2::new(8950.0, LINE_FLOOR + 6.0),
    ),
    (
        "rubble hall",
        "GritHeap",
        Vec2::new(11980.0, LINE_FLOOR + 6.0),
    ),
    (
        "cobble cave",
        "GritHeap",
        Vec2::new(13000.0, LINE_FLOOR + 6.0),
    ),
    (
        "pillar cave",
        "GritHeap",
        Vec2::new(14420.0, LINE_FLOOR + 6.0),
    ),
    ("reef west bay", "BambooCoral", Vec2::new(12500.0, -8594.0)),
    (
        "reef great hall",
        "BambooCoral",
        Vec2::new(13850.0, -8594.0),
    ),
    ("reef east bay", "BambooCoral", Vec2::new(14550.0, -8194.0)),
];

/// The places of the slope caves, for `Level::landmarks`.
pub(super) fn landmarks(level: &Level) -> Vec<Landmark> {
    if !level.has_slope_caves() {
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
    for r in rooms() {
        if r.name == "slope mouth" {
            // A door in the ground reaches up out of it, so a crab in the open
            // water above sees it.
            add(
                r.name,
                Rect::new(
                    r.rect.min.x,
                    r.rect.min.y,
                    r.rect.max.x,
                    r.rect.max.y + 40.0,
                ),
                Kind::Mouth,
            );
        } else {
            add(r.name, r.rect, Kind::Frame);
        }
    }
    add("sunken reef", reef(), Kind::Frame);
    // A refuge every few hundred units along the line, and some on the reef
    // passage (a long walk home).
    for (i, rect) in line_refuges().into_iter().enumerate() {
        add(&format!("cave refuge {i}"), rect, Kind::Refuge);
    }
    for (i, &x) in PASSAGE_REFUGES.iter().enumerate() {
        add(
            &format!("cave refuge {}", line_refuges().len() + i),
            Rect::new(x, -8200.0, x + 120.0, -8140.0),
            Kind::Refuge,
        );
    }
    // The stairwells' standing places: the slabs' free ends, and the foot.
    for s in &STAIRS {
        for (k, slab) in s.slab_list() {
            add(
                &format!("{} ledge {k}", s.name),
                Rect::new(slab.min.x, slab.max.y, slab.max.x, slab.max.y + 60.0),
                Kind::Refuge,
            );
        }
    }
    // The warren shaft's niches are places to rest.
    for (i, y) in NICHES.iter().enumerate() {
        add(
            &format!("warren shaft niche {i}"),
            Rect::new(15720.0, *y, 15880.0, *y + 60.0),
            Kind::Refuge,
        );
    }
    // The reef's shelves are places to stand, one refuge on each height.
    for (i, m) in MOUNDS.iter().enumerate() {
        add(
            &format!("reef shelf {i}"),
            Rect::new(m.min.x + 40.0, m.max.y, m.max.x - 40.0, m.max.y + 60.0),
            Kind::Frame,
        );
    }
    out
}
