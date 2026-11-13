//! The shallows near home, for a new crab's first hour (`first-hour.md`, "World
//! lane: the sites"): names a player can say, a rock pool, a lookout over
//! home, a hollow off the trench, and the data the economy places its first
//! nodes from.
//!
//! Nothing here moves a feel number or an existing rock, climb, foe or rest
//! point, and nothing touches the shelf (x 900 to 1,290): the creature tests
//! fight on its flat floor. The new ground is:
//!
//! - **No pool** (see `POOLS`): the pool machinery stays (a dip cut into rock
//!   on a bedrock bowl, as the bake sweeps loose sand into any hole), with no
//!   pool placed.
//! - **The lookout**: a bedrock ledge east of the kelp stand beside home
//!   (x 280, the climb to the surface), hung off nothing (rocks float, as the
//!   sea arch does), 704 up. A climber steps off the stalk's side onto it.
//! - **The trench hollow**: a bay cut into the west wall of the silt mouth
//!   shaft, floor 220 under the trench floor (the tier-2 band). The trench
//!   floor itself is full (the stepping stone, the rest, the mouth and a
//!   moray at each end), so the hollow opens off the mouth shaft; the shaft's
//!   coral stalk (`silt_galleries::CORAL`) is its way out.
//!
//! Sizes are the crab's: 65 units tall at its biggest, 97 wide.

use super::landmarks::{Kind, Landmark};
use super::layers::{Edges, Shape};
use super::{Climb, Growth, Level};
use crate::enemy::Kind as Foe;
use crate::underwater::Cell;
use bevy_math::{Rect, Vec2};

/// Thickness of the sediment a pool's bowl replaces (the sand and clay bands
/// are a hundred units thick: `layers.rs`).
const SEDIMENT: f32 = 100.0;

/// A rock pool: a dip cut into the bedrock.
pub struct Pool {
    pub name: &'static str,
    /// The floor line through the pool, from rim to rim (a cliff is two
    /// points less than 2 units apart).
    pub profile: &'static [Vec2],
    /// The rock surface the pool's water lies on, from rim to rim (the floor
    /// line without the rims' cliffs).
    pub rock: &'static [Vec2],
    /// The cavity's top: everything over the rock surface to here is water.
    pub top: f32,
    /// The standing floor of the pool.
    pub bottom: f32,
}

/// The pools cut into the shallows: none today. A ledge pool at the foot of the far ledge's slope (x 2,290 to
/// 2,440, 140 deep on a bedrock bowl) trapped a crab walking east in the cells world (the co-op cells check), and a
/// shallow one the bake fills with sand; the trench is the first dive instead.
pub const POOLS: [Pool; 0] = [];

/// The lookout ledge, east of the kelp stand at x 280 (which climbs to the
/// surface): 704 over the floor. A climber on the stalk's east side steps off
/// onto it; its west end stands a body's length (65) and a sway clear of the
/// stalk. (Under y 664 it sits in the columns the cell-join tests build pebbles in and a grain the region-store test lets fall must reach the floor; so it is over the region's bottom row.)
pub const LOOKOUT: Rect = Rect::new(368.0, 684.0, 458.0, 704.0);
/// The lookout's standing top.
pub const LOOKOUT_TOP: f32 = 704.0;
/// The kelp stand beside home that climbs to it (`royal::ROPE_X`).
pub const LOOKOUT_KELP_X: f32 = 280.0;

/// More weak creatures near home, for Fighting (`first-hour.md`): snapping
/// shrimp, which strike only within 72 of their burrow. They stand on the sea
/// floor whatever `y` says (`Sim::spawn_enemy`), so `y` here is the floor's.
/// Snapping shrimp live in colonies, so each joins an old one. Almost no
/// floor is left for more: every node and site keeps a shrimp 92 off, the
/// creature tests fight on the shelf (x 900 to 1,290), the plug shaft keeps
/// a hunter's whole reach (294) off, and the co-op walks, the rest points and
/// the moray rule out the rest of x 0 to 2,700.
pub const FIGHTS: &[(&str, Foe, Vec2)] = &[
    // Beside the shrimp at x 590, on the slope at the home burrow's second
    // mouth (its reach ends at x 916, short of the plug shaft).
    ("row shrimp", Foe::Snapper, Vec2::new(622.0, 34.7)),
    // Beside the shrimp at x 2,060, on its far side (the moray hunts the nearer one first).
    ("block shrimp", Foe::Snapper, Vec2::new(2074.0, 110.0)),
];

