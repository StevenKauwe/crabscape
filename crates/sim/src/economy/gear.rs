//! Tools, gear and stations (the spec's "Tools and gear" and "Stations").

use serde::{Deserialize, Serialize};

/// The three tool kinds; one of each is fitted at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    /// Opens shellfish off rock and out of mud (Prying).
    Pry,
    /// Digs finds out of sediment (Excavating).
    Scoop,
    /// Cuts kelp, sea pens, sponges and coral (Harvesting).
    Cutter,
}

impl ToolKind {
    pub const ALL: [ToolKind; 3] = [ToolKind::Pry, ToolKind::Scoop, ToolKind::Cutter];

    /// Its slot in `Economy::tools`.
    pub fn index(self) -> usize {
        match self {
            ToolKind::Pry => 0,
            ToolKind::Scoop => 1,
            ToolKind::Cutter => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ToolKind::Pry => "pry tip",
            ToolKind::Scoop => "scoop",
            ToolKind::Cutter => "cutter",
        }
    }

    /// The replay word.
    pub fn word(self) -> &'static str {
        match self {
            ToolKind::Pry => "pry",
            ToolKind::Scoop => "scoop",
            ToolKind::Cutter => "cutter",
        }
    }
}

/// Tool materials by tier: bare claw, shell, quill, glass, bone, nodule.
pub const TOOL_MATERIALS: [&str; 6] = ["bare claw", "shell", "quill", "glass", "bone", "nodule"];

/// Uses before a tool of this tier cracks (tier 0: never).
pub const TOOL_USES: [u16; 6] = [0, 60, 90, 120, 160, 220];

/// The highest tool tier.
pub const MAX_TOOL_TIER: u8 = 5;

/// A fitted tool. Tier 0 is the bare claw (never wears). A cracked tool (a
/// tier above 0 with no uses left) works as tier 0 until repaired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    pub kind: ToolKind,
    pub tier: u8,
    pub uses_left: u16,
}

impl Tool {
    /// The bare claw for a kind.
    pub const fn bare(kind: ToolKind) -> Self {
        Self {
            kind,
            tier: 0,
            uses_left: 0,
        }
    }

    /// A new tool of `tier`, with all its uses.
    pub fn new(kind: ToolKind, tier: u8) -> Self {
        let tier = tier.min(MAX_TOOL_TIER);
        Self {
            kind,
            tier,
            uses_left: TOOL_USES[tier as usize],
        }
    }

    /// Is it a fitted tool with no uses left?
    pub fn cracked(&self) -> bool {
        self.tier > 0 && self.uses_left == 0
    }

    /// The tier it works at now (a cracked tool works as the bare claw).
    pub fn working_tier(&self) -> u8 {
        if self.cracked() { 0 } else { self.tier }
    }

    /// Wear it by one use.
    pub fn wear(&mut self) {
        if self.tier > 0 {
            self.uses_left = self.uses_left.saturating_sub(1);
        }
    }

    /// Restore all its uses.
    pub fn repair(&mut self) {
        self.uses_left = TOOL_USES[self.tier.min(MAX_TOOL_TIER) as usize];
    }
}

/// Where gear is worn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Slot {
    Bag,
    Body,
    Claws,
}

/// Gear, visible on the crab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Gear {
    KelpSack,
    WovenPack,
    CoralPack,
    ShellPlating,
    QuillPlating,
    BonePlating,
    NodulePlating,
    ScaleMail,
    BlackCoralTips,
}

impl Gear {
    pub const ALL: [Gear; 9] = [
        Gear::KelpSack,
        Gear::WovenPack,
        Gear::CoralPack,
        Gear::ShellPlating,
        Gear::QuillPlating,
        Gear::BonePlating,
        Gear::NodulePlating,
        Gear::ScaleMail,
        Gear::BlackCoralTips,
    ];

    pub fn slot(self) -> Slot {
        match self {
            Gear::KelpSack | Gear::WovenPack | Gear::CoralPack => Slot::Bag,
            Gear::BlackCoralTips => Slot::Claws,
            _ => Slot::Body,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Gear::KelpSack => "Kelp sack",
            Gear::WovenPack => "Woven pack",
            Gear::CoralPack => "Coral-ribbed pack",
            Gear::ShellPlating => "Shell plating",
            Gear::QuillPlating => "Quill-ribbed plating",
            Gear::BonePlating => "Bone plating",
            Gear::NodulePlating => "Nodule plating",
            Gear::ScaleMail => "Scale mail",
            Gear::BlackCoralTips => "Black-coral claw tips",
        }
    }

