//! The test level as plain data: a strip of sea floor with a slope, some rock
//! ledges and a trench you can only cross with the paddle double jump. At
//! its right end a tall cliff that only a crab with all three strokes can
//! crawl over, and behind it a drop into a deep basin, with rock steps back
//! up. In the basin floor, a narrow lip over a deep pit: kelp near the top,
//! coral in the dark below, both climbable all the way back out.
//!
//! The physics asks it questions ("how high is the floor here?"). Drawing it
//! is the game's job (`src/level.rs`).
//!
//! World coordinates: x grows to the right, y grows UP (Bevy's convention),
//! and the flat sea floor where the crab starts is at y = 0.

use bevy_math::{Rect, Vec2};

use crate::enemy::Kind;
use crate::tuning::TideTuning;

pub mod bands;
mod bestiary;
mod circulation;
pub(crate) mod coral_rise;
pub mod deep_galleries;
mod exploration;
mod hub;
pub mod landmarks;
pub mod layers;
mod lower_canyon;
pub mod open_canyon;
mod plate_d;
pub mod refuges;
pub mod rest;
pub mod royal;
pub mod shallows;
pub mod silt_galleries;
pub mod slope_caves;
pub mod strata;

/// The canyon's floor (the sea-pen floor): the lower canyon is the deepest step.
pub(crate) const CANYON_FLOOR: f32 = -6000.0;

/// Where the crab starts (its feet), if there is no save.
pub const SPAWN: Vec2 = Vec2::new(160.0, 0.0);

/// Creature and local-distance scale: the 75-unit carapace represents 12 cm.
/// The demo compresses the vertical separation of ocean zones, so this does
/// not convert a position into an actual sea depth.
pub const UNITS_PER_METRE: f32 = 625.0;

/// The water's surface, eight metres above the starting floor at creature
/// scale. The opening kelp now leads to the owner's above-water royal region.
pub const WATER_SURFACE_Y: f32 = 8.0 * UNITS_PER_METRE;

/// Size of a fish egg. A crab eats one when it touches its body (with a
/// little reach for the claws).
pub const EGG_RADIUS: f32 = 14.0;

/// Something growing straight up that the crab can grip and climb: a
/// vertical line at `x`, from `bottom` to `top`.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Climb {
    pub x: f32,
    pub bottom: f32,
    pub top: f32,
    pub kind: Growth,
}

/// What a [`Climb`] is (for the drawing; the crab climbs both the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Growth {
    /// Giant kelp: grows where light still reaches, buoyed up by its floats.
    Kelp,
    /// Deep-sea coral: grows on rock in the dark, no light needed.
    Coral,
    /// Bare, pitted rock: nothing grows there, and a crab's leg tips slide
    /// off it. Only a crab with glass grips can climb it.
    Rock,
    /// Human litter made useful: a hanging rope, gripped just like a stalk.
    Rope,
}

/// A crevice under a rock ledge out in the level: a rest stop. A crab
/// tucks into it the way it tucks into its home shell (see
/// [`crate::home`]): walking into the doorway, it backs in to the den at the
/// back, and rests there. The last one it rested in is where it wakes after
/// dying. Like home, its opening faces right, so in is to the left.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Crevice {
    /// The doorway on the floor: walking left in here, a crab backs in.
    pub door: Rect,
    /// Where a crab sits tucked in (its feet), at the back under the rock.
    pub den: Vec2,
}

/// A place a crab can tuck into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Place {
    /// Its home shell.
    Home,
    /// One of the level's crevices (its index in [`Level::crevices`]).
    Crevice(usize),
}

/// A ghost net across a way through: solid until it's cut.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Net {
    pub rect: Rect,
    pub cut: bool,
    /// The side its knots can be reached from (see [`CutFrom`]).
    #[serde(default)]
    pub from: CutFrom,
}

/// Which side of a ghost net a crab must stand on to cut it. A net hauled
/// tight over a rock lip from one side bulges, slack and out of reach of the
/// claws, from the other: the shortcuts a crab opens on the way home (a
/// level-design call, not a biology claim).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CutFrom {
    /// From either side.
    #[default]
    Either,
    /// Only by a crab whose middle is west (left) of the net's.
    West,
    /// Only by a crab whose middle is east (right) of the net's.
    East,
    /// Only by a crab below it, digging up (a plug in a ceiling).
    Below,
}

impl Net {
    /// A net across a way through, cuttable from either side.
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            cut: false,
            from: CutFrom::Either,
        }
    }

    /// Can a crab whose middle is at `x` get its claws at the knots?
    pub fn cuttable_from(&self, x: f32) -> bool {
        let middle = self.rect.center().x;
        match self.from {
            CutFrom::Either | CutFrom::Below => true,
            CutFrom::West => x < middle,
            CutFrom::East => x > middle,
        }
    }

    /// Can a crab standing (feet) at `pos` get at the net?
    pub fn cuttable_at(&self, pos: Vec2) -> bool {
        match self.from {
            CutFrom::Below => pos.y < self.rect.min.y,
            _ => self.cuttable_from(pos.x),
        }
    }
}