/// A short kelp climb to a small rock ledge: (name, the stalk, the ledge). The
/// stalk is gripped from the ledge's side and stands 40 over the ledge's top;
/// its foot is on the floor.
pub const KELP_LEDGES: &[(&str, Climb, Rect)] = &[
    // On the block past the trench, 280 up from its top; the ledge lies west
    // of its stalk, over the far ledge's start and the trench's east rim.
    (
        "block kelp ledge",
        Climb {
            x: 1980.0,
            bottom: 200.0,
            top: 500.0,
            kind: Growth::Kelp,
        },
        Rect::new(1802.0, 440.0, 1892.0, 460.0),
    ),
    // Past the far ledge's foot, 180 up (low: a high one would shorten the
    // cliff's climb); the ledge lies east of its stalk, over the flats the
    // moray guards (out of its reach).
    (
        "flats kelp ledge",
        Climb {
            x: 2392.0,
            bottom: 20.0,
            top: 240.0,
            kind: Growth::Kelp,
        },
        Rect::new(2480.0, 180.0, 2570.0, 200.0),
    ),
];

/// The paddle gap: the lookout's east end to the far lookout, both 704 up.
/// A jump off the lookout's edge falls short; a stroke carries across. A crab
/// that falls lands on the floor, 600 and more below, and climbs the kelp again.
pub const GAP: f32 = 300.0;
pub const FAR_LOOKOUT: Rect = Rect::new(
    LOOKOUT.max.x + GAP,
    LOOKOUT.min.y,
    LOOKOUT.max.x + GAP + 90.0,
    LOOKOUT.max.y,
);

/// The high garden: a bedrock ledge 1,300 up, over the shelf, in plain sight from
/// the lookout, the far lookout and the floor, and out of reach of a new crab.
///
/// Why a crab cannot just glide to it: the kelp stand beside home climbs to the
/// surface, and a crab that kicks off the stalk drifts east a long way (about
/// two units across for every one it sinks, and a kick-off hops it 190 higher
/// first). So the garden lies under a **canopy**, a bedrock slab 230 over its
/// top that runs back west almost to the stalk: under it a drifting crab has
/// 165 to sink, and only the paddle strokes carry it the rest of the way. With
/// the strokes of one egg a crab lands about 640 east of the stalk; two eggs
/// carry it onto the ledge (which starts at 760), from a kick-off at about
/// 1,224 where the stalk is climbed in three steps.
pub const GARDEN: Rect = Rect::new(760.0, 1280.0, 940.0, 1300.0);
/// The garden's standing top.
pub const GARDEN_TOP: f32 = 1300.0;
/// The slab over the garden (its underside is 230 over the garden's top); its
/// east end meets the sea arch (x 1,250).
pub const CANOPY: Rect = Rect::new(350.0, 1530.0, 1250.0, 1550.0);

/// The glass stair: a bare rock face over the trench's west rim, east of the
/// sea arch (x 1,250 to 1,300: a crab that hangs west of it climbs into its
/// underside), up to the glow ledge. A crab's leg tips slide off it; only glass
/// grips climb it (`Growth::Rock`). A crab jumps to it off the rim and grips
/// on the way in. It stands 40 over the ledge's top, as the kelp ledges' do.
pub const GLASS_STAIR: Climb = Climb {
    x: 1400.0,
    bottom: 60.0,
    top: 1140.0,
    kind: Growth::Rock,
};
/// The glow ledge the stair climbs to: east of the stalk, over the trench.
pub const GLOW_LEDGE: Rect = Rect::new(1488.0, 1080.0, 1578.0, 1100.0);
/// The glow ledge's standing top.
pub const GLOW_LEDGE_TOP: f32 = 1100.0;

/// The hollow's bay: a cavity in the west wall of the silt mouth shaft, its
/// door on the shaft. The inside, floor to roof.
pub const HOLLOW: Rect = Rect::new(1380.0, -480.0, 1520.0, -340.0);
/// The shaft wall's lining as `silt_galleries::lining` makes it, where the
/// door is cut.
const SHAFT_WALL: Rect = Rect::new(1520.0, -820.0, 1540.0, -260.0);
/// The door through that wall.
const DOOR: (f32, f32) = (-480.0, -340.0);
const WALL: f32 = 20.0;

