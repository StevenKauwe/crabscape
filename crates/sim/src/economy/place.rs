//! Where the nodes lie on the Plate D map. A spot is either a floor `x` and a
//! small height above the floor (resolved against `Level::floor_height`), or
//! it names a landmark (`Level::landmarks()`) and lies inside it: on the
//! floor of the sealed cavity under that landmark's rect (the home burrow's
//! gallery, the bone hall, the seep garden), or on the floor inside an open
//! water landmark (the vent field). A named place that a level lacks drops
//! its spots, as a level that is too short drops a far spot. No `Level` field
//! and no floor or rock edit is needed.
//!
//! The tiers follow the depth bands (`level/bands.rs`; a tier-k node lies in
//! band k or deeper): tier 1 on the home side (x 0 to 1,300) and the far
//! ledge (1,820 to 2,716); tier 2 in the home burrow's clay gallery (clam
//! beds and clay banks, a tunnel under the shallows: safe) and, for the
//! other three skills, the far basin hub (4,490 to 5,000); tier 3 on the
//! doorstep (5,371 to 5,900), the far rim (7,900 to 8,300) and, as the
//! chamber prize, the bone hall's floor (a whale-fall's carcass and teeth:
//! the hall lies at about -2,500, the twilight's depth, so tier 4 cannot be
//! put there); tier 4 on the canyon floor's far end (7,090 to 7,650) and in
//! the seep garden (seep mussels and the sulfur mat); tier 5 in the vent
//! field's east end (`level::lower_canyon::VENTS`).
//!
//! Danger guards the way, never the harvest: no node lies within a foe's
//! danger reach (`tests/economy_every_way_pays.rs`), so the first basin
//! (a moray and a hermit crab), the canyon floor by the lobster, the shelf
//! (a moray and the squid's roam), the vent field's west end (amphipods, a
//! hagfish, an isopod) and the rim near the octopus hold no node.

use bevy_math::Vec2;

use super::nodes::NodeKind;
use crate::level::Level;
use crate::level::landmarks::{Kind as LandmarkKind, Landmark};
use crate::level::strata::StrataBody;

/// A node placed in the level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedNode {
    pub kind: NodeKind,
    /// Where it stands (on the floor).
    pub pos: Vec2,
}

/// An authored spot: an `x` on the map and a height above the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeSpot {
    pub kind: NodeKind,
    /// The world `x`; for a named place it is clamped inside the place.
    pub x: f32,
    /// Height above the floor (`Level::floor_height(x)`, or the floor of the
    /// named place's cavity).
    pub dy: f32,
    /// The landmark this spot lies inside, by name; `None` for the open map.
    pub place: Option<&'static str>,
}

/// How high above the floor a node's centre sits.
const SIT: f32 = 6.0;

use NodeKind as K;

const fn s(kind: NodeKind, x: f32) -> NodeSpot {
    NodeSpot {
        kind,
        x,
        dy: SIT,
        place: None,
    }
}

/// A spot inside the named landmark.
const fn at(place: &'static str, kind: NodeKind, x: f32) -> NodeSpot {
    NodeSpot {
        kind,
        x,
        dy: SIT,
        place: Some(place),
    }
}

const HALL: &str = "bone hall";
const GARDEN: &str = "seep garden";
const VENTS: &str = "vent field";
/// The Silt Galleries' upper gallery (its three sponge galleries; `level::silt_galleries`).
const GALLERIES: &str = "upper gallery";
/// The Deep Galleries' three halls (`level::deep_galleries`).
const LANTERN_HALL: &str = "lanternfish hall";
const CORAL_HALL: &str = "black coral hall";
const NODULE_FIELD: &str = "nodule field";
const WHALE_HALL: &str = "whale-fall hall";

