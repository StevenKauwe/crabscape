//! Foods (portions) and the buffs they give (the spec's "Cooking").

use serde::{Deserialize, Serialize};

use super::items::Item;

/// A kind of prepared portion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Food {
    ShellFood,
    ReserveFood,
    ShuckedMussels,
    SnailWrap,
    WormPaste,
    CuredScallop,
    FermentedLanternfish,
    GrenadierFerment,
    VentPoachedSeepMussels,
    VentClamFeast,
}

/// A timed buff from eating (one at a time; a new one replaces it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Buff {
    /// Gathering 10% faster (worm paste).
    Gather10,
    /// Gathering 15% faster (clay-cured scallop).
    Gather15,
    /// Reserve refills 25% faster (snail wrap).
    ReserveQuick,
    /// Pressure strain 30% less (fermented lanternfish).
    Pressure30,
    /// Claws 15% harder (grenadier ferment).
    Claws15,
    /// All of the above (vent clam feast).
    Feast,
}

/// Three minutes of buff, in ticks.
pub const BUFF_TICKS: u32 = 3 * 60 * 120;
/// The feast lasts four minutes.
pub const FEAST_TICKS: u32 = 4 * 60 * 120;

impl Buff {
    /// Percent faster gathering.
    pub fn gather_percent(self) -> u32 {
        match self {
            Buff::Gather10 => 10,
            Buff::Gather15 | Buff::Feast => 15,
            _ => 0,
        }
    }

    /// Percent harder claws.
    pub fn claw_percent(self) -> u32 {
        match self {
            Buff::Claws15 | Buff::Feast => 15,
            _ => 0,
        }
    }

    /// Percent less pressure strain.
    pub fn pressure_percent(self) -> u32 {
        match self {
            Buff::Pressure30 | Buff::Feast => 30,
            _ => 0,
        }
    }

    /// Percent faster reserve refill.
    pub fn reserve_percent(self) -> u32 {
        match self {
            Buff::ReserveQuick | Buff::Feast => 25,
            _ => 0,
        }
    }
}

impl Food {
    pub const ALL: [Food; 10] = [
        Food::ShellFood,
        Food::ReserveFood,
        Food::ShuckedMussels,
        Food::SnailWrap,
        Food::WormPaste,
        Food::CuredScallop,
        Food::FermentedLanternfish,
        Food::GrenadierFerment,
        Food::VentPoachedSeepMussels,
        Food::VentClamFeast,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Food::ShellFood => "Shell food",
            Food::ReserveFood => "Reserve food",
            Food::ShuckedMussels => "Shucked mussels",
            Food::SnailWrap => "Snail and kelp wrap",
            Food::WormPaste => "Worm paste",
            Food::CuredScallop => "Clay-cured scallop",
            Food::FermentedLanternfish => "Fermented lanternfish",
            Food::GrenadierFerment => "Grenadier ferment",
            Food::VentPoachedSeepMussels => "Vent-poached seep mussels",
            Food::VentClamFeast => "Vent clam feast",
        }
    }

    /// The buff eating one gives, and for how many ticks.
    pub fn buff(self) -> Option<(Buff, u32)> {
        match self {
            Food::WormPaste => Some((Buff::Gather10, BUFF_TICKS)),
            Food::CuredScallop => Some((Buff::Gather15, BUFF_TICKS)),
            Food::SnailWrap => Some((Buff::ReserveQuick, BUFF_TICKS)),
            Food::FermentedLanternfish => Some((Buff::Pressure30, BUFF_TICKS)),
            Food::GrenadierFerment => Some((Buff::Claws15, BUFF_TICKS)),
            Food::VentClamFeast => Some((Buff::Feast, FEAST_TICKS)),
            _ => None,
        }
    }

    /// Percent of the shell mended at once on eating.
    pub fn mend_percent(self) -> u32 {
        match self {
            Food::ShellFood => 20,
            Food::ShuckedMussels => 15,
            Food::VentPoachedSeepMussels => 40,
            Food::VentClamFeast => 40,
            _ => 0,
        }
    }

    /// Reserve strokes given at once on eating (`None` of them: a full
    /// reserve is `u8::MAX`).
    pub fn reserve_strokes(self) -> u8 {
        match self {
            Food::ReserveFood => 3,
            Food::SnailWrap => 1,
            Food::VentPoachedSeepMussels | Food::VentClamFeast => u8::MAX,
            _ => 0,
        }
    }

    /// Items making it also gives (shucking leaves a valve).
    pub fn extra(self) -> &'static [(Item, u32)] {
        match self {
            Food::ShuckedMussels => &[(Item::Valve, 1)],
            _ => &[],
        }
    }
}