/// A lump of brittle rock: solid until a heavy crusher's full charge cracks
/// it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Brittle {
    pub rect: Rect,
    pub cracked: bool,
}

/// The shape of the level. The [`Sim`](crate::Sim) owns it; the game draws it
/// from there.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Level {
    /// Optional, disposable material simulation for the opening section.
    #[serde(skip)]
    pub cells: Option<crate::underwater::World>,
    /// The sea floor's surface, as points from left to right. Between two
    /// points the floor is a straight line. Two points at (almost) the same x
    /// make a cliff.
    pub floor: Vec<Vec2>,
    /// Solid rocks, as axis-aligned boxes. The crab can stand on them and
    /// bumps into their sides and undersides.
    pub rocks: Vec<Rect>,
    /// Fish eggs drifting in the water (their middles). Eating one gives a
    /// crab one more swim stroke for good.
    pub eggs: Vec<Vec2>,
    /// Kelp and coral the crab can grip and climb.
    pub climbs: Vec<Climb>,
    /// Home: the doorway of an empty sea-snail shell near the start, its
    /// opening to the right. A crab walking in here on the floor backs inside
    /// to rest, and it's where things are brought back to (see
    /// [`crate::home`]).
    pub home: Rect,
    /// Where a crab sits tucked into the home shell (its feet), by the
    /// workbench.
    pub home_den: Vec2,
    /// Crevices under rock out in the level, to rest in on the way (see
    /// [`Crevice`]).
    pub crevices: Vec<Crevice>,
    /// Patches of wild glowing bacteria (their middles). A crab that
    /// touches one carries a lump home on its back; cultured there, it makes
    /// the crab's lantern brighter.
    pub cultures: Vec<Vec2>,
    /// Clues to the trench's mystery, where a crab finds one by touching it.
    pub clues: Vec<Vec2>,
    /// Shed teeth lying about: a crab can carry one home, where the
    /// workbench fits it to the crusher claw (scissor claws).
    pub teeth: Vec<Vec2>,
    /// Shell grit: a heap of old, broken shells on the floor (its middle).
    /// A crab carries one piece at a time home, where the workbench builds
    /// it into the home shell. Each lies there once.
    pub grit: Vec<Vec2>,
    /// Dead glass sponges' skeletons, lying on the floor (their middles). A
    /// crab carries one home, where the workbench fits its glass points to
    /// the leg tips (glass grips).
    pub sponges: Vec<Vec2>,
    /// Clam beds: a clam half-buried in the floor (its middle). A crab
    /// carries one home to its larder; a taken bed grows a new one in time.
    pub clams: Vec<Vec2>,
    /// Ghost nets: lost fishing nets snagged across a way through. Solid
    /// until a crab with scissor claws cuts them (then gone for everyone).
    pub nets: Vec<Net>,
    /// Brittle rock: solid until a crab with the heavy crusher (a feat)
    /// cracks it with a fully charged smash (then gone for everyone).
    pub brittle: Vec<Brittle>,
    /// Places hidden in darkness: they look like solid rock until a bright
    /// enough lantern comes close (the drawing's job; the rules let a crab
    /// in either way).
    pub secrets: Vec<Rect>,
    /// The animals living here, each at its burrow or den (see
    /// [`Sim::populate`](crate::Sim::populate)).
    pub foes: Vec<(Kind, Vec2)>,
    /// Where the tide runs: the middle of the canyon below the crack, as
    /// boxes of water that run up and down together (see [`Level::tide`]).
    pub currents: Vec<Rect>,
    /// Where water runs up all the time, whatever the tide does: a lane of
    /// upwelling along the canyon's near wall, from its floor to above the
    /// shelf, so no crab is ever stuck down there waiting for the flood.
    /// Real canyons carry upwelling, deep water flowing up-canyon onto the
    /// shelf while the wind drives it (Allen & Durrieu de Madron 2009,
    /// sourced in `world.md`); here it never stops.
    pub upwelling: Vec<Rect>,
    /// How the tide runs right now, set by the [`Sim`](crate::Sim) every
    /// tick from [`TideTuning::flow`](crate::tuning::TideTuning::flow): 1 =
    /// up at full speed, -1 = down, 0 = slack.
    pub tide: f32,
    /// The mouth of the crack in the far basin's floor, at the foot of its
    /// end wall: the way down into the canyon.
    pub crack: Vec2,
    /// The lights seen far below from the crack: the middle of the canyon's
    /// floor. The demo ends when a crab gets there.
    pub lights: Vec2,
    /// An octopus's den: a sheltered chamber under the rock at the top of the
    /// canyon's far wall, its floor a midden of shells (the inside, floor to
    /// roof). Empty for now: the common octopus moves in later.
    pub den: Rect,
    /// Open water out of the tide where something drifting can hang (spots
    /// for the alarm jelly, when it comes).
    pub drifting: Vec<Vec2>,
    /// The crab can't go further left or right than this (the level's walls).
    pub min_x: f32,
    pub max_x: f32,
    /// This level's layer map, kept once it is first asked for (derived:
    /// never saved, never compared; see [`LayerCache`]).
    #[serde(skip)]
    pub(crate) layer_cache: LayerCache,
}