/// The rocks the hollow is lined with (roof, west wall, floor), and the two
/// pieces of the shaft wall left either side of its door.
fn hollow_rocks() -> [Rect; 5] {
    [
        // Roof.
        Rect::new(
            HOLLOW.min.x - WALL,
            HOLLOW.max.y,
            HOLLOW.max.x,
            HOLLOW.max.y + WALL,
        ),
        // West wall.
        Rect::new(
            HOLLOW.min.x - WALL,
            HOLLOW.min.y - WALL,
            HOLLOW.min.x,
            HOLLOW.max.y,
        ),
        // Floor, on out through the door to the shaft's edge.
        Rect::new(
            HOLLOW.min.x - WALL,
            HOLLOW.min.y - WALL,
            SHAFT_WALL.max.x,
            HOLLOW.min.y,
        ),
        // The shaft wall, either side of the door.
        Rect::new(SHAFT_WALL.min.x, SHAFT_WALL.min.y, SHAFT_WALL.max.x, DOOR.0),
        Rect::new(SHAFT_WALL.min.x, DOOR.1, SHAFT_WALL.max.x, SHAFT_WALL.max.y),
    ]
}

/// Sites for the economy: (place, suggested node kind, position on a floor).
/// Tier 1 (above y -260) in groups of two to four different skills along the
/// way out, none within any foe's danger reach and none in a doorway, then
/// the tier-2 band's three in the trench hollow. The systems lead places the
/// nodes from these (`economy/place.rs`).
///
/// Groups, west to east: the home pocket, the shrimp row's pocket, the boulder,
/// the lookout up the kelp beside home, the shelf's west and east ends, the trench rim, the far ledge's
/// start, its slope and its foot (the crest of the cliff above the basin holds none: the loggerhead at
/// (2,429, 919) reaches it)
/// (reached only with the strokes the far ledge's eggs give).
/// Sights of later, seen from the way out and not reached by a new crab: the high garden (two eggs' strokes from the
/// kelp) and the glow ledge (glass grips). `(place, suggested node kind, point on its top)`, apart from `SITES` until
/// the systems lead wires them (every `SITES` entry holds a node: `economy_core`), tier 3 and tier 1 kinds for it to
/// decide.
pub const SIGHT_SITES: &[(&str, &str, Vec2)] = &[
    ("high garden", "GlassSpongeGarden", Vec2::new(810.0, 1300.0)),
    ("high garden", "TwilightGlow", Vec2::new(880.0, 1300.0)),
    ("glow ledge", "ShoreGlow", Vec2::new(1515.0, 1100.0)),
    ("glow ledge", "SnailRocks", Vec2::new(1550.0, 1100.0)),
];

pub const SITES: &[(&str, &str, Vec2)] = &[
    // Home's pocket, between the home burrow's first shaft and the first
    // snapping shrimp (x 340).
    ("home pocket", "ShoreGlow", Vec2::new(224.0, 0.0)),
    ("home pocket", "KelpStand", Vec2::new(234.0, 0.0)),
    ("home pocket", "SnailRocks", Vec2::new(244.0, 0.0)),
    // On the lookout ledge, up the kelp stand beside home.
    ("the lookout", "KelpStand", Vec2::new(390.0, 704.0)),
    ("the lookout", "MusselClump", Vec2::new(430.0, 704.0)),
    // On the far lookout, across the paddle gap.
    ("far lookout", "ShoreGlow", Vec2::new(785.0, 704.0)),
    ("far lookout", "SnailRocks", Vec2::new(820.0, 704.0)),
    // The shrimp row's pocket, between the two shrimp (x 340 and 590).
    ("shrimp row", "MusselClump", Vec2::new(440.0, 0.0)),
    ("shrimp row", "GritHeap", Vec2::new(458.0, 0.0)),
    ("shrimp row", "SnailRocks", Vec2::new(476.0, 0.0)),
    ("shrimp row", "ShoreGlow", Vec2::new(494.0, 0.0)),
    // On top of the boulder half-buried in the slope: a jump up.
    ("the boulder", "MusselClump", Vec2::new(695.0, 100.0)),
    ("the boulder", "SnailRocks", Vec2::new(710.0, 100.0)),
    // The shelf's west end, on the slope's top.
    ("the shelf", "KelpStand", Vec2::new(776.0, 94.0)),
    ("the shelf", "MusselClump", Vec2::new(806.0, 105.0)),
    ("the shelf", "SnailRocks", Vec2::new(836.0, 110.0)),
    // The shelf's east end, past the plug shaft and clear of the arena.
    ("shelf east", "GritHeap", Vec2::new(1070.0, 110.0)),
    ("shelf east", "ShoreGlow", Vec2::new(1100.0, 110.0)),
    // The trench's west rim, under the sea arch.
    ("trench rim", "SnailRocks", Vec2::new(1264.0, 110.0)),
    ("trench rim", "GritHeap", Vec2::new(1284.0, 110.0)),
    // The far ledge's start, past the trench, out of the shrimp at x 2,060.
    ("far ledge", "GritHeap", Vec2::new(1846.0, 110.0)),
    ("far ledge", "ShoreGlow", Vec2::new(1876.0, 110.0)),
    ("far ledge", "MusselClump", Vec2::new(1906.0, 110.0)),
    // The far ledge's slope.
    ("far ledge slope", "SnailRocks", Vec2::new(2190.0, 76.0)),
    ("far ledge slope", "MusselClump", Vec2::new(2235.0, 64.0)),
    ("far ledge slope", "KelpStand", Vec2::new(2280.0, 51.0)),
    // Further down the far ledge's slope.
    ("far ledge foot", "GritHeap", Vec2::new(2335.0, 33.0)),
    ("far ledge foot", "ShoreGlow", Vec2::new(2355.0, 30.0)),
    // Tier 2: the trench hollow, a level-1 crab's first dive.
    ("trench hollow", "ClamBed", Vec2::new(1415.0, -480.0)),
    ("trench hollow", "ClayBank", Vec2::new(1455.0, -480.0)),
    ("trench hollow", "WormMud", Vec2::new(1495.0, -480.0)),
];

