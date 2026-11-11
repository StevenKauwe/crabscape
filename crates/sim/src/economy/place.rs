//! Where the nodes lie on today's map. Placed by anchor: a floor `x` and a
//! small height above the floor, resolved against the level's heightmap
//! (`Level::floor_height`), so no `Level` field and no floor or rock edit is
//! needed. Final placement on the world build's new map is a later package.
//!
//! The tiers follow the depth bands (`level/bands.rs`): tier 1 on the home
//! side (x 0 to 1,300) and the far ledge (1,820 to 2,716); tier 2 in the
//! far basin hub (4,490 to 5,000); tier 3 on the doorstep (5,371 to 5,900)
//! and the far rim (7,900 to 8,300); tier 4 on the canyon floor's far end
//! (7,090 to 7,650); tier 5 in the vent field's east end
//! (`level::lower_canyon::VENTS`).
//!
//! Danger guards the way, never the harvest: no node lies within a foe's
//! danger reach (`tests/economy_every_way_pays.rs`), so the first basin
//! (a moray and a hermit crab), the canyon floor by the lobster, the shelf
//! (a moray and the squid's roam), the vent field's west end (amphipods, a
//! hagfish, an isopod) and the rim near the octopus hold no node.

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
/// at least 30 units apart (the hub's and the pockets' are closer).
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
    s(K::SnailRocks, 2170.0),
    s(K::ShoreGlow, 2215.0),
    s(K::MusselClump, 2260.0),
    s(K::KelpStand, 2305.0),
    s(K::GritHeap, 2350.0),
    s(K::SnailRocks, 2395.0),
    s(K::ShoreGlow, 2440.0),
    // Tier 2.
    // The far basin hub; the first basin is held by a moray and a hermit
    // crab, so no node lies there (danger guards the way, never the harvest).
    s(K::ClamBed, 4500.0),
    s(K::SeaPenField, 4534.0),
    s(K::ClayBank, 4568.0),
    s(K::WormMud, 4602.0),
    s(K::PitGlowSite, 4636.0),
    s(K::ClamBed, 4670.0),
    s(K::SeaPenField, 4704.0),
    s(K::ClayBank, 4738.0),
    s(K::WormMud, 4772.0),
    s(K::PitGlowSite, 4806.0),
    s(K::ClamBed, 4840.0),
    s(K::SeaPenField, 4874.0),
    s(K::ClayBank, 4908.0),
    s(K::WormMud, 4942.0),
    s(K::PitGlowSite, 4976.0),
    // Tier 3.
    s(K::ScallopBed, 5385.0),
    s(K::GlassSpongeGarden, 5443.0),
    s(K::ToothBed, 5501.0),
    s(K::FishFall, 5559.0),
    s(K::TwilightGlow, 5617.0),
    s(K::ScallopBed, 5675.0),
    s(K::GlassSpongeGarden, 5733.0),
    s(K::ToothBed, 5791.0),
    s(K::FishFall, 5849.0),
    s(K::TwilightGlow, 7905.0),
    s(K::ScallopBed, 7960.0),
    s(K::GlassSpongeGarden, 8015.0),
    s(K::ToothBed, 8070.0),
    s(K::FishFall, 8125.0),
    s(K::TwilightGlow, 8180.0),
    // Tier 4.
    s(K::SeepMusselBed, 7090.0),
    s(K::BlackCoral, 7130.0),
    s(K::WhaleFallBones, 7170.0),
    s(K::GrenadierCarcass, 7210.0),
    s(K::SulfurMatSite, 7250.0),
    s(K::SeepMusselBed, 7290.0),
    s(K::BlackCoral, 7330.0),
    s(K::WhaleFallBones, 7370.0),
    s(K::GrenadierCarcass, 7410.0),
    s(K::SulfurMatSite, 7450.0),
    s(K::SeepMusselBed, 7490.0),
    s(K::BlackCoral, 7530.0),
    s(K::WhaleFallBones, 7570.0),
    s(K::GrenadierCarcass, 7610.0),
    s(K::SulfurMatSite, 7650.0),
    // Tier 5.
    s(K::VentClamBed, 19730.0),
    s(K::BambooCoral, 19790.0),
    s(K::NoduleField, 19850.0),
    s(K::ScalyFootSnails, 19910.0),
    s(K::VentMatSite, 19970.0),
    s(K::VentClamBed, 20030.0),
    s(K::BambooCoral, 20090.0),
    s(K::NoduleField, 20150.0),
    s(K::ScalyFootSnails, 20210.0),
    s(K::VentMatSite, 20270.0),
    s(K::VentClamBed, 20330.0),
    s(K::BambooCoral, 20390.0),
    s(K::NoduleField, 20450.0),
    s(K::ScalyFootSnails, 20510.0),
    s(K::VentMatSite, 20570.0),
    s(K::VentPot, 19760.0),
    s(K::VentPot, 20360.0),
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