/// Every spot, by tier. Nodes of one kind are spread over the tier's places,
/// at least 30 units apart (the hub's and the pockets' are closer).
pub const SPOTS: &[NodeSpot] = &[
    // Tier 1.
    // The home side on foot, in two safe pockets (the snapping shrimp at
    // 340 and 590 notice a crab within 120 and strike within 72): one between
    // the home burrow's first silt shaft (its mouth reaches x 220) and the
    // first shrimp (x 222 to 242), the other between the shrimp (x 432 to
    // 492). Ten nodes, two of each skill, 10 units apart (a crab standing in
    // a pocket reaches the whole of it and works every skill in it).
    s(K::ShoreGlow, 222.0),
    s(K::KelpStand, 232.0),
    s(K::SnailRocks, 242.0),
    s(K::MusselClump, 432.0),
    s(K::KelpStand, 442.0),
    s(K::GritHeap, 452.0),
    s(K::SnailRocks, 462.0),
    s(K::ShoreGlow, 472.0),
    s(K::MusselClump, 482.0),
    s(K::GritHeap, 492.0),
    // The far ledge, clear of the snapper at 2,060 and the moray at 2,650.
    s(K::GritHeap, 1850.0),
    s(K::SnailRocks, 2165.0),
    s(K::ShoreGlow, 2200.0),
    s(K::MusselClump, 2235.0),
    s(K::KelpStand, 2270.0),
    s(K::GritHeap, 2305.0),
    s(K::SnailRocks, 2340.0),
    s(K::ShoreGlow, 2375.0),
    s(K::MusselClump, 2410.0),
    s(K::KelpStand, 2445.0),
    // Tier 2.
    // The home burrow's clay gallery holds no nodes: it is 23.5 s of dig from
    // the door and safe, so tier 2 there would pay twice the hub (938 XP a
    // minute); its two clams (the world's own gallery find) are the first
    // dig's payoff. Lead's call; see `skills-and-resources.md`.
    s(K::ClamBed, 4527.5),
    s(K::ClayBank, 4582.5),
    s(K::ClamBed, 4637.5),
    s(K::ClayBank, 4692.5),
    s(K::ClamBed, 4747.5),
    s(K::ClayBank, 4802.5),
    // The far basin hub for all five skills; the first basin is held
    // by a moray and a hermit crab, so no node lies there (danger guards the
    // way, never the harvest).
    s(K::SeaPenField, 4500.0),
    s(K::WormMud, 4555.0),
    s(K::PitGlowSite, 4610.0),
    s(K::SeaPenField, 4665.0),
    s(K::WormMud, 4720.0),
    s(K::PitGlowSite, 4775.0),
    s(K::SeaPenField, 4830.0),
    s(K::WormMud, 4885.0),
    s(K::PitGlowSite, 4940.0),
    // Tier 3.
    // The canyon's doorstep and the far rim: scallops and twilight glow.
    // The glass gardens grow in the Silt Galleries' first two sponge
    // galleries (#638), beside their own glass sponges: the lamp's want sends
    // the crab under the trench, not across the canyon. The third gallery
    // holds none: it lies within a hermit crab's reach of the plateau above.
    // (Same indices as the doorstep and rim gardens they replace.)
    s(K::ScallopBed, 5575.0),
    at(GALLERIES, K::GlassSpongeGarden, 2060.0),
    s(K::TwilightGlow, 5617.0),
    s(K::ScallopBed, 5675.0),
    at(GALLERIES, K::GlassSpongeGarden, 2620.0),
    s(K::TwilightGlow, 7905.0),
    s(K::ScallopBed, 7960.0),
    at(GALLERIES, K::GlassSpongeGarden, 2420.0),
    s(K::TwilightGlow, 5790.0),
    // The bone hall's floor: a whale-fall's carcass and teeth, in sight of
    // the door at 3,800 and the seep shaft at 4,110 (the egg and the tooth
    // lie further west, at 3,600 and 3,680).
    at(HALL, K::FishFall, 3880.0),
    at(HALL, K::ToothBed, 3930.0),
    at(HALL, K::FishFall, 3980.0),
    at(HALL, K::ToothBed, 4030.0),
    at(HALL, K::FishFall, 4080.0),
    at(HALL, K::ToothBed, 4130.0),
    // Tier 4.
    // The canyon floor's far end.
    s(K::BlackCoral, 7130.0),
    s(K::WhaleFallBones, 7170.0),
    s(K::GrenadierCarcass, 7210.0),
    s(K::BlackCoral, 7330.0),
    s(K::WhaleFallBones, 7370.0),
    s(K::GrenadierCarcass, 7410.0),
    s(K::BlackCoral, 7530.0),
    s(K::WhaleFallBones, 7570.0),
    s(K::GrenadierCarcass, 7610.0),
    // The seep garden, east of the door (its find lies at 19,160): seep
    // mussels and the sulfur mat, clear of the isopod's leash.
    at(GARDEN, K::SeepMusselBed, 19430.0),
    at(GARDEN, K::SulfurMatSite, 19465.0),
    at(GARDEN, K::SeepMusselBed, 19500.0),
    at(GARDEN, K::SulfurMatSite, 19535.0),
    at(GARDEN, K::SeepMusselBed, 19570.0),
    at(GARDEN, K::SulfurMatSite, 19605.0),
    // Tier 5.
    // The vent field's east end, west of the crust vault's door (x 20,480 to
    // 20,600).
    at(VENTS, K::VentClamBed, 19730.0),
    at(VENTS, K::VentPot, 19775.0),
    at(VENTS, K::BambooCoral, 19820.0),
    at(VENTS, K::NoduleField, 19865.0),
    at(VENTS, K::ScalyFootSnails, 19910.0),
    at(VENTS, K::VentMatSite, 19955.0),
    at(VENTS, K::VentClamBed, 20000.0),
    at(VENTS, K::BambooCoral, 20045.0),
    at(VENTS, K::NoduleField, 20090.0),
    at(VENTS, K::ScalyFootSnails, 20135.0),
    at(VENTS, K::VentMatSite, 20180.0),
    at(VENTS, K::VentPot, 20225.0),
    at(VENTS, K::VentClamBed, 20270.0),
    at(VENTS, K::BambooCoral, 20315.0),
    at(VENTS, K::NoduleField, 20360.0),
    at(VENTS, K::ScalyFootSnails, 20405.0),
    at(VENTS, K::VentMatSite, 20450.0),
    // The Deep Galleries under the Drop (`level::deep_galleries::NODE_SITES`,
    // the world lane's sites; appended, so earlier indices keep their
    // meaning). The lanternfish hall (floor -4,200) holds fish falls (tier 3)
    // and the whale-fall hall (-8,200) whale-fall bones (tier 4), as authored.
    // The black coral hall (-6,200) holds black coral (tier 4: black corals
    // do grow this deep, not verified), and the nodule field (-10,200, the
    // deep band) the tier 5 nodules.
    at(LANTERN_HALL, K::FishFall, 2100.0),
    at(LANTERN_HALL, K::FishFall, 2400.0),
    at(LANTERN_HALL, K::FishFall, 2700.0),
    at(CORAL_HALL, K::BlackCoral, 2200.0),
    at(CORAL_HALL, K::BlackCoral, 2500.0),
    at(CORAL_HALL, K::BlackCoral, 2800.0),
    at(WHALE_HALL, K::WhaleFallBones, 3400.0),
    at(WHALE_HALL, K::WhaleFallBones, 3700.0),
    at(WHALE_HALL, K::WhaleFallBones, 4000.0),
    // Region 5's shelf tops (`level::open_canyon::gather_spots`, the world
    // lane's: a glass sponge and, 90 east, a bed beside it), out past the
    // far rim, east of the canyon. The three deeper tops (the twilight band)
    // hold glass gardens and glass scallop beds (tier 3); the kelp ledge, at
    // -700, is still the basin band, so it holds a sea-pen field and a clam
    // bed (tier 2).
    at("sponge garden kelp ledge", K::SeaPenField, 11000.0),
    at("sponge garden kelp ledge", K::ClamBed, 11090.0),
    at("sponge garden tower shelf", K::GlassSpongeGarden, 9350.0),
    at("sponge garden tower shelf", K::ScallopBed, 9440.0),
    at("sponge garden first arch", K::GlassSpongeGarden, 11980.0),
    at("sponge garden first arch", K::ScallopBed, 12070.0),
    at("sponge garden second arch", K::GlassSpongeGarden, 13230.0),
    at("sponge garden second arch", K::ScallopBed, 13320.0),
    at(NODULE_FIELD, K::NoduleField, 1620.0),
    at(NODULE_FIELD, K::NoduleField, 1700.0),
    at(NODULE_FIELD, K::NoduleField, 1780.0),
    // The half-tier nodes (tier 1, level 8), three of each kind, provisional
    // (the world lane may move them). Two of each lie in the home pockets,
    // between the first pocket's nodes and the second's (5 units from them,
    // still clear of the shrimp at 340 and 590: 93 units off), so a crab at
    // level 8 finds them where it already works; the third of each lies on
    // the plateau past the second shrimp (floor 110: a walk and a hop from
    // the second pocket), clear of the moray at 1,446. Appended, so earlier
    // indices hold.
    s(K::BarnacleRock, 227.0),
    s(K::EelgrassBed, 237.0),
    s(K::BrittleStarCrevice, 247.0),
    s(K::SandDollarBed, 437.0),
    s(K::PurpleFilmMat, 447.0),
    s(K::BarnacleRock, 457.0),
    s(K::EelgrassBed, 467.0),
    s(K::BrittleStarCrevice, 477.0),
    s(K::SandDollarBed, 487.0),
    s(K::PurpleFilmMat, 497.0),
    s(K::BarnacleRock, 900.0),
    s(K::BrittleStarCrevice, 940.0),
    s(K::SandDollarBed, 980.0),
    s(K::EelgrassBed, 1020.0),
    s(K::PurpleFilmMat, 1060.0),
];