/// Small finds, for the systems lead to wire (drifting ones hang in the water
/// a little over the ground; buried ones lie a little under the sediment's
/// top). Each is a find's middle.
pub const FINDS: &[Vec2] = &[
    // Drifting over the boulder.
    Vec2::new(700.0, 140.0),
    // Drifting over the shelf's west end.
    Vec2::new(880.0, 150.0),
    // Drifting over the lookout ledge.
    Vec2::new(410.0, 754.0),
    // Drifting over the far lookout.
    Vec2::new(800.0, 754.0),
    // Buried at the trench rim.
    Vec2::new(1275.0, 86.0),
    // Drifting over the high garden.
    Vec2::new(850.0, 1350.0),
    // Drifting over the glow ledge.
    Vec2::new(1535.0, 1150.0),
    // Drifting in the hollow.
    Vec2::new(1400.0, -440.0),
    // Drifting on the far ledge.
    Vec2::new(1905.0, 140.0),
    // Buried on the far ledge's slope.
    Vec2::new(2215.0, 40.0),
    // Drifting low over the far ledge's foot.
    Vec2::new(2345.0, 80.0),
];

impl Level {
    /// Does this level carry the shallows (a custom level may not)? It does
    /// when the lookout ledge is among its rocks.
    pub fn has_shallows(&self) -> bool {
        self.rocks.contains(&LOOKOUT)
    }
}

/// Add the shallows to a level: the pools in the floor line, the lookout
/// ledge and its kelp, and the hollow's rocks (cut through the shaft wall).
pub(super) fn shape(level: &mut Level) {
    for pool in &POOLS {
        let (first, last) = (pool.profile[0].x, pool.profile[pool.profile.len() - 1].x);
        level.floor.retain(|p| p.x < first || p.x > last);
        let at = level.floor.partition_point(|p| p.x < first);
        for (i, p) in pool.profile.iter().enumerate() {
            level.floor.insert(at + i, *p);
        }
    }
    level.rocks.push(LOOKOUT);
    level.rocks.push(FAR_LOOKOUT);
    level.rocks.push(GARDEN);
    level.rocks.push(CANOPY);
    level.rocks.push(GLOW_LEDGE);
    level.climbs.push(GLASS_STAIR);
    for (_, climb, ledge) in KELP_LEDGES {
        level.climbs.push(*climb);
        level.rocks.push(*ledge);
    }
    level
        .foes
        .extend(FIGHTS.iter().map(|&(_, kind, at)| (kind, at)));
    // The hollow: the lined bay, with the shaft wall cut for its door.
    if let Some(i) = level.rocks.iter().position(|r| *r == SHAFT_WALL) {
        level.rocks.remove(i);
    }
    level.rocks.extend(hollow_rocks());
}