/// A level's layer map, remembered with a stamp of the floor and rocks it was
/// built from: a level whose floor or rocks have since changed (a test that
/// clears the rocks, say) misses it and goes to the shared cache as before.
/// Physics asks for the map under every crab in a cavern, every step, so the
/// shared cache's full comparison of the floor and every rock was too slow
/// to pay each time.
#[derive(Default)]
pub struct LayerCache(std::sync::Mutex<Option<(u64, std::sync::Arc<Derived>)>>);

/// What a level derives from its floor and rocks, kept in its [`LayerCache`].
pub(crate) struct Derived {
    map: std::sync::Arc<layers::LayerMap>,
    /// For each `ROCK_STRIP`-wide strip of x from `ROCK_X0`, the indices of
    /// the rocks reaching into it, in order.
    rock_strips: Vec<Vec<u32>>,
    /// Does the level have the Silt Galleries (`has_silt_galleries`)?
    galleries: bool,
}

const ROCK_STRIP: f32 = 256.0;
const ROCK_X0: f32 = -4096.0;
const ROCK_STRIPS: usize = 128;

fn rock_strip(x: f32) -> usize {
    let b = ((x - ROCK_X0) / ROCK_STRIP).floor();
    if b.is_nan() || b < 0.0 {
        0
    } else {
        (b as usize).min(ROCK_STRIPS - 1)
    }
}

impl Clone for LayerCache {
    fn clone(&self) -> Self {
        let entry = self.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        LayerCache(std::sync::Mutex::new(entry))
    }
}

impl PartialEq for LayerCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for LayerCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LayerCache")
    }
}

impl Level {
    /// The authored bed at this depth, shared by all players.
    pub fn band_at(&self, pos: Vec2) -> &bands::Band {
        bands::band_at(pos)
    }

    /// Vertical component, retained for scent and legacy scalar queries.
    pub fn water_at(&self, pos: Vec2, tide: &TideTuning) -> f32 {
        self.water_velocity(pos, tide).y
    }

    /// Ambient canyon water: a balanced near-wall circulation plus the existing
    /// central tidal through-flow. Each upwelling loop has balanced rise and return,
    /// with zero normal flow at its enclosing floor and side boundaries.
    /// This is an authored ambient field, not the cellular fluid solver.
    pub fn water_velocity(&self, pos: Vec2, tide: &TideTuning) -> Vec2 {
        let mut velocity = Vec2::ZERO;
        for lane in &self.upwelling {
            // Stay inside the open shaft, including the far-wall cave's
            // projecting face at x=7700. No flow is clipped after the curl.
            let right = (lane.max.x + 450.0).min(7650.0);
            if right <= lane.max.x {
                continue;
            }
            let bounds = Rect::new(lane.min.x, lane.min.y, right, lane.max.y + 500.0);
            velocity += circulation::circulation(
                pos,
                bounds,
                lane.max.x,
                tide.upwelling_speed,
                120.0,
                600.0,
            );
        }
        // The separate central tidal through-flow retains its established
        // traversal behavior. The near-wall circulation is additive and
        // does not clip or mask either field at their overlap.
        if self.currents.iter().any(|bounds| bounds.contains(pos)) {
            velocity.y += tide.speed(self.tide);
        }
        velocity
    }

    /// Every authored place tagged with its fear register (`world-vision-brief.md`).
    /// Derived from the level's own data on demand: never stored, saved or hashed.
    pub fn landmarks(&self) -> Vec<landmarks::Landmark> {
        landmarks::existing_level(self)
    }

    /// The 2D layer map this level authors: the source of every solid cell.
    /// Built on demand; cheap enough for setup and queries, not for a tick.
    pub fn layers(&self) -> layers::LayerMap {
        layers::LayerMap::of(self)
    }

    /// The layer map, shared between callers that ask for the same terrain
    /// (the cell generator asks once per column).
    pub(crate) fn layers_shared(&self) -> std::sync::Arc<layers::LayerMap> {
        std::sync::Arc::clone(&self.derived().map)
    }

