//! Gathering nodes: what each kind is, needs and gives (the spec's "Tiers"
//! and "Nodes"). Data only; the runtime is the gather code.

use serde::{Deserialize, Serialize};

use super::gear::ToolKind;
use super::items::Item;
use crate::skills::Skill;

/// One kind of node: 25 gathering kinds (five skills by five tiers), five
/// culture sites and the vent pot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    MusselClump,
    ClamBed,
    ScallopBed,
    SeepMusselBed,
    VentClamBed,
    KelpStand,
    SeaPenField,
    GlassSpongeGarden,
    BlackCoral,
    BambooCoral,
    GritHeap,
    ClayBank,
    ToothBed,
    WhaleFallBones,
    NoduleField,
    SnailRocks,
    WormMud,
    FishFall,
    GrenadierCarcass,
    ScalyFootSnails,
    ShoreGlow,
    PitGlowSite,
    TwilightGlow,
    SulfurMatSite,
    VentMatSite,
    VentPot,
}

/// What a kind of node is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeDef {
    pub kind: NodeKind,
    pub skill: Skill,
    pub tier: u8,
    /// The skill level needed.
    pub level: u32,
    /// The tool kind used (Foraging and Culturing use none).
    pub tool: Option<ToolKind>,
    /// The tool tier needed (tier - 1).
    pub tool_tier: u8,
    /// XP per gather.
    pub xp: u32,
    /// Base seconds per gather.
    pub base_secs: f32,
    /// Gathers before it is spent.
    pub charges: u8,
    /// Seconds until a spent node regrows all its charges.
    pub respawn_secs: f32,
    /// What it gives (the vent pot gives nothing: it cooks).
    pub yields: Option<Item>,
    pub name: &'static str,
    /// What the prompt says: "Hold E: pry mussels".
    pub verb: &'static str,
}

/// Level required, gather XP, base seconds, charges and respawn seconds by
/// tier 1 to 5.
const LEVEL: [u32; 5] = [1, 15, 30, 45, 60];
const XP: [u32; 5] = [10, 110, 160, 260, 750];
const SECS: [f32; 5] = [2.5, 3.0, 3.5, 4.0, 4.5];
const CHARGES: [u8; 5] = [5, 5, 4, 4, 4];
const RESPAWN: [f32; 5] = [45.0, 60.0, 90.0, 150.0, 240.0];
/// Culturing wild sites pay less than gathering.
const CULTURE_XP: [u32; 5] = [8, 88, 160, 208, 600];

const fn gather(
    kind: NodeKind,
    skill: Skill,
    tier: u8,
    tool: Option<ToolKind>,
    yields: Item,
    name: &'static str,
    verb: &'static str,
) -> NodeDef {
    let t = tier as usize - 1;
    NodeDef {
        kind,
        skill,
        tier,
        level: LEVEL[t],
        tool,
        tool_tier: tier - 1,
        xp: if matches!(skill, Skill::Culturing) {
            CULTURE_XP[t]
        } else {
            XP[t]
        },
        base_secs: SECS[t],
        charges: CHARGES[t],
        respawn_secs: RESPAWN[t],
        yields: Some(yields),
        name,
        verb,
    }
}

use NodeKind as K;
use Skill::{Culturing, Excavating, Foraging, Harvesting, Prying};
use ToolKind::{Cutter, Pry, Scoop};