/// The pool's bedrock bowl (the sediment under its rock surface, made rock)
/// and its cavity (the water over the surface, up to the top).
fn pool_shapes(pool: &Pool) -> [Shape; 2] {
    let mut bowl: Vec<Vec2> = pool.rock.to_vec();
    bowl.extend(
        pool.rock
            .iter()
            .rev()
            .map(|p| Vec2::new(p.x, p.y - SEDIMENT)),
    );
    let (first, last) = (pool.rock[0], pool.rock[pool.rock.len() - 1]);
    let mut cavity: Vec<Vec2> = pool.rock.to_vec();
    cavity.push(Vec2::new(last.x, pool.top));
    cavity.push(Vec2::new(first.x, pool.top));
    [
        Shape::poly("pool cavity", Cell::Water, cavity),
        Shape::poly("pool bowl", Cell::Rock, bowl),
    ]
}

/// The pools' bowls and cavities and the hollow's cavity, painted after the
/// strata and before the rocks.
pub(super) fn shapes(level: &Level) -> Vec<Shape> {
    if !level.has_shallows() {
        return Vec::new();
    }
    let mut out: Vec<Shape> = POOLS.iter().flat_map(pool_shapes).collect();
    // The cavity runs through the door, to the shaft.
    out.push(Shape::rect(
        "hollow cavity",
        Cell::Water,
        Rect::new(HOLLOW.min.x, HOLLOW.min.y, SHAFT_WALL.max.x, HOLLOW.max.y),
        Edges::HalfOpen,
    ));
    out
}

/// The old ground that gets a name only (open ground, not a sealed frame:
/// `tests/pocket_flood.rs` does not count these as dead ends behind a door).
pub const GROUND: [(&str, Rect); 5] = [
    ("shrimp row", Rect::new(300.0, -20.0, 640.0, 160.0)),
    ("the shelf", Rect::new(720.0, 0.0, 1330.0, 140.0)),
    ("the trench", Rect::new(1301.0, -260.0, 1819.0, 120.0)),
    ("far ledge", Rect::new(1850.0, 0.0, 2470.0, 140.0)),
    ("the crest", Rect::new(2717.0, 860.0, 2800.0, 960.0)),
];

/// The shallows' places, for `Level::landmarks`. All are plain names
/// (`Kind::Frame`): none is a refuge, a tunnel or a gate.
pub(super) fn landmarks(level: &Level) -> Vec<Landmark> {
    let mut out = Vec::new();
    let mut add = |name: &str, rect: Rect| {
        out.push(Landmark {
            name: name.into(),
            rect,
            kind: Kind::Frame,
        });
    };
    // The old ground, named: the row of burrows, the shelf, the trench, the
    // far ledge and the cliff's crest.
    for (name, rect) in GROUND {
        add(name, rect);
    }
    if !level.has_shallows() {
        return out;
    }
    for pool in &POOLS {
        let (first, last) = (pool.profile[0], pool.profile[pool.profile.len() - 1]);
        add(pool.name, Rect::new(first.x, pool.bottom, last.x, pool.top));
    }
    add(
        "the lookout",
        Rect::new(
            LOOKOUT.min.x,
            LOOKOUT_TOP,
            LOOKOUT.max.x,
            LOOKOUT_TOP + 60.0,
        ),
    );
    add("trench hollow", HOLLOW);
    add(
        "far lookout",
        Rect::new(
            FAR_LOOKOUT.min.x,
            LOOKOUT_TOP,
            FAR_LOOKOUT.max.x,
            LOOKOUT_TOP + 60.0,
        ),
    );
    add(
        "paddle gap",
        Rect::new(
            LOOKOUT.max.x,
            LOOKOUT_TOP - 200.0,
            FAR_LOOKOUT.min.x,
            LOOKOUT_TOP + 60.0,
        ),
    );
    add(
        "high garden",
        Rect::new(GARDEN.min.x, GARDEN_TOP, GARDEN.max.x, GARDEN_TOP + 60.0),
    );
    add(
        "glow ledge",
        Rect::new(
            GLOW_LEDGE.min.x,
            GLOW_LEDGE_TOP,
            GLOW_LEDGE.max.x,
            GLOW_LEDGE_TOP + 60.0,
        ),
    );
    for (name, _, ledge) in KELP_LEDGES {
        add(
            name,
            Rect::new(ledge.min.x, ledge.max.y, ledge.max.x, ledge.max.y + 60.0),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pools_lie_on_the_floor_line_and_go_left_to_right() {
        for pool in &POOLS {
            assert!(pool.profile.windows(2).all(|w| w[0].x < w[1].x));
            assert!(pool.rock.iter().all(|p| p.y >= pool.bottom));
            assert!(pool.profile.iter().all(|p| p.y >= pool.bottom));
        }
    }
}
