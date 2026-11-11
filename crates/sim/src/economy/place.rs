//! Where the nodes lie on today's map. Placed by anchor: a floor `x` and a
//! small height above the floor, resolved against the level's heightmap
//! (`Level::floor_height`), so no `Level` field and no floor or rock edit is
//! needed. Final placement on the world build's new map is a later package.
//!
//! The tiers follow the depth bands (`level/bands.rs`): tier 1 on the home
//! side (x 0 to 1,300) and the far ledge (1,820 to 2,716); tier 2 in the
//! basin (2,800 to 3,300) and the far basin hub (4,490 to 5,370); tier 3 on
//! the doorstep (5,371 to 5,900), the canyon shelf (5,900 to 6,150) and the
//! far rim (7,900 to 8,300); tier 4 on the canyon floor (6,150 to 7,650);
//! tier 5 in the vent field (`level::lower_canyon::VENTS`).

use bevy_math::Vec2;

use super::nodes::NodeKind;
use crate::level::Level;

/// A node placed in the level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedNode {
    pub kind: NodeKind,
    /// Where it stands (on the floor).
    pub pos: Vec2,
}

/// An authored spot: an `x` on today's map and a height above the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeSpot {
    pub kind: NodeKind,
    pub x: f32,
    /// Height above `Level::floor_height(x)`.
    pub dy: f32,
}

/// How high above the floor a node's centre sits.
const SIT: f32 = 6.0;

use NodeKind as K;

const fn s(kind: NodeKind, x: f32) -> NodeSpot {
    NodeSpot { kind, x, dy: SIT }
}

/// Every spot, by tier. Nodes of one kind are spread over the tier's places,
/// all at least 40 units apart.
pub const SPOTS: &[NodeSpot] = &[
    // Tier 1.
    // The home side on foot, in two safe pockets (the snapping shrimp at
    // 340 and 590 notice a crab within 120 and strike within 72; the mantis
    // at 1,120 hunts a crab it sees within 300 and roams 800): a pocket
    // before the first burrow (175 to 247) and one between the burrows and
    // short of the mantis (435 to 483). One kind of each skill in each, 12
    // units apart (a crab standing at 211 or 459 reaches the whole pocket
    // and works all five skills without moving), then two more in the first.
    s(K::MusselClump, 175.0),
    s(K::KelpStand, 187.0),
    s(K::GritHeap, 199.0),
    s(K::SnailRocks, 211.0),
    s(K::ShoreGlow, 223.0),
    s(K::MusselClump, 435.0),
    s(K::KelpStand, 447.0),
    s(K::GritHeap, 459.0),
    s(K::SnailRocks, 471.0),
    s(K::ShoreGlow, 483.0),
    s(K::MusselClump, 235.0),
    s(K::KelpStand, 247.0),
    s(K::GritHeap, 1850.0),
    s(K::SnailRocks, 2050.0),
    s(K::ShoreGlow, 2120.0),
    s(K::MusselClump, 2190.0),
    s(K::KelpStand, 2290.0),
    s(K::GritHeap, 2400.0),
    s(K::SnailRocks, 2520.0),
    s(K::ShoreGlow, 2640.0),
    // Tier 2.
    s(K::ClamBed, 2900.0),
    s(K::SeaPenField, 2955.0),
    s(K::ClayBank, 3010.0),
    s(K::WormMud, 3065.0),
    s(K::PitGlowSite, 3120.0),
    s(K::ClamBed, 3175.0),
    s(K::SeaPenField, 3230.0),
    s(K::ClayBank, 3280.0),
    s(K::WormMud, 4520.0),
    s(K::PitGlowSite, 4600.0),
    s(K::ClamBed, 4680.0),
    s(K::SeaPenField, 4760.0),
    s(K::ClayBank, 4840.0),
    s(K::WormMud, 4920.0),
    s(K::PitGlowSite, 5000.0),
    // Tier 3.
    s(K::ScallopBed, 5400.0),
    s(K::GlassSpongeGarden, 5470.0),
    s(K::ToothBed, 5540.0),
    s(K::FishFall, 5610.0),
    s(K::TwilightGlow, 5680.0),
    s(K::ScallopBed, 5750.0),
    s(K::GlassSpongeGarden, 5820.0),
    s(K::ToothBed, 6010.0),
    s(K::FishFall, 6090.0),
    s(K::TwilightGlow, 7960.0),
    s(K::ScallopBed, 8030.0),
    s(K::GlassSpongeGarden, 8100.0),
    s(K::ToothBed, 8170.0),
    s(K::FishFall, 8240.0),
    s(K::TwilightGlow, 8300.0),
    // Tier 4.
    s(K::SeepMusselBed, 6230.0),
    s(K::BlackCoral, 6320.0),
    s(K::WhaleFallBones, 6410.0),
    s(K::GrenadierCarcass, 6500.0),
    s(K::SulfurMatSite, 6590.0),
    s(K::SeepMusselBed, 6680.0),
    s(K::BlackCoral, 6770.0),
    s(K::WhaleFallBones, 6860.0),
    s(K::GrenadierCarcass, 6950.0),
    s(K::SulfurMatSite, 7040.0),
    s(K::SeepMusselBed, 7130.0),
    s(K::BlackCoral, 7220.0),
    s(K::WhaleFallBones, 7310.0),
    s(K::GrenadierCarcass, 7400.0),
    s(K::SulfurMatSite, 7490.0),
    // Tier 5.
    s(K::VentClamBed, 18420.0),
    s(K::BambooCoral, 18500.0),
    s(K::NoduleField, 18680.0),
    s(K::ScalyFootSnails, 18860.0),
    s(K::VentMatSite, 19040.0),
    s(K::VentClamBed, 19220.0),
    s(K::BambooCoral, 19400.0),
    s(K::NoduleField, 19600.0),
    s(K::ScalyFootSnails, 19780.0),
    s(K::VentMatSite, 19960.0),
    s(K::VentClamBed, 20140.0),
    s(K::BambooCoral, 20320.0),
    s(K::NoduleField, 20500.0),
    s(K::ScalyFootSnails, 20600.0),
    s(K::VentMatSite, 20680.0),
    s(K::VentPot, 18770.0),
    s(K::VentPot, 20050.0),
];

/// The spots that lie inside the level, placed on its floor. Which node is
/// which index is fixed by the order of [`SPOTS`] (the shared node state
/// and co-op messages name nodes by index).
pub fn nodes(level: &Level) -> Vec<PlacedNode> {
    SPOTS
        .iter()
        .filter(|s| s.x >= level.min_x && s.x <= level.max_x)
        .map(|s| PlacedNode {
            kind: s.kind,
            pos: Vec2::new(s.x, level.floor_height(s.x) + s.dy),
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