/// Where a spot lies on this level, or `None` when its named place is not
/// in the level.
fn place_of(
    level: &Level,
    marks: &[Landmark],
    bodies: &[StrataBody],
    s: &NodeSpot,
) -> Option<Vec2> {
    let Some(name) = s.place else {
        return Some(Vec2::new(s.x, level.floor_height(s.x) + s.dy));
    };
    let mark = marks.iter().find(|l| l.name == name)?;
    let x = s.x.clamp(mark.rect.min.x + 10.0, mark.rect.max.x - 10.0);
    // A sealed body: the floor of the cavity under `x` (its water void).
    if let Some(body) = bodies.iter().find(|b| b.frame == mark.rect) {
        let floor = body
            .voids
            .iter()
            .filter(|v| {
                v.min.x <= x
                    && x < v.max.x
                    && v.min.y >= body.frame.min.y
                    && v.max.y <= body.frame.max.y
            })
            .map(|v| v.min.y)
            .min_by(f32::total_cmp)?;
        return Some(Vec2::new(x, floor + s.dy));
    }
    // A cave in the layer map (the Silt Galleries): the ground under the
    // room's roof at `x`, if it lies inside the room.
    if matches!(mark.kind, LandmarkKind::Tunnel | LandmarkKind::Frame)
        && let Some(floor) = level.ground_below(Vec2::new(x, mark.rect.max.y - 1.0))
        && floor >= mark.rect.min.y - 1.0
    {
        return Some(Vec2::new(x, floor + s.dy));
    }
    // Open water (the vent field): its seabed. A pocket whose bedrock this
    // level lacks has nowhere to put a node.
    (mark.kind == LandmarkKind::OpenWater).then(|| Vec2::new(x, level.floor_height(x) + s.dy))
}

/// The spots that lie inside the level, placed on its floor. Which node is
/// which index is fixed by the order of [`SPOTS`] (the shared node state
/// and co-op messages name nodes by index).
pub fn nodes(level: &Level) -> Vec<PlacedNode> {
    let marks = level.landmarks();
    let bodies = level.strata();
    SPOTS
        .iter()
        .filter(|s| s.x >= level.min_x && s.x <= level.max_x)
        .filter_map(|s| {
            Some(PlacedNode {
                kind: s.kind,
                pos: place_of(level, &marks, &bodies, s)?,
            })
        })
        .collect()
}

/// The depth band (0 shallows to 4 deep) a point lies in.
pub fn band_index(level: &Level, pos: Vec2) -> usize {
    let name = level.band_at(pos).name;
    level
        .bands()
        .iter()
        .position(|b| b.name == name)
        .unwrap_or(0)
}