    /// The level's derived lookups, built once per floor and rocks.
    fn derived(&self) -> std::sync::Arc<Derived> {
        let stamp = self.layer_stamp();
        let mut kept = self.layer_cache.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, derived)) = kept.as_ref()
            && *at == stamp
        {
            return std::sync::Arc::clone(derived);
        }
        let mut rock_strips = vec![Vec::new(); ROCK_STRIPS];
        for (i, r) in self.rocks.iter().enumerate() {
            for strip in rock_strips
                .iter_mut()
                .take(rock_strip(r.max.x) + 1)
                .skip(rock_strip(r.min.x))
            {
                strip.push(i as u32);
            }
        }
        let derived = std::sync::Arc::new(Derived {
            map: self.layers_looked_up(),
            rock_strips,
            galleries: self.has_silt_galleries(),
        });
        *kept = Some((stamp, std::sync::Arc::clone(&derived)));
        derived
    }

    /// A cheap stamp of the floor and rocks: their counts and end points.
    fn layer_stamp(&self) -> u64 {
        let mut words = vec![
            self.floor.len() as u64,
            self.rocks.len() as u64,
            self.nets.len() as u64,
        ];
        for p in [self.floor.first(), self.floor.last()]
            .into_iter()
            .flatten()
        {
            words.push(u64::from(p.x.to_bits()) << 32 | u64::from(p.y.to_bits()));
        }
        for r in [self.rocks.first(), self.rocks.last()]
            .into_iter()
            .flatten()
        {
            words.push(u64::from(r.min.x.to_bits()) << 32 | u64::from(r.max.y.to_bits()));
        }
        words.iter().fold(0xcbf2_9ce4_8422_2325, |h, w| {
            (h ^ w).wrapping_mul(0x0100_0000_01b3)
        })
    }

    fn layers_looked_up(&self) -> std::sync::Arc<layers::LayerMap> {
        use std::sync::{Arc, Mutex};
        type Entry = (Vec<Vec2>, Vec<Rect>, Arc<layers::LayerMap>);
        static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, _, map)) = cache
            .iter()
            .find(|(floor, rocks, _)| *floor == self.floor && *rocks == self.rocks)
        {
            return Arc::clone(map);
        }
        let map = Arc::new(layers::LayerMap::of(self));
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((self.floor.clone(), self.rocks.clone(), Arc::clone(&map)));
        map
    }

    /// Is the ground solid at `p` (rock, silt or clay), caverns included?
    pub fn solid_at(&self, p: Vec2) -> bool {
        self.layers_shared().solid_at(p)
    }

    /// The nearest solid top at or under `p`: where a body at `p` lands. Inside
    /// a cavern this is the cavern's floor, not the sea floor above it.
    pub fn ground_below(&self, p: Vec2) -> Option<f32> {
        self.layers_shared().ground_below(p)
    }

    /// The ground under a body whose feet are at height `y`, over `x`: the
    /// sea floor when it is at or above it, and in a cavern the cavern's own
    /// floor (`ground_below`). A body inside bare soil stands on the sea floor.
    pub fn footing(&self, x: f32, y: f32) -> f32 {
        let surface = self.floor_height(x);
        if y >= surface - 2.0 {
            return surface;
        }
        self.cavern_footing(x, y, surface)
    }

    /// [`footing`](Self::footing) for a body *placed* by an author at `y`
    /// (a spawn: `Sim::spawn_enemy`): only a place a good way under the sea
    /// floor (a cavern, 400 units down) counts as another floor; anything
    /// nearer is the sea floor with its y ignored, as it has always been.
    pub fn placed_footing(&self, x: f32, y: f32) -> f32 {
        let surface = self.floor_height(x);
        if y >= surface - 400.0 {
            return surface;
        }
        self.cavern_footing(x, y, surface)
    }

    fn cavern_footing(&self, x: f32, y: f32, surface: f32) -> f32 {
        // Only in open water under the surface (a cavern): a body buried in
        // soil or rock stands on the sea floor as ever.
        let map = self.layers_shared();
        let at = Vec2::new(x, y);
        if map.solid_at(at) {
            return surface;
        }
        map.ground_below(at).unwrap_or(surface)
    }

    /// The level the game uses (for now, the only one).
    pub fn test_level() -> Self {
        let mut level = Self::before_galleries();
        silt_galleries::shape(&mut level);
        deep_galleries::shape(&mut level);
        open_canyon::shape(&mut level);
        shallows::shape(&mut level);
        slope_caves::shape(&mut level);
        // Last, so every place a creature anchors to (landmarks, refuges, the
        // lower canyon, the galleries) exists.
        bestiary::place(&mut level);
        level
    }

    /// The world as it was before the Silt Galleries: the reference the layer
    /// map's raster is proved against cell for cell.
    #[cfg(test)]
    pub(crate) fn world_without_galleries() -> Self {
        let mut level = Self::before_galleries();
        bestiary::place(&mut level);
        level
    }

    fn before_galleries() -> Self {
        let mut level = Self {
            cells: None,
            floor: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(520.0, 0.0),
                // A slope up onto a plateau.
                Vec2::new(820.0, 110.0),
                Vec2::new(1300.0, 110.0),
                // The trench: 520 units across, too wide for one jump (or a
                // dodge- or sprint-jump), even sinking slowly or diving.
                Vec2::new(1301.0, -260.0),
                Vec2::new(1819.0, -260.0),
                Vec2::new(1820.0, 110.0),
                Vec2::new(2090.0, 110.0),
                // A gentler slope back down.
                Vec2::new(2340.0, 20.0),
                // The way out: a cliff whose top a crab can only just crawl
                // onto at the peak of a three-stroke swim from the far ledge.
                Vec2::new(2716.0, 20.0),
                Vec2::new(2717.0, 860.0),
                Vec2::new(2800.0, 860.0),
                // Behind its crest, a sheer drop into a deep basin...
                Vec2::new(2801.0, -700.0),
                // ...whose floor runs flat to the hub (the old pit is filled:
                // its finds lie in the burrows under the floor now), rising
                // at its far end...
                Vec2::new(5200.0, -700.0),
                Vec2::new(5340.0, -560.0),
                Vec2::new(5370.0, -560.0),
                // ...to the crack at the foot of its end wall: a slot down
                // through the rock (a chimney, wide enough for the biggest
                // crab to climb its coral), its
                // floor running on under the wall and out onto the doorstep,
                // high on the near wall of a canyon.
                Vec2::new(5371.0, -1500.0),
                Vec2::new(5900.0, -1500.0),
                // The canyon's near wall: a sheer drop to a shelf (coral and
                // a rest crevice)...
                Vec2::new(5901.0, -3400.0),
                Vec2::new(6150.0, -3400.0),
                // ...and a longer one to the canyon's floor, where the
                // lights are (the lower canyon is the deepest step now).
                Vec2::new(6151.0, CANYON_FLOOR),
                Vec2::new(7900.0, CANYON_FLOOR),
                // Its far wall, up to the far rim and an octopus's den (the
                // east steps down 2,000 further: `plate_d::drop_east`).
                Vec2::new(7901.0, -1900.0),
                Vec2::new(8800.0, -1900.0),
            ],
            rocks: vec![
                // A low ledge near the start: one jump gets you up.
                Rect::new(400.0, 120.0, 520.0, 150.0),
                // A boulder half-buried in the slope.
                Rect::new(640.0, 0.0, 720.0, 100.0),
                // A high ledge over the plateau: needs the paddle stroke.
                Rect::new(930.0, 300.0, 1080.0, 330.0),
                // A sea arch at the plateau's seaward end: a rock pinnacle
                // standing up to the surface, with the plateau running on
                // under it. It stops a crab that climbs the shallow kelp
                // from gliding off the surface over the trench and the
                // cliff (#449): the gates are passed by swimming, not from
                // above.
                Rect::new(1250.0, 430.0, 1300.0, WATER_SURFACE_Y),
                // A stepping stone inside the trench, to climb back out.
                Rect::new(1330.0, -260.0, 1430.0, -130.0),
                // A block and a ledge above it, past the trench.
                Rect::new(1940.0, 110.0, 2020.0, 200.0),
                Rect::new(2070.0, 300.0, 2220.0, 330.0),
                // The way back up out of the basin: ledges in two columns
                // against the back of the cliff, each a jump and two strokes
                // above the last, and a rest stop.
                Rect::new(2950.0, -420.0, 3050.0, -390.0),
                Rect::new(2810.0, -110.0, 2890.0, -80.0),
                Rect::new(2950.0, 200.0, 3050.0, 230.0),
                Rect::new(2810.0, 510.0, 2890.0, 540.0),
                // The hub's ridge: the old pit wall's footing, kept as bedrock
                // under the floor (`hub.rs` raises it above the floor).
                Rect::new(4300.0, -4230.0, 4370.0, -700.0),
                // Past the pit: a pillar.
                Rect::new(5080.0, -700.0, 5160.0, -300.0),
                // The far basin's end wall, standing over the crack and far
                // too tall to swim over; under it, the way through to the
                // doorstep.
                Rect::new(5500.0, -1300.0, 5640.0, 560.0),
                // High in its canyon face, a notch (dead-ended on the basin's
                // side): only glass grips climb the bare face to it.
                Rect::new(5500.0, 560.0, 5540.0, 760.0),
                Rect::new(5500.0, 760.0, 5640.0, 3000.0),
                // A cave in the canyon's far wall, out of the tide's reach:
                // the wall's face stands out over the floor, with a hollow
                // cut back into it (hidden: see `secrets`), and the fifth
                // egg drifts inside. Below the cave and above it, the face.
                Rect::new(7700.0, CANYON_FLOOR, 7901.0, -5200.0),
                Rect::new(7700.0, -5030.0, 7901.0, -3900.0),
                // The rock over the octopus's den, at the end of the far rim.
                Rect::new(8350.0, -1620.0, 8800.0, 3000.0),
            ],
            // Each egg needs the paddle strokes the eggs before it give: A a
            // plain jump from the low ledge (it gives the double jump), B over
            // the high ledge with one stroke (the high ledge and the trench
            // need it too), C high over the far ledge with two.
            eggs: vec![
                Vec2::new(460.0, 330.0),
                Vec2::new(1005.0, 680.0),
                Vec2::new(2145.0, 810.0),
                // The bone hall's prize, under the basin floor.
                lower_canyon::BONE_PRIZE,
                // On the canyon's far shelf: only riding the flood up the
                // canyon and pushing off it at the right height gets there.
                Vec2::new(7810.0, -5170.0),
                // High over the far basin, above the coral on the end wall:
                // only resting on that coral leaves the strokes to get here.
                Vec2::new(5460.0, 1490.0),
            ],
            // Up the pit, a little out from the tip of the lip: coral in the
            // dark lower part, then a kelp stalk floating up from its
            // holdfast on the shelf to just above the lip, so climbing out
            // pulls the crab up beside it.
            climbs: vec![
                // A shallow-water kelp brings the surface into reach from
                // the plateau without altering the deep canyon routes.
                Climb {
                    x: royal::ROPE_X,
                    bottom: 0.0,
                    top: WATER_SURFACE_Y - 20.0,
                    kind: Growth::Kelp,
                },
                // Up the crack, back out onto the far basin.
                Climb {
                    x: 5440.0,
                    bottom: -1500.0,
                    top: -590.0,
                    kind: Growth::Coral,
                },
                // Cold-water coral on the canyon's near wall, from the shelf
                // up to the doorstep. Nothing climbs from the canyon's
                // floor: the way up from there is the water.
                Climb {
                    x: 6000.0,
                    bottom: -3400.0,
                    top: -1450.0,
                    kind: Growth::Coral,
                },
                // Bare rock, for glass grips only: the canyon's near wall
                // from its floor up to the shelf (no waiting for the
                // flood), and the end wall's canyon face from the doorstep
                // up to the notch.
                Climb {
                    x: 6175.0,
                    bottom: CANYON_FLOOR,
                    top: -3420.0,
                    kind: Growth::Rock,
                },
                Climb {
                    x: 5664.0,
                    bottom: -1500.0,
                    top: 560.0,
                    kind: Growth::Rock,
                },
                // Coral growing out of the end wall's face, high over the far
                // basin: no single stroke reaches its foot. An egg drifts four
                // strokes above its top, more than the legs have left on
                // arriving: only a crab that rests on the coral (gripping
                // dactyls) swims on up to it.
                Climb {
                    x: 5470.0,
                    bottom: 110.0,
                    top: 800.0,
                    kind: Growth::Coral,
                },
            ],
            home: Rect::new(40.0, -1.0, 150.0, 40.0),
            home_den: Vec2::new(95.0, 0.0),
            // Rest stops, each at a dead end so no crab walks into one by
            // the way: past the trench every wall foot on the way home is
            // already a moray's. One at the foot of the cliff's back, where
            // a crab drops into the basin (the low ledge above is clear of
            // it), and one at the bottom of the pit, against its left wall,
            // away from the coral.
            crevices: vec![
                Crevice {
                    door: Rect::new(2801.0, -701.0, 2880.0, -660.0),
                    den: Vec2::new(2840.0, -700.0),
                },
                // At the back of the canyon's shelf, against the wall below
                // the doorstep (the coral stands clear of it).
                Crevice {
                    door: Rect::new(5902.0, -3401.0, 5975.0, -3360.0),
                    den: Vec2::new(5941.0, -3400.0),
                },
            ],
            // At the bottom of the pit, across from the coral.
            // At the bottom of the pit, and in the notch high in the end
            // wall's canyon face.
            // And sealed inside the lump of brittle rock in the far basin.
            cultures: vec![
                // The pit's culture, on the basin floor by the ridge.
                Vec2::new(4240.0, -690.0),
                Vec2::new(5590.0, 570.0),
                Vec2::new(5042.0, -690.0),
            ],
            // A dead glass sponge's skeleton on the canyon's floor, among the
            // sea pens, near its far wall.
            sponges: vec![Vec2::new(7620.0, CANYON_FLOOR + 10.0)],
            // Beside the bone hall's prize: a shed tooth bigger than the crab.
            clues: vec![lower_canyon::BONE_TOOTH],
            teeth: vec![lower_canyon::BONE_TOOTH],
            // Clams, half-buried: two in the shallows near home (between
            // the snapping shrimp) and one in the basin floor below the
            // cliff; harder ones on the flats past the trench, by the moray,
            // and at the bottom of the pit.
            clams: vec![
                Vec2::new(250.0, 4.0),
                Vec2::new(480.0, 4.0),
                Vec2::new(3150.0, -696.0),
                Vec2::new(2450.0, 24.0),
                // The pit's clam, on the basin floor.
                Vec2::new(3500.0, -696.0),
                // In the canyon: two on its floor, one on the far rim.
                Vec2::new(6650.0, CANYON_FLOOR + 4.0),
                Vec2::new(7450.0, CANYON_FLOOR + 4.0),
                Vec2::new(8120.0, -1896.0),
            ],
            // Shell grit, more of it deeper: one heap in the shallows (by
            // the first snapping shrimp's burrow, clear of the clams), one on the
            // slope past the trench, one in the basin below the cliff, one
            // in the far basin past the ghost net, and two at the bottom of
            // the pit. Finishing the home takes a trip to the deep.
            grit: vec![
                Vec2::new(380.0, 4.0),
                Vec2::new(2200.0, 74.0),
                Vec2::new(3010.0, -696.0),
                Vec2::new(4900.0, -696.0),
                Vec2::new(3460.0, -696.0),
                Vec2::new(3620.0, -696.0),
                // The canyon: one on the shelf, two on its floor.
                Vec2::new(6100.0, -3396.0),
                Vec2::new(6900.0, CANYON_FLOOR + 4.0),
                Vec2::new(7600.0, CANYON_FLOOR + 4.0),
            ],
            // The only net left in the base level is the ridge's window
            // (`hub.rs` adds it): the scissor claw's gate to the hub.
            nets: vec![],
            // On the far basin's floor past the ghost net, at the foot of the
            // pillar, a lump of brittle rock (old coral rubble crusted together) with glowing bacteria
            // sealed in a hollow inside: only a heavy crusher's full charge
            // cracks it.
            brittle: vec![Brittle {
                rect: Rect::new(5005.0, -700.0, 5080.0, -610.0),
                cracked: false,
            }],
            secrets: vec![
                // The far wall's cave.
                Rect::new(7700.0, -5200.0, 7901.0, -5030.0),
            ],
            // A peacock mantis shrimp's burrow on the plateau, before the
            // trench: the first fight (it sees the crab coming up the slope).
            // The route, easy to hard, with room to breathe between: snapping
            // shrimp in the shallows rubble, the mantis on the plateau, a
            // moray in a crevice of the trench, and past the trench a shrimp
            // on the far ledge and a moray guarding the flats. At the very
            // end, past the canyon, the common octopus in its den: a rock
            // with its back to the wall, facing the way in.
            foes: vec![
                (Kind::Snapper, Vec2::new(590.0, 0.0)),
                // Buried beside the mantis mouth's door in the basin floor
                // (the pocket chain under it; it never enters).
                (Kind::Mantis, Vec2::new(3880.0, -700.0)),
                (Kind::Moray, Vec2::new(1760.0, -260.0)),
                (Kind::Snapper, Vec2::new(2060.0, 110.0)),
                (Kind::Moray, Vec2::new(2650.0, 20.0)),
                (Kind::Octopus, Vec2::new(8650.0, -1900.0)),
            ],
            // The tide runs up and down the canyon's middle, from its floor
            // to well above the doorstep, with still water along each wall.
            currents: vec![Rect::new(6450.0, CANYON_FLOOR, 7350.0, -700.0)],
            // Against the near wall, from the floor to a little above the
            // shelf: hop into it and it carries the crab up beside the
            // shelf, onto which it steps (and the coral climbs on up).
            upwelling: vec![Rect::new(6152.0, CANYON_FLOOR, 6400.0, -3150.0)],
            tide: 0.0,
            crack: Vec2::new(5440.0, -560.0),
            lights: Vec2::new(7000.0, CANYON_FLOOR),
            // Under the rock at the end of the far rim, back to the wall.
            den: Rect::new(8350.0, -1900.0, 8800.0, -1620.0),
            // Still water by the far wall, deep in the canyon.
            drifting: vec![Vec2::new(7520.0, -5380.0), Vec2::new(7510.0, -5390.0)],
            min_x: 0.0,
            max_x: 8800.0,
            layer_cache: LayerCache::default(),
        };
        exploration::extend(&mut level);
        coral_rise::shape(&mut level);
        hub::shape(&mut level);
        refuges::shape(&mut level);
        plate_d::drop_east(&mut level);
        lower_canyon::shape(&mut level);
        plate_d::place_finds(&mut level);
        level
    }

    /// The climb a crab standing (feet) at `pos` could grip, if any: one it
    /// overlaps sideways (within `reach` of its middle) and between its ends.
    /// Bare rock counts only for a crab with glass grips (`glass`).
    pub fn climb_at(&self, pos: Vec2, reach: f32, glass: bool) -> Option<Climb> {
        self.climbs
            .iter()
            .copied()
            .chain(royal::climbs())
            .map(|mut c| {
                if let Some(cells) = &self.cells
                    && c.kind == Growth::Kelp
                    && c.x == 280.0
                {
                    c.x = royal::tied_kelp_x(cells.giant_kelp_x(pos.y), pos.y);
                }
                c
            })
            .find(|c| {
                (glass || c.kind != Growth::Rock)
                    && (pos.x - c.x).abs() <= reach
                    && pos.y >= c.bottom - 1.0
                    && pos.y <= c.top
            })
    }

    /// The place whose doorway `pos` is in, if any.
    pub fn place_at(&self, pos: Vec2) -> Option<Place> {
        if self.home.contains(pos) {
            return Some(Place::Home);
        }
        self.crevices
            .iter()
            .position(|c| c.door.contains(pos))
            .map(Place::Crevice)
    }

    /// A place's doorway and den (an unknown crevice is home).
    pub fn door_and_den(&self, place: Place) -> (Rect, Vec2) {
        match place {
            Place::Crevice(i) if i < self.crevices.len() => {
                (self.crevices[i].door, self.crevices[i].den)
            }
            _ => (self.home, self.home_den),
        }
    }

    /// The pit's old footprint in the basin: (left edge x, right edge x,
    /// bottom y). It is filled to the basin floor (y = -700) now.
    pub fn pit(&self) -> (f32, f32, f32) {
        (3300.0, 4300.0, -700.0)
    }

    /// Height of the sea floor's surface at `x`.
    pub fn floor_height(&self, x: f32) -> f32 {
        if let Some(bottom) = self.hole_bottom(x) {
            return bottom;
        }
        let pts = &self.floor;
        if x <= pts[0].x {
            return pts[0].y;
        }
        for pair in pts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if x <= b.x {
                let t = (x - a.x) / (b.x - a.x);
                return a.y + (b.y - a.y) * t;
            }
        }
        pts[pts.len() - 1].y
    }

    /// Everything solid the crab can bump into or stand on: the rocks, plus
    /// a thin wall for each cliff in the sea floor. (Walls make cliff edges
    /// behave like rock edges: the crab only drops off once its whole shell
    /// is past the edge, and it can't walk into the side of a cliff.)
    pub fn solids(&self) -> impl Iterator<Item = Rect> + '_ {
        self.rocks.iter().copied().chain(self.solids_but_rocks())
    }

    /// [`solids`](Self::solids) that can reach across `x0..=x1`, in the same
    /// order: physics asks this for each crab several times a step, and most
    /// of the map's rocks are far away.
    pub fn solids_across(&self, x0: f32, x1: f32) -> impl Iterator<Item = Rect> + '_ {
        let derived = self.derived();
        let mut ids: Vec<u32> = derived.rock_strips
            [rock_strip(x0)..=rock_strip(x1).max(rock_strip(x0))]
            .iter()
            .flatten()
            .copied()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .map(|i| self.rocks[i as usize])
            .chain(self.solids_but_rocks_with(derived.galleries))
            .filter(move |r| r.max.x >= x0 && r.min.x <= x1)
    }

    fn solids_but_rocks(&self) -> impl Iterator<Item = Rect> + '_ {
        let galleries = self.cells.is_none() && self.derived().galleries;
        self.solids_but_rocks_with(galleries)
    }

    fn solids_but_rocks_with(&self, galleries: bool) -> impl Iterator<Item = Rect> + '_ {
        let cliffs = self.floor.windows(2).filter_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let is_cliff = (b.x - a.x) < 2.0 && (b.y - a.y).abs() > 1.0;
            // The material field supplies its own terrain here. Retaining
            // the old invisible cliff walls creates phantom pillars when
            // the visible cells have moved or been dug away.
            let in_cells = self.cells.as_ref().is_some_and(|cells| {
                let (left, _, width, _) = cells.game_bounds();
                a.x >= left && b.x <= left + width
            });
            (is_cliff && !in_cells)
                // The wall stands from the foot of the cliff to its top, not
                // on down through the ground: caverns under it are open.
                .then(|| Rect::new(a.x, a.y.min(b.y), b.x, a.y.max(b.y)))
        });
        let nets = self.nets.iter().filter(|n| !n.cut).map(|n| n.rect);
        let brittle = self.brittle.iter().filter(|b| !b.cracked).map(|b| b.rect);
        // Soil that holds a body up in a cavern when there is no cell world
        // to say so (the cells carry it otherwise, so a dug wall stays dug).
        let soil: &'static [Rect] = if self.cells.is_none() && galleries {
            silt_galleries::walls()
        } else {
            &[]
        };
        cliffs
            .chain(nets)
            .chain(brittle)
            .chain(royal::solids())
            .chain(soil.iter().copied())
    }

    /// The trench that needs the double jump: (left edge x, right edge x).
    pub fn trench(&self) -> (f32, f32) {
        (1300.0, 1820.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_height_follows_the_line() {
        let level = Level::test_level();
        assert_eq!(level.floor_height(-50.0), 0.0); // left of the first point
        assert_eq!(level.floor_height(300.0), 0.0); // flat start
        assert!((level.floor_height(670.0) - 55.0).abs() < 0.01); // half-way up the slope
        assert_eq!(level.floor_height(1450.0), -260.0); // bottom of the trench
        assert_eq!(level.floor_height(17150.0), -4800.0); // half-way up the cavern bank
        assert_eq!(level.floor_height(level.max_x + 100.0), -10000.0); // beyond the outer bank
    }

    #[test]
    fn spawn_is_on_open_floor() {
        let level = Level::test_level();
        assert_eq!(level.floor_height(SPAWN.x), SPAWN.y);
        assert!(!level.rocks.iter().any(|r| r.contains(SPAWN + Vec2::Y)));
    }

    #[test]
    fn floor_points_go_left_to_right() {
        let level = Level::test_level();
        assert!(level.floor.windows(2).all(|p| p[0].x < p[1].x));
    }
}