    /// The replay word.
    pub fn word(self) -> &'static str {
        match self {
            Gear::KelpSack => "kelp-sack",
            Gear::WovenPack => "woven-pack",
            Gear::CoralPack => "coral-pack",
            Gear::ShellPlating => "shell-plating",
            Gear::QuillPlating => "quill-plating",
            Gear::BonePlating => "bone-plating",
            Gear::NodulePlating => "nodule-plating",
            Gear::ScaleMail => "scale-mail",
            Gear::BlackCoralTips => "coral-tips",
        }
    }

    /// Extra shell slots (bags).
    pub fn bag_slots(self) -> u8 {
        match self {
            Gear::KelpSack => 2,
            Gear::WovenPack => 4,
            Gear::CoralPack => 6,
            _ => 0,
        }
    }

    /// Damage taken less, in percent (plating).
    pub fn damage_cut_percent(self) -> u32 {
        match self {
            Gear::ShellPlating => 10,
            Gear::QuillPlating => 15,
            Gear::BonePlating => 22,
            Gear::NodulePlating => 28,
            Gear::ScaleMail => 35,
            _ => 0,
        }
    }

    /// Claw damage more, in percent (claw tips).
    pub fn claw_percent(self) -> u32 {
        match self {
            Gear::BlackCoralTips => 15,
            _ => 0,
        }
    }

    /// Blows taken before the plating cracks (0: not plating).
    pub fn plating_uses(self) -> u16 {
        match self {
            Gear::ShellPlating => 40,
            Gear::QuillPlating => 60,
            Gear::BonePlating => 90,
            Gear::NodulePlating => 120,
            Gear::ScaleMail => 160,
            _ => 0,
        }
    }

    /// The plating's tier (1 to 5), which sets its repair cost.
    pub fn plating_tier(self) -> u8 {
        match self {
            Gear::ShellPlating => 1,
            Gear::QuillPlating => 2,
            Gear::BonePlating => 3,
            Gear::NodulePlating => 4,
            Gear::ScaleMail => 5,
            _ => 0,
        }
    }
}

/// What is worn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worn {
    pub bag: Option<Gear>,
    /// The body plating and the blows it can still take.
    pub body: Option<(Gear, u16)>,
    pub claws: Option<Gear>,
}

/// Stations built into the home (the workbench is always there).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Station {
    CultureBed,
    Loom,
    BrinePot,
    Grindstone,
    Crock,
}

impl Station {
    pub const ALL: [Station; 5] = [
        Station::CultureBed,
        Station::Loom,
        Station::BrinePot,
        Station::Grindstone,
        Station::Crock,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Station::CultureBed => "Culture bed",
            Station::Loom => "Loom",
            Station::BrinePot => "Brine pot",
            Station::Grindstone => "Grindstone",
            Station::Crock => "Fermenting crock",
        }
    }

    /// The replay word.
    pub fn word(self) -> &'static str {
        match self {
            Station::CultureBed => "culture-bed",
            Station::Loom => "loom",
            Station::BrinePot => "brine-pot",
            Station::Grindstone => "grindstone",
            Station::Crock => "crock",
        }
    }

    /// Building level needed (a culture bed: the first one).
    pub fn building_level(self) -> u32 {
        match self {
            Station::CultureBed => 5,
            Station::Loom => 12,
            Station::BrinePot => 10,
            Station::Grindstone => 18,
            Station::Crock => 30,
        }
    }
}

/// Culture beds: Building level and materials for beds 1 to 4.
pub const BED_LEVELS: [u32; 4] = [5, 20, 40, 60];

/// What a repair mends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MendTarget {
    /// The fitted tool of this kind.
    Tool(ToolKind),
    /// The body plating.
    Plating,
}

impl super::Economy {
    /// What a repair of this kind would mend now: the first fitted tool of
    /// the tier that has worn, or the plating of the tier if it has worn.
    pub fn repair_target(&self, repair: super::recipes::Repairs) -> Option<MendTarget> {
        use super::recipes::Repairs;
        match repair {
            Repairs::Tool(tier) => ToolKind::ALL
                .into_iter()
                .find(|&k| {
                    let t = self.tool(k);
                    t.tier == tier && tier > 0 && t.uses_left < TOOL_USES[usize::from(tier)]
                })
                .map(MendTarget::Tool),
            Repairs::Plating(tier) => match self.gear.body {
                Some((g, uses)) if g.plating_tier() == tier && uses < g.plating_uses() => {
                    Some(MendTarget::Plating)
                }
                _ => None,
            },
        }
    }

    /// Restore all the uses of what `target` names.
    pub fn mend(&mut self, target: MendTarget) {
        match target {
            MendTarget::Tool(kind) => self.tools[kind.index()].repair(),
            MendTarget::Plating => {
                if let Some((g, uses)) = &mut self.gear.body {
                    *uses = g.plating_uses();
                }
            }
        }
    }
}