/// Every node kind, by skill then tier (the spec's table).
pub const NODES: &[NodeDef] = &[
    gather(
        K::MusselClump,
        Prying,
        1,
        Some(Pry),
        Item::Mussel,
        "Mussel clump",
        "pry mussels",
    ),
    gather(
        K::ClamBed,
        Prying,
        2,
        Some(Pry),
        Item::Clam,
        "Clam bed",
        "pry clams",
    ),
    gather(
        K::ScallopBed,
        Prying,
        3,
        Some(Pry),
        Item::Scallop,
        "Glass scallop bed",
        "pry scallops",
    ),
    gather(
        K::SeepMusselBed,
        Prying,
        4,
        Some(Pry),
        Item::SeepMussel,
        "Seep mussel bed",
        "pry seep mussels",
    ),
    gather(
        K::VentClamBed,
        Prying,
        5,
        Some(Pry),
        Item::VentClam,
        "Vent clam bed",
        "pry vent clams",
    ),
    gather(
        K::KelpStand,
        Harvesting,
        1,
        Some(Cutter),
        Item::Kelp,
        "Kelp stand",
        "cut kelp",
    ),
    gather(
        K::SeaPenField,
        Harvesting,
        2,
        Some(Cutter),
        Item::Quill,
        "Sea-pen field",
        "cut sea-pen quills",
    ),
    gather(
        K::GlassSpongeGarden,
        Harvesting,
        3,
        Some(Cutter),
        Item::Glass,
        "Glass sponge garden",
        "cut glass sponge",
    ),
    gather(
        K::BlackCoral,
        Harvesting,
        4,
        Some(Cutter),
        Item::BlackCoral,
        "Black coral",
        "cut black coral",
    ),
    gather(
        K::BambooCoral,
        Harvesting,
        5,
        Some(Cutter),
        Item::BambooCoral,
        "Bamboo coral",
        "cut bamboo coral",
    ),
    gather(
        K::GritHeap,
        Excavating,
        1,
        Some(Scoop),
        Item::Grit,
        "Grit heap",
        "scoop grit",
    ),
    gather(
        K::ClayBank,
        Excavating,
        2,
        Some(Scoop),
        Item::Clay,
        "Clay bank",
        "dig clay",
    ),
    gather(
        K::ToothBed,
        Excavating,
        3,
        Some(Scoop),
        Item::Tooth,
        "Tooth bed",
        "dig fossil teeth",
    ),
    gather(
        K::WhaleFallBones,
        Excavating,
        4,
        Some(Scoop),
        Item::WhaleBone,
        "Whale-fall bones",
        "free whale bone",
    ),
    gather(
        K::NoduleField,
        Excavating,
        5,
        Some(Scoop),
        Item::Nodule,
        "Nodule field",
        "dig nodules",
    ),
    gather(
        K::SnailRocks,
        Foraging,
        1,
        None,
        Item::Snail,
        "Snail rocks",
        "pick snails",
    ),
    gather(
        K::WormMud,
        Foraging,
        2,
        None,
        Item::Worm,
        "Worm mud",
        "pick worms",
    ),
    gather(
        K::FishFall,
        Foraging,
        3,
        None,
        Item::Lanternfish,
        "Fish fall",
        "gather lanternfish",
    ),
    gather(
        K::GrenadierCarcass,
        Foraging,
        4,
        None,
        Item::Grenadier,
        "Grenadier carcass",
        "drag the grenadier",
    ),
    gather(
        K::ScalyFootSnails,
        Foraging,
        5,
        None,
        Item::ScalyFoot,
        "Scaly-foot snails",
        "pick scaly-foot snails",
    ),
    gather(
        K::ShoreGlow,
        Culturing,
        1,
        None,
        Item::ShoreStrain,
        "Shore glow",
        "take shore strain",
    ),
    gather(
        K::PitGlowSite,
        Culturing,
        2,
        None,
        Item::Culture,
        "Pit glow site",
        "take glowing culture",
    ),
    gather(
        K::TwilightGlow,
        Culturing,
        3,
        None,
        Item::TwilightStrain,
        "Twilight glow",
        "take twilight strain",
    ),
    gather(
        K::SulfurMatSite,
        Culturing,
        4,
        None,
        Item::SulfurMat,
        "Sulfur mat",
        "lift sulfur mat",
    ),
    gather(
        K::VentMatSite,
        Culturing,
        5,
        None,
        Item::VentMat,
        "Vent mat",
        "lift vent mat",
    ),
    // The vent pot: cooks what is on the shell, right at the vent.
    NodeDef {
        kind: K::VentPot,
        skill: Skill::Cooking,
        tier: 5,
        level: 50,
        tool: None,
        tool_tier: 0,
        xp: 0,
        base_secs: 4.5,
        charges: 1,
        respawn_secs: 0.0,
        yields: None,
        name: "Vent pot",
        verb: "cook at the vent",
    },
];

impl NodeKind {
    /// Its row in [`NODES`].
    pub fn def(self) -> &'static NodeDef {
        NODES
            .iter()
            .find(|d| d.kind == self)
            .expect("every node kind has a row")
    }

    /// The kind that gives `item` (the first, if several).
    pub fn giving(item: Item) -> Option<NodeKind> {
        NODES
            .iter()
            .find(|d| d.yields == Some(item))
            .map(|d| d.kind)
    }
}

impl NodeDef {
    /// Ticks one gather takes at skill `level` with a tool of `tool_tier`
    /// and `buff` percent from food:
    /// `base_s * 120 * 100 / (100 + 2*(level - req) + 15*(tool_tier - (tier - 1)) + buff)`,
    /// never below 40% of base.
    pub fn gather_ticks(&self, level: u32, tool_tier: u8, buff: u32) -> u32 {
        let base = self.base_secs * 120.0;
        let level_part = 2 * level.saturating_sub(self.level) as i32;
        let tool_part = 15 * (i32::from(tool_tier) - i32::from(self.tier - 1));
        let speed = (100 + level_part + tool_part + buff as i32).max(1) as f32;
        (base * 100.0 / speed).max(base * 0.4).round() as u32
    }
}
